#include "scanner_api.h"
#include <opencv2/opencv.hpp>
#include <vector>
#include <string>
#include <mutex>

using namespace cv;
using namespace std;

// Internal scanner state
struct ScannerImpl
{
    VideoCapture cap;
    ScanConfig config;
    Mat first_frame;
    Mat normalized_first;
    Rect p1_drive_roi, p2_drive_roi;
    Rect sys_notif_roi, frame_meter_roi;
    Rect p1_key_roi, p2_key_roi;
    double fps;
    int total_frames;
    int current_frame;
    string last_error;
    bool hud_detected;

    ScannerImpl() : fps(0), total_frames(0), current_frame(0), hud_detected(false) {}

    bool open_video(const char *path, const ScanConfig *cfg)
    {
        config = cfg ? *cfg : ScanConfig{1, 300, 0};
        cap.open(path);
        if (!cap.isOpened())
        {
            last_error = "Failed to open video: " + string(path);
            return false;
        }

        fps = cap.get(CAP_PROP_FPS);
        total_frames = (int)cap.get(CAP_PROP_FRAME_COUNT);
        current_frame = 0;

        // Read first frame for HUD detection
        if (!cap.read(first_frame) || first_frame.empty())
        {
            last_error = "Failed to read first frame";
            return false;
        }

        return true;
    }

    // Normalize frame to 1080p with letterboxing
    Mat normalize_frame(const Mat &src)
    {
        const int TARGET_W = 1920, TARGET_H = 1080;
        double scale = min((double)TARGET_W / src.cols, (double)TARGET_H / src.rows);
        int new_w = (int)(src.cols * scale);
        int new_h = (int)(src.rows * scale);

        Mat resized;
        resize(src, resized, Size(new_w, new_h), 0, 0, INTER_LINEAR);

        Mat canvas = Mat::zeros(TARGET_H, TARGET_W, src.type());
        int x_off = (TARGET_W - new_w) / 2;
        int y_off = (TARGET_H - new_h) / 2;
        Mat roi = canvas(Rect(x_off, y_off, new_w, new_h));
        resized.copyTo(roi);

        return canvas;
    }

    // Detect HUD layout using timer as anchor
    bool detect_hud()
    {
        Mat gray;
        cvtColor(normalized_first, gray, COLOR_BGR2GRAY);

        // Threshold to find bright timer region
        Mat binary;
        threshold(gray, binary, 200, 255, THRESH_BINARY);

        vector<vector<Point>> contours;
        findContours(binary, contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE);

        int frame_cx = normalized_first.cols / 2;
        int search_top = normalized_first.rows * 0.05;
        int search_bot = normalized_first.rows * 0.15;

        double best_score = 0;
        Point best_center;

        for (auto &cnt : contours)
        {
            double area = contourArea(cnt);
            if (area < 100 || area > 5000)
                continue;

            Rect r = boundingRect(cnt);
            int cx = r.x + r.width / 2;
            int cy = r.y + r.height / 2;

            if (cy < search_top || cy > search_bot)
                continue;

            double dist = abs(cx - frame_cx);
            double score = 1.0 / (1.0 + dist / 100.0);

            if (score > best_score)
            {
                best_score = score;
                best_center = Point(cx, cy);
            }
        }

        if (best_score == 0)
        {
            last_error = "Timer not found";
            return false;
        }

        // Calculate ROIs relative to timer (normalized coordinates)
        double tx = best_center.x / 1920.0;
        double ty = best_center.y / 1080.0;

        const double OFFSET_LEFT = 0.27;
        const double OFFSET_GAUGE_Y = 0.035;
        const double GAUGE_W = 0.21;
        const double GAUGE_H = 0.02;

        // 1P drive gauge (left of timer)
        int p1_gx1 = (int)((tx - OFFSET_LEFT - GAUGE_W) * 1920);
        int p1_gx2 = (int)((tx - OFFSET_LEFT) * 1920);
        int gy1 = (int)((ty + OFFSET_GAUGE_Y) * 1080);
        int gy2 = (int)((ty + OFFSET_GAUGE_Y + GAUGE_H) * 1080);
        p1_drive_roi = Rect(max(0, p1_gx1), max(0, gy1), max(1, p1_gx2 - p1_gx1), max(1, gy2 - gy1));

        // 2P drive gauge (right of timer)
        int p2_gx1 = (int)((tx + OFFSET_LEFT) * 1920);
        int p2_gx2 = (int)((tx + OFFSET_LEFT + GAUGE_W) * 1920);
        p2_drive_roi = Rect(max(0, p2_gx1), max(0, gy1), max(1, p2_gx2 - p2_gx1), max(1, gy2 - gy1));

        // System notification (center)
        sys_notif_roi = Rect(1920 * 0.65, 1080 * 0.43, 1920 * 0.20, 1080 * 0.05);

        // Frame meter (bottom)
        frame_meter_roi = Rect(1920 * 0.18, 1080 * 0.70, 1920 * 0.64, 1080 * 0.12);

        // Key history ROIs
        p1_key_roi = Rect(0, 1080 * 0.17, p1_drive_roi.x, 1080 * 0.65);
        p2_key_roi = Rect(p2_drive_roi.x + p2_drive_roi.width, 1080 * 0.17,
                          1920 - (p2_drive_roi.x + p2_drive_roi.width), 1080 * 0.65);

        hud_detected = true;
        return true;
    }

    // Preprocess ROI for text/icon detection
    Mat preprocess_roi(const Mat &roi, bool is_key_history = false, bool is_frame_meter = false)
    {
        Mat hsv, mask, result;
        cvtColor(roi, hsv, COLOR_BGR2HSV);

        Scalar lower, upper;
        if (is_key_history)
        {
            lower = Scalar(0, 0, 180);
            upper = Scalar(180, 50, 255);
        }
        else if (is_frame_meter)
        {
            lower = Scalar(0, 0, 150);
            upper = Scalar(180, 60, 255);
        }
        else
        {
            lower = Scalar(0, 0, 200);
            upper = Scalar(180, 30, 255);
        }

        inRange(hsv, lower, upper, mask);
        threshold(mask, result, 0, 255, THRESH_BINARY | THRESH_OTSU);
        return result;
    }

    // Extract drive gauge blocks (0-6) and burnout
    void extract_drive_gauge(const Mat &frame, const Rect &roi, uint8_t &blocks, uint8_t &burnout)
    {
        Mat gauge_roi = frame(roi);
        Mat hsv;
        cvtColor(gauge_roi, hsv, COLOR_BGR2HSV);

        blocks = 0;
        burnout = 0;
        int block_w = roi.width / 6;

        for (int i = 0; i < 6; i++)
        {
            Rect block_r(i * block_w, 0, block_w, roi.height);
            Mat block = gauge_roi(block_r);
            Mat block_hsv = hsv(block_r);

            Scalar mean_bgr = mean(block);
            Scalar mean_hsv = mean(block_hsv);

            if (mean_bgr[0] > 50)
                blocks++;
            if (mean_hsv[0] < 30 && mean_hsv[1] > 100)
                burnout = 1;
        }
    }

    // Extract system notification type
    uint8_t extract_notification(const Mat &frame, const Rect &roi, float &confidence)
    {
        Mat notif_roi = frame(roi);
        Scalar mean_bgr = mean(notif_roi);
        if (mean_bgr[0] < 30)
            return 255; // No notification

        Mat hsv;
        cvtColor(notif_roi, hsv, COLOR_BGR2HSV);
        Scalar mean_hsv = mean(hsv);

        double hue = mean_hsv[0];
        double sat = mean_hsv[1];
        confidence = 0.8f;

        if (hue < 15 && sat > 150)
            return 0; // Counter (red)
        if (hue >= 15 && hue < 35 && sat > 150)
            return 1; // PunishCounter (orange)
        if (hue >= 100 && hue < 140 && sat > 100)
            return 2; // Reversal (blue)
        return 0;     // Default Counter
    }

    // Extract frame meter data (placeholder - needs OCR)
    void extract_frame_meter(const Mat &frame, const Rect &roi,
                             int &startup, int &active, int &recovery,
                             int &hit_adv, int &block_adv, uint8_t &has_data)
    {
        Mat meter_roi = frame(roi);
        Scalar mean_bgr = mean(meter_roi);
        if (mean_bgr[0] < 20)
        {
            has_data = 0;
            return;
        }

        // TODO: Implement OCR for frame meter numbers
        // For now, return placeholder
        has_data = 1;
        startup = active = recovery = hit_adv = block_adv = 0;
    }

    // Extract key history (placeholder - needs template matching/ML)
    int extract_key_history(const Mat &frame, const Rect &roi, uint8_t *out_inputs, int max_inputs)
    {
        // TODO: Implement icon detection for directional arrows and buttons
        // For now, return empty
        return 0;
    }
};

// Global error buffer
static thread_local string g_last_error;

extern "C"
{

    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config)
    {
        if (!video_path)
            return nullptr;

        auto *impl = new ScannerImpl();
        if (!impl->open_video(video_path, config))
        {
            g_last_error = impl->last_error;
            delete impl;
            return nullptr;
        }

        impl->normalized_first = impl->normalize_frame(impl->first_frame);
        if (!impl->detect_hud())
        {
            g_last_error = impl->last_error;
            delete impl;
            return nullptr;
        }

        // Reset to beginning for scanning
        impl->cap.set(CAP_PROP_POS_FRAMES, 0);
        impl->current_frame = 0;

        return impl;
    }

    SCANNER_API int scanner_next_frame(ScannerHandle handle, FrameData *out_frame)
    {
        if (!handle || !out_frame)
            return -1;

        auto *impl = static_cast<ScannerImpl *>(handle);
        if (impl->config.max_frames > 0 && impl->current_frame >= impl->config.max_frames)
        {
            return 0; // End
        }

        Mat frame;
        if (!impl->cap.read(frame) || frame.empty())
        {
            return 0; // End of video
        }

        // Skip frames based on stride
        while (impl->current_frame % impl->config.frame_stride != 0)
        {
            impl->current_frame++;
            if (!impl->cap.read(frame) || frame.empty())
                return 0;
        }

        // Periodic HUD re-detection
        if (impl->current_frame > 0 && impl->current_frame % impl->config.hud_redetect_interval == 0)
        {
            Mat normalized = impl->normalize_frame(frame);
            impl->detect_hud(); // Update ROIs
        }

        Mat normalized = impl->normalize_frame(frame);

        // Fill output frame data
        memset(out_frame, 0, sizeof(FrameData));
        out_frame->frame_index = impl->current_frame;
        out_frame->timestamp_ms = (impl->current_frame / impl->fps) * 1000.0;

        // Extract drive gauges
        impl->extract_drive_gauge(normalized, impl->p1_drive_roi,
                                  out_frame->p1_drive_blocks, out_frame->p1_drive_burnout);
        impl->extract_drive_gauge(normalized, impl->p2_drive_roi,
                                  out_frame->p2_drive_blocks, out_frame->p2_drive_burnout);

        // System notification
        out_frame->notification_confidence = 0;
        out_frame->has_notification = 0;
        out_frame->notification_type = impl->extract_notification(normalized, impl->sys_notif_roi, out_frame->notification_confidence);
        if (out_frame->notification_type != 255)
        {
            out_frame->has_notification = 1;
        }

        // Frame meter
        impl->extract_frame_meter(normalized, impl->frame_meter_roi,
                                  out_frame->startup, out_frame->active, out_frame->recovery,
                                  out_frame->hit_advantage, out_frame->block_advantage,
                                  out_frame->has_frame_meter);

        // Key history (placeholder)
        out_frame->p1_input_count = impl->extract_key_history(normalized, impl->p1_key_roi, out_frame->p1_inputs, 64);
        out_frame->p2_input_count = impl->extract_key_history(normalized, impl->p2_key_roi, out_frame->p2_inputs, 64);

        impl->current_frame++;
        return 1;
    }

    SCANNER_API int scanner_get_total_frames(ScannerHandle handle)
    {
        if (!handle)
            return 0;
        auto *impl = static_cast<ScannerImpl *>(handle);
        return impl->total_frames;
    }

    SCANNER_API double scanner_get_fps(ScannerHandle handle)
    {
        if (!handle)
            return 0;
        auto *impl = static_cast<ScannerImpl *>(handle);
        return impl->fps;
    }

    SCANNER_API int scanner_get_width(ScannerHandle handle)
    {
        if (!handle)
            return 0;
        auto *impl = static_cast<ScannerImpl *>(handle);
        return impl->first_frame.cols;
    }

    SCANNER_API int scanner_get_height(ScannerHandle handle)
    {
        if (!handle)
            return 0;
        auto *impl = static_cast<ScannerImpl *>(handle);
        return impl->first_frame.rows;
    }

    SCANNER_API void scanner_destroy(ScannerHandle handle)
    {
        if (handle)
        {
            auto *impl = static_cast<ScannerImpl *>(handle);
            delete impl;
        }
    }

    SCANNER_API const char *scanner_get_last_error()
    {
        return g_last_error.c_str();
    }
}
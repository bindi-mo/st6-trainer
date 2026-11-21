#include "scanner_api.h"
#include "calibration.h"
#include "raster_normalizer.h"
#include "input_tracker.h"
#include "gauge.h"
#include "frame_meter.h"
#include <opencv2/core.hpp>
#include <opencv2/videoio.hpp>
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <string>
#include <cstring>
#include <ctime>
#include <cmath>
#include <iostream>
#include <sstream>
#include <memory>
#include <algorithm>
#include <filesystem>

using namespace cv;
using namespace std;
using namespace st6;

// Thread-local error buffer
static thread_local string g_last_error;

// Simple JSON parser for scanner_config.json
struct ScannerConfig
{
    int canonical_width = 1920;
    int canonical_height = 1080;
    double fps = 60.0;
    int rows_visible = 19;
    int fm_latch_frames = 3;
    int gauge_occluded_threshold = 10;

    // Calibration
    CalibrationConfig calib_config;

    // Template thresholds
    double threshold_display = 0.85;
    double threshold_calibration = 0.90;
    double threshold_frame_meter = 0.90;
    double threshold_input_glyph = 0.92;
    double threshold_attack_property = 0.92;

    // Round detection thresholds
    double hp_reset_low_threshold = 0.3;   // HP がこの値未満なら「ラウンド終了」とみなす（補完用）
    double hp_reset_high_threshold = 0.7;  // HP がこの値を超えたら「ラウンド開始」とみなす（補完用）
    int round_timer_gap_frames = 180;      // ラウンド間の最小フレーム間隔（3秒 @60fps、タイマーベース補完用）
    int round_duration_frames = 5940;      // 1ラウンドの想定フレーム数（99秒 @60fps）

    // Metadata
    string config_version = "cfg-1.0";
    string template_pack_version = "sf6-ui-2026.11";
    string extractor_version = "0.1.0";
};

// Character name template entry
struct CharNameTemplate
{
    string name;
    Mat template_img;
};

// Internal scanner state
struct ScannerImpl
{
    VideoCapture cap;
    double fps;
    int total_frames;
    int current_frame;

    string template_pack_path;
    ScannerConfig config;

    // Calibration
    unique_ptr<Calibration> calibration;
    bool calibration_done;
    int top_y_offset_px;
    int bottom_y_offset_px;
    double upper_confidence;
    double lower_confidence;

    // Raster normalizer
    unique_ptr<RasterNormalizer> normalizer;

    // Input tracker (Phase 3)
    unique_ptr<InputTracker> input_tracker;
    bool input_tracker_initialized;
    int input_frame_counter; // 入力イベント用フレームカウンタ

    // Gauge detector (Phase 4)
    unique_ptr<GaugeDetector> gauge_detector;
    bool gauge_detector_initialized;

    // Frame meter detector (Phase 4)
    unique_ptr<FrameMeterDetector> frame_meter_detector;
    bool frame_meter_detector_initialized;

    // Metadata
    bool metadata_emitted;
    MatchMetadata metadata;

    // Frame buffer for calibration
    Mat first_frame;
    bool has_first_frame;

    // Character name templates (Phase 5)
    vector<CharNameTemplate> char_templates;
    bool char_templates_loaded;

    // Round logo templates (Phase 5)
    vector<CharNameTemplate> round_logo_templates;
    bool round_logo_templates_loaded;

    // Round detection state (Phase 5)
    double prev_hp_p1;
    double prev_hp_p2;
    int round_index;
    bool round_initialized;
    int last_reset_frame;   // 前回のラウンドリセット検出フレーム（タイマーベース補完用）

    ScannerImpl() : fps(0), total_frames(0), current_frame(0),
                    calibration_done(false), top_y_offset_px(0), bottom_y_offset_px(0),
                    upper_confidence(0.0), lower_confidence(0.0),
                    metadata_emitted(false), has_first_frame(false),
                    input_tracker_initialized(false), input_frame_counter(0),
                    gauge_detector_initialized(false),
                    frame_meter_detector_initialized(false),
                    char_templates_loaded(false),
                    prev_hp_p1(1.0), prev_hp_p2(1.0),
                    round_index(1),
                    round_initialized(false),
                    last_reset_frame(0) {}

    ~ScannerImpl()
    {
        if (cap.isOpened())
        {
            cap.release();
        }
    }

    bool open_video(const char *path, const ScanConfig *cfg)
    {
        cap.open(path);
        if (!cap.isOpened())
        {
            last_error = "Failed to open video: " + string(path);
            return false;
        }

        fps = cap.get(CAP_PROP_FPS);
        if (fps <= 0)
            fps = 60.0;
        total_frames = (int)cap.get(CAP_PROP_FRAME_COUNT);
        current_frame = 0;

        // Store template pack path from config
        if (cfg && cfg->template_pack_path)
        {
            template_pack_path = cfg->template_pack_path;
        }

        // Initialize normalizer
        normalizer = make_unique<RasterNormalizer>(config.canonical_width, config.canonical_height);

        return true;
    }

    // Parse simple JSON config (extract numeric values)
    void parse_config_json(const char *json_str)
    {
        if (!json_str)
            return;

        string json(json_str);

        // Simple extraction of key values (not full JSON parser)
        // In production, use a proper JSON library like nlohmann/json

        // Extract calibration parameters
        auto extract_double = [&](const string &key, double &out)
        {
            size_t pos = json.find("\"" + key + "\"");
            if (pos != string::npos)
            {
                pos = json.find(":", pos);
                if (pos != string::npos)
                {
                    pos++;
                    while (pos < json.length() && isspace(json[pos]))
                        pos++;
                    size_t end = json.find_first_of(",}", pos);
                    if (end != string::npos)
                    {
                        try
                        {
                            out = stod(json.substr(pos, end - pos));
                        }
                        catch (...)
                        {
                            // Ignore parse errors
                        }
                    }
                }
            }
        };

        auto extract_int = [&](const string &key, int &out)
        {
            size_t pos = json.find("\"" + key + "\"");
            if (pos != string::npos)
            {
                pos = json.find(":", pos);
                if (pos != string::npos)
                {
                    pos++;
                    while (pos < json.length() && isspace(json[pos]))
                        pos++;
                    size_t end = json.find_first_of(",}", pos);
                    if (end != string::npos)
                    {
                        try
                        {
                            out = stoi(json.substr(pos, end - pos));
                        }
                        catch (...)
                        {
                            // Ignore parse errors
                        }
                    }
                }
            }
        };

        // Extract values (with safe defaults)
        extract_double("fps", config.fps);
        extract_int("rows_visible", config.rows_visible);
        extract_int("fm_latch_frames", config.fm_latch_frames);

        // Calibration config
        extract_double("confidence_threshold", config.calib_config.confidence_threshold);
        extract_double("prominence_threshold", config.calib_config.prominence_threshold);
        extract_int("max_dx_px", config.calib_config.max_dx_px);
        extract_double("consensus_survival_min", config.calib_config.consensus_survival_min);
        extract_int("consensus_vote_window_px", config.calib_config.consensus_vote_window_px);
        extract_int("search_x_tolerance_px", config.calib_config.search_x_tolerance_px);

        // Round detection config
        extract_double("hp_reset_low_threshold", config.hp_reset_low_threshold);
        extract_double("hp_reset_high_threshold", config.hp_reset_high_threshold);
        extract_int("round_timer_gap_frames", config.round_timer_gap_frames);
        extract_int("round_duration_frames", config.round_duration_frames);

        cout << "[Scanner] Config parsed: " << config.canonical_width << "x" << config.canonical_height
             << " fps=" << config.fps << endl;
    }

    // Load character name templates from names/ directory (Phase 5)
    void load_char_name_templates()
    {
        if (char_templates_loaded)
            return;

        namespace fs = std::filesystem;
        fs::path names_dir = fs::path(template_pack_path) / "names";
        if (!fs::exists(names_dir))
        {
            cout << "[Scanner] Character names directory not found: " << names_dir << endl;
            char_templates_loaded = true;
            return;
        }

        for (const auto &entry : fs::directory_iterator(names_dir))
        {
            if (entry.path().extension() == ".png")
            {
                Mat tmpl = imread(entry.path().string(), IMREAD_COLOR);
                if (!tmpl.empty())
                {
                    CharNameTemplate ctmpl;
                    ctmpl.name = entry.path().stem().string();
                    ctmpl.template_img = tmpl;
                    char_templates.push_back(move(ctmpl));
                    cout << "[Scanner] Loaded char template: " << ctmpl.name << endl;
                }
            }
        }

        char_templates_loaded = true;
        cout << "[Scanner] Loaded " << char_templates.size() << " character name templates" << endl;
    }

    // Load round-related logo templates from templates/round_logos/ directory (Phase 5)
    // 対象: round_1.png, round_2.png, final_round.png, ko.png, time_over.png など
    void load_round_logo_templates()
    {
        if (round_logo_templates_loaded)
            return;

        namespace fs = std::filesystem;
        fs::path logos_dir = fs::path(template_pack_path) / "round_logos";
        if (!fs::exists(logos_dir))
        {
            cout << "[Scanner] Round logos directory not found: " << logos_dir << endl;
            round_logo_templates_loaded = true;
            return;
        }

        for (const auto &entry : fs::directory_iterator(logos_dir))
        {
            if (entry.path().extension() == ".png")
            {
                Mat tmpl = imread(entry.path().string(), IMREAD_COLOR);
                if (!tmpl.empty())
                {
                    CharNameTemplate ctmpl;
                    ctmpl.name = entry.path().stem().string();
                    ctmpl.template_img = tmpl;
                    round_logo_templates.push_back(move(ctmpl));
                    cout << "[Scanner] Loaded round logo template: " << ctmpl.name << endl;
                }
            }
        }

        round_logo_templates_loaded = true;
        cout << "[Scanner] Loaded " << round_logo_templates.size() << " round logo templates" << endl;
    }

    // Match round logo against templates (Phase 5)
    // Returns matched logo name (e.g. "round_1", "round_2", "final_round") or empty string
    string match_round_logo(const Mat &frame, int x_start, int x_end, int y_start, int y_end)
    {
        if (round_logo_templates.empty())
            return "";

        Mat roi = frame.rowRange(y_start, y_end).colRange(x_start, x_end);
        if (roi.empty())
            return "";

        Mat gray_roi;
        if (roi.channels() == 3)
            cvtColor(roi, gray_roi, COLOR_BGR2GRAY);
        else
            gray_roi = roi.clone();

        double best_conf = 0.0;
        string best_name;

        for (const auto &tmpl : round_logo_templates)
        {
            Mat gray_tmpl;
            if (tmpl.template_img.channels() == 3)
                cvtColor(tmpl.template_img, gray_tmpl, COLOR_BGR2GRAY);
            else
                gray_tmpl = tmpl.template_img.clone();

            // テンプレートが ROI より大きい場合はスキップ
            if (gray_tmpl.cols > gray_roi.cols || gray_tmpl.rows > gray_roi.rows)
                continue;

            Mat result;
            matchTemplate(gray_roi, gray_tmpl, result, TM_CCOEFF_NORMED);
            double min_val, max_val;
            Point min_loc, max_loc;
            minMaxLoc(result, &min_val, &max_val, &min_loc, &max_loc);

            if (max_val > best_conf)
            {
                best_conf = max_val;
                best_name = tmpl.name;
            }
        }

        // 信頼度閾値（0.80 以上でマッチとみなす）
        if (best_conf > 0.80)
            return best_name;

        return "";
    }

    // Match character name against templates (Phase 5)
    string match_character_name(const Mat &frame, int x_start, int x_end, int y_start, int y_end)
    {
        if (char_templates.empty())
            return "";

        Mat roi = frame.rowRange(y_start, y_end).colRange(x_start, x_end);
        if (roi.empty())
            return "";

        Mat gray_roi;
        if (roi.channels() == 3)
            cvtColor(roi, gray_roi, COLOR_BGR2GRAY);
        else
            gray_roi = roi.clone();

        string best_name;
        double best_conf = 0.0;

        for (const auto &ctmpl : char_templates)
        {
            Mat gray_tmpl;
            if (ctmpl.template_img.channels() == 3)
                cvtColor(ctmpl.template_img, gray_tmpl, COLOR_BGR2GRAY);
            else
                gray_tmpl = ctmpl.template_img.clone();

            // Resize template to match ROI height
            double scale = static_cast<double>(gray_roi.rows) / static_cast<double>(gray_tmpl.rows);
            Mat resized_tmpl;
            resize(gray_tmpl, resized_tmpl, Size(), scale, scale, INTER_AREA);

            if (resized_tmpl.cols > gray_roi.cols || resized_tmpl.rows > gray_roi.rows)
                continue;

            Mat result;
            matchTemplate(gray_roi, resized_tmpl, result, TM_CCOEFF_NORMED);
            double min_val, max_val;
            Point min_loc, max_loc;
            minMaxLoc(result, &min_val, &max_val, &min_loc, &max_loc);

            if (max_val > best_conf && max_val > 0.80)
            {
                best_conf = max_val;
                best_name = ctmpl.name;
            }
        }

        return best_name;
    }

    // Detect Y offsets from real gauge positions (Phase 5)
    // Scans the first valid frame to find the actual Y positions of HP and SA gauges
    void detect_y_offsets(const Mat &frame)
    {
        Mat hsv;
        if (frame.channels() == 3)
            cvtColor(frame, hsv, COLOR_BGR2HSV);
        else
            return;

        // Scan upper band for HP gauge (green/yellow)
        Mat upper_roi = hsv.rowRange(0, static_cast<int>(hsv.rows * 0.25));
        Mat hp_mask;
        inRange(upper_roi, Scalar(40, 50, 50), Scalar(90, 255, 255), hp_mask);

        // Find rows with significant HP color
        int hp_top = -1, hp_bottom = -1;
        for (int y = 0; y < hp_mask.rows; y++)
        {
            double row_mean = mean(hp_mask.row(y))[0] / 255.0;
            if (row_mean > 0.01)
            {
                if (hp_top == -1)
                    hp_top = y;
                hp_bottom = y;
            }
        }

        // Scan lower band for SA gauge (blue)
        int lower_start = static_cast<int>(hsv.rows * 0.70);
        Mat lower_roi = hsv.rowRange(lower_start, hsv.rows);
        Mat sa_mask;
        inRange(lower_roi, Scalar(100, 50, 50), Scalar(130, 255, 255), sa_mask);

        int sa_top = -1, sa_bottom = -1;
        for (int y = 0; y < sa_mask.rows; y++)
        {
            double row_mean = mean(sa_mask.row(y))[0] / 255.0;
            if (row_mean > 0.01)
            {
                if (sa_top == -1)
                    sa_top = y;
                sa_bottom = y;
            }
        }

        // Set Y offsets based on detected gauge positions
        if (hp_top != -1)
        {
            top_y_offset_px = hp_top;
            upper_confidence = 0.9;
        }
        else
        {
            top_y_offset_px = 0;
            upper_confidence = 0.5;
        }

        if (sa_top != -1)
        {
            bottom_y_offset_px = lower_start + sa_top;
            lower_confidence = 0.9;
        }
        else
        {
            bottom_y_offset_px = lower_start;
            lower_confidence = 0.5;
        }

        cout << "[Scanner] Y offset detection: top_y=" << top_y_offset_px
             << " bottom_y=" << bottom_y_offset_px
             << " (upper_conf=" << upper_confidence << ", lower_conf=" << lower_confidence << ")" << endl;
    }

    // Perform calibration on first frame
    bool perform_calibration()
    {
        if (calibration_done)
        {
            return true;
        }

        if (!has_first_frame)
        {
            // Skip to actual gameplay (frame 918+ based on user input)
            // This ensures HUD is fully visible and stable
            Mat frame;
            int target_frame = 918; // Start of actual gameplay
            int max_attempts = 300; // Try up to 300 frames after target

            cap.set(CAP_PROP_POS_FRAMES, target_frame);

            bool found_content = false;
            for (int i = 0; i < max_attempts; i++)
            {
                if (!cap.read(frame) || frame.empty())
                {
                    g_last_error = "Failed to read frame for calibration";
                    return false;
                }

                // Check if frame has content (not black)
                cv::Mat gray;
                if (frame.channels() == 3)
                {
                    cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
                }
                else
                {
                    gray = frame.clone();
                }

                // Check if upper HUD region (Y=0-270) has content
                cv::Mat upper_region = gray.rowRange(0, std::min(270, gray.rows));
                double upper_min, upper_max;
                cv::minMaxLoc(upper_region, &upper_min, &upper_max);

                // Check if lower HUD region (Y=756-1080) has content
                cv::Mat lower_region = gray.rowRange(std::max(0, gray.rows - 324), gray.rows);
                double lower_min, lower_max;
                cv::minMaxLoc(lower_region, &lower_min, &lower_max);

                if (upper_max > 100.0 && lower_max > 100.0)
                { // Found a frame with full HUD
                    cout << "[Scanner] Found gameplay frame at index " << (target_frame + i)
                         << " (upper_max=" << upper_max << ", lower_max=" << lower_max << ")" << endl;
                    found_content = true;
                    break;
                }

                if (i < 5 || (i + 1) % 50 == 0)
                {
                    cout << "[Scanner] Skipping frame at index " << (target_frame + i)
                         << " (upper_max=" << upper_max << ", lower_max=" << lower_max << ")" << endl;
                }
            }

            if (!found_content)
            {
                g_last_error = "Could not find a frame with full HUD content after " + to_string(max_attempts) + " attempts";
                cout << "[Scanner] Error: " << g_last_error << endl;
                return false;
            }

            first_frame = frame.clone();
            has_first_frame = true;

            // Reset to beginning for actual scanning
            cap.set(CAP_PROP_POS_FRAMES, 0);
            current_frame = 0;
        }

        cout << "[Scanner] Starting simplified calibration..." << endl;

        // Simplified calibration: detect HUD presence and estimate Y offsets
        if (first_frame.empty())
        {
            g_last_error = "First frame is empty";
            return false;
        }

        // Convert to HSV for color detection
        cv::Mat hsv;
        if (first_frame.channels() == 3)
        {
            cv::cvtColor(first_frame, hsv, cv::COLOR_BGR2HSV);
        }
        else
        {
            g_last_error = "First frame is not color";
            return false;
        }

        // Detect HP gauge (typically green/yellow in upper portion)
        cv::Mat hp_region = hsv.rowRange(0, static_cast<int>(hsv.rows * 0.25));
        cv::Mat hp_mask;
        cv::inRange(hp_region, cv::Scalar(40, 50, 50), cv::Scalar(180, 255, 255), hp_mask);
        double hp_mean = cv::mean(hp_mask)[0] / 255.0;

        // Detect SA gauge (typically blue in lower portion)
        cv::Mat sa_region = hsv.rowRange(static_cast<int>(hsv.rows * 0.70), hsv.rows);
        cv::Mat sa_mask;
        cv::inRange(sa_region, cv::Scalar(100, 50, 50), cv::Scalar(130, 255, 255), sa_mask);
        double sa_mean = cv::mean(sa_mask)[0] / 255.0;

        cout << "[Scanner] HUD detection: hp_region_mean=" << hp_mean
             << " sa_region_mean=" << sa_mean << endl;

        // Check if HUD is present
        if (hp_mean < 0.01 && sa_mean < 0.01)
        {
            g_last_error = "E_HUD_ABSENT: No HUD detected (hp_mean=" + to_string(hp_mean) + ", sa_mean=" + to_string(sa_mean) + ")";
            cout << "[Scanner] Error: " << g_last_error << endl;
            return false;
        }

        // Phase 5: Detect Y offsets from real gauge positions
        detect_y_offsets(first_frame);

        // Phase 5: Load character name templates
        load_char_name_templates();

        // Phase 5: Load round logo templates
        load_round_logo_templates();

        calibration_done = true;
        cout << "[Scanner] Simplified calibration complete: top_y=" << top_y_offset_px
             << " bottom_y=" << bottom_y_offset_px
             << " (hp_mean=" << hp_mean << ", sa_mean=" << sa_mean << ")" << endl;

        return true;
    }

    string last_error;
};

// ===== C API Implementation =====

extern "C"
{

    SCANNER_API ScannerVersion scanner_api_version()
    {
        return {0, 5, 0}; // Phase 5
    }

    SCANNER_API const char *scanner_get_last_error()
    {
        return g_last_error.c_str();
    }

    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config)
    {
        try
        {
            if (!video_path)
            {
                g_last_error = "video_path is null";
                return nullptr;
            }

            auto *impl = new ScannerImpl();

            // Parse config JSON if provided
            if (config && config->config_json)
            {
                impl->parse_config_json(config->config_json);
            }

            if (!impl->open_video(video_path, config))
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
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_create: ") + e.what();
            return nullptr;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_create";
            return nullptr;
        }
    }

    SCANNER_API int scanner_get_metadata(ScannerHandle handle, MatchMetadata *out)
    {
        try
        {
            if (!handle || !out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // Perform calibration if not done yet
            if (!impl->calibration_done)
            {
                if (!impl->perform_calibration())
                {
                    return 0; // Failed
                }
            }

            // Fill metadata
            memset(out, 0, sizeof(MatchMetadata));

            // Generate match_id (simplified - should use SHA256 per §7.1)
            snprintf(out->match_id, sizeof(out->match_id), "match-%d-%d",
                     impl->top_y_offset_px, impl->bottom_y_offset_px);

            // Character names (Phase 5: detected from names/ templates)
            // Character name regions: upper HUD, left side for P1, right side for P2
            int char_name_y_start = static_cast<int>(impl->config.canonical_height * 0.15);
            int char_name_y_end = static_cast<int>(impl->config.canonical_height * 0.22);
            int char_name_x_mid = impl->config.canonical_width / 2;

            string char_p1 = impl->match_character_name(impl->first_frame, 0, char_name_x_mid, char_name_y_start, char_name_y_end);
            string char_p2 = impl->match_character_name(impl->first_frame, char_name_x_mid, impl->config.canonical_width, char_name_y_start, char_name_y_end);

            snprintf(out->character_name_p1, sizeof(out->character_name_p1), "%s", char_p1.c_str());
            snprintf(out->character_name_p2, sizeof(out->character_name_p2), "%s", char_p2.c_str());

            // Control types will be resolved after input tracker is initialized below
            snprintf(out->control_type_p1, sizeof(out->control_type_p1), "");
            snprintf(out->control_type_p2, sizeof(out->control_type_p2), "");

            out->detected_at_frame = 1;

            // Source resolution
            snprintf(out->source_resolution, sizeof(out->source_resolution), "%dx%d",
                     (int)impl->cap.get(CAP_PROP_FRAME_WIDTH),
                     (int)impl->cap.get(CAP_PROP_FRAME_HEIGHT));

            out->source_fps = impl->fps;
            out->canonical_raster[0] = impl->config.canonical_width;
            out->canonical_raster[1] = impl->config.canonical_height;

            // Calibration results
            out->top_y_offset_px = impl->top_y_offset_px;
            out->bottom_y_offset_px = impl->bottom_y_offset_px;
            out->offset_snapped = 0;   // Always 0 per §2.2
            out->manual_verified = -1; // null

            // Input history (Phase 3)
            out->input_history_available = 1;

            // Initialize input tracker
            if (!impl->input_tracker_initialized)
            {
                impl->input_tracker = make_unique<InputTracker>();
                impl->input_tracker->set_template_pack_path(impl->template_pack_path);
                impl->input_tracker->rows_visible = impl->config.rows_visible;
                impl->input_tracker->input_glyph_threshold = impl->config.threshold_input_glyph;
                impl->input_tracker_initialized = true;
                cout << "[Scanner] Input tracker initialized" << endl;
            }

            // Resolve control types from classic.png/modern.png classifier templates (Phase 6)
            // Must be after input_tracker is initialized
            auto ctrl_p1 = impl->input_tracker->resolve_control_type(impl->first_frame, impl->config.threshold_input_glyph);
            auto ctrl_p2 = impl->input_tracker->resolve_control_type(impl->first_frame, impl->config.threshold_input_glyph);
            impl->input_tracker->set_control_type("P1", ctrl_p1.control_type);
            impl->input_tracker->set_control_type("P2", ctrl_p2.control_type);

            snprintf(out->control_type_p1, sizeof(out->control_type_p1), "%s", ctrl_p1.control_type.c_str());
            snprintf(out->control_type_p2, sizeof(out->control_type_p2), "%s", ctrl_p2.control_type.c_str());

            cout << "[Scanner] Control type P1: " << ctrl_p1.control_type
                 << " (conf=" << ctrl_p1.confidence << ")" << endl;
            cout << "[Scanner] Control type P2: " << ctrl_p2.control_type
                 << " (conf=" << ctrl_p2.confidence << ")" << endl;

            // Initialize gauge detector (Phase 4)
            if (!impl->gauge_detector_initialized)
            {
                impl->gauge_detector = make_unique<GaugeDetector>();
                GaugeConfig gc;
                gc.hp_hsv_min = Scalar(40, 50, 50);
                gc.hp_hsv_max = Scalar(90, 255, 255);
                gc.drive_hsv_min = Scalar(15, 100, 100);
                gc.drive_hsv_max = Scalar(35, 255, 255);
                gc.sa_hsv_min = Scalar(100, 50, 50);
                gc.sa_hsv_max = Scalar(130, 255, 255);
                gc.occluded_threshold = impl->config.gauge_occluded_threshold;
                gc.min_confidence = 0.5;
                impl->gauge_detector->set_config(gc);
                impl->gauge_detector_initialized = true;
                cout << "[Scanner] Gauge detector initialized" << endl;
            }

            // Initialize frame meter detector (Phase 4)
            if (!impl->frame_meter_detector_initialized)
            {
                impl->frame_meter_detector = make_unique<FrameMeterDetector>();
                FrameMeterConfig fmc;
                fmc.latch_frames = impl->config.fm_latch_frames;
                fmc.threshold = impl->config.threshold_frame_meter;
                fmc.roi_y_start = static_cast<int>(impl->config.canonical_height * 0.70);
                fmc.roi_y_end = impl->config.canonical_height;
                impl->frame_meter_detector->set_config(fmc);
                impl->frame_meter_detector->set_template_pack_path(impl->template_pack_path);
                impl->frame_meter_detector_initialized = true;
                cout << "[Scanner] Frame meter detector initialized" << endl;
            }

            // Attack property locale (Phase 4)
            snprintf(out->attack_property_locale, sizeof(out->attack_property_locale), "en");

            // Timestamp
            time_t now = time(nullptr);
            strftime(out->extracted_at_utc, sizeof(out->extracted_at_utc), "%Y-%m-%dT%H:%M:%SZ", gmtime(&now));

            // Versions
            snprintf(out->config_version, sizeof(out->config_version), "%s", impl->config.config_version.c_str());
            snprintf(out->template_pack_version, sizeof(out->template_pack_version), "%s", impl->config.template_pack_version.c_str());
            snprintf(out->extractor_version, sizeof(out->extractor_version), "%s", impl->config.extractor_version.c_str());

            impl->metadata_emitted = true;
            cout << "[Scanner] Metadata emitted: match_id=" << out->match_id
                 << " top_y=" << out->top_y_offset_px
                 << " bottom_y=" << out->bottom_y_offset_px << endl;

            return 1;
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_get_metadata: ") + e.what();
            return -1;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_get_metadata";
            return -1;
        }
    }

    SCANNER_API int scanner_set_manual_offsets(ScannerHandle handle, int top_y_offset_px, int bottom_y_offset_px, int manual_verified)
    {
        try
        {
            if (!handle)
            {
                g_last_error = "Invalid handle";
                return 0;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // 手動オフセットを適用
            impl->top_y_offset_px = top_y_offset_px;
            impl->bottom_y_offset_px = bottom_y_offset_px;

            // メタデータが既に出力済みの場合は manual_verified フラグを更新
            if (impl->metadata_emitted)
            {
                impl->metadata.manual_verified = manual_verified;
            }

            cout << "[Scanner] Manual offsets set: top_y=" << top_y_offset_px
                 << " bottom_y=" << bottom_y_offset_px
                 << " manual_verified=" << manual_verified << endl;

            return 1;
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_set_manual_offsets: ") + e.what();
            return 0;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_set_manual_offsets";
            return 0;
        }
    }

    SCANNER_API int scanner_next_telemetry(ScannerHandle handle, FrameTelemetry *out)
    {
        try
        {
            if (!handle || !out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // Read frame
            Mat frame;
            if (!impl->cap.read(frame) || frame.empty())
            {
                return 0; // End of video
            }

            impl->current_frame++;

            // Skip black frames at the beginning (intro/loading)
            // Check if frame is black
            cv::Mat gray;
            if (frame.channels() == 3)
            {
                cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
            }
            else
            {
                gray = frame.clone();
            }

            double min_val, max_val;
            cv::minMaxLoc(gray, &min_val, &max_val);

            if (max_val < 10.0)
            {
                // Skip this black frame
                return 1; // Continue to next frame
            }

            // Normalize frame
            NormalizationResult norm_result = impl->normalizer->normalize(frame);
            if (!norm_result.success)
            {
                g_last_error = norm_result.error_code + ": " + norm_result.error_message;
                cout << "[Scanner] Normalization failed: " << g_last_error << endl;
                return -1;
            }

            // Fill telemetry with real gauge detection (Phase 4)
            memset(out, 0, sizeof(FrameTelemetry));
            out->frame_index = impl->current_frame;
            out->source_frame_index = impl->current_frame;
            out->timestamp_ms = (int64_t)((impl->current_frame / impl->fps) * 1000.0);
            out->round_index = impl->round_index;
            out->hp_reset = 0;

            // Gauge detection (Phase 4)
            double hp_p1 = 1.0, hp_p2 = 1.0;
            if (impl->gauge_detector && impl->gauge_detector_initialized)
            {
                int upper_y_start = 0;
                int upper_y_end = static_cast<int>(impl->config.canonical_height * 0.25);
                int lower_y_start = static_cast<int>(impl->config.canonical_height * 0.70);
                int lower_y_end = static_cast<int>(impl->config.canonical_height);

                FrameGauges gauges = impl->gauge_detector->detect(norm_result.normalized_frame, upper_y_start, upper_y_end, lower_y_start, lower_y_end);

                hp_p1 = gauges.hp_p1.value;
                out->drive_p1 = gauges.drive_p1.value * 6.0; // Drive ゲージは 0-6 段階
                out->sa_p1 = static_cast<int>(std::round(gauges.sa_p1.value * 3.0)); // SA は 0-3 段階
                if (out->sa_p1 < 0) out->sa_p1 = 0; // §7.5 項 7: クランプ
                if (out->sa_p1 > 3) out->sa_p1 = 3;
                out->in_burnout_p1 = gauges.in_burnout_p1 ? 1 : 0;

                hp_p2 = gauges.hp_p2.value;
                out->drive_p2 = gauges.drive_p2.value * 6.0;
                out->sa_p2 = static_cast<int>(std::round(gauges.sa_p2.value * 3.0));
                if (out->sa_p2 < 0) out->sa_p2 = 0; // §7.5 項 7: クランプ
                if (out->sa_p2 > 3) out->sa_p2 = 3;
                out->in_burnout_p2 = gauges.in_burnout_p2 ? 1 : 0;

                // Quality flags for occluded gauges
                if (gauges.hp_p1.occluded)
                {
                    snprintf(out->quality_flags[out->quality_flags_count], sizeof(out->quality_flags[0]), "HP_P1_OCCLUDED");
                    out->quality_flags_count++;
                }
                if (gauges.hp_p2.occluded)
                {
                    snprintf(out->quality_flags[out->quality_flags_count], sizeof(out->quality_flags[0]), "HP_P2_OCCLUDED");
                    out->quality_flags_count++;
                }
                if (gauges.sa_p1.occluded)
                {
                    snprintf(out->quality_flags[out->quality_flags_count], sizeof(out->quality_flags[0]), "SA_P1_OCCLUDED");
                    out->quality_flags_count++;
                }
                if (gauges.sa_p2.occluded)
                {
                    snprintf(out->quality_flags[out->quality_flags_count], sizeof(out->quality_flags[0]), "SA_P2_OCCLUDED");
                    out->quality_flags_count++;
                }
            }
            else
            {
                // Fallback stub values if gauge detector not ready
                hp_p1 = 1.0;
                out->drive_p1 = 6.0;
                out->sa_p1 = 3;
                out->in_burnout_p1 = 0;
                hp_p2 = 1.0;
                out->drive_p2 = 6.0;
                out->sa_p2 = 3;
                out->in_burnout_p2 = 0;
            }

            out->hp_p1 = hp_p1;
            out->hp_p2 = hp_p2;

            // Phase 5: Round detection via timer (99秒カウントダウン) + HP reset 補完 + ラウンドロゴ検出
            if (!impl->round_initialized)
            {
                impl->prev_hp_p1 = hp_p1;
                impl->prev_hp_p2 = hp_p2;
                impl->round_initialized = true;
                impl->last_reset_frame = impl->current_frame;
                // ラウンド1は最初の有効フレームから開始
            }
            else
            {
                int frames_since_reset = impl->current_frame - impl->last_reset_frame;

                // タイマーベース: 99秒（5940フレーム）経過でラウンド終了と判定
                bool timer_expired = (frames_since_reset >= impl->config.round_duration_frames);

                // HPリセット補完: HP が低値から高値にジャンプした場合
                bool hp_reset_p1 = (impl->prev_hp_p1 < impl->config.hp_reset_low_threshold && hp_p1 > impl->config.hp_reset_high_threshold);
                bool hp_reset_p2 = (impl->prev_hp_p2 < impl->config.hp_reset_low_threshold && hp_p2 > impl->config.hp_reset_high_threshold);
                bool hp_reset_detected = hp_reset_p1 || hp_reset_p2;

                // ラウンドロゴ検出: 画面中央の "Round ? / Final Round / K.O. / TIME OVER" ロゴ
                string round_logo = impl->match_round_logo(norm_result.normalized_frame,
                    static_cast<int>(impl->config.canonical_width * 0.30),
                    static_cast<int>(impl->config.canonical_width * 0.70),
                    static_cast<int>(impl->config.canonical_height * 0.35),
                    static_cast<int>(impl->config.canonical_height * 0.65));
                bool round_logo_detected = !round_logo.empty();

                // ラウンド開始ロゴかラウンド終了ロゴかを区別
                bool is_round_start_logo = (round_logo == "round" || round_logo == "final_round");
                bool is_round_end_logo = (round_logo == "ko" || round_logo == "time_over");

                // 最小間隔チェック（誤検出防止）
                bool min_interval_passed = (frames_since_reset >= impl->config.round_timer_gap_frames);

                // ラウンド開始ロゴ: round_index をインクリメントしてラウンドリセットと判定
                if (is_round_start_logo && min_interval_passed)
                {
                    impl->round_index++;
                    out->hp_reset = 1;
                    impl->last_reset_frame = impl->current_frame;
                    cout << "[Scanner] Round start logo detected: " << round_logo
                         << " at frame " << impl->current_frame
                         << " -> round " << impl->round_index << endl;
                }
                // ラウンド終了ロゴ: round_index は変更せず、ラウンド終了のみ記録
                else if (is_round_end_logo && min_interval_passed)
                {
                    impl->last_reset_frame = impl->current_frame;
                    cout << "[Scanner] Round end logo detected: " << round_logo
                         << " at frame " << impl->current_frame
                         << " (round " << impl->round_index << " ended)" << endl;
                }
                // タイマーまたはHPリセットでラウンドリセット
                else if ((timer_expired || hp_reset_detected) && min_interval_passed)
                {
                    impl->round_index++;
                    out->hp_reset = 1;
                    impl->last_reset_frame = impl->current_frame;
                    cout << "[Scanner] Round reset detected at frame " << impl->current_frame
                         << " -> round " << impl->round_index
                         << " (timer=" << timer_expired << ", hp_reset=" << hp_reset_detected << ")" << endl;
                }

                impl->prev_hp_p1 = hp_p1;
                impl->prev_hp_p2 = hp_p2;
            }

            out->quality_flags_count = std::min(out->quality_flags_count, 16);

            return 1;
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_next_telemetry: ") + e.what();
            cout << "[Scanner] Exception: " << g_last_error << endl;
            return -1;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_next_telemetry";
            cout << "[Scanner] Unknown exception" << endl;
            return -1;
        }
    }

    SCANNER_API int scanner_next_input_event(ScannerHandle handle, InputEvent *out)
    {
        try
        {
            if (!handle || !out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // Input tracker が初期化されていない場合はエラー
            if (!impl->input_tracker_initialized || !impl->input_tracker)
            {
                g_last_error = "Input tracker not initialized. Call scanner_get_metadata first.";
                return -1;
            }

            // フレームを読み込む
            Mat frame;
            if (!impl->cap.read(frame) || frame.empty())
            {
                return 0; // End of video
            }

            impl->current_frame++;
            impl->input_frame_counter++;

            // 黒フレームをスキップ
            cv::Mat gray;
            if (frame.channels() == 3)
            {
                cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
            }
            else
            {
                gray = frame.clone();
            }

            double min_val, max_val;
            cv::minMaxLoc(gray, &min_val, &max_val);

            if (max_val < 10.0)
            {
                // 黒フレームはスキップして次へ
                return 1;
            }

            // 正規化
            NormalizationResult norm_result = impl->normalizer->normalize(frame);
            if (!norm_result.success)
            {
                g_last_error = norm_result.error_code + ": " + norm_result.error_message;
                cout << "[Scanner] Normalization failed: " << g_last_error << endl;
                return -1;
            }

            // 19行スロットを読み取り（P1とP2の両方）
            InputEvent event;
            memset(&event, 0, sizeof(InputEvent));
            InputSegment seg;
            bool got_segment = false;

            // P1 の入力履歴エリアをスキャン
            for (int row = 0; row < impl->config.rows_visible && !got_segment; row++)
            {
                InputRow row_p1 = impl->input_tracker->read_input_row(frame, row, "P1");
                if (row_p1.valid && impl->input_tracker->build_segment(row_p1, seg))
                {
                    snprintf(event.player, sizeof(event.player), "P1");
                    event.frame_start = seg.frame_start;
                    event.frame_end = seg.frame_end;
                    event.round_index = seg.round_index;
                    event.event_index = seg.event_index;
                    event.hold_frames = seg.hold_frames;
                    event.counter_clipped = seg.counter_clipped ? 1 : 0;
                    event.hold_frames_lower_bound = seg.hold_frames_lower_bound ? 1 : 0;
                    event.time_dilation_suspected = seg.time_dilation_suspected ? 1 : 0;
                    event.resync_used = seg.resync_used ? 1 : 0;
                    event.unknown_glyph = seg.unknown_glyph ? 1 : 0;
                    event.is_preround = seg.is_preround ? 1 : 0;

                    event.directions_count = 0;
                    for (size_t i = 0; i < seg.directions.size() && i < 8; i++)
                    {
                        snprintf(event.directions[i], sizeof(event.directions[i]), "%s", seg.directions[i].c_str());
                        event.directions_count++;
                    }

                    event.buttons_count = 0;
                    for (size_t i = 0; i < seg.buttons.size() && i < 7; i++)
                    {
                        snprintf(event.buttons[i], sizeof(event.buttons[i]), "%s", seg.buttons[i].c_str());
                        event.buttons_count++;
                    }

                    event.quality_flags_count = 0;
                    for (size_t i = 0; i < seg.quality_flags.size() && i < 16; i++)
                    {
                        snprintf(event.quality_flags[i], sizeof(event.quality_flags[i]), "%s", seg.quality_flags[i].c_str());
                        event.quality_flags_count++;
                    }

                    got_segment = true;
                }
            }

            // P1 で見つからなければ P2 をスキャン
            if (!got_segment)
            {
                for (int row = 0; row < impl->config.rows_visible && !got_segment; row++)
                {
                    InputRow row_p2 = impl->input_tracker->read_input_row(frame, row, "P2");
                    if (row_p2.valid && impl->input_tracker->build_segment(row_p2, seg))
                    {
                        snprintf(event.player, sizeof(event.player), "P2");
                        event.frame_start = seg.frame_start;
                        event.frame_end = seg.frame_end;
                        event.round_index = seg.round_index;
                        event.event_index = seg.event_index;
                        event.hold_frames = seg.hold_frames;
                        event.counter_clipped = seg.counter_clipped ? 1 : 0;
                        event.hold_frames_lower_bound = seg.hold_frames_lower_bound ? 1 : 0;
                        event.time_dilation_suspected = seg.time_dilation_suspected ? 1 : 0;
                        event.resync_used = seg.resync_used ? 1 : 0;
                        event.unknown_glyph = seg.unknown_glyph ? 1 : 0;
                        event.is_preround = seg.is_preround ? 1 : 0;

                        event.directions_count = 0;
                        for (size_t i = 0; i < seg.directions.size() && i < 8; i++)
                        {
                            snprintf(event.directions[i], sizeof(event.directions[i]), "%s", seg.directions[i].c_str());
                            event.directions_count++;
                        }

                        event.buttons_count = 0;
                        for (size_t i = 0; i < seg.buttons.size() && i < 7; i++)
                        {
                            snprintf(event.buttons[i], sizeof(event.buttons[i]), "%s", seg.buttons[i].c_str());
                            event.buttons_count++;
                        }

                        event.quality_flags_count = 0;
                        for (size_t i = 0; i < seg.quality_flags.size() && i < 16; i++)
                        {
                            snprintf(event.quality_flags[i], sizeof(event.quality_flags[i]), "%s", seg.quality_flags[i].c_str());
                            event.quality_flags_count++;
                        }

                        got_segment = true;
                    }
                }
            }

            if (got_segment)
            {
                memcpy(out, &event, sizeof(InputEvent));
                return 1;
            }

            // イベントが見つからなかった場合は続行
            return 1;
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_next_input_event: ") + e.what();
            cout << "[Scanner] Exception: " << g_last_error << endl;
            return -1;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_next_input_event";
            cout << "[Scanner] Unknown exception" << endl;
            return -1;
        }
    }

    SCANNER_API int scanner_next_move_event(ScannerHandle handle, MoveEvent *out)
    {
        try
        {
            if (!handle || !out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // Frame meter detector が初期化されていない場合は未検出
            if (!impl->frame_meter_detector_initialized || !impl->frame_meter_detector)
            {
                return 0; // No move events yet
            }

            // フレームを読み込む (telemetry と同じストリームを共有)
            Mat frame;
            if (!impl->cap.read(frame) || frame.empty())
            {
                return 0; // End of video
            }

            impl->current_frame++;

            // 黒フレームをスキップ
            cv::Mat gray;
            if (frame.channels() == 3)
            {
                cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
            }
            else
            {
                gray = frame.clone();
            }

            double min_val, max_val;
            cv::minMaxLoc(gray, &min_val, &max_val);

            if (max_val < 10.0)
            {
                return 1; // Skip black frame
            }

            // 正規化
            NormalizationResult norm_result = impl->normalizer->normalize(frame);
            if (!norm_result.success)
            {
                g_last_error = norm_result.error_code + ": " + norm_result.error_message;
                cout << "[Scanner] Normalization failed: " << g_last_error << endl;
                return -1;
            }

            // フレームメーター検出 (3フレームラッチ)
            FrameMeterReading fm = impl->frame_meter_detector->detect(norm_result.normalized_frame, impl->current_frame);

            memset(out, 0, sizeof(MoveEvent));
            out->frame_start = impl->current_frame;
            out->frame_end = impl->current_frame;
            out->round_index = 1;

            if (fm.detected)
            {
                out->fm_startup_p1 = fm.startup_frames;
                out->fm_total_p1 = fm.startup_frames + fm.active_frames + fm.recovery_frames;
                out->fm_advantage_p1 = fm.advantage;
                out->fm_startup_p2 = fm.startup_frames;
                out->fm_total_p2 = fm.startup_frames + fm.active_frames + fm.recovery_frames;
                out->fm_advantage_p2 = fm.advantage;
                out->damage_p1 = 0;
                out->combo_damage_p1 = 0;
                out->damage_p2 = 0;
                out->combo_damage_p2 = 0;
                snprintf(out->attack_property_p1, sizeof(out->attack_property_p1), "");
                snprintf(out->attack_property_p2, sizeof(out->attack_property_p2), "");
                snprintf(out->attack_property_locale, sizeof(out->attack_property_locale), "en");
                out->conf = fm.confidence;
            }
            else
            {
                // 未検出: デフォルト値
                out->fm_startup_p1 = 0;
                out->fm_total_p1 = 0;
                out->fm_advantage_p1 = 0;
                out->fm_startup_p2 = 0;
                out->fm_total_p2 = 0;
                out->fm_advantage_p2 = 0;
                out->damage_p1 = 0;
                out->combo_damage_p1 = 0;
                out->damage_p2 = 0;
                out->combo_damage_p2 = 0;
                snprintf(out->attack_property_p1, sizeof(out->attack_property_p1), "");
                snprintf(out->attack_property_p2, sizeof(out->attack_property_p2), "");
                snprintf(out->attack_property_locale, sizeof(out->attack_property_locale), "en");
                out->conf = 0.0;
            }

            out->quality_flags_count = 0;

            return fm.detected ? 1 : 1; // 継続してイテレーション
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_next_move_event: ") + e.what();
            cout << "[Scanner] Exception: " << g_last_error << endl;
            return -1;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_next_move_event";
            cout << "[Scanner] Unknown exception" << endl;
            return -1;
        }
    }

    SCANNER_API void scanner_destroy(ScannerHandle handle)
    {
        if (handle)
        {
            auto *impl = static_cast<ScannerImpl *>(handle);
            delete impl;
        }
    }

} // extern "C"
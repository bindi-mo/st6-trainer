#include "scanner_api.h"
#include <opencv2/core.hpp>
#include <opencv2/videoio.hpp>
#include <string>
#include <cstring>
#include <ctime>

using namespace cv;
using namespace std;

// Thread-local error buffer
static thread_local string g_last_error;

// Internal scanner state (minimal stub)
struct ScannerImpl
{
    VideoCapture cap;
    double fps;
    int total_frames;
    int current_frame;
    int telemetry_emitted; // 0=none, 1=emitted once
    string last_error;
    string template_pack_path; // Path to template pack directory

    ScannerImpl() : fps(0), total_frames(0), current_frame(0), telemetry_emitted(0) {}

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
        telemetry_emitted = 0;

        // Store template pack path from config
        if (cfg && cfg->template_pack_path)
        {
            template_pack_path = cfg->template_pack_path;
        }
        return true;
    }
};

// ===== C API Implementation (stub) =====

extern "C"
{

    SCANNER_API ScannerVersion scanner_api_version()
    {
        return {0, 1, 0};
    }

    SCANNER_API const char *scanner_get_last_error()
    {
        return g_last_error.c_str();
    }

    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config)
    {
        if (!video_path)
        {
            g_last_error = "video_path is null";
            return nullptr;
        }

        auto *impl = new ScannerImpl();
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

    SCANNER_API int scanner_get_metadata(ScannerHandle handle, MatchMetadata *out)
    {
        if (!handle || !out)
        {
            g_last_error = "Invalid handle or output pointer";
            return -1;
        }
        auto *impl = static_cast<ScannerImpl *>(handle);

        memset(out, 0, sizeof(MatchMetadata));
        snprintf(out->match_id, sizeof(out->match_id), "stub-match-id-%d", impl->current_frame);
        snprintf(out->character_name_p1, sizeof(out->character_name_p1), "RYU");
        snprintf(out->character_name_p2, sizeof(out->character_name_p2), "KEN");
        snprintf(out->control_type_p1, sizeof(out->control_type_p1), "classic");
        snprintf(out->control_type_p2, sizeof(out->control_type_p2), "modern");
        out->detected_at_frame = 1;
        snprintf(out->source_resolution, sizeof(out->source_resolution), "1920x1080");
        out->source_fps = impl->fps;
        out->canonical_raster[0] = 1920;
        out->canonical_raster[1] = 1080;
        out->top_y_offset_px = 0;
        out->bottom_y_offset_px = 0;
        out->offset_snapped = 0;
        out->manual_verified = -1; // null
        out->input_history_available = 1;
        snprintf(out->attack_property_locale, sizeof(out->attack_property_locale), "en,ja");
        // extracted_at_utc: current time
        time_t now = time(nullptr);
        strftime(out->extracted_at_utc, sizeof(out->extracted_at_utc), "%Y-%m-%dT%H:%M:%SZ", gmtime(&now));
        snprintf(out->config_version, sizeof(out->config_version), "cfg-1.0");
        snprintf(out->template_pack_version, sizeof(out->template_pack_version), "sf6-ui-2026.11");
        snprintf(out->extractor_version, sizeof(out->extractor_version), "0.1.0-stub");

        return 1;
    }

    SCANNER_API int scanner_next_telemetry(ScannerHandle handle, FrameTelemetry *out)
    {
        if (!handle || !out)
        {
            g_last_error = "Invalid handle or output pointer";
            return -1;
        }
        auto *impl = static_cast<ScannerImpl *>(handle);

        // Emit only one dummy telemetry frame, then return 0 (end)
        if (impl->telemetry_emitted)
        {
            return 0;
        }

        Mat frame;
        if (!impl->cap.read(frame) || frame.empty())
        {
            return 0;
        }
        impl->current_frame++;

        memset(out, 0, sizeof(FrameTelemetry));
        out->frame_index = impl->current_frame;
        out->source_frame_index = impl->current_frame;
        out->timestamp_ms = (int64_t)((impl->current_frame / impl->fps) * 1000.0);
        out->round_index = 1;
        out->hp_reset = 0;
        out->hp_p1 = 1.0;
        out->drive_p1 = 6.0;
        out->sa_p1 = 3;
        out->in_burnout_p1 = 0;
        out->hp_p2 = 1.0;
        out->drive_p2 = 6.0;
        out->sa_p2 = 3;
        out->in_burnout_p2 = 0;
        // quality_flags empty
        out->quality_flags_count = 0;

        impl->telemetry_emitted = 1;
        return 1;
    }

    SCANNER_API int scanner_next_input_event(ScannerHandle handle, InputEvent *out)
    {
        (void)handle;
        (void)out;
        // Stub: no input events
        return 0;
    }

    SCANNER_API int scanner_next_move_event(ScannerHandle handle, MoveEvent *out)
    {
        (void)handle;
        (void)out;
        // Stub: no move events
        return 0;
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
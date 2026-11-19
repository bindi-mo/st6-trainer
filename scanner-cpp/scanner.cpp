#include "scanner_api.h"
#include "calibration.h"
#include "raster_normalizer.h"
#include <opencv2/core.hpp>
#include <opencv2/videoio.hpp>
#include <opencv2/imgproc.hpp>
#include <string>
#include <cstring>
#include <ctime>
#include <iostream>
#include <sstream>
#include <memory>

using namespace cv;
using namespace std;

// Thread-local error buffer
static thread_local string g_last_error;

// Simple JSON parser for scanner_config.json
struct ScannerConfig {
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
    
    // Metadata
    string config_version = "cfg-1.0";
    string template_pack_version = "sf6-ui-2026.11";
    string extractor_version = "0.1.0";
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
    
    // Metadata
    bool metadata_emitted;
    MatchMetadata metadata;
    
    // Frame buffer for calibration
    Mat first_frame;
    bool has_first_frame;
    
    ScannerImpl() : fps(0), total_frames(0), current_frame(0),
                    calibration_done(false), top_y_offset_px(0), bottom_y_offset_px(0),
                    upper_confidence(0.0), lower_confidence(0.0),
                    metadata_emitted(false), has_first_frame(false) {}

    ~ScannerImpl() {
        if (cap.isOpened()) {
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
    void parse_config_json(const char* json_str) {
        if (!json_str) return;
        
        string json(json_str);
        
        // Simple extraction of key values (not full JSON parser)
        // In production, use a proper JSON library like nlohmann/json
        
        // Extract calibration parameters
        auto extract_double = [&](const string& key, double& out) {
            size_t pos = json.find("\"" + key + "\"");
            if (pos != string::npos) {
                pos = json.find(":", pos);
                if (pos != string::npos) {
                    pos++;
                    while (pos < json.length() && isspace(json[pos])) pos++;
                    size_t end = json.find_first_of(",}", pos);
                    if (end != string::npos) {
                        try {
                            out = stod(json.substr(pos, end - pos));
                        } catch (...) {
                            // Ignore parse errors
                        }
                    }
                }
            }
        };
        
        auto extract_int = [&](const string& key, int& out) {
            size_t pos = json.find("\"" + key + "\"");
            if (pos != string::npos) {
                pos = json.find(":", pos);
                if (pos != string::npos) {
                    pos++;
                    while (pos < json.length() && isspace(json[pos])) pos++;
                    size_t end = json.find_first_of(",}", pos);
                    if (end != string::npos) {
                        try {
                            out = stoi(json.substr(pos, end - pos));
                        } catch (...) {
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
        
        cout << "[Scanner] Config parsed: " << config.canonical_width << "x" << config.canonical_height 
             << " fps=" << config.fps << endl;
    }
    
    // Perform calibration on first frame
    bool perform_calibration() {
        if (calibration_done) {
            return true;
        }
        
        if (!has_first_frame) {
            // Skip to actual gameplay (frame 918+ based on user input)
            // This ensures HUD is fully visible and stable
            Mat frame;
            int target_frame = 918; // Start of actual gameplay
            int max_attempts = 300; // Try up to 300 frames after target
            
            cap.set(CAP_PROP_POS_FRAMES, target_frame);
            
            bool found_content = false;
            for (int i = 0; i < max_attempts; i++) {
                if (!cap.read(frame) || frame.empty()) {
                    g_last_error = "Failed to read frame for calibration";
                    return false;
                }
                
                // Check if frame has content (not black)
                cv::Mat gray;
                if (frame.channels() == 3) {
                    cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
                } else {
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
                
                if (upper_max > 100.0 && lower_max > 100.0) { // Found a frame with full HUD
                    cout << "[Scanner] Found gameplay frame at index " << (target_frame + i) 
                         << " (upper_max=" << upper_max << ", lower_max=" << lower_max << ")" << endl;
                    found_content = true;
                    break;
                }
                
                if (i < 5 || (i + 1) % 50 == 0) {
                    cout << "[Scanner] Skipping frame at index " << (target_frame + i) 
                         << " (upper_max=" << upper_max << ", lower_max=" << lower_max << ")" << endl;
                }
            }
            
            if (!found_content) {
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
        if (first_frame.empty()) {
            g_last_error = "First frame is empty";
            return false;
        }
        
        // Convert to HSV for color detection
        cv::Mat hsv;
        if (first_frame.channels() == 3) {
            cv::cvtColor(first_frame, hsv, cv::COLOR_BGR2HSV);
        } else {
            g_last_error = "First frame is not color";
            return false;
        }
        
        // Detect HP gauge (typically green/yellow in upper portion)
        // HP gauge is usually in the upper 25% of the screen
        cv::Mat hp_region = hsv.rowRange(0, static_cast<int>(hsv.rows * 0.25));
        
        // Green HP (hue ~60-180 in OpenCV HSV)
        cv::Mat hp_mask;
        cv::inRange(hp_region, cv::Scalar(40, 50, 50), cv::Scalar(180, 255, 255), hp_mask);
        
        double hp_mean = cv::mean(hp_mask)[0] / 255.0;
        
        // Detect SA gauge (typically blue in lower portion)
        // SA gauge is usually in the lower 30% of the screen
        cv::Mat sa_region = hsv.rowRange(static_cast<int>(hsv.rows * 0.70), hsv.rows);
        
        // Blue SA (hue ~100-130 in OpenCV HSV)
        cv::Mat sa_mask;
        cv::inRange(sa_region, cv::Scalar(100, 50, 50), cv::Scalar(130, 255, 255), sa_mask);
        
        double sa_mean = cv::mean(sa_mask)[0] / 255.0;
        
        cout << "[Scanner] HUD detection: hp_region_mean=" << hp_mean 
             << " sa_region_mean=" << sa_mean << endl;
        
        // Simplified Y offset estimation
        // Upper HUD: assume it's at the top of the HP region
        // Lower HUD: assume it's at the top of the SA region
        
        // For now, use default offsets (0 for upper, 0 for lower)
        // Phase 3-4 will refine these based on actual gauge detection
        top_y_offset_px = 0;
        bottom_y_offset_px = 0;
        upper_confidence = 0.8; // Moderate confidence
        lower_confidence = 0.8;
        
        // Check if HUD is present
        if (hp_mean < 0.01 && sa_mean < 0.01) {
            g_last_error = "E_HUD_ABSENT: No HUD detected (hp_mean=" + to_string(hp_mean) + ", sa_mean=" + to_string(sa_mean) + ")";
            cout << "[Scanner] Error: " << g_last_error << endl;
            return false;
        }
        
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
        return {0, 2, 0};  // Phase 2
    }

    SCANNER_API const char *scanner_get_last_error()
    {
        return g_last_error.c_str();
    }

    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config)
    {
        try {
            if (!video_path)
            {
                g_last_error = "video_path is null";
                return nullptr;
            }

            auto *impl = new ScannerImpl();
            
            // Parse config JSON if provided
            if (config && config->config_json) {
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
        } catch (const std::exception& e) {
            g_last_error = std::string("Exception in scanner_create: ") + e.what();
            return nullptr;
        } catch (...) {
            g_last_error = "Unknown exception in scanner_create";
            return nullptr;
        }
    }

    SCANNER_API int scanner_get_metadata(ScannerHandle handle, MatchMetadata *out)
    {
        try {
            if (!handle || !out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // Perform calibration if not done yet
            if (!impl->calibration_done) {
                if (!impl->perform_calibration()) {
                    return 0;  // Failed
                }
            }

            // Fill metadata
            memset(out, 0, sizeof(MatchMetadata));
            
            // Generate match_id (simplified - should use SHA256 per §7.1)
            snprintf(out->match_id, sizeof(out->match_id), "match-%d-%d", 
                     impl->top_y_offset_px, impl->bottom_y_offset_px);
            
            // Character names (stub for now - Phase 3+)
            snprintf(out->character_name_p1, sizeof(out->character_name_p1), "");
            snprintf(out->character_name_p2, sizeof(out->character_name_p2), "");
            
            // Control types (stub for now - Phase 3+)
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
            out->offset_snapped = 0;  // Always 0 per §2.2
            out->manual_verified = -1;  // null
            
            // Input history (stub for now - Phase 3+)
            out->input_history_available = 0;
            
            // Attack property locale (stub for now - Phase 4+)
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
        } catch (const std::exception& e) {
            g_last_error = std::string("Exception in scanner_get_metadata: ") + e.what();
            return -1;
        } catch (...) {
            g_last_error = "Unknown exception in scanner_get_metadata";
            return -1;
        }
    }

    SCANNER_API int scanner_next_telemetry(ScannerHandle handle, FrameTelemetry *out)
    {
        try {
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
                return 0;  // End of video
            }
            
            impl->current_frame++;

            // Skip black frames at the beginning (intro/loading)
            // Check if frame is black
            cv::Mat gray;
            if (frame.channels() == 3) {
                cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
            } else {
                gray = frame.clone();
            }
            
            double min_val, max_val;
            cv::minMaxLoc(gray, &min_val, &max_val);
            
            if (max_val < 10.0) {
                // Skip this black frame
                return 1; // Continue to next frame
            }

            // Normalize frame
            NormalizationResult norm_result = impl->normalizer->normalize(frame);
            if (!norm_result.success) {
                g_last_error = norm_result.error_code + ": " + norm_result.error_message;
                cout << "[Scanner] Normalization failed: " << g_last_error << endl;
                return -1;
            }

            // Fill telemetry (stub for now - Phase 4+)
            memset(out, 0, sizeof(FrameTelemetry));
            out->frame_index = impl->current_frame;
            out->source_frame_index = impl->current_frame;
            out->timestamp_ms = (int64_t)((impl->current_frame / impl->fps) * 1000.0);
            out->round_index = 1;
            out->hp_reset = 0;
            
            // Stub values (Phase 4 will implement actual gauge detection)
            out->hp_p1 = 1.0;
            out->drive_p1 = 6.0;
            out->sa_p1 = 3;
            out->in_burnout_p1 = 0;
            out->hp_p2 = 1.0;
            out->drive_p2 = 6.0;
            out->sa_p2 = 3;
            out->in_burnout_p2 = 0;
            
            out->quality_flags_count = 0;

            return 1;
        } catch (const std::exception& e) {
            g_last_error = std::string("Exception in scanner_next_telemetry: ") + e.what();
            cout << "[Scanner] Exception: " << g_last_error << endl;
            return -1;
        } catch (...) {
            g_last_error = "Unknown exception in scanner_next_telemetry";
            cout << "[Scanner] Unknown exception" << endl;
            return -1;
        }
    }

    SCANNER_API int scanner_next_input_event(ScannerHandle handle, InputEvent *out)
    {
        (void)handle;
        (void)out;
        // Stub: no input events (Phase 3+)
        return 0;
    }

    SCANNER_API int scanner_next_move_event(ScannerHandle handle, MoveEvent *out)
    {
        (void)handle;
        (void)out;
        // Stub: no move events (Phase 4+)
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
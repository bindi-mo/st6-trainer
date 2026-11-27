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
    double hp_reset_low_threshold = 0.3;  // HP がこの値未満なら「ラウンド終了」とみなす（補完用）
    double hp_reset_high_threshold = 0.7; // HP がこの値を超えたら「ラウンド開始」とみなす（補完用）
    int round_timer_gap_frames = 180;     // ラウンド間の最小フレーム間隔（3秒 @60fps、タイマーベース補完用）
    int round_duration_frames = 5940;     // 1ラウンドの想定フレーム数（99秒 @60fps）

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
    int last_reset_frame; // 前回のラウンドリセット検出フレーム（タイマーベース補完用）

    // Character name detection state
    bool character_names_detected;
    string detected_char_p1;
    string detected_char_p2;

    // Control type icon locations (for character name ROI calculation)
    cv::Point ctrl_icon_loc_p1;
    cv::Point ctrl_icon_loc_p2;

    // Early termination after round start (user request: stop after a few frames)
    bool stop_analysis;
    int frames_after_round_start;

    ScannerImpl() : fps(0), total_frames(0), current_frame(0),
                    calibration_done(false), top_y_offset_px(0), bottom_y_offset_px(0),
                    upper_confidence(0.0), lower_confidence(0.0),
                    metadata_emitted(false), has_first_frame(false),
                    input_tracker_initialized(false), input_frame_counter(0),
                    gauge_detector_initialized(false),
                    frame_meter_detector_initialized(false),
                    char_templates_loaded(false),
                    round_logo_templates_loaded(false),
                    prev_hp_p1(1.0), prev_hp_p2(1.0),
                    round_index(0),
                    round_initialized(false),
                    last_reset_frame(0),
                    character_names_detected(false),
                    detected_char_p1(""),
                    detected_char_p2(""),
                    stop_analysis(false),
                    frames_after_round_start(0) {}

    ~ScannerImpl()
    {
        if (cap.isOpened())
        {
            cap.release();
        }
    }

    bool open_video(const char *path, const ScanConfig *cfg)
    {
        // Try MSMF backend first for hardware decoding (DXVA2/D3D11VA on Windows)
        cap.open(path, cv::CAP_MSMF);
        if (!cap.isOpened())
        {
            // Fallback to default backend (FFmpeg)
            cap.open(path);
            if (!cap.isOpened())
            {
                last_error = "Failed to open video: " + string(path);
                return false;
            }
            else
            {
                std::cerr << "[Scanner] Opened with default backend (FFmpeg)" << std::endl;
            }
        }
        else
        {
            std::cerr << "[Scanner] Opened with MSMF backend (hardware decode via DXVA2/D3D11VA)" << std::endl;
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
        {
            cout << "[Scanner] load_round_logo_templates: already loaded, returning (templates_count=" << round_logo_templates.size() << ")" << endl;
            return;
        }

        namespace fs = std::filesystem;
        fs::path logos_dir = fs::path(template_pack_path) / "round_logos";
        if (!fs::exists(logos_dir))
        {
            cout << "[Scanner] load_round_logo_templates: Round logos directory not found: " << logos_dir << endl;
            round_logo_templates_loaded = true;
            return;
        }

        for (const auto &entry : fs::directory_iterator(logos_dir))
        {
            if (entry.path().extension() == ".png")
            {
                string full_path = entry.path().string();
                string stem = entry.path().stem().string();
                Mat tmpl = imread(full_path, IMREAD_COLOR);
                if (!tmpl.empty())
                {
                    CharNameTemplate ctmpl;
                    ctmpl.name = stem;
                    ctmpl.template_img = tmpl;
                    round_logo_templates.push_back(move(ctmpl));
                }
                else
                {
                    cout << "[Scanner] Failed to load image: " << full_path << endl;
                }
            }
        }

        round_logo_templates_loaded = true;
        cout << "[Scanner] load_round_logo_templates: Loaded " << round_logo_templates.size() << " round logo templates" << endl;
        for (const auto &tmpl : round_logo_templates)
        {
            cout << "[Scanner] load_round_logo_templates:   template '" << tmpl.name << "' size=" << tmpl.template_img.cols << "x" << tmpl.template_img.rows << endl;
        }
    }

    // Match round logo against templates (Phase 5)
    // Returns matched logo name (e.g. "round_1", "round_2", "final_round") or empty string
    string match_round_logo(const Mat &frame, int x_start, int x_end, int y_start, int y_end)
    {
        static int call_count = 0;
        call_count++;
        // cout << "[Scanner] match_round_logo #" << call_count << ": called with x=[" << x_start << "," << x_end << "] y=[" << y_start << "," << y_end << "] frame_size=" << frame.cols << "x" << frame.rows << " templates_count=" << round_logo_templates.size() << endl;
        if (round_logo_templates.empty())
        {
            cout << "[Scanner] match_round_logo: no templates loaded!" << endl;
            return "";
        }

        Mat roi = frame.rowRange(y_start, y_end).colRange(x_start, x_end);
        if (roi.empty())
        {
            cout << "[Scanner] match_round_logo: ROI is empty!" << endl;
            return "";
        }

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
            {
                continue;
            }

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

        // 信頼度閾値（0.85 以上でマッチとみなす）
        if (best_conf > 0.85)
            return best_name;

        return "";
    }

    // 指定名テンプレートのサイズを動的に取得（見つからなければ false）
    bool get_template_size(const string &name, int &w, int &h) const
    {
        for (const auto &tmpl : round_logo_templates)
        {
            if (tmpl.name == name)
            {
                w = static_cast<int>(tmpl.template_img.cols);
                h = static_cast<int>(tmpl.template_img.rows);
                return true;
            }
        }
        return false;
    }

    // 指定テンプレートが ROI 内で検出されるか（スコアを返す、見つからなければ -1）
    double match_single_template(const Mat &gray_roi, const string &name) const
    {
        for (const auto &tmpl : round_logo_templates)
        {
            if (tmpl.name != name)
                continue;

            Mat gray_tmpl;
            if (tmpl.template_img.channels() == 3)
                cvtColor(tmpl.template_img, gray_tmpl, COLOR_BGR2GRAY);
            else
                gray_tmpl = tmpl.template_img.clone();

            if (gray_tmpl.cols > gray_roi.cols || gray_tmpl.rows > gray_roi.rows)
                return -1.0;

            Mat result_mat;
            matchTemplate(gray_roi, gray_tmpl, result_mat, TM_CCOEFF_NORMED);
            double min_val, max_val;
            Point min_loc, max_loc;
            minMaxLoc(result_mat, &min_val, &max_val, &min_loc, &max_loc);
            return max_val;
        }
        return -1.0;
    }

    // ラウンド開始ロゴ（fight.png）を検出（スコアを返す、テンプレ無し/ROI外は -1）
    // ROI サイズはテンプレートファイルから動的に取得する（固定値は使わない）
    double detect_fight_logo(const Mat &frame, int roi_x, int roi_y) const
    {
        int w = 0, h = 0;
        if (!get_template_size("fight", w, h) || w <= 0 || h <= 0)
        {
            cout << "[Scanner] detect_fight_logo: fight template not found or invalid size" << endl;
            return -1.0;
        }

        const int margin = 50;
        int x_start = std::max(0, roi_x - margin);
        int y_start = std::max(0, roi_y - margin);
        int x_end = std::min(frame.cols, roi_x + w + margin);
        int y_end = std::min(frame.rows, roi_y + h + margin);
        if (x_start >= x_end || y_start >= y_end)
            return -1.0;

        Mat roi = frame.rowRange(y_start, y_end).colRange(x_start, x_end);
        if (roi.empty())
            return -1.0;

        Mat gray_roi;
        if (roi.channels() == 3)
            cvtColor(roi, gray_roi, COLOR_BGR2GRAY);
        else
            gray_roi = roi.clone();

        return match_single_template(gray_roi, "fight");
    }

    // 毎フレームのラウンド検出（telemetry / all 共通）
    // 戻り値: ラウンドリセット（round_index インクリメント）が発生したら true
    bool update_round_state(const Mat &frame, double hp_p1, double hp_p2, int *hp_reset_out)
    {
        if (hp_reset_out)
            *hp_reset_out = 0;

        if (!round_initialized)
        {
            prev_hp_p1 = hp_p1;
            prev_hp_p2 = hp_p2;
            round_initialized = true;
            last_reset_frame = current_frame;
            // ラウンド1は最初の有効フレームから開始
            return false;
        }

        int frames_since_reset = current_frame - last_reset_frame;

        // タイマーベース: 99秒（5940フレーム）経過でラウンド終了と判定
        bool timer_expired = (frames_since_reset >= config.round_duration_frames);

        // HPリセット補完: HP が低値から高値にジャンプした場合
        bool hp_reset_p1 = (prev_hp_p1 < config.hp_reset_low_threshold && hp_p1 > config.hp_reset_high_threshold);
        bool hp_reset_p2 = (prev_hp_p2 < config.hp_reset_low_threshold && hp_p2 > config.hp_reset_high_threshold);
        bool hp_reset_detected = hp_reset_p1 || hp_reset_p2;

        // ラウンド開始ロゴ検出: fight.png（ユーザー指定位置 x=529, y=370、サイズはテンプレートから動的取得）
        double fight_conf = detect_fight_logo(frame, 529, 370);
        bool is_round_start_logo = (fight_conf > 0.60);

        // ラウンド終了ロゴ検出: 既存の全テンプレートマッチング（ko / time_over）
        string round_logo = match_round_logo(frame, 0, frame.cols, 0, frame.rows);
        bool is_round_end_logo = (round_logo == "ko" || round_logo == "time_over");

        // 最小間隔チェック（誤検出防止）
        bool min_interval_passed = (frames_since_reset >= config.round_timer_gap_frames);

        bool reset = false;

        // ラウンド開始ロゴ（fight.png）: ラウンドが始まったので round_index をインクリメント
        if (is_round_start_logo && min_interval_passed)
        {
            round_index++; // ラウンド開始（0→1→2→3…）
            cout << "[Scanner] Round start logo detected: fight"
                 << " at frame " << current_frame
                 << " (round " << round_index << " started)" << endl;
        }
        // ラウンド終了ロゴ: ラウンド終了を検知（round_index は開始時にカウントアップするため変更しない）
        else if (is_round_end_logo && min_interval_passed)
        {
            last_reset_frame = current_frame;
            reset = true;
            cout << "[Scanner] Round end logo detected: " << round_logo
                 << " at frame " << current_frame
                 << " (round " << round_index << " ended)" << endl;
        }
        // タイマーまたはHPリセットでラウンドリセット（round_index は開始時にカウントアップするため変更しない）
        else if ((timer_expired || hp_reset_detected) && min_interval_passed)
        {
            last_reset_frame = current_frame;
            reset = true;
            cout << "[Scanner] Round reset detected at frame " << current_frame
                 << " (round " << round_index
                 << " (timer=" << timer_expired << ", hp_reset=" << hp_reset_detected << ")" << endl;
        }

        prev_hp_p1 = hp_p1;
        prev_hp_p2 = hp_p2;

        if (reset && hp_reset_out)
            *hp_reset_out = 1;
        return reset;
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

            cout << "[Scanner] match_character_name: template=" << ctmpl.name << " conf=" << max_val << endl;

            if (max_val > best_conf && max_val > 0.10)
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
            // ラウンド開始フレームを動的に検出（fight.png でテンプレートマッチング）
            // ユーザー指定位置: fight.png x=529, y=370（サイズはテンプレートから動的取得）
            // 1000フレーム以内に検出できなければエラー
            Mat frame;
            int max_attempts = 1000;

            cap.set(CAP_PROP_POS_FRAMES, 0);

            // ラウンドロゴテンプレートを読み込み
            load_round_logo_templates();

            // fight.png のサイズをテンプレートから動的に取得（固定値は使わない）
            int fight_w = 0, fight_h = 0;
            if (!get_template_size("fight", fight_w, fight_h) || fight_w <= 0 || fight_h <= 0)
            {
                g_last_error = "fight.png template not found or invalid size";
                cout << "[Scanner] Error: " << g_last_error << endl;
                return false;
            }
            const int fight_x = 529;
            const int fight_y = 370;
            const int fight_margin = 50;
            cout << "[Scanner] fight.png template size=" << fight_w << "x" << fight_h
                 << " roi origin=(" << fight_x << "," << fight_y << ")" << endl;

            bool found_round_start = false;
            int round_start_frame = -1;

            for (int i = 0; i < max_attempts; i++)
            {
                if (!cap.read(frame) || frame.empty())
                {
                    break;
                }

                int x_start = std::max(0, fight_x - fight_margin);
                int y_start = std::max(0, fight_y - fight_margin);
                int x_end = std::min(frame.cols, fight_x + fight_w + fight_margin);
                int y_end = std::min(frame.rows, fight_y + fight_h + fight_margin);
                if (x_start >= x_end || y_start >= y_end)
                    continue;

                Mat roi = frame.rowRange(y_start, y_end).colRange(x_start, x_end);
                if (roi.empty())
                    continue;

                Mat gray_roi;
                if (roi.channels() == 3)
                    cvtColor(roi, gray_roi, COLOR_BGR2GRAY);
                else
                    gray_roi = roi.clone();

                double conf = match_single_template(gray_roi, "fight");
                if (conf > 0.60)
                {
                    round_start_frame = i;
                    found_round_start = true;
                    cout << "[Scanner] Round start detected at frame " << i
                         << " (logo=fight, conf=" << conf << ")" << endl;
                    break;
                }
            }

            if (!found_round_start)
            {
                g_last_error = "Could not detect round start frame after " + to_string(max_attempts) + " attempts";
                cout << "[Scanner] Error: " << g_last_error << endl;
                return false;
            }

            // ラウンド開始フレームを first_frame として保存
            cap.set(CAP_PROP_POS_FRAMES, round_start_frame);
            if (!cap.read(frame) || frame.empty())
            {
                g_last_error = "Failed to read round start frame";
                return false;
            }

            first_frame = frame.clone();
            has_first_frame = true;

            // 実際のスキャンは先頭から開始
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

            // メタデータは一度だけ解析すれば十分（キャラクター名、コントロールタイプ、
            // X/Yオフセットは動画全体を通じて不変）。2回目以降はキャッシュを返す。
            if (impl->metadata_emitted)
            {
                memcpy(out, &impl->metadata, sizeof(MatchMetadata));
                return 1;
            }

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
            // Character name regions: top of screen where character names actually appear
            // P1: left side, P2: right side
            // アンカー位置（upper_timer_top_edge.png）からのオフセットで算出
            // アンカー位置: テンプレートマッチングで動的に特定
            // Character name offset: P1 x=-896, y=18（左端基準）
            // Character name end offset: P2 x=993, y=2（右端基準、右寄せ）
            // P2は右寄せなので、テンプレート幅を引いた左端を計算

            // アンカーテンプレートをマッチングして位置を特定
            int anchor_x = 912; // デフォルト値
            int anchor_y = 46;  // デフォルト値

            // アンカーテンプレートを読み込み
            Mat anchor_tmpl = imread(impl->template_pack_path + "/anchor/upper_timer_top_edge.png", IMREAD_COLOR);
            if (!anchor_tmpl.empty())
            {
                Mat anchor_gray, frame_gray;
                cvtColor(anchor_tmpl, anchor_gray, COLOR_BGR2GRAY);
                cvtColor(impl->first_frame, frame_gray, COLOR_BGR2GRAY);

                if (anchor_gray.cols <= frame_gray.cols && anchor_gray.rows <= frame_gray.rows)
                {
                    Mat result_mat;
                    matchTemplate(frame_gray, anchor_gray, result_mat, TM_CCOEFF_NORMED);
                    double min_val, max_val;
                    Point min_loc, max_loc;
                    minMaxLoc(result_mat, &min_val, &max_val, &min_loc, &max_loc);

                    if (max_val > 0.80)
                    {
                        anchor_x = max_loc.x;
                        anchor_y = max_loc.y;
                        cout << "[Scanner] Anchor detected at (" << anchor_x << "," << anchor_y << ") conf=" << max_val << endl;
                    }
                    else
                    {
                        cout << "[Scanner] Anchor not found, using default (" << anchor_x << "," << anchor_y << ") conf=" << max_val << endl;
                    }
                }
            }

            // P1: 左端基準
            int char_p1_x_start = std::max(0, anchor_x + (-896));
            int char_p1_x_end = char_p1_x_start + 192; // 192px幅
            int char_p1_y_start = anchor_y - 18;       // ユーザー指定: y=-18
            int char_p1_y_end = char_p1_y_start + 20;  // キャラクター名の高さを20px

            // P2: 右端基準（テンプレート幅を引いて左端を計算、各テンプレートごとに計算）
            // y座標はP1と同じ
            int char_p2_y_start = char_p1_y_start;
            int char_p2_y_end = char_p1_y_end;

            string char_p1, char_p2;

            // Use already detected character names if available
            if (!impl->detected_char_p1.empty() && !impl->detected_char_p2.empty())
            {
                char_p1 = impl->detected_char_p1;
                char_p2 = impl->detected_char_p2;
                cout << "[Scanner] Using pre-detected character names: P1='" << char_p1 << "' P2='" << char_p2 << "'" << endl;
            }
            else
            {
                // Detect from first frame
                // P1: 左端基準、P2: 右端基準（テンプレート幅を引いて左端を計算）
                cout << "[Scanner] Character name ROI P1: x=[" << char_p1_x_start << "," << char_p1_x_end << "] y=[" << char_p1_y_start << "," << char_p1_y_end << "]" << endl;

                char_p1 = impl->match_character_name(impl->first_frame, char_p1_x_start, char_p1_x_end, char_p1_y_start, char_p1_y_end);

                cout << "[Scanner] Character name P1: '" << char_p1 << "'" << endl;

                // P2: 各テンプレートごとに右端基準でROIを計算
                // テンプレート幅を減算して左端を導出（固定値ではなく毎回計算）
                // y座標はP1と同じ（ユーザー指定）、高さはテンプレート画像の実際の高さに合わせる
                int char_p2_x_start = 0;
                int char_p2_x_end = 0;
                string best_p2_name;
                double best_p2_conf = 0.0;

                for (const auto &ctmpl : impl->char_templates)
                {
                    int tmpl_width = ctmpl.template_img.cols;
                    int tmpl_height = ctmpl.template_img.rows;
                    int p2_x_start = anchor_x + 994 - tmpl_width;
                    int p2_x_end = anchor_x + 994;
                    int p2_y_start = char_p2_y_start;
                    int p2_y_end = char_p2_y_start + tmpl_height;

                    cout << "[Scanner] Character name ROI P2 (template=" << ctmpl.name << "): x=[" << p2_x_start << "," << p2_x_end << "] y=[" << p2_y_start << "," << p2_y_end << "]" << endl;

                    // そのテンプレート幅・高さに基づいたROI内で、そのテンプレート自身のマッチ度を評価
                    Mat roi = impl->first_frame.rowRange(p2_y_start, p2_y_end).colRange(p2_x_start, p2_x_end);
                    if (roi.empty())
                        continue;

                    Mat gray_roi;
                    if (roi.channels() == 3)
                        cvtColor(roi, gray_roi, COLOR_BGR2GRAY);
                    else
                        gray_roi = roi.clone();

                    Mat gray_tmpl;
                    if (ctmpl.template_img.channels() == 3)
                        cvtColor(ctmpl.template_img, gray_tmpl, COLOR_BGR2GRAY);
                    else
                        gray_tmpl = ctmpl.template_img.clone();

                    // テンプレートをROIサイズにリサイズ（アスペクト比維持）
                    double scale = static_cast<double>(gray_roi.rows) / static_cast<double>(gray_tmpl.rows);
                    Mat resized_tmpl;
                    resize(gray_tmpl, resized_tmpl, Size(), scale, scale, INTER_AREA);

                    if (resized_tmpl.cols > gray_roi.cols || resized_tmpl.rows > gray_roi.rows)
                        continue;

                    Mat result_mat;
                    matchTemplate(gray_roi, resized_tmpl, result_mat, TM_CCOEFF_NORMED);
                    double min_val, max_val;
                    Point min_loc, max_loc;
                    minMaxLoc(result_mat, &min_val, &max_val, &min_loc, &max_loc);

                    cout << "[Scanner]   self-match template=" << ctmpl.name << " conf=" << max_val << endl;

                    if (max_val > best_p2_conf && max_val > 0.10)
                    {
                        best_p2_conf = max_val;
                        best_p2_name = ctmpl.name;
                    }
                }
                char_p2 = best_p2_name;

                cout << "[Scanner] Character name P2: '" << char_p2 << "'" << endl;
            }

            snprintf(out->character_name_p1, sizeof(out->character_name_p1), "%s", char_p1.c_str());
            snprintf(out->character_name_p2, sizeof(out->character_name_p2), "%s", char_p2.c_str());

            // Store detected character names for early termination
            impl->detected_char_p1 = char_p1;
            impl->detected_char_p2 = char_p2;
            impl->character_names_detected = (!char_p1.empty() && !char_p2.empty());

            // Early termination: stop analysis after character names detected
            if (impl->character_names_detected)
            {
                impl->stop_analysis = true;
                impl->frames_after_round_start = 0;
                cout << "[Scanner] Character names detected from first frame. Will stop after 5 more frames." << endl;
            }

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

            // Total frames for progress reporting (Phase 4)
            out->total_frames = impl->total_frames;

            // Initialize input tracker
            if (!impl->input_tracker_initialized)
            {
                impl->input_tracker = make_unique<InputTracker>();
                impl->input_tracker->set_template_pack_path(impl->template_pack_path);
                impl->input_tracker->rows_visible = impl->config.rows_visible;
                impl->input_tracker->input_glyph_threshold = impl->config.threshold_input_glyph;
                impl->input_tracker->init_templates();
                impl->input_tracker_initialized = true;
                cout << "[Scanner] Input tracker initialized" << endl;
            }

            // Resolve control types from classic.png/modern.png classifier templates (Phase 6)
            // Must be after input_tracker is initialized
            // Pass player parameter to search in player-specific regions
            auto ctrl_p1 = impl->input_tracker->resolve_control_type(impl->first_frame, impl->config.threshold_input_glyph, "P1");
            auto ctrl_p2 = impl->input_tracker->resolve_control_type(impl->first_frame, impl->config.threshold_input_glyph, "P2");
            impl->input_tracker->set_control_type("P1", ctrl_p1.control_type);
            impl->input_tracker->set_control_type("P2", ctrl_p2.control_type);

            snprintf(out->control_type_p1, sizeof(out->control_type_p1), "%s", ctrl_p1.control_type.c_str());
            snprintf(out->control_type_p2, sizeof(out->control_type_p2), "%s", ctrl_p2.control_type.c_str());

            cout << "[Scanner] Control type P1: " << ctrl_p1.control_type
                 << " (conf=" << ctrl_p1.confidence << ")" << endl;
            cout << "[Scanner] Control type P2: " << ctrl_p2.control_type
                 << " (conf=" << ctrl_p2.confidence << ")" << endl;

            // Save control type icon locations for character name ROI calculation
            impl->ctrl_icon_loc_p1 = ctrl_p1.location;
            impl->ctrl_icon_loc_p2 = ctrl_p2.location;

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
                impl->frame_meter_detector->init_templates();
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

            // メタデータをキャッシュ（2回目以降の呼び出しで再解析をスキップ）
            memcpy(&impl->metadata, out, sizeof(MatchMetadata));
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

            // Debug: check normalized frame
            if (impl->current_frame < 5 || impl->current_frame % 100 == 0)
            {
                cv::Mat norm_gray;
                if (norm_result.normalized_frame.channels() == 3)
                    cv::cvtColor(norm_result.normalized_frame, norm_gray, cv::COLOR_BGR2GRAY);
                else
                    norm_gray = norm_result.normalized_frame.clone();
                cv::Scalar norm_mean = cv::mean(norm_gray);
                cout << "[Scanner] Normalized frame " << impl->current_frame
                     << " mean=" << norm_mean[0]
                     << " size=" << norm_result.normalized_frame.cols << "x" << norm_result.normalized_frame.rows
                     << " content_offset=(" << norm_result.content_x_offset << "," << norm_result.content_y_offset << ")"
                     << " content_size=" << norm_result.content_width << "x" << norm_result.content_height << endl;
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
                out->drive_p1 = gauges.drive_p1.value * 6.0;                         // Drive ゲージは 0-6 段階
                out->sa_p1 = static_cast<int>(std::round(gauges.sa_p1.value * 3.0)); // SA は 0-3 段階
                if (out->sa_p1 < 0)
                    out->sa_p1 = 0; // §7.5 項 7: クランプ
                if (out->sa_p1 > 3)
                    out->sa_p1 = 3;
                out->in_burnout_p1 = gauges.in_burnout_p1 ? 1 : 0;

                hp_p2 = gauges.hp_p2.value;
                out->drive_p2 = gauges.drive_p2.value * 6.0;
                out->sa_p2 = static_cast<int>(std::round(gauges.sa_p2.value * 3.0));
                if (out->sa_p2 < 0)
                    out->sa_p2 = 0; // §7.5 項 7: クランプ
                if (out->sa_p2 > 3)
                    out->sa_p2 = 3;
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
            int hp_reset_flag = 0;
            impl->update_round_state(frame, hp_p1, hp_p2, &hp_reset_flag);
            if (hp_reset_flag)
                out->hp_reset = 1;

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

    // ===== Phase 3: 1パス統合イテレータ =====
    SCANNER_API int scanner_next_all(ScannerHandle handle, FrameTelemetry *telemetry_out, InputEvent *input_out, MoveEvent *move_out)
    {
        try
        {
            if (!handle || !telemetry_out || !input_out || !move_out)
            {
                g_last_error = "Invalid handle or output pointer";
                return -1;
            }
            auto *impl = static_cast<ScannerImpl *>(handle);

            // 各サブシステムが初期化済みか確認
            if (!impl->input_tracker_initialized || !impl->input_tracker)
            {
                g_last_error = "Input tracker not initialized. Call scanner_get_metadata first.";
                return -1;
            }
            if (!impl->frame_meter_detector_initialized || !impl->frame_meter_detector)
            {
                g_last_error = "Frame meter detector not initialized. Call scanner_get_metadata first.";
                return -1;
            }

            // Early termination: stop analysis after round start + a few frames
            if (impl->stop_analysis)
            {
                impl->frames_after_round_start++;
                if (impl->frames_after_round_start >= 5)
                {
                    cout << "[Scanner] Early termination: stopping analysis after round start + "
                         << impl->frames_after_round_start << " frames" << endl;
                    return 0; // End of video
                }
            }

            // フレームを読み込む（1パスで1フレームのみ）
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
                // 黒フレームはスキップして次へ（継続）
                return 1;
            }

            // 正規化（1回のみ）
            NormalizationResult norm_result = impl->normalizer->normalize(frame);
            if (!norm_result.success)
            {
                g_last_error = norm_result.error_code + ": " + norm_result.error_message;
                cout << "[Scanner] Normalization failed: " << g_last_error << endl;
                return -1;
            }

            // ---- Telemetry (ゲージ検出) ----
            memset(telemetry_out, 0, sizeof(FrameTelemetry));
            telemetry_out->frame_index = impl->current_frame;
            telemetry_out->source_frame_index = impl->current_frame;
            telemetry_out->timestamp_ms = (int64_t)((impl->current_frame / impl->fps) * 1000.0);
            telemetry_out->round_index = impl->round_index;
            telemetry_out->hp_reset = 0;

            double hp_p1 = 1.0, hp_p2 = 1.0;
            if (impl->gauge_detector && impl->gauge_detector_initialized)
            {
                int upper_y_start = 0;
                int upper_y_end = static_cast<int>(impl->config.canonical_height * 0.25);
                int lower_y_start = static_cast<int>(impl->config.canonical_height * 0.70);
                int lower_y_end = static_cast<int>(impl->config.canonical_height);

                FrameGauges gauges = impl->gauge_detector->detect(norm_result.normalized_frame, upper_y_start, upper_y_end, lower_y_start, lower_y_end);

                hp_p1 = gauges.hp_p1.value;
                telemetry_out->drive_p1 = gauges.drive_p1.value * 6.0;
                telemetry_out->sa_p1 = static_cast<int>(std::round(gauges.sa_p1.value * 3.0));
                if (telemetry_out->sa_p1 < 0)
                    telemetry_out->sa_p1 = 0;
                if (telemetry_out->sa_p1 > 3)
                    telemetry_out->sa_p1 = 3;
                telemetry_out->in_burnout_p1 = gauges.in_burnout_p1 ? 1 : 0;

                hp_p2 = gauges.hp_p2.value;
                telemetry_out->drive_p2 = gauges.drive_p2.value * 6.0;
                telemetry_out->sa_p2 = static_cast<int>(std::round(gauges.sa_p2.value * 3.0));
                if (telemetry_out->sa_p2 < 0)
                    telemetry_out->sa_p2 = 0;
                if (telemetry_out->sa_p2 > 3)
                    telemetry_out->sa_p2 = 3;
                telemetry_out->in_burnout_p2 = gauges.in_burnout_p2 ? 1 : 0;

                if (gauges.hp_p1.occluded)
                {
                    snprintf(telemetry_out->quality_flags[telemetry_out->quality_flags_count], sizeof(telemetry_out->quality_flags[0]), "HP_P1_OCCLUDED");
                    telemetry_out->quality_flags_count++;
                }
                if (gauges.hp_p2.occluded)
                {
                    snprintf(telemetry_out->quality_flags[telemetry_out->quality_flags_count], sizeof(telemetry_out->quality_flags[0]), "HP_P2_OCCLUDED");
                    telemetry_out->quality_flags_count++;
                }
                if (gauges.sa_p1.occluded)
                {
                    snprintf(telemetry_out->quality_flags[telemetry_out->quality_flags_count], sizeof(telemetry_out->quality_flags[0]), "SA_P1_OCCLUDED");
                    telemetry_out->quality_flags_count++;
                }
                if (gauges.sa_p2.occluded)
                {
                    snprintf(telemetry_out->quality_flags[telemetry_out->quality_flags_count], sizeof(telemetry_out->quality_flags[0]), "SA_P2_OCCLUDED");
                    telemetry_out->quality_flags_count++;
                }
            }
            else
            {
                hp_p1 = 1.0;
                telemetry_out->drive_p1 = 6.0;
                telemetry_out->sa_p1 = 3;
                telemetry_out->in_burnout_p1 = 0;
                hp_p2 = 1.0;
                telemetry_out->drive_p2 = 6.0;
                telemetry_out->sa_p2 = 3;
                telemetry_out->in_burnout_p2 = 0;
            }

            telemetry_out->hp_p1 = hp_p1;
            telemetry_out->hp_p2 = hp_p2;

            // ラウンド検出（telemetryと同じロジック）
            int hp_reset_flag = 0;
            impl->update_round_state(frame, hp_p1, hp_p2, &hp_reset_flag);
            if (hp_reset_flag)
                telemetry_out->hp_reset = 1;

            telemetry_out->quality_flags_count = std::min(telemetry_out->quality_flags_count, 16);

            // ---- Input Event (入力トラッキング) ----
            memset(input_out, 0, sizeof(InputEvent));
            InputSegment seg;
            bool got_segment = false;

            // P1 の入力履歴エリアをスキャン
            for (int row = 0; row < impl->config.rows_visible && !got_segment; row++)
            {
                InputRow row_p1 = impl->input_tracker->read_input_row(frame, row, "P1");
                if (row_p1.valid && impl->input_tracker->build_segment(row_p1, seg))
                {
                    snprintf(input_out->player, sizeof(input_out->player), "P1");
                    input_out->frame_start = seg.frame_start;
                    input_out->frame_end = seg.frame_end;
                    input_out->round_index = seg.round_index;
                    input_out->event_index = seg.event_index;
                    input_out->hold_frames = seg.hold_frames;
                    input_out->counter_clipped = seg.counter_clipped ? 1 : 0;
                    input_out->hold_frames_lower_bound = seg.hold_frames_lower_bound ? 1 : 0;
                    input_out->time_dilation_suspected = seg.time_dilation_suspected ? 1 : 0;
                    input_out->resync_used = seg.resync_used ? 1 : 0;
                    input_out->unknown_glyph = seg.unknown_glyph ? 1 : 0;
                    input_out->is_preround = seg.is_preround ? 1 : 0;

                    input_out->directions_count = 0;
                    for (size_t i = 0; i < seg.directions.size() && i < 8; i++)
                    {
                        snprintf(input_out->directions[i], sizeof(input_out->directions[i]), "%s", seg.directions[i].c_str());
                        input_out->directions_count++;
                    }

                    input_out->buttons_count = 0;
                    for (size_t i = 0; i < seg.buttons.size() && i < 7; i++)
                    {
                        snprintf(input_out->buttons[i], sizeof(input_out->buttons[i]), "%s", seg.buttons[i].c_str());
                        input_out->buttons_count++;
                    }

                    input_out->quality_flags_count = 0;
                    for (size_t i = 0; i < seg.quality_flags.size() && i < 16; i++)
                    {
                        snprintf(input_out->quality_flags[i], sizeof(input_out->quality_flags[i]), "%s", seg.quality_flags[i].c_str());
                        input_out->quality_flags_count++;
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
                        snprintf(input_out->player, sizeof(input_out->player), "P2");
                        input_out->frame_start = seg.frame_start;
                        input_out->frame_end = seg.frame_end;
                        input_out->round_index = seg.round_index;
                        input_out->event_index = seg.event_index;
                        input_out->hold_frames = seg.hold_frames;
                        input_out->counter_clipped = seg.counter_clipped ? 1 : 0;
                        input_out->hold_frames_lower_bound = seg.hold_frames_lower_bound ? 1 : 0;
                        input_out->time_dilation_suspected = seg.time_dilation_suspected ? 1 : 0;
                        input_out->resync_used = seg.resync_used ? 1 : 0;
                        input_out->unknown_glyph = seg.unknown_glyph ? 1 : 0;
                        input_out->is_preround = seg.is_preround ? 1 : 0;

                        input_out->directions_count = 0;
                        for (size_t i = 0; i < seg.directions.size() && i < 8; i++)
                        {
                            snprintf(input_out->directions[i], sizeof(input_out->directions[i]), "%s", seg.directions[i].c_str());
                            input_out->directions_count++;
                        }

                        input_out->buttons_count = 0;
                        for (size_t i = 0; i < seg.buttons.size() && i < 7; i++)
                        {
                            snprintf(input_out->buttons[i], sizeof(input_out->buttons[i]), "%s", seg.buttons[i].c_str());
                            input_out->buttons_count++;
                        }

                        input_out->quality_flags_count = 0;
                        for (size_t i = 0; i < seg.quality_flags.size() && i < 16; i++)
                        {
                            snprintf(input_out->quality_flags[i], sizeof(input_out->quality_flags[i]), "%s", seg.quality_flags[i].c_str());
                            input_out->quality_flags_count++;
                        }

                        got_segment = true;
                    }
                }
            }

            // イベントが見つからなかった場合はデフォルト値（frame_start/end = current_frame）
            if (!got_segment)
            {
                input_out->frame_start = impl->current_frame;
                input_out->frame_end = impl->current_frame;
                input_out->round_index = impl->round_index;
                input_out->event_index = -1;
            }

            // ---- Move Event (フレームメーター) ----
            memset(move_out, 0, sizeof(MoveEvent));
            FrameMeterReading fm = impl->frame_meter_detector->detect(norm_result.normalized_frame, impl->current_frame);

            move_out->frame_start = impl->current_frame;
            move_out->frame_end = impl->current_frame;
            move_out->round_index = impl->round_index;

            if (fm.detected)
            {
                move_out->fm_startup_p1 = fm.startup_frames;
                move_out->fm_total_p1 = fm.startup_frames + fm.active_frames + fm.recovery_frames;
                move_out->fm_advantage_p1 = fm.advantage;
                move_out->fm_startup_p2 = fm.startup_frames;
                move_out->fm_total_p2 = fm.startup_frames + fm.active_frames + fm.recovery_frames;
                move_out->fm_advantage_p2 = fm.advantage;
                move_out->damage_p1 = 0;
                move_out->combo_damage_p1 = 0;
                move_out->damage_p2 = 0;
                move_out->combo_damage_p2 = 0;
                snprintf(move_out->attack_property_p1, sizeof(move_out->attack_property_p1), "");
                snprintf(move_out->attack_property_p2, sizeof(move_out->attack_property_p2), "");
                snprintf(move_out->attack_property_locale, sizeof(move_out->attack_property_locale), "en");
                move_out->conf = fm.confidence;
            }
            else
            {
                move_out->fm_startup_p1 = 0;
                move_out->fm_total_p1 = 0;
                move_out->fm_advantage_p1 = 0;
                move_out->fm_startup_p2 = 0;
                move_out->fm_total_p2 = 0;
                move_out->fm_advantage_p2 = 0;
                move_out->damage_p1 = 0;
                move_out->combo_damage_p1 = 0;
                move_out->damage_p2 = 0;
                move_out->combo_damage_p2 = 0;
                snprintf(move_out->attack_property_p1, sizeof(move_out->attack_property_p1), "");
                snprintf(move_out->attack_property_p2, sizeof(move_out->attack_property_p2), "");
                snprintf(move_out->attack_property_locale, sizeof(move_out->attack_property_locale), "en");
                move_out->conf = 0.0;
            }

            move_out->quality_flags_count = 0;

            return 1;
        }
        catch (const std::exception &e)
        {
            g_last_error = std::string("Exception in scanner_next_all: ") + e.what();
            cout << "[Scanner] Exception: " << g_last_error << endl;
            return -1;
        }
        catch (...)
        {
            g_last_error = "Unknown exception in scanner_next_all";
            cout << "[Scanner] Unknown exception" << endl;
            return -1;
        }
    }

} // extern "C"
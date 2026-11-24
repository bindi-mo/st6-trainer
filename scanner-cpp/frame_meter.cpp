#include "frame_meter.h"
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <algorithm>
#include <iostream>
#include <cstring>
#include <filesystem>

using namespace cv;
using namespace std;

namespace st6
{

    FrameMeterDetector::FrameMeterDetector()
    {
        config_.latch_frames = 3;
        config_.threshold = 0.90;
        config_.roi_y_start = 756;
        config_.roi_y_end = 1080;
        stable_count_ = 0;
        last_frame_index_ = -1;
        templates_initialized_ = false;
    }

    FrameMeterDetector::~FrameMeterDetector() {}

    void FrameMeterDetector::set_config(const FrameMeterConfig &cfg)
    {
        config_ = cfg;
    }

    void FrameMeterDetector::set_template_pack_path(const std::string &path)
    {
        template_pack_path_ = path;
        templates_initialized_ = false;
    }

    void FrameMeterDetector::init_templates()
    {
        if (templates_initialized_)
            return;

        namespace fs = std::filesystem;
        fs::path num_dir = fs::path(template_pack_path_) / "num";
        if (!fs::exists(num_dir))
        {
            templates_initialized_ = true;
            return;
        }

        const vector<string> digit_names = {"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "F", "minus"};

        for (const auto &name : digit_names)
        {
            fs::path tmpl_path = num_dir / (name + ".png");
            if (!fs::exists(tmpl_path))
                continue;

            Mat tmpl = imread(tmpl_path.string(), IMREAD_COLOR);
            if (tmpl.empty())
                continue;

            Mat tmpl_gray;
            if (tmpl.channels() == 3)
                cvtColor(tmpl, tmpl_gray, COLOR_BGR2GRAY);
            else
                tmpl_gray = tmpl.clone();

            digit_templates_[name] = tmpl_gray;
        }

        templates_initialized_ = true;
    }

    void FrameMeterDetector::reset()
    {
        latch_buffer_.clear();
        stable_count_ = 0;
        last_frame_index_ = -1;
    }

    int FrameMeterDetector::read_digit(const Mat &roi, int /*frame_index*/)
    {
        // キャッシュされた数字テンプレートでテンプレートマッチング
        // テンプレートは init_templates() で初期化時に1回読み込み済み
        if (!templates_initialized_)
            init_templates();

        Mat roi_gray;
        if (roi.channels() == 3)
            cvtColor(roi, roi_gray, COLOR_BGR2GRAY);
        else
            roi_gray = roi.clone();

        const vector<string> digit_names = {"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "F", "minus"};

        double best_conf = 0.0;
        string best_label;

        for (const auto &name : digit_names)
        {
            auto it = digit_templates_.find(name);
            if (it == digit_templates_.end())
                continue;

            const Mat &tmpl_gray = it->second;

            // テンプレートが ROI より大きい場合はスキップ
            if (tmpl_gray.cols > roi_gray.cols || tmpl_gray.rows > roi_gray.rows)
                continue;

            Mat result;
            matchTemplate(roi_gray, tmpl_gray, result, TM_CCOEFF_NORMED);
            double min_val, max_val;
            Point min_loc, max_loc;
            minMaxLoc(result, &min_val, &max_val, &min_loc, &max_loc);

            if (max_val > best_conf && max_val >= config_.threshold)
            {
                best_conf = max_val;
                best_label = name;
            }
        }

        if (best_label.empty())
            return -1; // 未検出

        if (best_label == "F")
            return -1; // F = 不明/フリーズ
        if (best_label == "minus")
            return 0;

        try
        {
            return stoi(best_label);
        }
        catch (...)
        {
            return -1;
        }
    }

    FrameMeterReading FrameMeterDetector::read_meter_block(const Mat &frame)
    {
        FrameMeterReading reading;
        reading.startup_frames = 0;
        reading.active_frames = 0;
        reading.recovery_frames = 0;
        reading.advantage = 0;
        reading.confidence = 0.0;
        reading.detected = false;

        // 下部 HUD 領域を切り出し (フレームメーターは下部に配置)
        int y_start = config_.roi_y_start;
        int y_end = config_.roi_y_end;
        if (y_end > frame.rows)
            y_end = frame.rows;
        if (y_start >= y_end)
            return reading;

        Mat meter_roi = frame.rowRange(y_start, y_end);

        // グレースケール変換
        Mat gray;
        if (meter_roi.channels() == 3)
            cvtColor(meter_roi, gray, COLOR_BGR2GRAY);
        else
            gray = meter_roi.clone();

        // フレームメーターの存在を簡易チェック (コントラストがある領域)
        double min_val, max_val;
        minMaxLoc(gray, &min_val, &max_val);

        if (max_val < 30.0)
        {
            // コントラストが低い = フレームメーター未表示
            reading.detected = false;
            reading.confidence = 0.0;
            return reading;
        }

        // フレームメーター数字読み取り (Phase 6: テンプレートマッチング精緻化)
        // フレームメーター表示エリアを 4 ブロックに分割:
        //   startup (左), active (中左), recovery (中右), advantage (右)
        int meter_width = gray.cols;
        int block_w = meter_width / 4;

        auto read_block_digit = [&](int block_idx) -> int
        {
            int x_start = block_idx * block_w;
            int x_end = (block_idx + 1) * block_w;
            if (x_end > gray.cols)
                x_end = gray.cols;
            if (x_start >= x_end)
                return -1;

            Mat digit_roi = gray.colRange(x_start, x_end);
            return read_digit(digit_roi, 0);
        };

        int startup = read_block_digit(0);
        int active = read_block_digit(1);
        int recovery = read_block_digit(2);
        int advantage = read_block_digit(3);

        // すべての数字が検出された場合のみ detected = true
        if (startup >= 0 && active >= 0 && recovery >= 0 && advantage >= 0)
        {
            reading.detected = true;
            reading.startup_frames = startup;
            reading.active_frames = active;
            reading.recovery_frames = recovery;
            reading.advantage = advantage;
            reading.confidence = config_.threshold; // テンプレートマッチング閾値を信頼度として使用
        }
        else
        {
            // 部分検出: detected = false、デフォルト値
            reading.detected = false;
            reading.confidence = 0.0;
        }

        return reading;
    }

    FrameMeterReading FrameMeterDetector::detect(const Mat &frame, int frame_index)
    {
        // 3フレームラッチ: 同一読み取りが latch_frames 連続で確定した場合のみ出力
        FrameMeterReading raw = read_meter_block(frame);

        // ラッチバッファに追加
        latch_buffer_.push_back(raw);

        // バッファサイズを latch_frames に制限
        while (static_cast<int>(latch_buffer_.size()) > config_.latch_frames)
        {
            latch_buffer_.erase(latch_buffer_.begin());
        }

        // 同一読み取りの連続カウント
        if (raw.detected && latch_buffer_.size() >= static_cast<size_t>(config_.latch_frames))
        {
            bool all_same = true;
            for (size_t i = 1; i < latch_buffer_.size(); i++)
            {
                if (latch_buffer_[i].startup_frames != latch_buffer_[0].startup_frames ||
                    latch_buffer_[i].active_frames != latch_buffer_[0].active_frames ||
                    latch_buffer_[i].recovery_frames != latch_buffer_[0].recovery_frames ||
                    latch_buffer_[i].advantage != latch_buffer_[0].advantage)
                {
                    all_same = false;
                    break;
                }
            }

            if (all_same)
            {
                stable_count_++;
            }
            else
            {
                stable_count_ = 0;
            }
        }
        else
        {
            stable_count_ = 0;
        }

        // ラッチ完了: stable_count_ >= latch_frames で出力
        if (stable_count_ >= config_.latch_frames)
        {
            last_frame_index_ = frame_index;
            return latch_buffer_[0];
        }

        // ラッチ未完了: 未検出として返す
        FrameMeterReading pending;
        pending.detected = false;
        pending.confidence = 0.0;
        return pending;
    }

} // namespace st6

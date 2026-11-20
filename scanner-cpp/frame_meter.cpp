#include "frame_meter.h"
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <algorithm>
#include <iostream>
#include <cstring>

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
    }

    FrameMeterDetector::~FrameMeterDetector() {}

    void FrameMeterDetector::set_config(const FrameMeterConfig &cfg)
    {
        config_ = cfg;
    }

    void FrameMeterDetector::reset()
    {
        latch_buffer_.clear();
        stable_count_ = 0;
        last_frame_index_ = -1;
    }

    int FrameMeterDetector::read_digit(const Mat &roi, int /*frame_index*/)
    {
        // num/ テンプレート (0-9, F, minus) でテンプレートマッチング
        // ここでは簡易実装: テンプレートパスが未設定の場合は -1 (未検出)
        // 実際のテンプレートマッチングは InputTracker と同様のロジックを使用
        // フレームメーター用の専用テンプレートディレクトリは num/ を流用
        (void)roi;
        return -1; // 未検出
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

        // 簡易実装: フレームメーターが検出されたと仮定し、デフォルト値を返す
        // 実際の数字読み取りは Phase 4 のテンプレートマッチング実装時に精緻化
        reading.detected = true;
        reading.confidence = 0.5; // 暫定: 低信頼度
        reading.startup_frames = 0;
        reading.active_frames = 0;
        reading.recovery_frames = 0;
        reading.advantage = 0;

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

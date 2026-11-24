#ifndef FRAME_METER_H
#define FRAME_METER_H

#include "scanner_api.h"
#include <opencv2/core.hpp>
#include <map>
#include <string>
#include <vector>

namespace st6
{

    // フレームメーター検出結果 (1 フレーム分)
    struct FrameMeterReading
    {
        int startup_frames;   // 発生フレーム数
        int active_frames;    // アクティブフレーム数
        int recovery_frames;  // リカバリーフレーム数
        int advantage;        // フレーム有利/不利 (正=有利、負=不利)
        double confidence;    // 検出信頼度 (0.0-1.0)
        bool detected;        // 検出成功フラグ
    };

    // フレームメーター設定
    struct FrameMeterConfig
    {
        int latch_frames;     // ラッチフレーム数 (3)
        double threshold;     // テンプレートマッチング閾値 (0.90)
        int roi_y_start;      // 下部 HUD バンド開始 Y
        int roi_y_end;        // 下部 HUD バンド終了 Y
    };

    // フレームメーター検出器 (3フレームラッチ付き)
    class FrameMeterDetector
    {
    public:
        FrameMeterDetector();
        ~FrameMeterDetector();

        // 設定を適用
        void set_config(const FrameMeterConfig &cfg);

        // テンプレートパックパスを設定
        void set_template_pack_path(const std::string &path);

        // テンプレートを初期化時に1回読み込んでキャッシュする
        void init_templates();

        // フレームからフレームメーターを検出
        // 3フレームラッチ: 同一読み取りが3フレーム連続で確定した場合のみ出力
        FrameMeterReading detect(const cv::Mat &frame, int frame_index);

        // ラッチ状態をリセット (ラウンド切り替え時など)
        void reset();

    private:
        FrameMeterConfig config_;

        // テンプレートパックパス (num/ テンプレート用)
        std::string template_pack_path_;

        // 数字テンプレートキャッシュ (ラベル -> グレースケール画像)
        std::map<std::string, cv::Mat> digit_templates_;
        bool templates_initialized_;

        // ラッチ用バッファ
        std::vector<FrameMeterReading> latch_buffer_;
        int stable_count_;
        int last_frame_index_;

        // テンプレートマッチングで数字を読み取り (キャッシュ使用)
        int read_digit(const cv::Mat &roi, int frame_index);

        // フレームメーター領域から startup/active/recovery を読み取り
        FrameMeterReading read_meter_block(const cv::Mat &frame);
    };

} // namespace st6

#endif // FRAME_METER_H

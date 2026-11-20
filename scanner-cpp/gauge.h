#ifndef GAUGE_H
#define GAUGE_H

#include "scanner_api.h"
#include <opencv2/core.hpp>
#include <string>
#include <vector>

namespace st6
{

    // 単一ゲージの検出結果
    struct GaugeReading
    {
        double value;        // 0.0-1.0 (正規化済みゲージ残量)
        double confidence;   // 検出信頼度 (0.0-1.0)
        bool occluded;       // ゲージが遮蔽/不明瞭か
        int fill_px;         // ゲージ充填ピクセル数
        int total_px;        // ゲージ全体ピクセル数
    };

    // ゲージ設定
    struct GaugeConfig
    {
        // HSV 範囲 (OpenCV: H 0-179, S 0-255, V 0-255)
        cv::Scalar hp_hsv_min;   // HP ゲージ (緑/黄)
        cv::Scalar hp_hsv_max;
        cv::Scalar drive_hsv_min; // Drive ゲージ
        cv::Scalar drive_hsv_max;
        cv::Scalar sa_hsv_min;   // SA ゲージ (青)
        cv::Scalar sa_hsv_max;
        int occluded_threshold;  // ゲージ遮蔽判定閾値 (ピクセル数)
        double min_confidence;   // 最小信頼度
    };

    // フレーム全体のゲージ読み取り結果
    struct FrameGauges
    {
        GaugeReading hp_p1;
        GaugeReading drive_p1;
        GaugeReading sa_p1;
        bool in_burnout_p1;
        GaugeReading hp_p2;
        GaugeReading drive_p2;
        GaugeReading sa_p2;
        bool in_burnout_p2;
    };

    // ゲージ検出器
    class GaugeDetector
    {
    public:
        GaugeDetector();
        ~GaugeDetector();

        // 設定を適用
        void set_config(const GaugeConfig &cfg);

        // フレームからゲージを読み取り (canonical 1920x1080 想定)
        // upper_roi_y: 上部 HUD バンドの Y 範囲 [start, end)
        // lower_roi_y: 下部 HUD バンドの Y 範囲 [start, end)
        FrameGauges detect(const cv::Mat &frame,
                           int upper_y_start, int upper_y_end,
                           int lower_y_start, int lower_y_end);

    private:
        GaugeConfig config_;

        // 単一ゲージバーの充填率を検出
        GaugeReading detect_gauge_bar(const cv::Mat &hsv_roi,
                                      const cv::Scalar &hsv_min,
                                      const cv::Scalar &hsv_max,
                                      int gauge_height_px);

        // バーンアウト判定 (ゲージが赤/点滅状態)
        bool detect_burnout(const cv::Mat &hsv_roi);

        // マスクから充填率を計算 (左から右への連続領域)
        double compute_fill_ratio(const cv::Mat &mask, int total_width);
    };

} // namespace st6

#endif // GAUGE_H

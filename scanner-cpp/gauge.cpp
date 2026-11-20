#include "gauge.h"
#include <opencv2/imgproc.hpp>
#include <algorithm>
#include <iostream>

using namespace cv;
using namespace std;

namespace st6
{

    GaugeDetector::GaugeDetector()
    {
        // デフォルト設定 (OpenCV HSV: H 0-179, S 0-255, V 0-255)
        config_.hp_hsv_min = Scalar(40, 50, 50);    // 緑/黄 HP ゲージ
        config_.hp_hsv_max = Scalar(90, 255, 255);
        config_.drive_hsv_min = Scalar(15, 100, 100); // オレンジ Drive ゲージ
        config_.drive_hsv_max = Scalar(35, 255, 255);
        config_.sa_hsv_min = Scalar(100, 50, 50);   // 青 SA ゲージ
        config_.sa_hsv_max = Scalar(130, 255, 255);
        config_.occluded_threshold = 10;
        config_.min_confidence = 0.5;
    }

    GaugeDetector::~GaugeDetector() {}

    void GaugeDetector::set_config(const GaugeConfig &cfg)
    {
        config_ = cfg;
    }

    double GaugeDetector::compute_fill_ratio(const Mat &mask, int total_width)
    {
        if (total_width <= 0)
            return 0.0;

        // 左から右への連続した白ピクセル数を測定 (ゲージは左から充填される)
        int fill_px = 0;
        for (int x = 0; x < total_width; x++)
        {
            bool column_has_white = false;
            for (int y = 0; y < mask.rows; y++)
            {
                if (mask.at<uchar>(y, x) > 0)
                {
                    column_has_white = true;
                    break;
                }
            }
            if (column_has_white)
                fill_px++;
            else
                break; // 最初の空き列でストップ
        }

        return static_cast<double>(fill_px) / static_cast<double>(total_width);
    }

    GaugeReading GaugeDetector::detect_gauge_bar(const Mat &hsv_roi,
                                                 const Scalar &hsv_min,
                                                 const Scalar &hsv_max,
                                                 int gauge_height_px)
    {
        GaugeReading reading;
        reading.value = 0.0;
        reading.confidence = 0.0;
        reading.occluded = false;
        reading.fill_px = 0;
        reading.total_px = 0;

        // HSV マスク生成
        Mat mask;
        inRange(hsv_roi, hsv_min, hsv_max, mask);

        // ノイズ除去 (モルフォロジカルオープニング)
        Mat kernel = getStructuringElement(MORPH_RECT, Size(3, 3));
        morphologyEx(mask, mask, MORPH_OPEN, kernel);

        // ゲージバーの高さ (ROI の下部に配置されていると仮定)
        int bar_y_start = max(0, hsv_roi.rows - gauge_height_px);
        Mat bar_region = mask.rowRange(bar_y_start, hsv_roi.rows);

        // 充填率計算
        int total_width = bar_region.cols;
        reading.total_px = total_width * bar_region.rows;

        // 白ピクセル数
        int white_px = countNonZero(bar_region);
        reading.fill_px = white_px;

        // 充填率 (左からの連続充填を優先)
        double fill_ratio = compute_fill_ratio(bar_region, total_width);
        reading.value = std::min(std::max(fill_ratio, 0.0), 1.0);

        // 信頼度: ゲージ領域に十分な色が検出できたか
        double color_ratio = static_cast<double>(white_px) /
                             static_cast<double>(total_width * bar_region.rows);
        reading.confidence = std::min(color_ratio * 5.0, 1.0); // スケーリング

        // 遮蔽判定: 白ピクセルが閾値未満なら遮蔽とみなす
        reading.occluded = (white_px < config_.occluded_threshold);

        return reading;
    }

    bool GaugeDetector::detect_burnout(const Mat &hsv_roi)
    {
        // バーンアウト: ゲージが赤色 (HSV 赤領域) または点滅
        Mat red_mask;
        inRange(hsv_roi, Scalar(0, 100, 100), Scalar(10, 255, 255), red_mask);
        Mat red_mask2;
        inRange(hsv_roi, Scalar(170, 100, 100), Scalar(179, 255, 255), red_mask2);
        red_mask |= red_mask2;

        int red_px = countNonZero(red_mask);
        return red_px > config_.occluded_threshold * 5;
    }

    FrameGauges GaugeDetector::detect(const Mat &frame,
                                      int upper_y_start, int upper_y_end,
                                      int lower_y_start, int lower_y_end)
    {
        FrameGauges gauges;
        memset(&gauges, 0, sizeof(gauges));

        // HSV 変換
        Mat hsv;
        if (frame.channels() == 3)
            cvtColor(frame, hsv, COLOR_BGR2HSV);
        else
            hsv = frame.clone();

        // 上部 HUD 領域 (HP/Drive/キャラ名)
        Mat upper_roi = hsv.rowRange(upper_y_start, upper_y_end);

        // 下部 HUD 領域 (SA/フレームメーター)
        Mat lower_roi = hsv.rowRange(lower_y_start, lower_y_end);

        // P1 ゲージ (左側: x 0-960)
        Mat p1_upper = upper_roi.colRange(0, upper_roi.cols / 2);
        Mat p1_lower = lower_roi.colRange(0, lower_roi.cols / 2);

        // P2 ゲージ (右側: x 960-1920)
        Mat p2_upper = upper_roi.colRange(upper_roi.cols / 2, upper_roi.cols);
        Mat p2_lower = lower_roi.colRange(lower_roi.cols / 2, lower_roi.cols);

        // HP ゲージ: 上部領域の上部 (y 0-30% of upper band)
        int hp_h = max(1, (upper_y_end - upper_y_start) / 4);
        Mat p1_hp_roi = p1_upper.rowRange(0, hp_h);
        Mat p2_hp_roi = p2_upper.rowRange(0, hp_h);

        gauges.hp_p1 = detect_gauge_bar(p1_hp_roi, config_.hp_hsv_min, config_.hp_hsv_max, hp_h);
        gauges.hp_p2 = detect_gauge_bar(p2_hp_roi, config_.hp_hsv_min, config_.hp_hsv_max, hp_h);

        // Drive ゲージ: 上部領域の中部
        int drive_y_start = hp_h;
        int drive_h = max(1, (upper_y_end - upper_y_start) / 4);
        Mat p1_drive_roi = p1_upper.rowRange(drive_y_start, drive_y_start + drive_h);
        Mat p2_drive_roi = p2_upper.rowRange(drive_y_start, drive_y_start + drive_h);

        gauges.drive_p1 = detect_gauge_bar(p1_drive_roi, config_.drive_hsv_min, config_.drive_hsv_max, drive_h);
        gauges.drive_p2 = detect_gauge_bar(p2_drive_roi, config_.drive_hsv_min, config_.drive_hsv_max, drive_h);

        // SA ゲージ: 下部領域
        int sa_h = max(1, (lower_y_end - lower_y_start) / 3);
        Mat p1_sa_roi = p1_lower.rowRange(0, sa_h);
        Mat p2_sa_roi = p2_lower.rowRange(0, sa_h);

        gauges.sa_p1 = detect_gauge_bar(p1_sa_roi, config_.sa_hsv_min, config_.sa_hsv_max, sa_h);
        gauges.sa_p2 = detect_gauge_bar(p2_sa_roi, config_.sa_hsv_min, config_.sa_hsv_max, sa_h);

        // バーンアウト判定 (HP ゲージ領域の赤検出)
        gauges.in_burnout_p1 = detect_burnout(p1_hp_roi) ? 1 : 0;
        gauges.in_burnout_p2 = detect_burnout(p2_hp_roi) ? 1 : 0;

        // ゲージ値が閾値未満なら 0 にクランプ (ノイズ除去)
        if (gauges.hp_p1.confidence < config_.min_confidence)
            gauges.hp_p1.value = 0.0;
        if (gauges.hp_p2.confidence < config_.min_confidence)
            gauges.hp_p2.value = 0.0;
        if (gauges.drive_p1.confidence < config_.min_confidence)
            gauges.drive_p1.value = 0.0;
        if (gauges.drive_p2.confidence < config_.min_confidence)
            gauges.drive_p2.value = 0.0;
        if (gauges.sa_p1.confidence < config_.min_confidence)
            gauges.sa_p1.value = 0.0;
        if (gauges.sa_p2.confidence < config_.min_confidence)
            gauges.sa_p2.value = 0.0;

        // SA ゲージ値を整数化 (0-3 段階など)
        gauges.sa_p1.value = std::round(gauges.sa_p1.value * 3.0) / 3.0;
        gauges.sa_p2.value = std::round(gauges.sa_p2.value * 3.0) / 3.0;

        return gauges;
    }

} // namespace st6

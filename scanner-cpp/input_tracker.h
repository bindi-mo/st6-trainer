#pragma once

#include "scanner_api.h"
#include <opencv2/core.hpp>
#include <string>
#include <vector>
#include <memory>

namespace st6
{

    // テンプレートマッチング結果
    struct GlyphMatch
    {
        std::string label;  // テンプレートラベル（例: "6", "LP", "L" など）
        double confidence;  // マッチング信頼度 (0.0-1.0)
        cv::Point location; // マッチ位置（ROI内座標）
    };

    // 入力履歴の1行（1フレーム分）の読み取り結果
    struct InputRow
    {
        int row_index;                      // 0-18 (19行スロット)
        std::vector<GlyphMatch> directions; // 方向キーマッチ結果
        std::vector<GlyphMatch> buttons;    // ボタンマッチ結果
        int counter_value;                  // カウンター値（-1=未検出）
        bool valid;                         // 行が有効か
    };

    // 操作系判定結果
    struct ControlTypeResult
    {
        std::string control_type; // "classic" / "modern" / ""
        double confidence;        // 判定信頼度
        bool resolved;            // 判定完了フラグ
    };

    // 入力イベントセグメント（連続する同一入力をまとめたもの）
    struct InputSegment
    {
        int frame_start;
        int frame_end;
        std::string player; // "P1" / "P2"
        int round_index;
        int event_index;
        std::vector<std::string> directions;
        std::vector<std::string> buttons;
        int hold_frames;
        bool counter_clipped;
        bool hold_frames_lower_bound;
        bool time_dilation_suspected;
        bool resync_used;
        bool unknown_glyph;
        bool is_preround;
        std::vector<std::string> quality_flags;
    };

    // 入力履歴追跡器
    class InputTracker
    {
    public:
        InputTracker();
        ~InputTracker();

        // テンプレートパスを設定
        void set_template_pack_path(const std::string &path);

        // 操作系（classic/modern）を判定（1回のみ実行）
        ControlTypeResult resolve_control_type(const cv::Mat &frame, double threshold);

        // 1フレーム分の入力履歴を読み取り
        InputRow read_input_row(const cv::Mat &frame, int row_index, const std::string &player);

        // 入力イベントセグメントを構築（状態マシンによる）
        bool build_segment(const InputRow &row, InputSegment &out);

        // 設定値
        int rows_visible = 19;
        double input_glyph_threshold = 0.92;

    private:
        std::string template_pack_path_;

        // テンプレート画像を読み込み
        cv::Mat load_template(const std::string &subdir, const std::string &name);

        // テンプレートマッチングを実行
        std::vector<GlyphMatch> match_templates(const cv::Mat &roi,
                                                const std::string &subdir,
                                                double threshold,
                                                int max_matches);

        // 方向キーテンプレートをマッチング
        std::vector<GlyphMatch> match_directions(const cv::Mat &roi, double threshold);

        // ボタンテンプレートをマッチング（操作系に応じて classic/modern を選択）
        std::vector<GlyphMatch> match_buttons(const cv::Mat &roi, double threshold,
                                              const std::string &control_type);

        // カウンター値を読み取り（数字テンプレート使用）
        int read_counter(const cv::Mat &roi);

        // 操作系バッジを検出
        ControlTypeResult detect_control_badge(const cv::Mat &frame, double threshold);

        // 状態マシン用
        InputSegment current_segment_;
        bool segment_active_ = false;
        int event_counter_ = 0;
        int last_frame_index_ = -1;
    };

} // namespace st6

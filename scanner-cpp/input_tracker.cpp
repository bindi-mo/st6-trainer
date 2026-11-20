#include "input_tracker.h"
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <iostream>
#include <sstream>
#include <algorithm>
#include <cstring>

using namespace cv;
using namespace std;

namespace st6
{

    InputTracker::InputTracker() {}

    InputTracker::~InputTracker() {}

    void InputTracker::set_template_pack_path(const std::string &path)
    {
        template_pack_path_ = path;
    }

    Mat InputTracker::load_template(const std::string &subdir, const std::string &name)
    {
        string path = template_pack_path_ + "/" + subdir + "/" + name + ".png";
        Mat img = imread(path, IMREAD_COLOR);
        if (img.empty())
        {
            // グレースケールで再試行
            img = imread(path, IMREAD_GRAYSCALE);
        }
        return img;
    }

    vector<GlyphMatch> InputTracker::match_templates(const Mat &roi,
                                                     const std::string &subdir,
                                                     double threshold,
                                                     int max_matches)
    {
        vector<GlyphMatch> results;

        // テンプレートディレクトリ内のファイルを走査（ここでは既知のファイル名を直接指定）
        // 実際にはディレクトリ走査を行うが、シンプルさのため既知のセットを使用
        vector<string> template_names;

        if (subdir == "dir")
        {
            template_names = {"1", "2", "3", "4", "6", "7", "8", "9"};
        }
        else if (subdir == "num")
        {
            template_names = {"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "F", "minus"};
        }
        else if (subdir == "btn_classic")
        {
            template_names = {"LP", "MP", "HP", "LK", "MK", "HK"};
        }
        else if (subdir == "btn_modern")
        {
            template_names = {"L", "M", "H", "SP", "AUTO", "DP"};
        }

        for (const auto &name : template_names)
        {
            Mat tmpl = load_template(subdir, name);
            if (tmpl.empty())
                continue;

            // テンプレートをグレースケールに変換
            Mat tmpl_gray, roi_gray;
            if (tmpl.channels() == 3)
                cvtColor(tmpl, tmpl_gray, COLOR_BGR2GRAY);
            else
                tmpl_gray = tmpl.clone();

            if (roi.channels() == 3)
                cvtColor(roi, roi_gray, COLOR_BGR2GRAY);
            else
                roi_gray = roi.clone();

            // テンプレートがROIより大きい場合はスキップ
            if (tmpl_gray.cols > roi_gray.cols || tmpl_gray.rows > roi_gray.rows)
                continue;

            Mat result;
            matchTemplate(roi_gray, tmpl_gray, result, TM_CCOEFF_NORMED);

            double min_val, max_val;
            Point min_loc, max_loc;
            minMaxLoc(result, &min_val, &max_val, &min_loc, &max_loc);

            if (max_val >= threshold)
            {
                GlyphMatch gm;
                gm.label = name;
                gm.confidence = max_val;
                gm.location = max_loc;
                results.push_back(gm);
            }
        }

        // 信頼度順にソート
        sort(results.begin(), results.end(),
             [](const GlyphMatch &a, const GlyphMatch &b)
             {
                 return a.confidence > b.confidence;
             });

        // 最大マッチ数に制限
        if (max_matches > 0 && (int)results.size() > max_matches)
        {
            results.resize(max_matches);
        }

        return results;
    }

    vector<GlyphMatch> InputTracker::match_directions(const Mat &roi, double threshold)
    {
        return match_templates(roi, "dir", threshold, 2); // 方向は最大2つまで（斜め入力など）
    }

    vector<GlyphMatch> InputTracker::match_buttons(const Mat &roi, double threshold,
                                                   const std::string &control_type)
    {
        if (control_type == "modern")
        {
            return match_templates(roi, "btn_modern", threshold, 3);
        }
        else
        {
            // classic または未判定の場合は classic を優先
            return match_templates(roi, "btn_classic", threshold, 3);
        }
    }

    int InputTracker::read_counter(const Mat &roi)
    {
        // カウンターは数字テンプレートで読み取り（簡易実装）
        auto matches = match_templates(roi, "num", 0.85, 1);
        if (!matches.empty())
        {
            // ラベルを数値に変換
            if (matches[0].label == "F")
                return -1; // F = 不明/フリーズ
            if (matches[0].label == "minus")
                return 0;
            try
            {
                return stoi(matches[0].label);
            }
            catch (...)
            {
                return -1;
            }
        }
        return -1;
    }

    ControlTypeResult InputTracker::detect_control_badge(const Mat &frame, double threshold)
    {
        ControlTypeResult result;
        result.control_type = "";
        result.confidence = 0.0;
        result.resolved = false;

        // 操作系バッジは画面上部付近に配置されていると仮定
        // 実際の位置はキャリブレーション結果に依存するが、ここでは簡易的に上部領域を検索
        Mat roi = frame.rowRange(0, frame.rows / 4);

        // §5.3: classic.png / modern.png は操作系分類器テンプレート（ボタングリフではない）
        // これらを明示的にマッチングして操作系を判定
        Mat classic_tmpl = load_template("btn_classic", "classic");
        Mat modern_tmpl = load_template("btn_modern", "modern");

        double classic_conf = 0.0;
        double modern_conf = 0.0;

        if (!classic_tmpl.empty())
        {
            Mat roi_gray, tmpl_gray;
            if (roi.channels() == 3)
                cvtColor(roi, roi_gray, COLOR_BGR2GRAY);
            else
                roi_gray = roi.clone();
            if (classic_tmpl.channels() == 3)
                cvtColor(classic_tmpl, tmpl_gray, COLOR_BGR2GRAY);
            else
                tmpl_gray = classic_tmpl.clone();

            if (tmpl_gray.cols <= roi_gray.cols && tmpl_gray.rows <= roi_gray.rows)
            {
                Mat result_mat;
                matchTemplate(roi_gray, tmpl_gray, result_mat, TM_CCOEFF_NORMED);
                double min_val, max_val;
                Point min_loc, max_loc;
                minMaxLoc(result_mat, &min_val, &max_val, &min_loc, &max_loc);
                classic_conf = max_val;
            }
        }

        if (!modern_tmpl.empty())
        {
            Mat roi_gray, tmpl_gray;
            if (roi.channels() == 3)
                cvtColor(roi, roi_gray, COLOR_BGR2GRAY);
            else
                roi_gray = roi.clone();
            if (modern_tmpl.channels() == 3)
                cvtColor(modern_tmpl, tmpl_gray, COLOR_BGR2GRAY);
            else
                tmpl_gray = modern_tmpl.clone();

            if (tmpl_gray.cols <= roi_gray.cols && tmpl_gray.rows <= roi_gray.rows)
            {
                Mat result_mat;
                matchTemplate(roi_gray, tmpl_gray, result_mat, TM_CCOEFF_NORMED);
                double min_val, max_val;
                Point min_loc, max_loc;
                minMaxLoc(result_mat, &min_val, &max_val, &min_loc, &max_loc);
                modern_conf = max_val;
            }
        }

        // §5.3: どちらも閾値未満の場合は control_type は null（空文字）
        if (classic_conf < threshold && modern_conf < threshold)
        {
            result.control_type = "";
            result.confidence = 0.0;
            result.resolved = false;
            return result;
        }

        // 両方検出された場合は信頼度の高い方を採用
        if (classic_conf >= modern_conf)
        {
            result.control_type = "classic";
            result.confidence = classic_conf;
        }
        else
        {
            result.control_type = "modern";
            result.confidence = modern_conf;
        }
        result.resolved = true;

        return result;
    }

    ControlTypeResult InputTracker::resolve_control_type(const Mat &frame, double threshold)
    {
        return detect_control_badge(frame, threshold);
    }

    void InputTracker::set_control_type(const std::string &player, const std::string &control_type)
    {
        if (player == "P1")
            control_type_p1_ = control_type;
        else if (player == "P2")
            control_type_p2_ = control_type;
    }

    InputRow InputTracker::read_input_row(const Mat &frame, int row_index, const std::string &player)
    {
        InputRow row;
        row.row_index = row_index;
        row.counter_value = -1;
        row.valid = false;

        // 19行スロットの各行のY位置を計算（入力履歴エリア: Y=0.170-0.820）
        // P1側: X=0.010-0.090, P2側: X=0.910-0.990
        int y_start = static_cast<int>(frame.rows * 0.170);
        int y_end = static_cast<int>(frame.rows * 0.820);
        int row_height = (y_end - y_start) / rows_visible;

        int row_y = y_start + row_index * row_height;
        if (row_y + row_height > y_end)
            return row;

        int x_start, x_end;
        if (player == "P1")
        {
            x_start = static_cast<int>(frame.cols * 0.010);
            x_end = static_cast<int>(frame.cols * 0.090);
        }
        else
        {
            x_start = static_cast<int>(frame.cols * 0.910);
            x_end = static_cast<int>(frame.cols * 0.990);
        }

        Rect roi_rect(x_start, row_y, x_end - x_start, row_height);
        if (roi_rect.x + roi_rect.width > frame.cols || roi_rect.y + roi_rect.height > frame.rows)
        {
            return row;
        }

        Mat roi = frame(roi_rect);

        // 方向キーとボタンをマッチング
        // プレイヤーごとに解決済みの操作系を使用（未解決の場合は classic をデフォルト）
        string control_type = "classic";
        if (player == "P1" && !control_type_p1_.empty())
            control_type = control_type_p1_;
        else if (player == "P2" && !control_type_p2_.empty())
            control_type = control_type_p2_;

        row.directions = match_directions(roi, input_glyph_threshold);
        row.buttons = match_buttons(roi, input_glyph_threshold, control_type);

        // カウンター読み取り（P1 の場合は counter スロットが最初、P2 は最後）
        // 簡易実装: 方向・ボタンが検出されれば有効とみなす
        row.valid = (!row.directions.empty() || !row.buttons.empty());

        return row;
    }

    bool InputTracker::build_segment(const InputRow &row, InputSegment &out)
    {
        // 状態マシン: 連続する同一入力をセグメント化
        // シグネチャ/カウンター状態マシン(§4) に基づく

        if (!row.valid)
        {
            // 無効行: セグメントを終了
            if (segment_active_)
            {
                out = current_segment_;
                segment_active_ = false;
                return true;
            }
            return false;
        }

        // 現在の行から入力を構築
        vector<string> dirs;
        for (const auto &d : row.directions)
        {
            dirs.push_back(d.label);
        }
        vector<string> btns;
        for (const auto &b : row.buttons)
        {
            btns.push_back(b.label);
        }

        if (!segment_active_)
        {
            // 新しいセグメントを開始
            current_segment_.frame_start = row.row_index; // 一時的に row_index を使用（後でフレームインデックスで上書き）
            current_segment_.frame_end = row.row_index;
            current_segment_.player = "P1"; // デフォルト
            current_segment_.round_index = 1;
            current_segment_.event_index = event_counter_++;
            current_segment_.directions = dirs;
            current_segment_.buttons = btns;
            current_segment_.hold_frames = 1;
            current_segment_.counter_clipped = false;
            current_segment_.hold_frames_lower_bound = false;
            current_segment_.time_dilation_suspected = false;
            current_segment_.resync_used = false;
            current_segment_.unknown_glyph = false;
            current_segment_.is_preround = false;
            current_segment_.quality_flags.clear();
            segment_active_ = true;
        }
        else
        {
            // 既存セグメントと比較
            bool same_input = (dirs == current_segment_.directions) && (btns == current_segment_.buttons);

            if (same_input)
            {
                // 継続
                current_segment_.frame_end = row.row_index;
                current_segment_.hold_frames++;
            }
            else
            {
                // 入力変更: 現在のセグメントを終了し、新しいセグメントを開始
                out = current_segment_;
                segment_active_ = false;

                current_segment_.frame_start = row.row_index;
                current_segment_.frame_end = row.row_index;
                current_segment_.directions = dirs;
                current_segment_.buttons = btns;
                current_segment_.hold_frames = 1;
                current_segment_.event_index = event_counter_++;
                current_segment_.quality_flags.clear();
                segment_active_ = true;

                return true; // 完了したセグメントを返す
            }
        }

        return false;
    }

} // namespace st6

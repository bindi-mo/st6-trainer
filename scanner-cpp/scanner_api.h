#ifndef SCANNER_API_H
#define SCANNER_API_H

#include <cstdint>

#ifdef _WIN32
#ifdef SCANNER_EXPORTS
#define SCANNER_API __declspec(dllexport)
#else
#define SCANNER_API __declspec(dllimport)
#endif
#else
#define SCANNER_API
#endif

#ifdef __cplusplus
extern "C"
{
#endif

    // ===== Version =====
    typedef struct
    {
        int major, minor, patch;
    } ScannerVersion;
    SCANNER_API ScannerVersion scanner_api_version();

    // ===== エラー =====
    SCANNER_API const char *scanner_get_last_error();

    // ===== 設定 =====
    typedef enum
    {
        CALIB_FAIL = 0,
        CALIB_ASK = 1
    } CalibFailureMode;

    typedef struct
    {
        const char *template_pack_path; // UTF-8, 呼び出し側所有
        const char *config_json;        // JSON 文字列（スキャナー設定）、呼び出し側所有
        CalibFailureMode on_calib_failure;
        int frame_stride;
        int hud_redetect_interval;
        int max_frames; // 0 = 全フレーム
    } ScanConfig;

    // ===== 不透明ハンドル =====
    typedef struct ScannerImpl *ScannerHandle;

    // ===== 出力構造体（POD、固定長配列） =====

    // MatchMetadata（1 回のみ）
    typedef struct
    {
        char match_id[64];
        char character_name_p1[32];
        char character_name_p2[32];
        char control_type_p1[16]; // "classic" / "modern" / ""
        char control_type_p2[16];
        int detected_at_frame;
        char source_resolution[32];
        double source_fps;
        int canonical_raster[2];
        int top_y_offset_px;
        int bottom_y_offset_px;
        int offset_snapped;  // 常に 0
        int manual_verified; // -1=null, 0=false, 1=true
        int input_history_available;
        char attack_property_locale[32]; // "en,ja" 等
        char extracted_at_utc[32];
        char config_version[32];
        char template_pack_version[32];
        char extractor_version[32];
    } MatchMetadata;

    // FrameTelemetry (60Hz each)
    typedef struct
    {
        int frame_index;
        int source_frame_index;
        int64_t timestamp_ms;
        int round_index;
        int hp_reset;
        double hp_p1, drive_p1, sa_p1;
        int in_burnout_p1;
        double hp_p2, drive_p2, sa_p2;
        int in_burnout_p2;
        char quality_flags[16][32];
        int quality_flags_count;
    } FrameTelemetry;

    // InputEvent（セグメント毎）
    typedef struct
    {
        int frame_start, frame_end;
        int frame_span_video;
        int hold_frames;
        int counter_clipped;
        int hold_frames_lower_bound;
        int time_dilation_suspected;
        int resync_used;
        int unknown_glyph;
        int is_preround;
        char player[4];
        int round_index;
        int event_index;
        char directions[8][8];
        int directions_count;
        char buttons[7][8];
        int buttons_count;
        char quality_flags[16][32];
        int quality_flags_count;
    } InputEvent;

    // MoveEvent（ラッチ済みブロック毎）
    typedef struct
    {
        int frame_start, frame_end;
        int round_index;
        int fm_startup_p1, fm_total_p1, fm_advantage_p1;
        int fm_startup_p2, fm_total_p2, fm_advantage_p2;
        int damage_p1, combo_damage_p1;
        int damage_p2, combo_damage_p2;
        char attack_property_p1[16];
        char attack_property_p2[16];
        char attack_property_locale[16];
        double conf;
        char quality_flags[16][32];
        int quality_flags_count;
    } MoveEvent;

    // ===== メイン API =====

    // video_path: UTF-8 ファイルパス。config: NULL 可（デフォルト値使用）
    // 戻り値: 成功時はハンドル、失敗時は NULL（scanner_get_last_error で詳細取得）
    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config);

    // キャリブレーション完了後、最初の telemetry 取得前に呼ぶ
    // 戻り値: 1=成功, 0=失敗（-1 は将来の拡張用に予約）
    SCANNER_API int scanner_get_metadata(ScannerHandle h, MatchMetadata *out);

    // 手動キャリブレーションオフセットを設定（§2.7 / §7.1 準拠）
    // top_y_offset_px / bottom_y_offset_px を手動で上書きし、manual_verified フラグを設定する
    // 戻り値: 1=成功, 0=失敗
    SCANNER_API int scanner_set_manual_offsets(ScannerHandle h, int top_y_offset_px, int bottom_y_offset_px, int manual_verified);

    // イテレータ: 1=成功, 0=終了, -1=エラー（scanner_get_last_error で詳細）
    SCANNER_API int scanner_next_telemetry(ScannerHandle h, FrameTelemetry *out);
    SCANNER_API int scanner_next_input_event(ScannerHandle h, InputEvent *out); // player は構造体内
    SCANNER_API int scanner_next_move_event(ScannerHandle h, MoveEvent *out);

    SCANNER_API void scanner_destroy(ScannerHandle h);

#ifdef __cplusplus
}
#endif

#endif // SCANNER_API_H
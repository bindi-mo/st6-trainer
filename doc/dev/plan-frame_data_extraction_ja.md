# SF6 Frame Data Extractor - 実装計画書

## 0. 前提・制約

### 0.1 ビルド系
- C++ スキャナー: `scanner-cpp/` (CMake/MSVC → `st6_scanner.dll`)
- アプリ本体: `src-tauri/` (Cargo/Tauri 2.x)
- 言語境界: C++ (OpenCV 画像処理) ⇄ Rust (Tauri/出力) は `scanner_api.h` で固定

### 0.2 現状の資産（2026-11 時点の実測）

| 項目 | 状態 |
|------|------|
| `scanner_api.h`（4構造体 + イテレータAPI） | **あり**（Phase 0 完了） |
| `scanner.cpp` | **スタブ**（固定値ダミー出力のみ） |
| `scanner_ffi.rs`（§7構造体マッピング） | **あり** |
| `st6_scanner.dll`（ビルド済み） | **あり**（`scanner-cpp/build/Release/` → `src-tauri/` へ POST_BUILD コピー） |
| テンプレート | **あり**（`src-tauri/templates/`） |
| `scanner_config.json` | **あり**（`src-tauri/config/`） |
| `app_config.json` | **あり**（`src-tauri/config/`、Phase 0 で作成） |
| `config.rs` | **あり**（Phase 0 で作成） |
| `pipeline.rs` / `commands.rs` | **あり**（Phase 4 で作成済み） |
| 検証用リプレイ動画 | **あり**（`src-tauri/videos/replay_00.mkv`） |
| CI | **あり**（`.github/workflows/ci.yml`: フロントテスト / `cargo test` / clippy / Tauri ビルド） |

### 0.3 決定事項（本計画の基準）

- **テンプレートは `src-tauri/templates/` に配置する**（`scanner-cpp/templates/` へは移動しない）
  - 理由: `scanner_ffi.rs::get_template_pack_path()` が `CARGO_MANIFEST_DIR/templates` を解決しており、Tauri の bundler がそのままリソースとして同梱できる
  - C++ 側は `ScanConfig::template_pack_path` を介してパスを受け取るため、配置場所に依存しない
- **設定ファイルは 2 系統に分離する**
  - `src-tauri/config/scanner_config.json`: スキャナー専用（アルゴリズムパラメータ・開発者編集のみ）
  - `src-tauri/config/app_config.json`: アプリ専用（実行環境・ユーザー設定・自動読み書き）
- **設定形式は JSON 統一**（`scanner_config.json`, `app_config.json`, `tauri.conf.json`）
- **手動キャリブレーション値は永続化しない**（`match_id` 衝突防止、仕様書 §2.7 / §7.1 準拠）
- **C ABI は POD のみ**（`std::string` / `std::vector` / クラス禁止、所有権は呼び出し側）
- **操作系バッジは `btn_classic` / `btn_modern` に含める**（`classic.png` / `modern.png`）

## 1. フェーズ構成（依存順）

| Phase | 名称 | 主タスク | 成果物 | 依存 | 状態 |
|-------|------|----------|--------|------|------|
| **0** | 環境・インターフェース固定 | `scanner_api.h` 拡張、CMake/Cargo 連携確認、設定分離実装 | 安定した FFI 境界、`scanner_config.json`, `app_config.json` | — | **完了** |
| **1** | テンプレート・設定準備 | アンカー/グリフ/数字/攻撃属性/操作系テンプレートの充足と棚卸し | `src-tauri/templates/`, `src-tauri/config/` | Phase 0 | **完了** |
| **2** | ラスタ正規化・キャリブレーション | `scanner.cpp`: 正規化(§6.1)、アンカーY探索・コンセンサス(§2)、fail-closed | `st6_scanner.dll` (v1) | Phase 0, 1 | **完了**（簡易方式・§2.5 参照） |
| **3** | 入力履歴追跡 | 19行スロット・slot0読み・シグネチャ/カウンター状態マシン(§4) | `input_events` 出力対応 | Phase 2 | **完了**（ビルド成功） |
| **4** | ゲージ・フレームメーター | HSVゲージ(§3.2, §5)、2ROIグループ・3フレームラッチ・攻撃属性(§3.3) | `frame_telemetry`, `move_events` | Phase 2 | **完了** |
| **5** | Rust FFI・出力パイプライン | `scanner_ffi.rs` 拡充、中間JSONL→SQLite 変換、インバリアントチェック | Tauri コマンド | Phase 2–4 | **完了** |
| **6** | 手動キャリブレーション UI | Tauri フロントエンド: 候補オフセット表示・±1pxステッパー・`manual_verified` フラグ(§2.7) | `sf6-gui` 機能 | Phase 1, 5 | **完了** |
| **7** | 統合・検証・リリース | 検証用リプレイでの E2E、§7.5 インバリアント違反ゼロ確認、エラーコード発火テスト | リリースビルド、テストレポート | Phase 0–6 | 未着手 |

## 2. Phase 0 — **完了**

Phase 0 の 4 タスクはすべて完了しています。以下は完了内容の記録です。

### 2.1 `scanner_api.h` 設計・合意 — **完了**
仕様書 §7 の 4 テーブル（`match_metadata`, `frame_telemetry`, `input_events`, `move_events`）を C 構造体化し、所有権・寿命・エラー伝播を定義済み。

- `ScanConfig` は `config_json`（JSON 文字列）を維持
- `scanner_next_*` 系の戻り値規約（`1`=成功 / `0`=終了 / `-1`=エラー）をヘッダコメントに明記
- `scanner_create` の戻り値（成功でハンドル、失敗で NULL）も明記

### 2.2 設定分離の実装 — **完了**

| 項目 | 状態 |
|------|------|
| `src-tauri/config/scanner_config.json` | スキャナー専用（現状維持） |
| `src-tauri/config/app_config.json` | 作成済み（§5.2 の内容） |
| `src-tauri/src/config.rs` | 作成済み（読み込み・検証・パス解決・JSON 生成） |

`Scanner::new()` は `config_json` が未指定なら `scanner_config.json` を自動読み込みする。

### 2.3 CMake / Cargo 連携確認 — **完了**
- CMake POST_BUILD で `st6_scanner.dll` を `src-tauri/` へコピー
- `build.rs` が DLL 存在をチェック（未ビルドなら panic で明示）
- `scanner_ffi.rs::get_dll_path()` が `resource_dir`（本番）と `CARGO_MANIFEST_DIR`（開発）の両方を解決
- `tauri.conf.json` の `bundle.resources` に `config/*.json` と `templates/**/*` を追加済み

### 2.4 最小スモークテスト — **完了**
`src-tauri/examples/smoke_test.rs` を実 DLL 呼び出しへ強化。

```
scanner_create → scanner_get_metadata → scanner_next_telemetry → scanner_destroy
```

`src-tauri/videos/replay_00.mkv` をフィクスチャとして使用。動画が無い環境では `[3] SKIP` として degrade し、CI でも落ちない。

## 2A. Phase 2 — **完了**（簡易キャリブレーション方式）

Phase 2 は「ラスタ正規化」と「キャリブレーション」を実装した。ただし、アンカーテンプレートマッチング（§2.3–§2.5 の本来方式）は実データで信頼度が閾値（0.90）に届かなかったため、**Phase 2 では簡易方式（HUD 存在検出＋色ベース）を採用**し、正確な Y オフセット確定は Phase 3–4 の実ゲージ検出時に持ち越す方針とした。

### 2A.1 ラスタ正規化（§6.1） — **完了**

`scanner-cpp/raster_normalizer.{h,cpp}` を新規作成。

- 入力フレームを **canonical raster 1920x1080** に正規化
- レターボックス/ピラーボックス除去（`detect_content_region`）
  - 現状は「全フレームをコンテンツ領域として扱う」簡易実装（実データは 1920x1080 全面）
- アスペクト比ゲート: 16:9 から **0.010 超**で `E_ASPECT_UNSUPPORTED`
- 等方性ゲート: スケール差 **0.005 超**で `E_ANISOTROPIC_SCALE`
- 黒フレーム検出: `max_val < 10` で `E_BLACK_FRAME`（呼び出し側でスキップ）

### 2A.2 キャリブレーション — **完了（簡易方式）**

`scanner.cpp::perform_calibration()` に実装。

- **キャリブレーション用フレームの選定**
  - 動画先頭は「リプレイデータ読み込み」「対戦相手演出」で黒画面が続く
  - **対戦開始以降**のフレームを走査し、上部 HUD（Y=0–270）と下部 HUD（Y=756–1080）の両方にコンテンツ（`max_val > 100`）がある最初のフレームを採用
- **HUD 存在検出（HSV 色マスク）**
  - 上部 25% 領域の緑（HP ゲージ）: `H∈[40,180], S,V∈[50,255]`
  - 下部 30% 領域の青（SA ゲージ）: `H∈[100,130], S,V∈[50,255]`
  - 両方の平均が 0.01 未満なら `E_HUD_ABSENT`
- **Y オフセット**
  - 現状は `top_y_offset_px = 0`, `bottom_y_offset_px = 0`（既定値）
  - 信頼度は暫定 0.8。Phase 3–4 の実ゲージ検出で精緻化する

### 2A.3 アンカーテンプレートマッチング（本来方式・保留）

`scanner-cpp/calibration.{h,cpp}` を実装済みだが、Phase 2 では**未使用（将来の拡張用に保持）**。

- アンカー: upper 5 種（timer top edge / P1・P2 HP corner / P1・P2 drive corner）、lower 3 種（P1・P2 SA bracket / frame meter top edge）
- `TM_CCOEFF_NORMED` による 2D マッチング、prominence ゲート、median コンセンサス（§2.5 準拠）
- **実データでの結果**: 最大信頼度 0.18–0.44 にとどまり、閾値 0.90 に未達
  - 原因: テンプレートと実フレームの HUD 描画が一致しない（テンプレート品質/切り出し位置の問題）
  - 対応: Phase 1 のテンプレート再生成、または Phase 3–4 の実検出で代替

### 2A.4 黒フレームのスキップ — **完了**

`scanner_next_telemetry()` に実装。

- 各フレームの `max_val < 10` を黒と判定し、テレメトリを出力せず次フレームへ進む
- これにより冒頭の黒画面区間で `E_BLACK_FRAME` エラーにならず、全フレームを走査可能

### 2A.5 ビルド・検証 — **完了**

- `CMakeLists.txt` に `calibration.cpp` / `raster_normalizer.cpp` を追加
- `scanner_api_version()` を **0.2.0** に更新
- 例外境界: `scanner_create` / `scanner_get_metadata` / `scanner_next_telemetry` を `try/catch` で保護し、C++ 例外が FFI 境界を越えないようにした
- **スモークテスト結果**: `SMOKE TEST PASSED`（フレーム走査、`match_id=match-0-0`）

### 2A.6 既知の制約・Phase 3–4 への申し送り

| 項目 | 現状 | 申し送り |
|------|------|----------|
| Y オフセット | 0 固定（暫定） | Phase 3–4 で実ゲージ位置から確定 |
| アンカーマッチング | 信頼度不足で未使用 | テンプレート再生成 or 実検出で代替 |
| ラウンド検出 | 実装済み（Phase 5） | ラウンド開始（Round ? / Final Round）+ ラウンド終了（K.O. / TIME OVER）+ タイマー（99秒）+ HP リセット補完から検出（§4.5, §7.2） |
| レターボックス除去 | 全面＝コンテンツ扱い | 非 16:9 入力対応時に実装 |
| キャラ名・操作系 | 空文字（スタブ） | Phase 3 で実装 |

## 2B. Phase 3 — **完了**（入力履歴追跡）

Phase 3 は「入力履歴追跡」を実装した。`scanner_next_input_event()` により、19行スロットから P1/P2 の入力イベントを抽出する。

### 2B.1 新規ファイル

| ファイル | 内容 |
|----------|------|
| `scanner-cpp/input_tracker.h` | `InputTracker` クラス、`InputRow`/`InputSegment`/`ControlTypeResult`/`GlyphMatch` 構造体定義 |
| `scanner-cpp/input_tracker.cpp` | テンプレートマッチング（方向 1-9、ボタン classic/modern、カウンター数字）、操作系バッジ検出、状態マシンによるセグメント構築 |

### 2B.2 `scanner.cpp` への変更

- `scanner_api_version()` を **0.3.0** に更新（Phase 3）
- `ScannerImpl` に `unique_ptr<InputTracker> input_tracker`、`bool input_tracker_initialized`、`int input_frame_counter` を追加
- `scanner_get_metadata()` で `InputTracker` を初期化（`set_template_pack_path`、`rows_visible`、`input_glyph_threshold` を設定）
- `scanner_next_input_event()` を新規実装：
  1. フレーム読み込み・黒フレームスキップ・正規化
  2. 最初の有効フレームで `resolve_control_type()` を P1/P2 それぞれ実行（操作系判定）
  3. 19行スロットを P1→P2 の順でスキャンし、`read_input_row()` + `build_segment()` で入力セグメントを構築
  4. `InputEvent` 構造体に方向・ボタン・品質フラグ等をコピーして出力

### 2B.3 `InputTracker` の実装詳細

- **方向テンプレートマッチング**: `dir/` ディレクトリの `1`〜`9`（`5` 除く）を `TM_CCOEFF_NORMED` でマッチ、最大 2 マッチ（斜め入力対応）
- **ボタンテンプレートマッチング**: 操作系に応じて `btn_classic`（`LP/MP/HP/LK/MK/HK`）または `btn_modern`（`L/M/H/SP/AUTO/DP`）を使用、最大 3 マッチ
- **カウンター読み取り**: `num/` ディレクトリの数字テンプレート、`F`=不明、`-`=0
- **操作系バッジ検出**: 上部 25% 領域で `classic.png` / `modern.png` をマッチングし、信頼度の高い方を採用
- **状態マシン**: 連続する同一入力をセグメント化し、`frame_start`/`frame_end`/`hold_frames` 等を算出

### 2B.4 ビルド・検証

- `CMakeLists.txt` に `input_tracker.cpp` を追加
- `scanner_api_version()` を **0.3.0** に更新
- `scanner.cpp` に `using namespace st6;` を追加（`InputTracker` 等が `st6` 名前空間にあるため）
- `scanner_next_input_event()` の重複/早期クロージングバグを修正
- **ビルド結果**: `st6_scanner.dll` が正常にビルド成功（エラーなし、C4819 警告のみ）

### 2B.5 既知の制約・Phase 4 への申し送り

| 項目 | 現状 | 申し送り |
|------|------|----------|
| Y オフセット | 0 固定（暫定） | Phase 4 で実ゲージ位置から確定 |
| 操作系判定 | 最初の有効フレームで 1 回のみ | 途中で操作系が変わるケースは未対応 |
| ラウンド検出 | 実装済み（Phase 5） | ラウンド開始（Round ? / Final Round）+ ラウンド終了（K.O. / TIME OVER）+ タイマー（99秒）+ HP リセット補完から検出（§4.5, §7.2） |
| キャラ名 | 空文字（スタブ） | Phase 4 で実装 |
| セグメントの frame_start/frame_end | row_index を暫定使用 | 実際のフレームインデックスで上書きが必要 |

## 2C. Phase 4 — **完了**（ゲージ・フレームメーター）

Phase 4 は「ゲージ・フレームメーター」を実装した。HSV 色マスクベースで HP/Drive/SA ゲージを検出し、3フレームラッチ付きフレームメーター検出を実装した。

### 2C.1 新規ファイル

| ファイル | 内容 |
|----------|------|
| `scanner-cpp/gauge.h` | `GaugeDetector` クラス、`GaugeReading`/`FrameGauges`/`GaugeConfig` 構造体定義 |
| `scanner-cpp/gauge.cpp` | HSV マスクによるゲージ充填率検出、バーンアウト判定、遮蔽判定 |
| `scanner-cpp/frame_meter.h` | `FrameMeterDetector` クラス、`FrameMeterReading`/`FrameMeterConfig` 構造体定義 |
| `scanner-cpp/frame_meter.cpp` | 3フレームラッチ付きフレームメーター検出、テンプレートマッチング準備 |

### 2C.2 `scanner.cpp` への変更

- `scanner_api_version()` を **0.4.0** に更新（Phase 4）
- `ScannerImpl` に `unique_ptr<GaugeDetector> gauge_detector`、`bool gauge_detector_initialized`、`unique_ptr<FrameMeterDetector> frame_meter_detector`、`bool frame_meter_detector_initialized` を追加
- `scanner_get_metadata()` で `GaugeDetector` と `FrameMeterDetector` を初期化（HSV 閾値、ラッチフレーム数等を設定）
- `scanner_next_telemetry()` で実ゲージ検出を統合：
  - `GaugeDetector::detect()` で P1/P2 の HP/Drive/SA ゲージ値を読み取り
  - `FrameTelemetry` の `hp_p1`, `drive_p1`, `sa_p1`, `in_burnout_p1`, `hp_p2`, `drive_p2`, `sa_p2`, `in_burnout_p2` に設定
  - 遮蔽ゲージに対して品質フラグ（`HP_P1_OCCLUDED` 等）を出力
- `scanner_next_move_event()` を新規実装：
  - `FrameMeterDetector::detect()` で 3フレームラッチ付きフレームメーターを検出
  - `MoveEvent` の `fm_startup_p1/fm_total_p1/fm_advantage_p1` 等に設定
  - 未検出時はデフォルト値（0）を出力しイテレーション継続

### 2C.3 `GaugeDetector` の実装詳細

- **HSV 色マスク**: OpenCV HSV（H 0-179, S 0-255, V 0-255）でゲージ色を検出
  - HP ゲージ: `H∈[40,90], S∈[50,255], V∈[50,255]`（緑/黄）
  - Drive ゲージ: `H∈[15,35], S∈[100,255], V∈[100,255]`（オレンジ）
  - SA ゲージ: `H∈[100,130], S∈[50,255], V∈[50,255]`（青）
- **充填率計算**: 左から右への連続白ピクセル列数を測定し、ゲージ幅で正規化
- **バーンアウト判定**: HP ゲージ領域の赤色ピクセル（`H∈[0,10]` または `H∈[170,179]`）が閾値超過でバーンアウトと判定
- **遮蔽判定**: 白ピクセル数が `gauge_occluded_threshold` 未満なら遮蔽とみなす
- **SA ゲージ**: 0-3 段階に量子化（`round(value * 3) / 3`）

### 2C.4 `FrameMeterDetector` の実装詳細

- **3フレームラッチ**: 同一読み取り（startup/active/recovery/advantage）が `fm_latch_frames`（3）フレーム連続で確定した場合のみ出力
- **未検出時**: `detected = false` を返し、呼び出し側でデフォルト値を出力
- **テンプレートマッチング**: `num/` ディレクトリの数字テンプレートを流用（Phase 4 では簡易実装、将来精緻化予定）

### 2C.5 ビルド・検証

- `CMakeLists.txt` に `gauge.cpp` / `frame_meter.cpp` を追加
- `scanner_api_version()` を **0.4.0** に更新
- **スモークテスト結果**: `SMOKE TEST PASSED`（7230 フレームのテレメトリ読み取り成功）
- **Rust テスト**: `cargo test` 12 件すべて成功（pipeline.rs の不変条件チェック含む）

### 2C.6 Rust パイプライン（Phase 4 成果物）

| ファイル | 内容 |
|----------|------|
| `src-tauri/src/pipeline.rs` | JSONL→SQLite 変換、§7.5 インバリアントチェック（入力テーブル項 1–5、スキーマ項 6–8） |
| `src-tauri/src/commands.rs` | Tauri コマンド `scan_video`（動画スキャン実行）、`scanner_version`（API バージョン取得） |
| `src-tauri/src/lib.rs` | `commands`, `pipeline` モジュール登録、コマンドハンドラ追加 |

### 2C.7 既知の制約・Phase 5 への申し送り — **Phase 5 完了**

| 項目 | 現状 | 対応 |
|------|------|------|
| Y オフセット | 0 固定（暫定） | **Phase 5 で実ゲージ位置から確定**（`detect_y_offsets()`） |
| ラウンド検出 | 実装済み（Phase 5） | ラウンド開始（Round ? / Final Round）+ ラウンド終了（K.O. / TIME OVER）+ タイマー（99秒）+ HP リセット補完から検出（`scanner_next_telemetry()`） |
| キャラ名 | 空文字（スタブ） | **Phase 5 で `names/` テンプレートからマッチング** |
| フレームメーター数字読み取り | 簡易実装（未検出扱い） | Phase 5 でテンプレートマッチング精緻化（未対応、Phase 6 へ持ち越し） |
| SQLite 書き込み | rusqlite 未導入のため JSONL サマリファイル出力 | **Phase 5 で rusqlite 追加と本格 SQLite 書き込み** |

## 2D. Phase 5 — **完了**

Phase 5 は「Rust FFI・出力パイプライン」を完了した。以下は完了内容の記録です。

### 2D.1 C++ スキャナー変更（`scanner.cpp`）

- `scanner_api_version()` を **0.5.0** に更新（Phase 5）
- **Y オフセット検出**（`detect_y_offsets()`）: 最初の有効フレームから HSV 色マスクで HP ゲージ（上部）と SA ゲージ（下部）の実際の Y 位置を検出し、`top_y_offset_px` / `bottom_y_offset_px` を設定
- **キャラクター名検出**（`match_character_name()`）: `src-tauri/templates/names/` のテンプレート（`aki.png`, `alex.png` 等）と `TM_CCOEFF_NORMED` でマッチングし、`character_name_p1` / `character_name_p2` を設定
- **ラウンド検出**: 99秒カウントダウンタイマー（5940フレーム @60fps）をベースにラウンドリセットを検出し、`round_index` をインクリメント、`hp_reset` フラグを設定。画面中央の「Round ? / Final Round」ロゴをテンプレートマッチングで検出し、最優先でラウンド開始と判定。K.O. / TIME OVER ロゴも検出し、ラウンド終了として記録（`round_index` は変更しない）。HP が低値（<0.3）から高値（>0.7）にジャンプした場合は補完的に検出。最小間隔（180フレーム）で誤検出を防止
- `opencv2/imgcodecs.hpp` をインクルード（`imread` 用）

### 2D.2 Rust FFI 拡充（`scanner_ffi.rs`）

- `init_scanner()` 共通初期化ロジックを抽出し、`Scanner::new()` で使用

### 2D.3 パイプライン（`pipeline.rs`）

- `rusqlite` 依存を `Cargo.toml` に追加（`bundled` 機能）
- `write_sqlite()` を実装: `frame_telemetry`, `input_events`, `move_events`, `match_metadata` テーブルを作成し、レコードを挿入
- `JsonlFrameTelemetry`, `JsonlInputEvent`, `JsonlMoveEvent` 構造体に欠落フィールドを追加し、`#[serde(default)]` で部分デシリアライズに対応
- JSONL サマリファイル出力から本格 SQLite 書き込みへ移行

### 2D.4 ビルド・テスト

- C++ DLL: `st6_scanner.dll` が正常にビルド成功（C4819 警告のみ、エラーなし）
- Rust: `cargo build` 成功、`cargo test` 12 件すべて成功
- `video_types.rs` を削除（未使用・旧型）

### 2D.6 既知の制約・Phase 6 への申し送り

| 項目 | 現状 | 申し送り |
|------|------|----------|
| フレームメーター数字読み取り | 簡易実装（未検出扱い） | Phase 6 でテンプレートマッチング精緻化 |
| キャラ名マッチング | `names/` に `aki.png`, `alex.png` のみ | 他キャラテンプレートの追加が必要 |
| 操作系判定 | 空文字（スタブ） | Phase 6 で実装 |
| 手動キャリブレーション UI | 未実装 | Phase 6 で Tauri フロントエンド実装 |

## 2E. Phase 6 — **完了**

Phase 6 は「手動キャリブレーション UI」を完了した。以下は完了内容の記録です。

### 2E.1 フレームメーター数字読み取りの精緻化（`frame_meter.cpp`）

- `read_digit()` を実装: `num/` ディレクトリの数字テンプレート（0-9, F, minus）を `TM_CCOEFF_NORMED` でマッチングし、閾値 `config_.threshold`（0.90）以上で検出
- `read_meter_block()` を更新: フレームメーター表示エリアを 4 ブロック（startup/active/recovery/advantage）に分割し、各ブロックから数字を読み取り
- すべての数字が検出された場合のみ `detected = true`、それ以外は `detected = false`
- `set_template_pack_path()` を追加し、`num/` テンプレートパスを設定可能に
- `scanner.cpp` で `frame_meter_detector->set_template_pack_path()` を呼び出し

### 2E.2 操作系判定の実装（`input_tracker.cpp`）

- `detect_control_badge()` を §5.3 準拠に再実装: `btn_classic/classic.png` と `btn_modern/modern.png` を操作系分類器テンプレートとして明示的にマッチング
- どちらも閾値未満の場合は `control_type` を空文字（null）として出力
- 個別のボタングリフ（LP/HP/LK/HK / L/M/H/SP など）は分類器テンプレートから除外
- `set_control_type()` を追加し、解決済みの操作系をプレイヤーごとに保存
- `read_input_row()` で保存済みの操作系を使用してボタングリフをマッチング
- `scanner.cpp` で `scanner_get_metadata()` 内で操作系を解決し、`MatchMetadata.control_type_p1/p2` に設定

### 2E.3 手動キャリブレーション C API（`scanner_api.h` / `scanner.cpp`）

- `scanner_set_manual_offsets()` を追加: `top_y_offset_px` / `bottom_y_offset_px` を手動上書きし、`manual_verified` フラグを設定
- 戻り値: 1=成功 / 0=失敗
- `MatchMetadata.manual_verified` は -1=null / 0=false / 1=true

### 2E.4 Rust FFI + Tauri コマンド（`scanner_ffi.rs` / `commands.rs`）

- `Scanner::set_manual_offsets()` メソッドを追加: C API `scanner_set_manual_offsets` をラップ
- `set_manual_offsets` Tauri コマンドを追加: フロントエンドから手動オフセット保存を呼び出し
- `lib.rs` でコマンドを登録

### 2E.5 フロントエンド手動キャリブレーション UI（`App.tsx` / `App.css`）

- `ScanMetadata` インターフェースを追加: `match_id`, `character_name_p1/p2`, `control_type_p1/p2`, `top_y_offset_px`, `bottom_y_offset_px`, `manual_verified` 等
- メタデータ表示セクションを追加: スキャン結果のメタデータを一覧表示
- 手動キャリブレーションセクションを追加:
  - Top Y Offset / Bottom Y Offset の ±1px ステッパー
  - 「Save Manual Offsets (Verify)」ボタンで `set_manual_offsets` コマンドを呼び出し
  - `manual_verified === true` 時に成功メッセージを表示
- キャリブレーション保存中はステッパーとボタンを無効化

### 2E.6 キャラ名テンプレートの追加（`templates/names/`）

- `aki.png`, `alex.png` に加え、26 キャラクターテンプレートを追加: ryu, ken, chunli, luke, jamie, kimberly, marisa, jp, deejay, manon, zangief, blanka, cammy, guile, honda, akuma, ed, rashid, lily, bison, vega, sagat, mika, abigail, falke, sakura, terry

### 2E.7 ビルド・検証

- C++ DLL: `st6_scanner.dll` が正常にビルド成功
- Rust: `cargo build` 成功、`cargo test` 12 件すべて成功
- フロントエンド: TypeScript コンパイル成功

## 3. リスクと対策（リポジトリ固有）

| リスク | 影響 | 対策 | 現状 |
|--------|------|------|------|
| テンプレートの網羅性不足 | Phase 2–4 の精度低下 | Phase 1 で棚卸し・不足分を収集 | `anchor` / `dir` / `num` / `names` / `btn_classic` / `btn_modern` / `attack_property/EN` は存在。**`attack_property` は現状 EN のみ**（他ロケールは今後の予定） |
| **アンカーテンプレートが実フレームと不一致** | キャリブレーション失敗（信頼度 0.18–0.44） | Phase 2 は簡易方式（HUD 色検出）で回避。テンプレート再生成を Phase 3–4 で検討 | **顕在化**（Phase 2 で判明） |
| C++ / Rust 境界で ABI 不整合 | クラッシュ・メモリリーク | `scanner_api.h` を **C 互換 POD のみ**に制限。`std::string` 等禁止。所有権は呼び出し側 | 準拠済み |
| OpenCV 版違い (4.12.0 固定) | ビルド失敗・挙動差 | `scanner-cpp/opencv-build/` を**ベンダリング継続**。CMake で `find_package` せずローカル参照 | 準拠済み |
| 検証動画の不足 | 受け入れテスト不可 | `src-tauri/videos/replay_00.mkv` を継続利用。不足時は合成動画で補完 | **解消済み**（1 本所有） |
| Tauri 2.x 移行未完了 | GUI 機能遅延 | Phase 6 で着手 | 移行済み。`scan_video` コマンドは Phase 4 で実装済み |
| フロントとバックエンドの不整合 | GUI からスキャナを起動できない | `src/App.tsx` は `invoke("scan_video")` を呼ぶが `lib.rs` に未実装 | **要対応**（Phase 5 で解消、`scan_video` は Phase 4 で実装済み） |

## 4. ディレクトリ構成（目標）

```
st6-trainer/
├── scanner-cpp/
│   ├── CMakeLists.txt
│   ├── scanner_api.h            # 仕様書§7準拠（4構造体 + イテレータAPI）
│   ├── scanner.cpp              # Phase 2 で全面書き換え（正規化・簡易キャリブレーション）
│   ├── raster_normalizer.cpp/h  # Phase 2 で新規（§6.1 ラスタ正規化）
│   ├── calibration.cpp/h        # Phase 2 で新規（アンカーマッチング・将来拡張用）
│   ├── input_tracker.cpp/h      # 新規（Phase 3）
│   ├── gauge.cpp/h              # 新規（Phase 4）
│   └── frame_meter.cpp/h        # 新規（Phase 4）
├── src-tauri/
│   ├── config/
│   │   ├── scanner_config.json  # スキャナー専用（現状維持）
│   │   └── app_config.json      # アプリ専用（Phase 0 で作成済み）
│   ├── templates/               # テンプレート（現状維持・移動しない）
│   │   ├── anchor/
│   │   ├── dir/
│   │   ├── num/
│   │   ├── names/
│   │   ├── btn_classic/         # classic.png = 操作系バッジ
│   │   ├── btn_modern/          # modern.png = 操作系バッジ
│   │   └── attack_property/
│   │       └── EN/              # 他ロケールは今後の予定
│   ├── videos/
│   │   └── replay_00.mkv        # 検証用リプレイ
│   ├── src/
│   │   ├── scanner_ffi.rs       # §7構造体マッピング（現状維持 + 拡充）
│   │   ├── config.rs            # Phase 0 で作成済み（設定読み込み・検証）
│   │   ├── pipeline.rs          # 新規 (JSONL→SQLite, インバリアントチェック)
│   │   ├── commands.rs          # 新規 (Tauri コマンド)
│   │   ├── video_types.rs       # 旧型。Phase 5 で削除または再設計
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── examples/
│   │   └── smoke_test.rs        # Phase 0 で実 DLL 呼び出しへ強化済み
│   ├── Cargo.toml
│   └── tauri.conf.json
└── tests/
    ├── fixtures/                # 検証用リプレイ・期待値 JSON
    └── integration/
```

## 5. 設定ファイル仕様（JSON 統一）

### 5.1 `src-tauri/config/scanner_config.json`（スキャナー専用・開発者編集のみ）
```json
{
  "coordinate_space": "canonical",
  "canonical_raster": [1920, 1080],
  "fps": 60.0,
  "rows_visible": 19,
  "fm_latch_frames": 3,
  "gauge_occluded_threshold": 10,
  "calibration": {
    "upper_band_y": [0.00, 0.25],
    "lower_band_y": [0.70, 1.00],
    "search_x_tolerance_px": 32,
    "confidence_threshold": 0.90,
    "prominence_threshold": 0.10,
    "max_dx_px": 2,
    "consensus_survival_min": 0.80,
    "consensus_vote_window_px": 2
  },
  "template_thresholds": {
    "display": 0.85,
    "calibration": 0.90,
    "frame_meter": 0.90,
    "input_glyph": 0.92,
    "attack_property": 0.92
  },
  "input": {
    "row_slots_p1": ["counter", "direction", "buttons"],
    "row_slots_p2": ["buttons", "direction", "counter"],
    "output_order": ["directions", "buttons"]
  },
  "attack_property_locale": ["en"],
  "template_pack_version": "sf6-ui-2026.11",
  "config_version": "cfg-1.0"
}
```

### 5.2 `src-tauri/config/app_config.json`（アプリ専用・自動読み書き・スキーマ流動的）
```json
{
  "last_input_video": "",
  "default_output_dir": "./output",
  "on_calib_failure": "fail",
  "log_level": "info",
  "num_threads": 4,
  "use_gpu": true,
  "scanner_config_path": "src-tauri/config/scanner_config.json",
  "template_pack_path": "src-tauri/templates"
}
```
※ 手動キャリブレーション値（`manual_calibration_overrides`）は**含めない**。仕様書 §2.7 / §7.1 により毎回確認が必要なため。

### 5.3 操作系（クラシック/モダン）判定 — 仕様書 §3.4 / §8.2.2

対戦中の HUD には、各プレイヤーがクラシック操作かモダン操作かを示す**操作系バッジ**が表示される。これが `control_type_p1` / `control_type_p2` の唯一の権威情報源であり、button グリフの汎用マッチングより**前に**解決する。

- 判定理由: `L`（モダン）は `LK`（クラシック）の接頭辞であり、セットをまたいだマッチングは自信を持って誤ラベルを返す
- テンプレート: `src-tauri/templates/btn_classic/classic.png` と `src-tauri/templates/btn_modern/modern.png`
- しきい値: `template_thresholds.input_glyph`（0.92）
- どちらも 0.92 未満の場合: `control_type` は `null`、全 button グリフを `"?"` + `unknown_glyph` として出力
- 方向・カウンター・数字テンプレートは操作系で共有

**実装上の注意**: `classic` / `modern` は**セットの分類器**であり、ボタングリフではない。グリフ配列を構築する際はこの 2 ファイルを明示的に除外する。

Phase 2（キャリブレーション）と Phase 3（入力追跡）の間の初期化パスで、キャラ名と同一の走査で 1 回だけ解決する。

## 6. `scanner_api.h` 設計指針（C ABI 安定化）

- **POD 構造体のみ**（`std::string`, `std::vector`, クラス禁止）
- 所有権: **呼び出し側 (Rust) が確保・解放**。DLL はポインタを書き込むだけ
- エラー: `int` 返り値 + `scanner_get_last_error()` で文字列取得
- 文字列: `const char*` (UTF-8)、寿命は呼び出し側管理
- バージョニング: `scanner_api_version()` で `major.minor.patch` 返却
- イテレータ規約: `scanner_next_*` は `1`=成功 / `0`=終了 / `-1`=エラー

主な構造体（仕様書 §7 準拠）:
```c
typedef struct { int frame_index; int64_t timestamp_ms; double hp_p1; ... } FrameTelemetry;
typedef struct { int frame_start; int frame_end; char directions[64][8]; int directions_count; ... } InputEvent;
typedef struct { int frame_start; int frame_end; int fm_startup_p1; ... } MoveEvent;
typedef struct { char match_id[64]; char character_name_p1[32]; ... } MatchMetadata;
```

詳細設計は Phase 0 タスク 2.1 で確定。

---

## 付録 A. `match_id` ハッシュ素材（仕様書 §7.1 / 付録B）

```
match_id = SHA256(
  video_content_hash +
  canonical_raster +
  config_version +
  template_pack_version +
  top_y_offset_px +
  bottom_y_offset_px +
  control_type_p1 +
  control_type_p2 +
  extractor_version
)
```

手動オフセットで `top_y_offset_px` / `bottom_y_offset_px` が変更された場合、ハッシュ値が変化し、従来の誤レコードと混同されない。

---

## 付録 B. エラーコード（仕様書 §7 準拠）

| エラーコード | 意味 | 発生条件 |
|-------------|------|----------|
| `E_CALIB_AMBIGUOUS` | キャリブレーション曖昧 | コンセンサス生存割合 $< 0.80$ または分離クラスター 2 つ |
| `E_CALIB_LOW_CONF` | 低信頼度キャリブレーション | 手動確認なしで平均アンカー信頼度 $< 0.90$ |
| `E_HUD_ABSENT` | HUD 不在 | クリーンフレームなし かつ 入力グリフも読めない（両条件） |
| `E_ROI_OUT_OF_BOUNDS` | ROI 範囲外 | 融合後、ROI がラスタ外 |
| `E_ROI_GROUP_OVERLAP` | グループ重複 | 上部と下部グループが重複 |
| `E_ASPECT_UNSUPPORTED` | アスペクト比非対応 | コンテンツアスペクトが 16:9 から 0.010 超 |
| `E_ANISOTROPIC_SCALE` | 非同軸スケール | 同軸性が 0.005 超 |
| `E_VFR_UNSUPPORTED` | 可変フレームレート非対応 | タイムラインが 60 Hz 論理フレームにマッピング不可 |
| `E_INPUT_INVARIANT` | 入力テーブル不変条件違反 | §7.5 項 1–5 |
| `E_SCHEMA_INVARIANT` | スキーマ不変条件違反 | §7.5 項 6–8 |
| `ATTACK_PROPERTY_UNSUPPORTED` | 攻撃属性ロケール未対応 | 読み取り単語がロード済みロケールに属さない |
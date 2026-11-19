# SF6 Frame Data Extractor - 実装計画書

## 0. 前提・制約

### 0.1 ビルド系
- C++ スキャナー: `scanner-cpp/` (CMake/MSVC → `st6_scanner.dll`)
- アプリ本体: `src-tauri/` (Cargo/Tauri 2.x)
- 言語境界: C++ (OpenCV 画像処理) ⇄ Rust (Tauri/CLI/出力) は `scanner_api.h` で固定

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
| `pipeline.rs` / `commands.rs` | **未作成** |
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
| **2** | ラスタ正規化・キャリブレーション | `scanner.cpp`: 正規化(§6.1)、アンカーY探索・コンセンサス(§2)、fail-closed | `st6_scanner.dll` (v1) | Phase 0, 1 | 未着手 |
| **3** | 入力履歴追跡 | 19行スロット・slot0読み・シグネチャ/カウンター状態マシン(§4) | `input_events` 出力対応 | Phase 2 | 未着手 |
| **4** | ゲージ・フレームメーター | HSVゲージ(§3.2, §5)、2ROIグループ・3フレームラッチ・攻撃属性(§3.3) | `frame_telemetry`, `move_events` | Phase 2 | 未着手 |
| **5** | Rust FFI・出力パイプライン | `scanner_ffi.rs` 拡充、中間JSONL→SQLite 変換、インバリアントチェック | `sf6-extract` CLI / Tauri コマンド | Phase 2–4 | 30% |
| **6** | 手動キャリブレーション UI | Tauri フロントエンド: 候補オフセット表示・±1pxステッパー・`manual_verified` フラグ(§2.7) | `sf6-gui` 機能 | Phase 1, 5 | 未着手 |
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

## 3. リスクと対策（リポジトリ固有）

| リスク | 影響 | 対策 | 現状 |
|--------|------|------|------|
| テンプレートの網羅性不足 | Phase 2–4 の精度低下 | Phase 1 で棚卸し・不足分を収集 | `anchor` / `dir` / `num` / `names` / `btn_classic` / `btn_modern` / `attack_property/EN` は存在。**`attack_property` は現状 EN のみ**（他ロケールは今後の予定） |
| C++ / Rust 境界で ABI 不整合 | クラッシュ・メモリリーク | `scanner_api.h` を **C 互換 POD のみ**に制限。`std::string` 等禁止。所有権は呼び出し側 | 準拠済み |
| OpenCV 版違い (4.12.0 固定) | ビルド失敗・挙動差 | `scanner-cpp/opencv-build/` を**ベンダリング継続**。CMake で `find_package` せずローカル参照 | 準拠済み |
| 検証動画の不足 | 受け入れテスト不可 | `src-tauri/videos/replay_00.mkv` を継続利用。不足時は合成動画で補完 | **解消済み**（1 本所有） |
| Tauri 2.x 移行未完了 | GUI 機能遅延 | Phase 6 は **CLI 完成後**に着手。Phase 5 までヘッドレスで完結 | 移行済み。ただし `lib.rs` の `scan_video` がコメントアウトのまま |
| フロントとバックエンドの不整合 | GUI からスキャナを起動できない | `src/App.tsx` は `invoke("scan_video")` を呼ぶが `lib.rs` に未実装 | **要対応**（Phase 5 で解消） |

## 4. ディレクトリ構成（目標）

```
st6-trainer/
├── scanner-cpp/
│   ├── CMakeLists.txt
│   ├── scanner_api.h            # 仕様書§7準拠（4構造体 + イテレータAPI）
│   ├── scanner.cpp              # ← 全面書き換え（現状スタブ）
│   ├── calibration.cpp/h        # 新規
│   ├── input_tracker.cpp/h      # 新規
│   ├── gauge.cpp/h              # 新規
│   └── frame_meter.cpp/h        # 新規
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
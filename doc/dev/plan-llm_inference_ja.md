# 実装計画書 — SF6-Trainer 残り機能 (全フェーズ)

## 現状

`plan-frame_data_extraction.md` のフェーズ0-7（スキャナー・FFI・パイプライン・UI）は完了している。しかし、詳細設計書 `sf6_trainer_detailed_design_ja.md` で規定されている以下の主要機能が未実装：

1. **LLM 推論モジュール** (§4) — **子プロセス方式 (llama.cpp + LFM3-1.7B GGUF, Named Pipe IPC)**
2. **SQLite 構造化対戦データベース** (§3.1) — `st6_trainer.db` への本格統合 (match_metadata テーブル等)
3. **フレームデータマスター JSON インポート** (§3.2) — キャラクターフレームデータの SQLite 取り込み
4. **フロントエンド機能** (§5) — キャラ選択、タイムラインナビ、統計ダッシュボード、レポートビューア
5. **設定ファイル管理** — `$HOME/.st6-trainer/config.json`
6. **モデルダウンロード・管理** — `$HOME/.st6-trainer/models/`

---

## 確定した技術選定

| 項目 | 採用方式 | 根拠 |
|------|----------|------|
| **LLM 実行方式** | **Subprocess** (llama.cpp 子プロセス) | プロセス分離でメモリリーク・クラッシュを本体から隔離、モデル切替容易 |
| **IPC 方式** | **Named Pipe (Windows)** | 双方向ストリーミング対応、フレーミング明確、Windows ネイティブ |
| **プロンプトテンプレート** | **handlebars** (Rust, logic-less) | 依存最小、ロジックレスで安全、テンプレート継承対応 |
| **フロントエンド状態管理** | **現状維持 (App.tsx 単体)** | 現段階では機能数が限定的、将来的に zustand 等へ移行可能 |
| **SQLite マイグレーション** | **rusqlite user_version** | 依存ゼロ、スキーマバージョン管理のみで十分 |
| **同時推論数** | **1 (シングルキュー、順次処理)** | VRAM 節約、実装簡潔、キューイングで対応 |

---

## フェーズ構成（依存順）

| Phase | 名称 | 主タスク | 依存 | 状態 |
|-------|------|----------|------|------|
| **8** | 設定ファイル・モデル管理基盤 | `$HOME/.st6-trainer/config.json`、`models/` ディレクトリ、モデルダウンロード | Phase 0-7 | 完了 |
| **9** | LLM 推論モジュール (子プロセス + Named Pipe) | `llm_worker.rs`、Named Pipe IPC、モデルロード・推論・ストリーミング | Phase 8 | 完了 |
| **10** | 構造化 RAG & コンテキストアセンブリ | SQLite からスキャン結果読み取り、handlebars テンプレートでプロンプト組み立て | Phase 9 | 完了 |
| **11** | LLM Tauri コマンド・フロントエンド UI | `llm_infer`/`llm_stream` コマンド、チャット/レポート UI | Phase 9, 10 | 未実装 |
| **12** | SQLite データベース本格統合 | `match_metadata` テーブル、フレームデータマスター import、試合記録保存、user_version マイグレーション | Phase 0-7 | 未実装 |
| **13** | フレームデータマスター JSON インポート | `frame_data/*.json` → SQLite `moves` テーブル (差分インポート) | Phase 12 | 未実装 |
| **14** | フロントエンド機能拡張 | キャラ選択、タイムラインナビ、統計ダッシュボード | Phase 11, 12 | 未実装 |
| **15** | 統合・検証・リリース | E2E テスト、リリースビルド | Phase 8-14 | 未実装 |

---

## Phase 8 — 設定ファイル・モデル管理基盤

### 目的
アプリ設定と LLM モデルファイルの配置・ダウンロード基盤を構築する。

### 作業項目

| # | タスク | ファイル | 内容 | 状態 |
|---|--------|----------|------|------|
| 8.1 | パス解決ユーティリティ | `src-tauri/src/paths.rs` (新規) | `$HOME/.st6-trainer/` 解決、ディレクトリ作成、権限チェック、`config.json`/`models/`/`st6_trainer.db` パス提供 | 完了 |
| 8.2 | 設定構造体定義 | `src-tauri/src/config.rs` | `AppConfig` に LLM 関連フィールド追加 (model_path, model_url, ctx_size, gpu_layers, pipe_name 等) | 完了 |
| 8.3 | 設定読み書き | `src-tauri/src/config.rs` | `$HOME/.st6-trainer/config.json` の読み込み・保存・デフォルト生成・マイグレーション | 完了 |
| 8.4 | モデル管理 | `src-tauri/src/model_manager.rs` (新規) | `$HOME/.st6-trainer/models/` 管理、GGUF ヘッダー検証、SHA256 チェックサム検証、存在チェック | 完了 |
| 8.5 | モデルダウンロード | `src-tauri/src/model_downloader.rs` (新規) | Hugging Face から `LFM3-1.7B-Q8_0.gguf` をダウンロード (`reqwest` + 進捗コールバック)、再開対応 | 完了 |
| 8.6 | Tauri コマンド | `src-tauri/src/commands.rs` | `get_app_config`, `update_app_config`, `download_model`, `check_model_exists`, `verify_model` | 完了 |
| 8.7 | ビルド設定 | `src-tauri/Cargo.toml` | `reqwest`, `tokio`, `sha2`, `dirs-next`, `handlebars` 等を追加 | 完了 |

### 成果物
- `$HOME/.st6-trainer/config.json` の読み書き・マイグレーション機能
- `$HOME/.st6-trainer/models/` ディレクトリ管理・モデル検証
- モデルダウンロード機能 (進捗表示・再開対応)
- Named Pipe 名 (`\\.\pipe\st6_llm_worker`) の設定・管理

---

## Phase 9 — LLM 推論モジュール (子プロセス + Named Pipe)

### 目的
llama.cpp を子プロセスとして起動し、Named Pipe (Windows) で推論リクエスト・レスポンスをやり取りする基盤を構築する。

### アーキテクチャ概要

```
┌─────────────────────────────────────────────────────────────────┐
│ Rust バックエンド (Tauri)                                        │
│  ┌──────────────┐    Named Pipe (\\.\pipe\st6_llm_worker)      │
│  │ LLMWorker    │ ◄──────────────────────────────────────►      │
│  │  - 起動/停止  │    JSONL フレーミング (改行区切り)            │
│  │  - キュー管理  │                                             │
│  │  - 状態管理   │                                             │
│  └──────────────┘                                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 子プロセス: st6_llm_worker.exe (llama.cpp ベース)               │
│  - モデルロード (起動時 1 回)                                    │
│  - 推論ループ: リクエスト受信 → 推論 → ストリーミング送信        │
│  - Named Pipe サーバー (CreateNamedPipe + ConnectNamedPipe)     │
└─────────────────────────────────────────────────────────────────┘
```

### IPC プロトコル (JSONL over Named Pipe)

**リクエスト (Rust → Worker):**
```json
{"id": "req-123", "type": "infer", "prompt": "...", "params": {"temperature": 0.7, "top_p": 0.9, "max_tokens": 512, "stream": true}}
{"id": "req-124", "type": "infer", "prompt": "...", "params": {"stream": false}}
{"id": "req-125", "type": "health_check"}
{"id": "req-126", "type": "shutdown"}
```

**レスポンス (Worker → Rust):**
```json
{"id": "req-123", "type": "token", "token": "Hello", "index": 0}
{"id": "req-123", "type": "token", "token": " world", "index": 1}
{"id": "req-123", "type": "done", "usage": {"prompt_tokens": 10, "completion_tokens": 20}}
{"id": "req-124", "type": "result", "text": "Complete response...", "usage": {...}}
{"id": "req-125", "type": "health", "status": "ready", "model_loaded": true}
{"id": "req-126", "type": "ack"}
{"id": "req-123", "type": "error", "code": "MODEL_NOT_LOADED", "message": "..."}
```

### 作業項目

| # | タスク | ファイル | 内容 | 状態 |
|---|--------|----------|------|------|
| 9.1 | IPC プロトコル定義 | `src-tauri/src/llm_protocol.rs` (新規) | Request/Response/StreamChunk/Error の serde 構造体、メッセージフレーミング、バージョンヘッダー | 完了 |
| 9.2 | Named Pipe クライアント | `src-tauri/src/named_pipe.rs` (新規) | Windows API (`CreateFile`, `ReadFile`, `WriteFile`, `WaitNamedPipe`) ラッパー、非同期 I/O (`tokio::net::windows::named_pipe`) | 完了 |
| 9.3 | LLM ワーカー管理 | `src-tauri/src/llm_worker.rs` (新規) | 子プロセス起動・停止・監視、Named Pipe 接続管理、リクエストキュー (シングルキュー、順次処理) | 完了 |
| 9.4 | モデルロード管理 | `src-tauri/src/llm_worker.rs` | 起動時引数でモデルパス・ctx_size・gpu_layers 渡し、Ready 通知待ち | 完了 |
| 9.5 | 推論リクエスト送信 | `src-tauri/src/llm_worker.rs` | キューイング、タイムアウト (デフォルト 120s)、キャンセル対応 | 完了 |
| 9.6 | ストリーミング受信 | `src-tauri/src/llm_worker.rs` | トークン単位でコールバック/チャネル経由でフロントエンドへ転送 | 完了 |
| 9.7 | エラーハンドリング | `src-tauri/src/llm_worker.rs` | 起動失敗、ロード失敗、Pipe 断、タイムアウト、プロトコルエラー、自動再接続 (最大 3 回) | 完了 |
| 9.8 | 状態管理 | `src-tauri/src/llm_worker.rs` | `Uninitialized`/`Starting`/`Connecting`/`LoadingModel`/`Ready`/`Busy`/`Error`/`ShuttingDown` | 完了 |
| 9.9 | 子プロセス実装 | `st6_llm_worker/` (新規 Cargo プロジェクト) | llama.cpp ベースのスタンドアロン実行ファイル、Named Pipe サーバー、推論ループ | 完了 |

### 成果物
- LLM ワーカー子プロセス (`st6_llm_worker.exe`) の起動・停止・管理
- Named Pipe (Windows) による双方向ストリーミング通信
- JSONL フレーミングプロトコル (リクエスト/レスポンス/トークン/エラー)
- シングルトンキューによる順次推論処理
- 状態管理・自動再接続・ヘルスチェック

---

## Phase 10 — 構造化 RAG & コンテキストアセンブリ

### 目的
SQLite に保存されたスキャン結果から、handlebars テンプレートを用いて LLM 用のプロンプトを組み立てる。

### テンプレート構成

```
src-tauri/prompts/
├── base.hbs              # 共通システムプロンプト・フォーマット指定
├── analyze_damage.hbs    # 被弾原因分析
├── verify_punish.hbs     # 確定反撃検証
├── training_menu.hbs     # トレモメニュー提示
└── partials/
    ├── frame_data.hbs    # フレームデータ整形パーシャル
    ├── move_event.hbs    # 技イベント整形パーシャル
    └── input_sequence.hbs # 入力シーケンス整形パーシャル
```

### 作業項目

| # | タスク | ファイル | 内容 | 状態 |
|---|--------|----------|------|------|
| 10.1 | スキャン結果読み取り | `src-tauri/src/db.rs` (新規) | SQLite から `match_metadata`, `frame_telemetry`, `input_events`, `move_events` を読み取り | 完了 |
| 10.2 | フレームデータ参照 | `src-tauri/src/db.rs` | SQLite `moves` テーブルからフレームデータを取得 (キャラ名・操作系でフィルタ) | 完了 |
| 10.3 | handlebars エンジン初期化 | `src-tauri/src/rag.rs` (新規) | テンプレートディレクトリ読み込み、パーシャル登録、ヘルパー関数登録 | 完了 |
| 10.4 | コンテキスト組み立て | `src-tauri/src/rag.rs` | 試合メタデータ・テレメトリ・入力・技イベントを構造化データに変換 | 完了 |
| 10.5 | プロンプト生成 (被弾分析) | `src-tauri/src/rag.rs` | `analyze_damage.hbs` レンダリング: 状況・入力ミス・フレーム不利・推奨アクション | 完了 |
| 10.6 | プロンプト生成 (反撃検証) | `src-tauri/src/rag.rs` | `verify_punish.hbs` レンダリング: 硬直差・発生・リーチ・確定可否・代替手段 | 完了 |
| 10.7 | プロンプト生成 (練習メニュー) | `src-tauri/src/rag.rs` | `training_menu.hbs` レンダリング: 反復練習・セットアップ・成功基準・デバイス別操作 | 完了 |
| 10.8 | トークン予算管理 | `src-tauri/src/rag.rs` | コンテキストサイズ制限 (ctx_size 考慮)、優先度付き切り詰め | 完了 |

### 成果物
- SQLite からのスキャン結果読み取り・構造化 (`src-tauri/src/db.rs`)
- handlebars テンプレート群 (ベース・3ユースケース・パーシャル) (`src-tauri/prompts/`)
- 日本語プロンプト生成関数 (3種類) (`src-tauri/src/rag.rs`)
- トークン予算を考慮したコンテキスト切り詰めロジック (`src-tauri/src/rag.rs`)
- Tauri コマンド 5種: `rag_generate_analyze_damage`, `rag_generate_verify_punish`, `rag_generate_training_menu`, `rag_list_matches`, `rag_get_moves` (`src-tauri/src/commands.rs`)

---

## Phase 11 — LLM Tauri コマンド・フロントエンド UI

### 目的
フロントエンドから LLM 推論を呼び出し、結果を表示する UI を構築する。

### Tauri コマンド設計

| コマンド | 用途 | ストリーミング |
|----------|------|----------------|
| `llm_infer` | 単発推論 (完了まで待機) | × |
| `llm_stream` | ストリーミング推論 | ○ (Event/Channel でトークン逐次送信) |
| `llm_worker_status` | ワーカー状態取得 | - |
| `llm_cancel` | 実行中推論のキャンセル | - |

### 作業項目

| # | タスク | ファイル | 内容 |
|---|--------|----------|------|
| 11.1 | LLM 推論コマンド | `src-tauri/src/commands.rs` | `llm_infer`, `llm_stream`, `llm_worker_status`, `llm_cancel` |
| 11.2 | コマンド登録 | `src-tauri/src/lib.rs` | `invoke_handler` に新規コマンドを登録 |
| 11.3 | LLM チャット UI | `src/App.tsx` | チャット形式の推論結果表示、ストリーミング対応 (トークン逐次追加) |
| 11.4 | レポートビューア | `src/App.tsx` | Markdown レンダラー (`react-markdown` + `remark-gfm`) で分析レポート表示 |
| 11.5 | モデルダウンロード進捗 | `src/App.tsx` | ダウンロード状態・進捗バー表示、完了時トースト通知 |
| 11.6 | LLM ワーカー状態表示 | `src/App.tsx` | ヘッダーに `LLM: Ready🟢 / Busy🟡 / Error🔴 / Loading⏳` 表示 |
| 11.7 | エラー境界・トースト | `src/components/ErrorBoundary.tsx`, `src/hooks/useToast.ts` | ユーザー向けエラー表示・導線 (再試行・設定画面・ログ送信) |
| 11.8 | LLM UI スタイル | `src/App.css` | チャット/レポート/トースト/進捗バー用スタイル |

### 成果物
- `llm_infer`/`llm_stream`/`llm_worker_status`/`llm_cancel` Tauri コマンド
- LLM チャット UI (ストリーミング対応、履歴保持)
- Markdown レポートビューア (コードブロック・テーブル対応)
- モデルダウンロード進捗表示・完了通知
- ワーカー状態インジケーター (ヘッダー常駐)
- エラー境界・トースト通知システム

---

## Phase 12 — SQLite データベース本格統合

### 目的
`st6_trainer.db` に試合記録・フレームデータを保存・参照する基盤を構築する。`rusqlite` の `user_version` プラグマでスキーマバージョン管理を行う。

### スキーマ設計 (v1)

```sql
-- スキーマバージョン管理
PRAGMA user_version = 1;

-- 対戦セッションテーブル
CREATE TABLE IF NOT EXISTS matches (
    id TEXT PRIMARY KEY,
    played_at DATETIME NOT NULL,
    my_character TEXT NOT NULL,
    opponent_character TEXT NOT NULL,
    my_rank TEXT,
    opponent_rank TEXT,
    control_type TEXT NOT NULL, -- classic / modern
    total_rounds INTEGER NOT NULL,
    result TEXT NOT NULL, -- WIN / LOSS
    source_video_path TEXT,
    scan_config_version TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- キャラクターフレームデータマスターテーブル (JSONからインポート)
CREATE TABLE IF NOT EXISTS moves (
    move_id TEXT PRIMARY KEY,
    character_id TEXT NOT NULL,
    character_name TEXT NOT NULL,
    control_type TEXT NOT NULL, -- classic / modern
    version TEXT NOT NULL,
    move_name TEXT NOT NULL,
    category TEXT NOT NULL,
    command TEXT NOT NULL,
    startup TEXT NOT NULL,
    active TEXT NOT NULL,
    recovery TEXT NOT NULL,
    hit_advantage TEXT NOT NULL,
    block_advantage TEXT NOT NULL,
    cancel TEXT NOT NULL,
    damage TEXT NOT NULL,
    combo_scaling TEXT, -- JSON array string
    drive_gauge_gain_hit INTEGER,
    drive_gauge_lose_dguard INTEGER,
    drive_gauge_lose_punish INTEGER,
    sa_gauge_gain INTEGER,
    attribute TEXT,
    note TEXT
);

-- スキャン詳細テーブル (既存 pipeline.rs のテーブルを統合)
CREATE TABLE IF NOT EXISTS frame_telemetry (
    match_id TEXT NOT NULL,
    frame_index INTEGER NOT NULL,
    source_frame_index INTEGER,
    timestamp_ms INTEGER,
    round_index INTEGER,
    hp_reset INTEGER,
    hp_p1 REAL,
    drive_p1 REAL,
    sa_p1 INTEGER,
    in_burnout_p1 INTEGER,
    hp_p2 REAL,
    drive_p2 REAL,
    sa_p2 INTEGER,
    in_burnout_p2 INTEGER,
    quality_flags TEXT, -- JSON array
    PRIMARY KEY (match_id, frame_index),
    FOREIGN KEY (match_id) REFERENCES matches(id)
);

CREATE TABLE IF NOT EXISTS input_events (
    match_id TEXT NOT NULL,
    frame_start INTEGER NOT NULL,
    frame_end INTEGER NOT NULL,
    frame_span_video INTEGER,
    hold_frames INTEGER,
    counter_clipped INTEGER,
    hold_frames_lower_bound INTEGER,
    time_dilation_suspected INTEGER,
    resync_used INTEGER,
    unknown_glyph INTEGER,
    is_preround INTEGER,
    player TEXT NOT NULL, -- P1 / P2
    round_index INTEGER NOT NULL,
    event_index INTEGER NOT NULL,
    directions TEXT, -- JSON array
    buttons TEXT, -- JSON array
    quality_flags TEXT, -- JSON array
    PRIMARY KEY (match_id, player, round_index, event_index),
    FOREIGN KEY (match_id) REFERENCES matches(id)
);

CREATE TABLE IF NOT EXISTS move_events (
    match_id TEXT NOT NULL,
    frame_start INTEGER NOT NULL,
    frame_end INTEGER NOT NULL,
    round_index INTEGER NOT NULL,
    fm_startup_p1 INTEGER,
    fm_total_p1 INTEGER,
    fm_advantage_p1 INTEGER,
    fm_startup_p2 INTEGER,
    fm_total_p2 INTEGER,
    fm_advantage_p2 INTEGER,
    damage_p1 INTEGER,
    combo_damage_p1 INTEGER,
    damage_p2 INTEGER,
    combo_damage_p2 INTEGER,
    attack_property_p1 TEXT,
    attack_property_p2 TEXT,
    attack_property_locale TEXT,
    conf REAL,
    quality_flags TEXT, -- JSON array
    PRIMARY KEY (match_id, frame_start, round_index),
    FOREIGN KEY (match_id) REFERENCES matches(id)
);

-- インデックス
CREATE INDEX IF NOT EXISTS idx_frame_telemetry_match_round ON frame_telemetry(match_id, round_index);
CREATE INDEX IF NOT EXISTS idx_input_events_match_player ON input_events(match_id, player);
CREATE INDEX IF NOT EXISTS idx_move_events_match_round ON move_events(match_id, round_index);
CREATE INDEX IF NOT EXISTS idx_moves_character_control ON moves(character_id, control_type);
```

### 作業項目

| # | タスク | ファイル | 内容 | 状態 |
|---|--------|----------|------|------|
| 12.1 | データベースパス解決 | `src-tauri/src/paths.rs` | `$HOME/.st6-trainer/st6_trainer.db` 解決、ディレクトリ作成 | 完了 (Phase 8) |
| 12.2 | 接続プール・WAL モード | `src-tauri/src/db.rs` | `rusqlite::Connection` 管理、WAL モード有効化、busy_timeout 設定 | 完了 (Phase 10) |
| 12.3 | スキーマ初期化・マイグレーション | `src-tauri/src/db.rs` | `user_version` チェック、v1 スキーマ適用、将来のマイグレーション関数雛形 | 完了 (Phase 10) |
| 12.4 | matches テーブル操作 | `src-tauri/src/db.rs` | `insert_match`, `get_match`, `list_matches`, `update_match_result` | 完了 (Phase 10) |
| 12.5 | スキャン詳細保存 | `src-tauri/src/db.rs` | `save_scan_result` (match_id + telemetry/input/move 一括トランザクション) | 完了 (Phase 10) |
| 12.6 | 既存パイプライン統合 | `src-tauri/src/pipeline.rs` | `run_pipeline` を `save_scan_result` 呼び出しに置換、match_metadata 挿入 | 未実装 |
| 12.7 | RAG 用クエリ関数 | `src-tauri/src/db.rs` | `get_match_with_details`, `get_moves_for_character`, `get_recent_matches` | 完了 (Phase 10) |

### 成果物
- `st6_trainer.db` のフルスキーマ (v1, user_version 管理)
- WAL モード・トランザクション対応の DB アクセス層 (`src-tauri/src/db.rs`)
- 試合記録の保存・取得・一覧機能
- 既存パイプライン (`scan_video` → `run_pipeline` → DB保存) の完全統合 (未実装)
- RAG フェーズで使用するクエリ関数群

---

## Phase 13 — フレームデータマスター JSON インポート

### 目的
`frame_data/*.json` を SQLite `moves` テーブルに**差分インポート**する。起動時の全件再インポートを避け、変更分のみ適用する。

### JSON 構造 (詳細設計書 §3.2 準拠)

```json
{
  "character_id": "luke",
  "character_name": "LUKE",
  "version": "sf6-ui-2026.11",
  "moves": [
    {
      "move_name": "M Sand Blast",
      "command": {
        "classic": "236MP",
        "modern": "5[SP]",
        "modern_manual": "236M"
      },
      "startup": "5",
      "active": "5-6",
      "recovery": "7",
      "hit": "4",
      "block": "-1",
      "cancel": "C",
      "damage": "300",
      "combo_scaling": "Starter scaling 20%",
      "drive_gauge_gain_hit": "250",
      "drive_gauge_lose_dguard": "-500",
      "drive_gauge_lose_punish": "-2000",
      "sa_gauge_gain": "300",
      "attribute": "High",
      "miscellaneous": "Can be rapid canceled"
    }
  ]
}
```

### 作業項目

| # | タスク | ファイル | 内容 |
|---|--------|----------|------|
| 13.1 | JSON パーサー | `src-tauri/src/frame_data_importer.rs` (新規) | `frame_data/*.json` のパース、スキーマ検証 (必須フィールドチェック) |
| 13.2 | 差分検出 | `src-tauri/src/frame_data_importer.rs` | ファイル mtime / SHA256 ハッシュ比較、変更分のみ抽出 |
| 13.3 | SQLite UPSERT | `src-tauri/src/frame_data_importer.rs` | `moves` テーブルへ `INSERT OR REPLACE` (move_id = character_id + control_type + move_name) |
| 13.4 | クラシック/モダン展開 | `src-tauri/src/frame_data_importer.rs` | `command` オブジェクトから `control_type` 別レコード生成 (classic/modern/modern_manual) |
| 13.5 | 起動時インポートトリガー | `src-tauri/src/commands.rs` | `import_frame_data` コマンド、手動実行・初回起動時のみ自動実行 |
| 13.6 | 進捗・ログ | `src-tauri/src/frame_data_importer.rs` | インポート件数・スキップ件数・エラー件数のログ出力、Tauri イベントで進捗通知 |

### 成果物
- フレームデータマスター JSON → SQLite 差分インポート (mtime/SHA256 ベース)
- クラシック/モダン/モダンマニュアル操作系別レコード生成
- 手動トリガー可能なインポートコマンド (`import_frame_data`)
- インポート統計ログ・進捗通知

---

## Phase 14 — フロントエンド機能拡張

### 目的
詳細設計書 §5 の UI 機能を実装する。現状 `App.tsx` 単体構成を維持し、コンポーネント関数として分割して可読性を確保する。

### UI レイアウト (詳細設計書 §5 準拠)

```
+-----------------------------------------------------------------------------------+
| ST6 AI Trainer v1.6.1                           [ 🎬 動画読込 ] [ LLM: Ready🟢 ] |
| [自キャラ: A.K.I.▼] [敵キャラ: Ken▼] [操作: モダン▼] [デバイス: パッド▼]           |
+--------------------------+--------------------------------------------------------+
| 検出タイムライン         | レポート詳細表示エリア (Markdown Renderer)               |
| ------------------------ | ------------------------------------------------------ |
| [0:14] Punish Counter 🔴 | # 📁 試合分析レポート                                  |
|  - 硬直差: -14F          | ## 1. 何が起きたか（状況分析）                          |
|  - 入力履歴取得済み      | A.K.I.の技をガード後、硬直差 -14F が発生していましたが... |
+--------------------------+--------------------------------------------------------+
```

### 作業項目

| # | タスク | ファイル | 内容 |
|---|--------|----------|------|
| 14.1 | ヘッダーコントロール | `src/App.tsx` | 動画読込ボタン、LLM状態インジケーター、キャラ選択×2、操作系選択、デバイス選択 |
| 14.2 | キャラクター選択 | `src/App.tsx` | `moves` テーブルからキャラ一覧取得、自キャラ/敵キャラ別ドロップダウン |
| 14.3 | 操作系選択 | `src/App.tsx` | クラシック/モダン/モダンマニュアル選択、選択に応じてフレームデータフィルタ |
| 14.4 | デバイス選択 | `src/App.tsx` | パッド/アケコン/レバーレス選択、トレモメニューの操作表記に反映 |
| 14.5 | タイムラインナビ | `src/App.tsx` | 検出イベント (move_events + input_events) を時系列表示、クリックで該当フレーム/レポートへジャンプ |
| 14.6 | レポート詳細エリア | `src/App.tsx` | Markdown レンダラー (`react-markdown`)、LLM 分析結果・手動入力メモ表示 |
| 14.7 | 統計ダッシュボード | `src/App.tsx` | タブ切替: 被弾原因内訳 / 反撃成功率 / トレモメニュー達成率 / 入力傾向 |
| 14.8 | 試合履歴サイドバー | `src/App.tsx` | `matches` テーブルから過去試合一覧 (日時・キャラ・結果)、クリックで読み込み |
| 14.9 | フレームデータ参照モーダル | `src/App.tsx` | 技名クリックでフレームデータ詳細表示 (発生・持続・硬直・有利不利・キャンセル) |

### 成果物
- ヘッダー: 動画読込・LLM状態・キャラ/操作系/デバイス選択
- 左ペイン: タイムラインナビ (イベント一覧・ジャンプ)
- 右ペイン: レポート詳細 (Markdown) / 統計ダッシュボード (タブ) / フレームデータ参照
- サイドバー: 試合履歴一覧・読み込み
- 全コンポーネントが `App.tsx` 内の関数コンポーネントとして分割済み

---

## Phase 15 — 統合・検証・リリース

### 目的
全機能を統合し、E2E テストとリリースビルドを行う。各フェーズで単体テストを積み上げ、最後に統合テストで全体フローを検証する。

### 非機能要件 (受け入れ基準)

| 指標 | 目標値 | 測定方法 |
|------|--------|----------|
| フル動画スキャン (3分・60FPS) | ≤ 15 秒 | 推奨GPU環境で `replay_00.mkv` 測定 |
| First Token Latency (LLM) | ≤ 2.0 秒 | 推奨GPU環境、ストリーミングモード |
| ストリーミング速度 | ≥ 20 tok/s | ストリーミングモードで 512 tokens 生成時 |
| VRAM 使用量 (LLM) | ≤ 4 GB | Q8_0 約 1.7 GB + KV cache、GPU レイヤー調整で達成 |
| 同時推論数 | 1 (キューイング) | シングルトンキューで順次処理、待ち時間測定 |
| SQLite 書き込み (1試合分) | ≤ 500 ms | WAL モード、トランザクション込み |
| フレームデータインポート (全キャラ) | ≤ 3 秒 | 差分インポート (初回除く) |

### 作業項目

| # | タスク | ファイル | 内容 |
|---|--------|----------|------|
| 15.1 | 単体テスト: 設定・モデル管理 | `src-tauri/tests/test_config.rs` | パス解決、設定読み書き、モデル検証、ダウンロード再開 |
| 15.2 | 単体テスト: LLM ワーカー | `src-tauri/tests/test_llm_worker.rs` | 子プロセス起動・停止、Named Pipe 接続、推論・ストリーミング、タイムアウト、再接続 |
| 15.3 | 単体テスト: RAG・プロンプト | `src-tauri/tests/test_rag.rs` | テンプレートレンダリング、トークン予算切り詰め、スナップショットテスト |
| 15.4 | 単体テスト: SQLite・マイグレーション | `src-tauri/tests/test_db.rs` | スキーマ作成、user_version マイグレーション、トランザクション、WAL、クエリ関数 |
| 15.5 | 単体テスト: フレームデータインポート | `src-tauri/tests/test_frame_data_import.rs` | JSON パース、差分検出 (mtime/SHA256)、UPSERT、クラシック/モダン展開 |
| 15.6 | E2E 統合テスト | `src-tauri/tests/test_e2e.rs` | 動画スキャン → SQLite 保存 → LLM 推論 (3ユースケース) → UI 表示 の一連フロー |
| 15.7 | パフォーマンステスト | `src-tauri/tests/test_perf.rs` | 上記非機能要件の自動測定・閾値チェック |
| 15.8 | リリースビルド | `Cargo.toml`, `CMakeLists.txt` | リリース用バイナリ・DLL ビルド、バンドルサイズ確認、署名・証明書 |
| 15.9 | テストレポート | `doc/dev/phase15_test_report_ja.md` | テスト結果レポート (日本語)、不具合一覧、改善項目 |

### 成果物
- 単体テストスイート (各フェーズ網羅)
- E2E 統合テスト (全フロー自動検証)
- パフォーマンステスト (非機能要件ゲート)
- リリースビルド (MSI/NSIS インストーラー)
- テストレポート (日本語、CI アーティファクトとして公開)

---

## リスクと対策

| リスク | 影響 | 対策 |
|---|---|---|
| 子プロセス起動失敗 | LLM 機能停止 | エラーメッセージ表示、再起動リトライ (最大 3 回)、手動起動ガイド |
| Named Pipe 接続失敗 | 推論不可 | `WaitNamedPipe` タイムアウト (10s)、再接続 (指数バックオフ)、フォールバック: stdin/stdout |
| モデルダウンロード失敗 | LLM 機能不可 | 再試行 (レジューム対応)、手動配置サポート (`models/` 直置き)、エラー表示・導線 |
| IPC 通信エラー (Pipe 断) | 推論中断 | ハートビート (health_check 30s 間隔)、自動再接続、キュー内リクエスト再送 |
| 推論レイテンシ超過 | UX 劣化 | GPU レイヤー数自動調整、ストリーミング必須、コンテキスト切り詰め、進捗表示 |
| 子プロセスクラッシュ (OOM/SEGV) | 本体に影響なし | プロセス分離により本体生存、ワーカー自動再起動、クラッシュログ保存 |
| SQLite ロック/破損 | 書き込み失敗・データ損失 | WAL モード、busy_timeout (5s)、トランザクション、定期バックアップ (コマンド提供) |
| ビルド失敗 (reqwest/tokio/handlebars) | ビルド停止 | バージョン固定 (`Cargo.lock` コミット)、MSVC 対応確認、最小機能フラグ |
| フレームデータインポート重複 | 主キー衝突・性能劣化 | `INSERT OR REPLACE` (move_id PK)、mtime/SHA256 差分検出、冪等性保証 |
| プロンプトテンプレート構文エラー | 推論時クラッシュ | 起動時テンプレート検証 (handlebars compile check)、フォールバック組み込みテンプレート |

---

## データフロー概要

```
動画ファイル
    │
    ▼
[スキャナー (C++ DLL: st6_scanner.dll)]
    │  OpenCV 解析 (ラスタ正規化・キャリブレーション・入力追跡・ゲージ/フレームメーター)
    ▼
SQLite (st6_trainer.db) ◄──── [フレームデータマスター JSON インポート (frame_data/*.json)]
    │  (matches, frame_telemetry, input_events, move_events, moves)
    │
    ├─► [RAG & コンテキストアセンブリ] ──► handlebars テンプレート ──► プロンプト生成
    │      SQLite から試合データ・フレームデータ読み取り
    │
    ▼
[LLM ワーカー子プロセス (st6_llm_worker.exe)]
    │  Named Pipe (\\.\pipe\st6_llm_worker) で JSONL 通信
    │  llama.cpp (LFM3-1.7B-Q8_0.gguf) で推論実行
    ▼
ストリーミングトークン / 完了レスポンス
    │
    ▼
[Tauri フロントエンド (React + TypeScript)]
    ├─ ヘッダー: 動画読込 / LLM状態 / キャラ・操作系・デバイス選択
    ├─ 左ペイン: タイムラインナビ (イベント一覧・ジャンプ)
    ├─ 右ペイン: レポート詳細 (Markdown) / 統計ダッシュボード / フレームデータ参照
    └─ サイドバー: 試合履歴一覧・読み込み
```

---

この計画書は、別セッションで実装を開始する際の参照資料として使用してください。
各フェーズの優先順位や範囲に変更が必要な場合は、計画書を更新してから実装を進めてください。
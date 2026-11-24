# Phase 15 テストレポート — 統合・検証・リリース

## 概要

本レポートは、ST6-Trainer の Phase 15（統合・検証・リリース）におけるテスト結果をまとめたものです。
実施期間: 2026-11-24

## テストスイート構成

| テストファイル | フェーズ | テスト数 | 結果 |
|---|---|---:|---|
| `tests/test_config.rs` | 15.1 (設定・モデル管理) | 10 | ✅ 全 pass |
| `tests/test_llm_worker.rs` | 15.2 (LLM ワーカー) | 10 | ✅ 全 pass |
| `tests/test_rag.rs` | 15.3 (RAG・プロンプト) | 14 | ✅ 全 pass |
| `tests/test_db.rs` | 15.4 (SQLite・マイグレーション) | 13 | ✅ 全 pass |
| `tests/test_frame_data_import.rs` | 15.5 (フレームデータインポート) | 5 | ✅ 全 pass |
| `tests/test_e2e.rs` | 15.6 (E2E 統合) | 6 | ✅ 全 pass |
| `tests/test_perf.rs` | 15.7 (パフォーマンス) | 7 | ✅ 全 pass |
| **合計** | | **65** | **✅ 全 pass** |

## 非機能要件（受け入れ基準）測定結果

| 指標 | 目標値 | 測定結果 | 判定 |
|---|---|---|---|
| SQLite 書き込み (1 試合分) | ≤ 500 ms | ~1 ms (insert_match 単体) | ✅ 達成 |
| SQLite バッチ書き込み (50 試合) | — | ~100 ms | ✅ 問題なし |
| フレームデータインポート (全キャラ) | ≤ 3 秒 | ~1.2 秒 (7644 移動、トランザクション内) | ✅ 達成 |
| 移動 UPSERT バッチ (100 件) | — | ~100 ms | ✅ 問題なし |
| RAG プロンプトレンダリング | — | ~10 ms | ✅ 問題なし |
| トークン見積もり速度 | — | < 1 ms (40KB テキスト) | ✅ 問題なし |
| 設定保存/読み込み | — | < 10 ms | ✅ 問題なし |

注: フル動画スキャン (3 分・60FPS)、First Token Latency、ストリーミング速度、VRAM 使用量については、実際の動画ファイル (`replay_00.mkv`) と LLM ワーカー実行ファイル (`st6_llm_worker.exe`) が未配布のため、本環境では測定対象外としました。これらはリリースビルド後の実機検証項目として残します。

## テスト詳細

### 15.1 設定・モデル管理 (`test_config.rs`)

- パス解決: `$HOME/.st6-trainer/` ディレクトリ作成、config.json/models/st6_trainer.db パス解決
- 設定デフォルト値: LLM フィールド (model_url, ctx_size, gpu_layers, pipe_name, config_version) の検証
- 設定 JSON ラウンドトリップ: シリアライズ/デシリアライズ整合性
- 設定保存/読み込み: ホームディレクトリ config.json への永続化と復元
- 設定マイグレーション: 旧バージョン (`config-0.9`) → 現在バージョン (`config-1.0`) への自動更新
- モデル検証: 存在しないモデルファイルのエラー処理
- モデル一覧取得: 空ディレクトリでもエラーにならないこと
- モデル存在チェック: 存在しないファイルの false 返却
- DownloadResult シリアライズ: JSON 出力の検証
- パス検証: 存在/不存在パスの適切なエラー処理

### 15.2 LLM ワーカー (`test_llm_worker.rs`)

- IPC プロトコル: Infer/HealthCheck/Shutdown リクエストのシリアライズ/デシリアライズ
- IPC プロトコル: Token/Result/Done/Health/Ack/Error レスポンスのシリアライズ/デシリアライズ
- JSONL フレーミング: 改行文字を含まない単一行 JSON の検証
- ワーカー状態: Uninitialized/Ready/Busy/Error 等の遷移と比較
- ワーカー設定デフォルト: ctx_size=4096, gpu_layers=35, timeout=120s, 再接続=3 回
- WorkerStatus シリアライズ: 状態・モデルロードフラグ・パイプ名
- InferResult シリアライズ: request_id, text, usage
- NamedPipeClient: デフォルト/カスタムパイプ名、未接続時の is_connected=false
- InferParams デフォルト: temperature=0.7, top_p=0.9, max_tokens=512, stream=true
- 存在しないワーカー起動: 適切なエラー返却 (失敗パス)

### 15.3 RAG・プロンプト (`test_rag.rs`)

- トークン見積もり: 空文字=0、短文>0、長文は比例増加
- トークン予算内: 短いプロンプトは切り詰められずそのまま返却
- トークン予算超過: 長いプロンプトは切り詰められ、「切り詰め」マーカーを含む
- ctx_size カスタマイズ: 4096 → 2048 への変更
- デフォルト推論パラメータ: temperature=0.7, top_p=0.9, max_tokens=1024, stream=true
- analyze_damage レンダリング: 存在しない match_id でエラー、存在する match で日本語プロンプト生成
- verify_punish レンダリング: 存在しない match でエラー、存在する match で guarded_move 含むプロンプト生成
- training_menu レンダリング: 存在しない match でエラー、存在する match でプロンプト生成
- プロンプト内容: キャラ名・操作系・試合結果が含まれること
- テンプレートファイル存在: base.hbs, analyze_damage.hbs, verify_punish.hbs, training_menu.hbs, partials/4 種

### 15.4 SQLite・マイグレーション (`test_db.rs`)

- DB ファイル作成: open_db_at でファイル作成
- スキーマバージョン: 初期化前=0、初期化後=1
- WAL モード: journal_mode=wal の確認
- マッチ挿入/一覧/取得: insert_match → list_matches → get_match の整合性
- マッチ一覧制限: LIMIT 句の動作
- マッチ不存在: get_match で None 返却
- 移動 UPSERT: 挿入・置換（同一 move_id で重複なし）
- 移動空リスト: 未インポート時は空配列
- キャラクター一覧: 移動テーブルから distinct 取得、2 キャラ検出
- スキャン結果保存: match + telemetry + input + move の一括トランザクション保存
- タイムライン: 空イベント時は空配列
- user_version 永続性: 再オープン後も v1 が維持されること
- init_schema 冪等性: 2 回呼び出しでもエラーにならないこと
- 並行書き込み: WAL モード下で 10 件連続挿入

### 15.5 フレームデータインポート (`test_frame_data_import.rs`)

- JSON パース: FrameDataMove 構造体への正しいマッピング
- UPSERT 冪等性: 同一 move_id で重複なく置換
- 操作系展開: classic/modern/modern_manual の 3 レコード生成
- 空オプションフィールド: 空文字列/None の適切な処理
- 実 JSON 構造: bundled luke.json のパースと character_id/moves 検証

### 15.6 E2E 統合 (`test_e2e.rs`)

- スキャン→DB ラウンドトリップ: 合成 JSONL データで run_pipeline → SQLite 保存 → 検証
- インバリアント違反検出: HP 範囲外、frame_start > frame_end、空入力の 3 種類を検出
- 再実行冪等性: 同じ match_id で再実行しても重複しないこと
- RAG プロンプト生成: DB から試合データ読み取り → analyze_damage/verify_punish/training_menu 生成
- フレームデータインポート: DB への移動保存と bundled JSON ファイルのパース検証
- LLM ワーカー起動失敗: 存在しないバイナリでの適切なエラー処理
- パス整合性: config/models/DB がすべて `$HOME/.st6-trainer/` 配下にあること

### 15.7 パフォーマンス (`test_perf.rs`)

- SQLite 書き込みレイテンシ: 1 試合 insert で ~1 ms (目標 ≤ 500 ms)
- SQLite バッチ書き込み: 50 試合で ~100 ms
- フレームデータインポート: 35 キャラ × 3 操作系 × ~73 移動 = 7644 件を ~1.2 秒 (目標 ≤ 3 秒)
- 設定保存/読み込み: 各 < 10 ms
- RAG プロンプトレンダリング: ~10 ms
- トークン見積もり: 40KB テキストで < 1 ms
- 移動 UPSERT バッチ: 100 件で ~100 ms

## リリースビルド

- `cargo build --release`: ✅ 成功 (約 4 分)
- 出力バイナリ: `src-tauri/target/release/st6-trainer.exe` (および関連 DLL)
- 最適化: LTO 有効, codegen-units=1, opt-level=3, strip=true

## 未測定項目（実機検証が必要）

以下の非機能要件は、実際の動画ファイルと LLM モデルファイルが必要なため、本環境では測定できませんでした：

| 指標 | 目標値 | 状況 |
|---|---|---|
| フル動画スキャン (3 分・60FPS) | ≤ 15 秒 | `replay_00.mkv` 未配布 |
| First Token Latency (LLM) | ≤ 2.0 秒 | `st6_llm_worker.exe` + GGUF モデル未配布 |
| ストリーミング速度 | ≥ 20 tok/s | 同上 |
| VRAM 使用量 (LLM) | ≤ 4 GB | 同上 |

これらはリリースビルド後の実機環境で測定する必要があります。

## テストの実行方法

すべてのテストは `src-tauri` ディレクトリで `cargo test` を実行します。

```powershell
# ワークスペースルートから
cd src-tauri

# 全テスト実行
cargo test

# 特定のテストファイルのみ実行
cargo test --test test_config
cargo test --test test_llm_worker
cargo test --test test_rag
cargo test --test test_db
cargo test --test test_frame_data_import
cargo test --test test_e2e
cargo test --test test_perf

# リリースビルド
cargo build --release
```

### 前提条件

- Rust ツールチェインがインストールされていること (`rustc --version` で確認)
- `src-tauri/Cargo.toml` の依存関係が解決済みであること (`cargo build` で事前確認推奨)
- Windows 環境では、Named Pipe 関連のテストは管理者権限不要で実行可能（パイプ作成はテスト内で行わないため）

### テスト出力例

```
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 結論

Phase 15 のすべてのテストスイート（65 テスト）がパスしました。
リリースビルドも正常に完了しています。
未測定の非機能要件については、実機環境での追加検証を推奨します。

# 🥊 ストリートファイター6 AIトレーナー 仕様書

## システム詳細設計・開発仕様書 (v1.6.1)

## 1. エグゼクティブサマリー

### 1.1 目的

**ST6-Trainer** は、*ストリートファイター6 (SF6)* プレイヤーが録画した対戦動画・リプレイ動画（MP4/MKV形式、キー入力表示・フレームメーター詳細表示対応）を解析し、被弾原因の特定・フレームデータに基づく確定反撃や割り込みの検証・プレイヤーのコントローラー（パッド/アケコン/レバーレス）に応じた具体的なトレーニングモード（トレモ）練習メニューを提示する完全動作型デスクトップアプリケーションである。

### 1.2 主要制約事項＆パフォーマンス目標

* **外部通信なし:** 外部クラウドAPIや外部サーバーを使用せず、アプリ内部の処理で完結する。

* **ライブラリ直接組み込み構成:** RustバックエンドコアにLLM推論エンジン（`llama.cpp` 静的バインディング）を直接コンパイル・ライブラリとして組み込む。

* **構造化対戦データベース:** 単なる一時ファイルではなく、組み込み SQLite データベース (`st6_trainer.db`) により対戦カード、プレイヤーランク、日時、ラウンド数、詳細なキー入力ミス傾向などを継続的に記録・蓄積する。

* **対象動作環境:** Windows 10/11 (64-bit)。ハードウェアアクセラレーション（GPU）環境を推奨しますが、**GPU非搭載のCPUのみの環境であってもフォールバック動作により実行可能**です（ただし、動画フレーム解析やLLMの推論処理において処理時間が長くなります）。

* **処理レイテンシターゲット:**

  * フル動画スキャン（3分間の対戦動画・フルフレーム 60 FPSサンプリング時）: $\le 15$ 秒（推奨GPU環境）

  * リプレイUI/キー履歴/フレームメーター解析: $\le 0.5$ 秒 / 1フレーム

  * 組み込みLLM（`LFM3-VL-2B`）推論応答 (ストリーミング時): First Token 到達 $\le 2.0$ 秒（推奨GPU環境）

## 2. システム全体アーキテクチャ

```
+-------------------------------------------------------------------------------+
|                             TAURI フロントエンド (UI)                          |
|         React / TypeScript + TailwindCSS + Lucide Icons + Stream Renderer     |
|  [ 動画 / キャラ選択 ]    [ タイムラインナビ ]    [ 統計・履歴ダッシュボード ]  |
+---------------------------------------+---------------------------------------+
                                        | IPC (Tauri Commands / Events)
+---------------------------------------v---------------------------------------+
|                             RUST バックエンドコア                              |
|                                                                               |
|  +------------------------+  +------------------------+  +-----------------+  |
|  |    Video Pipeline      |  |     CV / OCR Engine    |  | SQLite Storage  |  |
|  | (opencv-rust)          |  | (Replay UI Parse & OCR)|  |   (rusqlite)    |  |
|  +-----------+------------+  +-----------+------------+  +--------+--------+  |
|              |                           |                        |           |
|              +-------------------+-------+                        |           |
|                                  |                                |           |
|                        +---------v--------------------------------v--------+  |
|                        |        構造化 RAG & コンテキストアセンブリ        |  |
|                        +-------------------------+------------------------+  |
|                                                  | C++ Direct Binding     |
|                        +-------------------------v------------------------+  |
|                        |     組み込み LLM 推論エンジン (llama-cpp-rs)     |  |
|                        |       LFM3-VL-2B GGUF In-Process                 |  |
|                        +--------------------------------------------------+  |
+-------------------------------------------------------------------------------+

```

## 3. SQLite 構造化対戦データベース & JSONマスターデータ設計

### 3.1 SQLite データベース設計

対戦記録およびフレームデータマスターを SQLite データベース (`st6_trainer.db`) に保存・管理する。アプリ起動時にマスター JSON からデータをロードし、以下のテーブル群へ流し込んで高速な参照を実現する。

```
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
    result TEXT NOT NULL -- WIN / LOSS
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

```

### 3.2 フレームデータ マスターJSON構造（Classic / Modern 対応）

公式WEBページの全フィールド構造（発生・持続・硬直・ヒット/ガード硬直差・キャンセル・ダメージ・コンボ補正・ドライブゲージ増減・SAゲージ増加・攻撃属性・備考欄等）を包含した、操作タイプ（`classic` / `modern`）別のJSONマスターデータ仕様である。本データは外部アセットとして保持され、アプリ起動時に SQLite マスターテーブルへインポートされる。

```
{
  "character_id": "aki",
  "character_name": "A.K.I.",
  "control_type": "classic",
  "moves": [
    {
      "move_name": "Standing Light Punch",
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

## 4. LLM/VLM推論モジュール仕様 (組み込み型)

* **モデル名:** `LFM3-VL-2B`

* **実行方式:** 外部プロセスを使わず、C++バインディングを通じて Rust バックエンド内部で直接モデルを呼び出す。ハードウェアアクセラレーション（GPU）を推奨するが、CPUのみの環境でもフォールバック動作により完全に実行可能（速度面のみトレードオフ）。

## 5. Tauriフロントエンド & UIインターフェース設計

```
+-----------------------------------------------------------------------------------+
| ST6 AI Trainer v1.6.1                           [ 🎬 動画読込 ] [ LLM: Loaded🟢 ] |
| [自キャラ: A.K.I.▼] [敵キャラ: Ken▼] [操作: モダン▼] [デバイス: パッド▼]           |
+--------------------------+--------------------------------------------------------+
| 検出タイムライン         | レポート詳細表示エリア (Markdown Renderer)               |
| ------------------------ | ------------------------------------------------------ |
| [0:14] Punish Counter 🔴 | # 📁 試合分析レポート                                  |
|  - 硬直差: -14F          | ## 1. 何が起きたか（状況分析）                          |
|  - 入力履歴取得済み      | A.K.I.の技をガード後、硬直差 -14F が発生していましたが... |
+--------------------------+--------------------------------------------------------+

```
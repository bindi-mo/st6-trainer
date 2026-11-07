# 🎬 Street Fighter 6-Trainer 動画スキャン仕様書 (v1.7.0)

本書は、対戦動画（MP4 / MKV）から1フレームごとに映像を読み込み、画面上の指定領域（ROI）から生データを抽出・出力するための具体的実装仕様を定義する。

## 1. 目的とモジュール概要

入力された動画ファイルを先頭から1フレームずつ順次読み込み、指定された関心領域（ROI）のピクセル情報を解析して、フレームごとの生データを出力する。

## 2. 依存ライブラリ & デコード処理仕様

### 2.1 採用ライブラリ

* **動画デコード・画像処理:** `opencv` (Rust `opencv-rust` バインディング)

* **並列処理:** `rayon`（フレーム単位の並列イテレーション）

### 2.2 フレームデコードのループ構造

動画ファイルはメモリ全体へ一括展開せず、100フレーム単位のリングバッファを用いてストリーミング方式で順次デコードする。

```
use opencv::prelude::*;
use opencv::videoio::{VideoCapture, CAP_PROP_FRAME_COUNT, CAP_PROP_FPS};

pub fn scan_video_file(path: &str) -> Result<Vec<RawFrameLog>, opencv::Error> {
    let mut cap = VideoCapture::from_file(path, opencv::videoio::CAP_ANY)?;
    let total_frames = cap.get(CAP_PROP_FRAME_COUNT)? as usize;
    let fps = cap.get(CAP_PROP_FPS)?;

    let mut frame_logs = Vec::with_capacity(total_frames);
    let mut frame = Mat::default();
    let mut frame_index = 0;

    // 最初のフレームでHUDの動的アンカー（位置）を一度特定する
    let mut hud_layout = None;

    while cap.read(&mut frame)? {
        if frame.empty() {
            break;
        }

        // 1. 解像度の正規化 (1080p キャンバスへアスペクト比補正)
        let normalized_frame = normalize_resolution(&frame)?;

        // 2. 初回フレーム、または一定間隔でHUD位置を動的検出・校正
        if hud_layout.is_none() || frame_index % 300 == 0 {
            hud_layout = Some(detect_hud_layout(&normalized_frame)?);
        }

        // 3. 動的校正されたROIを使用して各領域を切り出し・抽出
        let raw_log = extract_frame_signals(&normalized_frame, hud_layout.as_ref().unwrap(), frame_index, fps);
        frame_logs.push(raw_log);

        frame_index += 1;
    }

    Ok(frame_logs)
}




```

## 3. 解像度の正規化パイプライン

異なるアスペクト比（16:9, 16:10, 21:9等）の動画入力を統一するため、全フレームを $1920 \times 1080$ の標準座標系へマッピングする。

* **スケール計算式:**

  $$
  \text{Scale} = \min\left(\frac{1920}{W_{\text{src}}}, \frac{1080}{H_{\text{src}}}\right)
  $$

* **パディングオフセット:**

  $$
  \Delta X = \frac{1920 - (W_{\text{src}} \times \text{Scale})}{2}, \quad \Delta Y = \frac{1080 - (H_{\text{src}} \times \text{Scale})}{2}
  $$

## 4. ドライブゲージおよびHUD要素の動的検出仕様

ストリートファイター6のゲーム内設定やリプレイ形式により、HUD（体力ゲージ、ドライブゲージ等）の表示位置が変動する場合がある。そのため、固定座標に依存せず、以下の視覚的アンカー（基準点）をテンプレートマッチングまたは輪郭検出によりスキャン開始時に特定し、各ROIの座標を動的に決定する。

### 4.1 動的アンカーの検出対象

1. **中央タイマー領域:** 画面上部中央の残り時間表示数字（例: "98", "ROUND 1" 等の背景フレーム）。これを原点 $(X_{\text{timer}}, Y_{\text{timer}})$ とする。

2. **体力ゲージの起算点:** 左右の体力ゲージの端部フレーム枠。

### 4.2 ドライブゲージの動的算出ロジック

タイマーの中心位置を基準 $(X_0, Y_0)$ とし、そこからのオフセット量として1P/2Pのドライブゲージ位置を算術的に確定する。

* **1Pドライブゲージ動的計算例:**

  $$
  X_{\text{min\_1p}} = X_0 - \text{Offset}_{\text{left}}, \quad Y_{\text{min\_1p}} = Y_0 + \text{Offset}_{\text{gauge\_y}}
  $$

* これにより、ユーザーがHUD位置を微調整した場合でも、ドライブゲージの6ブロック構造やバーンアウト状態（ゲージの点滅・色変化）を正確に追従して切り出すことが可能となる。

## 5. 関心領域 (ROI) 座標定義 (1080p基準 / 動的補正ベースライン)

| 領域名 | 1P側 検出範囲 $(X_{\min}, Y_{\min}, X_{\max}, Y_{\max})$ | 2P側 検出範囲 $(X_{\min}, Y_{\min}, X_{\max}, Y_{\max})$ | 目的・用途 |
| ----- | ----- | ----- | ----- |
| **キー入力履歴** | `(0.010, 0.170, 0.090, 0.820)` | `(0.910, 0.170, 0.990, 0.820)` | コマンド履歴のアイコン・矢印検出 |
| **ドライブゲージ** | `(0.280, 0.115, 0.490, 0.135)` *(動的補正可)* | `(0.510, 0.115, 0.720, 0.135)` *(動的補正可)* | ゲージ残量・ピクセル変化・バーンアウト検出 |
| **システム通知** | 画面中央 `(0.650, 0.430, 0.850, 0.480)` *(共通)* | \- | "COUNTER", "PUNISH COUNTER" 等の文字検出 |
| **フレームメーター** | 画面下部 `(0.180, 0.700, 0.820, 0.820)` *(共通)* | \- | 発生、全体、硬直差の数値読み取り |

## 6. 前処理パイプライン（カラーマスキング & 二値化）

キー入力履歴およびシステムテキストの精度を担保するため、以下の OpenCV 画像前処理を順次適用する。

1. **半透明背景の分離:** 背景のステージオブジェクト等との同化を防ぐため、対象 ROI に対して明度バランスの補正を行う。

2. **HSVカラーマスキング (`cv2.inRange` 相当):**

   * 白文字や特定のUI発光カラーを抽出するため、HSV色空間における閾値処理を行う。

   ```
   let mut hsv_mat = Mat::default();
   opencv::imgproc::cvt_color(&roi_mat, &hsv_mat, opencv::imgproc::COLOR_BGR2HSV, 0)?;
   let mut mask = Mat::default();
   opencv::core::in_range(&hsv_mat, &Scalar::new(0.0, 0.0, 200.0, 0.0), &Scalar::new(180.0, 30.0, 255.0), &mut mask)?;




   ```

3. **大津の二値化 (Otsu's Thresholding):**

   ```
   let mut thresh_mat = Mat::default();
   opencv::imgproc::threshold(&mask, &mut thresh_mat, 0.0, 255.0, opencv::imgproc::THRESH_BINARY | opencv::imgproc::THRESH_OTSU)?;




   ```

## 7. 入力アイコンおよびキー認識ルール

キー入力履歴エリアから抽出される生データは、以下のグラフィック・アイコンパターンに分類して認識する。

* **方向キー（テンキー対応）:**

  * `5` (ニュートラル / 欠損)

  * `2`, `4`, `6`, `8` (上下左右)

  * `1`, `3`, `7`, `9` (斜め入力)

* **クラシック操作の攻撃ボタン:**

  * パンチ: `LP`, `MP`, `HP`

  * キック: `LK`, `MK`, `HK`

  * 同時押し: 複数ボタンの重なり検知

* **モダン操作の攻撃ボタン:**

  * `L` (弱攻撃), `M` (中攻撃), `H` (強攻撃), `SP` (スペシャルボタン), `AUTO` (オートボタン), `DP` (DPボタン)

## 8. 出力データ構造体 (Rust `RawFrameLog`)

スキャン処理の結果は、以下の構造体形式でメモリ上に保持され、後続の処理へ引き渡される。

```
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFrameLog {
    pub frame_index: usize,
    pub timestamp_ms: u64,
    pub p1_input_raw: Option<String>,
    pub p2_input_raw: Option<String>,
    pub system_text_raw: Option<String>,
    pub p1_drive_gauge_raw: u8,
    pub p2_drive_gauge_raw: u8,
    pub p1_burnout: bool,
    pub p2_burnout: bool,
    pub frame_meter_raw: Option<FrameMeterRaw>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMeterRaw {
    pub startup_detected: Option<i32>,
    pub active_detected: Option<i32>,
    pub advantage_detected: Option<i32>,
}




```
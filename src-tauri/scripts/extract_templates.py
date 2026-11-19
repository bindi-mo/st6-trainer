#!/usr/bin/env python3
"""
SF6 Template Extractor
- ファイルセレクターで動画選択
- OpenCV で動画を開く
- キー操作でフレーム送り/戻し
- マウスで矩形選択 → Enter で PNG 保存
- マウスホイールで拡大/縮小
- 右ボタンドラッグでビューポート移動
- 保存先: src-tauri/templates/<category>/<name>.png
"""

import sys
import threading
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, simpledialog

import cv2
import numpy as np

# OpenCVウィンドウ専用スレッド開始（Tkinterダイアログ中もウィンドウ正常動作）
cv2.startWindowThread()


# ===== 修飾キー検出 =====
# OpenCV の waitKeyEx は Windows では修飾キー状態をキーコードに含めない
# （矢印キーは VK コード << 16、文字キーは文字コードのみ）。
# そのため Win32 API の GetKeyState で直接取得する。
if sys.platform == "win32":
    import ctypes

    _user32 = ctypes.windll.user32
    _user32.GetKeyState.restype = ctypes.c_short
    _VK_SHIFT = 0x10
    _VK_CONTROL = 0x11

    def is_shift_down():
        return bool(_user32.GetKeyState(_VK_SHIFT) & 0x8000)

    def is_ctrl_down():
        return bool(_user32.GetKeyState(_VK_CONTROL) & 0x8000)

else:

    def is_shift_down():
        return False

    def is_ctrl_down():
        return False


# ===== 設定 =====
DEFAULT_VIDEO_PATH = Path(__file__).parent.parent / "videos"
TEMPLATE_ROOT = Path(__file__).parent.parent / "templates"

# カテゴリ定義
# 各エントリ: "キー": (ディレクトリ名, [テンプレート名...])
# 保存先は CATEGORY_LOCALES を参照して決定される（多言語カテゴリは <dir>/<locale>/ 以下）
CATEGORIES = {
    "1": (
        "anchor",
        [
            "upper_timer_top_edge",
            "upper_p1_hp_corner",
            "upper_p2_hp_corner",
            "upper_p1_drive_corner",
            "upper_p2_drive_corner",
            "lower_p1_sa_bracket",
            "lower_p2_sa_bracket",
            "lower_frame_meter_top_edge",
        ],
    ),
    "2": (
        "btn_classic",
        ["LP", "MP", "HP", "LK", "MK", "HK", "classic"],
    ),
    "3": (
        "btn_modern",
        ["L", "M", "H", "SP", "DP", "DI", "A", "modern"],
    ),
    "4": ("dir", ["1", "2", "3", "4", "6", "7", "8", "9"]),
    "5": (
        "num",
        [
            "0",
            "1",
            "2",
            "3",
            "4",
            "5",
            "6",
            "7",
            "8",
            "9",
            "minus",
            "F",
        ],
    ),
    "6": (
        "names",
        [
            "ryu",
            "luke",
            "jamie",
            "chunli",
            "guile",
            "kimberly",
            "juri",
            "ken",
            "blanka",
            "dhalsim",
            "ehonda",
            "deejay",
            "manon",
            "jp",
            "zangief",
            "lily",
            "cammy",
            "rashid",
            "aki",
            "ed",
            "gouki_akuma",
            "vega_mbison",
            "terry",
            "mai",
            "elena",
            "sagat",
            "cviper",
            "alex",
            "ingrid",
            "yasmine",
            "arjun",
        ],
    ),
    "7": ("attack_property", ["High", "Mid", "Low", "Throw"]),
}

# ===== multilingual categories =====
# dir name -> locale definitions (order = Prev/Next cycle order)
# Categories listed here are saved under <dir>/<locale>/.
# Categories not listed keep the legacy layout <dir>/.
#
# ファイル名は言語間で共通（例: attack_property/EN/High.png と
# attack_property/JP/High.png）。言語ごとに別名を付けたい場合は
# 3 要素目の dict で {locale: name} を指定する。
CATEGORY_LOCALES = {
    "attack_property": [
        ("EN", ["High", "Mid", "Low", "Throw"]),
        ("JP", ["High", "Mid", "Low", "Throw"]),
    ],
}

# Keys for language switching (L = Language)
KEY_NEXT_LOCALE = ord("l")
KEY_PREV_LOCALE = ord("L")

# ===== 定数 =====
WND_NAME = "Template Extractor"
SEEKBAR_HEIGHT = 40
STATUSBAR_HEIGHT = 33
DISP_W, DISP_H = 1280, 720


# ===== グローバル状態 =====
class State:
    def __init__(self):
        self.cap = None
        self.frame = None
        self.frame_idx = 0
        self.total_frames = 0
        self.selecting = False
        self.sel_start = (0, 0)
        self.sel_end = (0, 0)
        self.current_category = ""
        self.current_names = []
        self.name_idx = 0
        self.locale_idx = 0  # CATEGORY_LOCALES 内の言語インデックス
        self.zoom = 1.0
        self.pan_x = 0
        self.pan_y = 0
        self.crop_x = 0  # 表示画像の切り出しオフセット（resized 座標系）
        self.crop_y = 0
        self.panning = False
        self.pan_start = (0, 0)
        self.seeking = False
        self.seek_thread = None
        self.seek_target_frame = -1
        self.seek_result_frame = -1
        self.seek_result_frame_data = None
        self.seek_done = False  # ワーカー完了フラグ（失敗時も True）
        self.seekbar_dragging = False
        self.seekbar_hover = -1
        self.show_help = False  # ヘルプ表示フラグ
        self._lock = threading.Lock()  # スレッド同期用ロック
        self._cap_lock = threading.Lock()  # VideoCapture アクセス用ロック


S = State()


# ===== ロケール解決 =====
def get_locales(category):
    """カテゴリのロケール定義を返す。

    多言語カテゴリ（CATEGORY_LOCALES に登録済み）は (locale, names) のリスト、
    それ以外は [("", names)] 形式の単一要素リストを返す。
    """
    if category in CATEGORY_LOCALES:
        return CATEGORY_LOCALES[category]
    return [("", CATEGORIES_BY_DIR.get(category, []))]


def get_locale_name(category, locale_idx):
    locales = get_locales(category)
    if not locales:
        return ""
    locale_idx = max(0, min(locale_idx, len(locales) - 1))
    return locales[locale_idx][0]


def get_locale_names(category, locale_idx):
    locales = get_locales(category)
    if not locales:
        return []
    locale_idx = max(0, min(locale_idx, len(locales) - 1))
    return locales[locale_idx][1]


def get_output_dir(category, locale_idx):
    """保存先ディレクトリを返す。

    ロケールが定義されている場合は <dir>/<locale>/、未定義の場合は <dir>/。
    """
    base = TEMPLATE_ROOT / category
    locale = get_locale_name(category, locale_idx)
    return base / locale if locale else base


def is_multilang(category):
    return len(get_locales(category)) > 1


# ディレクトリ名 → テンプレート名リスト（カテゴリ選択の初期化に使う）
CATEGORIES_BY_DIR = {v[0]: v[1] for v in CATEGORIES.values()}


# ===== ユーティリティ =====
def draw_text_with_bg(
    img,
    text,
    pos,
    font_scale=0.6,
    color=(255, 255, 255),
    thickness=1,
    bg_color=(0, 0, 0),
    alpha=0.7,
    padding=4,
):
    font = cv2.FONT_HERSHEY_SIMPLEX
    (tw, th), baseline = cv2.getTextSize(text, font, font_scale, thickness)
    x, y = pos
    overlay = img.copy()
    cv2.rectangle(
        overlay,
        (x - 4 - padding, y - (th + baseline) - padding),
        (x + tw + padding, y + 3 + baseline + padding),
        bg_color,
        -1,
    )
    cv2.addWeighted(overlay, alpha, img, 1 - alpha, 0, img)
    cv2.putText(
        img, text, (x, y), font, font_scale, color, thickness, cv2.LINE_AA
    )


def add_info(text, color=(255, 255, 255)):
    """状態メッセージをコンソールに出力する"""
    print(f"[Info] {text}")


# ===== 描画 =====
def draw_ui(img):
    if S.selecting or (
        S.sel_end[0] != S.sel_start[0] and S.sel_end[1] != S.sel_start[1]
    ):
        ds = (
            int(S.sel_start[0] * S.zoom - S.crop_x),
            int(S.sel_start[1] * S.zoom - S.crop_y),
        )
        de = (
            int(S.sel_end[0] * S.zoom - S.crop_x),
            int(S.sel_end[1] * S.zoom - S.crop_y),
        )
        cv2.rectangle(img, ds, de, (0, 255, 0), 2)
        x1, y1 = S.sel_start
        x2, y2 = S.sel_end
        label = f"{abs(x2 - x1)}x{abs(y2 - y1)}"
        # 座標ラベルが矩形の線と重ならないよう、開始点の外側に表示する
        font = cv2.FONT_HERSHEY_SIMPLEX
        (_, th), baseline = cv2.getTextSize(label, font, 0.5, 1)
        pad = 4
        above_y = ds[1] - (baseline + pad + 7)
        below_y = ds[1] + th + baseline + pad + 4
        if y2 >= y1:
            # 下方向に作成: 上辺の少し上に表示
            label_y = above_y
            if label_y - (th + baseline) - pad < 0:
                label_y = below_y
        else:
            # 上方向に作成: 下辺の少し下に表示
            label_y = below_y
            if label_y + baseline + pad > img.shape[0]:
                label_y = above_y
        draw_text_with_bg(
            img,
            label,
            (ds[0], label_y),
            font_scale=0.5,
            color=(0, 255, 0),
        )


def draw_seekbar(img):
    h, w = img.shape[:2]
    bar_y = h - SEEKBAR_HEIGHT
    cv2.rectangle(
        img, (0, bar_y), (w, bar_y + SEEKBAR_HEIGHT), (40, 40, 40), -1
    )
    prog = S.frame_idx / max(1, S.total_frames - 1)
    pw = int(w * prog)
    cv2.rectangle(
        img, (0, bar_y), (pw, bar_y + SEEKBAR_HEIGHT), (0, 120, 255), -1
    )
    draw_text_with_bg(
        img,
        f"Frame: {S.frame_idx}/{S.total_frames}",
        (14, bar_y + 26),
        font_scale=0.5,
        color=(255, 255, 255),
    )
    if S.seekbar_dragging and S.seekbar_hover >= 0:
        cv2.line(
            img,
            (S.seekbar_hover, bar_y),
            (S.seekbar_hover, bar_y + SEEKBAR_HEIGHT),
            (255, 255, 0),
            2,
        )
        pf = int(S.seekbar_hover / w * S.total_frames)
        draw_text_with_bg(
            img,
            f"{pf}/{S.total_frames}",
            (S.seekbar_hover + 5, bar_y + 26),
            font_scale=0.5,
            color=(255, 255, 0),
        )
    cx = int(w * prog)
    cv2.line(
        img, (cx, bar_y), (cx, bar_y + SEEKBAR_HEIGHT), (255, 255, 255), 2
    )


def draw_status_bar(img):
    h, w = img.shape[:2]
    bar_y = h - SEEKBAR_HEIGHT - STATUSBAR_HEIGHT
    overlay = img.copy()
    cv2.rectangle(
        overlay, (0, bar_y), (w, bar_y + STATUSBAR_HEIGHT), (0, 0, 0), -1
    )
    cv2.addWeighted(overlay, 0.7, img, 0.3, 0, img)
    parts = [f"Zoom: {S.zoom:.1f}x"]
    if S.current_category:
        parts.append(f"Category: {S.current_category}")
        if is_multilang(S.current_category):
            locales = get_locales(S.current_category)
            cur = get_locale_name(S.current_category, S.locale_idx)
            parts.append(
                f"Language: {cur} ({S.locale_idx + 1}/{len(locales)}) [L/Shift+L]"
            )
        if S.name_idx < len(S.current_names):
            parts.append(
                f"Next: {S.current_names[S.name_idx]} ({S.name_idx + 1}/{len(S.current_names)})"
            )
        else:
            parts.append("Complete!")
    else:
        parts.append(
            f"Category: Select a category from 1 to {len(CATEGORIES)}."
        )
    if S.seeking:
        parts.append("SEEKING...")
    draw_text_with_bg(
        img,
        " | ".join(parts),
        (8, bar_y + STATUSBAR_HEIGHT - 11),
        font_scale=0.5,
        color=(200, 255, 200),
        thickness=1,
    )


def draw_help(img):
    """操作説明ヘルプを画面中央に2列で表示"""
    h, w = img.shape[:2]
    # 半透明オーバーレイ
    overlay = img.copy()
    cv2.rectangle(overlay, (0, 0), (w, h), (0, 0, 0), -1)
    cv2.addWeighted(overlay, 0.85, img, 0.15, 0, img)

    # 左列：キー操作
    left_lines = [
        "=== Controls ===",
        "",
        "  Right arrow        : Next frame",
        "  Shift+Right        : +10 frames",
        "  Ctrl+Right         : +100 frames",
        "  Left arrow         : Previous frame",
        "  Shift+Left         : -10 frames",
        "  Ctrl+Left          : -100 frames",
        "  Space              : Next frame",
        "  Backspace          : Previous frame",
        "  Trackbar (bottom)  : Click to jump",
        "",
        "  1-7                : Select category",
        "  Left drag          : Select ROI",
        "  Right drag         : Pan viewport",
        "  Mouse wheel        : Zoom in/out",
        "  Enter              : Save template",
        "  n                  : Next template",
        "  p                  : Prev template",
        "  l / Shift+L        : Next/Prev language",
        "  r                  : Reset zoom/pan",
        "  /                  : Show help",
        "  q                  : Quit",
    ]

    # 右列：カテゴリ
    right_lines = [
        "=== Categories ===",
        "",
        "  1: anchor (8 items)",
        "  2: btn_classic (7, incl. classic badge)",
        "  3: btn_modern (8, incl. modern badge)",
        "  4: dir (8 items)",
        "  5: num (13 items)",
        "  6: names (dynamic)",
        "  7: attack_property (4, multi-lang)",
        "",
        "Press any key or click to close",
    ]

    font = cv2.FONT_HERSHEY_SIMPLEX
    font_scale = 0.6
    thickness = 1
    line_height = 24
    col_gap = 40  # 列間の隙間

    # 各列の最大幅を計算
    def get_max_width(lines):
        max_w = 0
        for line in lines:
            (tw, _), _ = cv2.getTextSize(line, font, font_scale, thickness)
            max_w = max(max_w, tw)
        return max_w

    left_w = get_max_width(left_lines)
    right_w = get_max_width(right_lines)
    total_w = left_w + col_gap + right_w

    # 開始位置（中央揃え）
    start_x = (w - total_w) // 2
    left_x = start_x
    right_x = start_x + left_w + col_gap
    start_y = (h - max(len(left_lines), len(right_lines)) * line_height) // 2

    # 左列描画
    for i, line in enumerate(left_lines):
        if line == "":
            continue
        y = start_y + i * line_height
        color = (255, 255, 255)
        if line.startswith("==="):
            color = (200, 255, 200)
        draw_text_with_bg(
            img,
            line,
            (left_x, y),
            font_scale=font_scale,
            color=color,
            thickness=thickness,
            alpha=0.9,
        )

    # 右列描画
    for i, line in enumerate(right_lines):
        if line == "":
            continue
        y = start_y + i * line_height
        color = (255, 255, 255)
        if line.startswith("==="):
            color = (200, 255, 200)
        if line.startswith("Press any key"):
            color = (0, 255, 255)
        draw_text_with_bg(
            img,
            line,
            (right_x, y),
            font_scale=font_scale,
            color=color,
            thickness=thickness,
            alpha=0.9,
        )


# ===== マウス =====
def on_mouse(event, x, y, flags, param):
    sb_y = DISP_H - SEEKBAR_HEIGHT
    st_y = DISP_H - SEEKBAR_HEIGHT - STATUSBAR_HEIGHT
    in_sb = y >= sb_y
    in_st = st_y <= y < sb_y

    # ヘルプ表示中は左右どちらのクリックでも閉じる
    if S.show_help and event in (
        cv2.EVENT_LBUTTONDOWN,
        cv2.EVENT_RBUTTONDOWN,
        cv2.EVENT_MBUTTONDOWN,
    ):
        S.show_help = False
        return

    # 動画未読み込み（プレースホルダ表示中）は何もしない
    if S.frame is None or S.cap is None:
        return

    img_x = int((x + S.crop_x) / S.zoom)
    img_y = int((y + S.crop_y) / S.zoom)

    if in_sb:
        if event == cv2.EVENT_LBUTTONDOWN:
            S.seekbar_dragging = True
            S.seekbar_hover = x
        elif event == cv2.EVENT_MOUSEMOVE:
            S.seekbar_hover = x
        elif event == cv2.EVENT_LBUTTONUP and S.seekbar_dragging:
            tf = int(x / DISP_W * S.total_frames)
            tf = max(0, min(tf, S.total_frames - 1))
            if start_seek(tf):
                add_info(f"Seek: {tf}/{S.total_frames}", (0, 255, 255))
            S.seekbar_dragging = False
            S.seekbar_hover = -1
        return

    if in_st:
        return

    if event == cv2.EVENT_LBUTTONDOWN:
        S.selecting = True
        S.sel_start = (img_x, img_y)
        S.sel_end = (img_x, img_y)
    elif event == cv2.EVENT_MOUSEMOVE and S.selecting:
        S.sel_end = (img_x, img_y)
    elif event == cv2.EVENT_LBUTTONUP and S.selecting:
        S.selecting = False
        S.sel_end = (img_x, img_y)
        x1, y1 = S.sel_start
        x2, y2 = S.sel_end
        if x1 != x2 and y1 != y2:
            x1, x2 = sorted([x1, x2])
            y1, y2 = sorted([y1, y2])
            S.sel_start = (x1, y1)
            S.sel_end = (x2, y2)
            add_info(
                f"Selected: ({x1},{y1})-({x2},{y2}) size={x2 - x1}x{y2 - y1}",
                (0, 255, 255),
            )

    if event == cv2.EVENT_RBUTTONDOWN:
        S.panning = True
        S.pan_start = (x, y)
    elif event == cv2.EVENT_MOUSEMOVE and S.panning:
        dx = x - S.pan_start[0]
        dy = y - S.pan_start[1]
        S.pan_x -= dx
        S.pan_y -= dy
        S.pan_start = (x, y)
    elif event == cv2.EVENT_RBUTTONUP:
        S.panning = False
    elif event == cv2.EVENT_MOUSEWHEEL:
        fac = 1.1 if flags > 0 else 1 / 1.1
        nz = max(0.1, min(10.0, S.zoom * fac))
        if S.zoom != nz:
            # マウス直下の元画像座標を維持するように pan を更新する。
            # 表示座標 (x,y) → resized 座標 (x+crop) → 元画像座標 (÷zoom)
            rx = x + S.crop_x
            ry = y + S.crop_y
            ox = rx / S.zoom
            oy = ry / S.zoom
            h, w = S.frame.shape[:2]
            nw, nh = int(w * nz), int(h * nz)
            # 新しい zoom で同じ元画像座標が同じ画面位置に来る crop を求める
            new_crop_x = ox * nz - x
            new_crop_y = oy * nz - y
            # crop = nw//2 + pan - DISP//2 の逆算
            S.pan_x = int(new_crop_x - nw // 2 + DISP_W // 2)
            S.pan_y = int(new_crop_y - nh // 2 + DISP_H // 2)
            S.zoom = nz


# ===== シークワーカー =====
def seek_worker(tf):
    """指定フレームへシークする。

    VideoCapture は複数スレッドから同時に触ると OpenCV 内部で
    "Unknown C++ exception" を投げることがあるため、
    _cap_lock で排他制御する。
    """
    print(f"[Worker] Seeking to frame {tf}")
    frame_data = None
    ok = False
    try:
        with S._cap_lock:
            cap = S.cap
            if cap is None or not cap.isOpened():
                print("[Worker] Capture is not available")
                return
            tf = max(0, min(int(tf), max(0, S.total_frames - 1)))
            cap.set(cv2.CAP_PROP_POS_FRAMES, tf)
            ret, nf = cap.read()
            if ret and nf is not None and nf.size > 0:
                frame_data = nf.copy()
                ok = True
            print(f"[Worker] Read result: ret={ret}")
    except cv2.error as e:
        print(f"[Worker] OpenCV error: {e}")
    except Exception as e:
        print(f"[Worker] Error: {type(e).__name__}: {e}")
    finally:
        with S._lock:
            if ok:
                S.seek_result_frame = tf
                S.seek_result_frame_data = frame_data
            else:
                S.seek_result_frame = -1
                S.seek_result_frame_data = None
            S.seek_done = True


def start_seek(tf):
    """シークを開始する（多重起動防止付き）。"""
    if S.seeking:
        return False
    tf = max(0, min(int(tf), max(0, S.total_frames - 1)))
    if tf == S.frame_idx:
        return False
    with S._lock:
        S.seek_result_frame = -1
        S.seek_result_frame_data = None
        S.seek_done = False
    S.seek_target_frame = tf
    S.seeking = True
    S.seek_thread = threading.Thread(
        target=seek_worker, args=(tf,), daemon=True
    )
    S.seek_thread.start()
    return True


# ===== ファイル選択 =====
def select_video_file():
    fp = filedialog.askopenfilename(
        title="Select Replay Video",
        filetypes=[
            ("Video files", "*.mkv *.mp4 *.avi *.mov"),
            ("All files", "*.*"),
        ],
        initialdir=str(DEFAULT_VIDEO_PATH)
        if DEFAULT_VIDEO_PATH.exists()
        else str(Path.home()),
    )
    return Path(fp) if fp else None


def go_to_frame():
    root = tk.Tk()
    root.withdraw()  # 最初に非表示（チラつき防止）
    root.attributes("-topmost", True)
    fn = simpledialog.askinteger(
        "Go to Frame",
        f"Enter frame number (0 - {S.total_frames - 1}):",
        minvalue=0,
        maxvalue=S.total_frames - 1,
        initialvalue=S.frame_idx,
    )
    root.destroy()
    if fn is not None:
        start_seek(fn)


def make_placeholder(text):
    """動画読み込み前に表示するプレースホルダ画像を作る。"""
    img = np.zeros((DISP_H, DISP_W, 3), dtype=np.uint8)
    font = cv2.FONT_HERSHEY_SIMPLEX
    font_scale = 0.9
    thickness = 2
    (tw, th), _ = cv2.getTextSize(text, font, font_scale, thickness)
    cv2.putText(
        img,
        text,
        ((DISP_W - tw) // 2, (DISP_H + th) // 2),
        font,
        font_scale,
        (200, 200, 200),
        thickness,
        cv2.LINE_AA,
    )
    return img


def main():
    # 1. ウィンドウを作成して表示する
    cv2.namedWindow(WND_NAME, cv2.WINDOW_NORMAL)
    cv2.resizeWindow(WND_NAME, DISP_W, DISP_H)
    cv2.setMouseCallback(WND_NAME, on_mouse)
    cv2.imshow(WND_NAME, make_placeholder("Select a video file..."))
    cv2.waitKey(1)  # ウィンドウ存在確定

    # 2. ウィンドウ表示後にファイル選択ダイアログを出す
    vp = select_video_file()
    if vp is None or not vp.exists():
        print("No video selected or not found.")
        cv2.destroyAllWindows()
        return

    try:
        S.cap = cv2.VideoCapture(
            str(vp),
            cv2.CAP_FFMPEG,
            params=[
                cv2.CAP_PROP_HW_ACCELERATION,
                cv2.VIDEO_ACCELERATION_ANY,
                cv2.CAP_PROP_N_THREADS,
                0,
            ],
        )
        if not S.cap.isOpened():
            print("Failed to open video")
            return

        S.total_frames = int(S.cap.get(cv2.CAP_PROP_FRAME_COUNT))
        print(f"Video: {vp} ({S.total_frames} frames)")

        # 3. 最初のフレーム読み込み
        S.cap.set(cv2.CAP_PROP_POS_FRAMES, 0)
        ret, S.frame = S.cap.read()
        if not ret:
            print("Failed to read first frame")
            return

        while True:
            # ヘルプ表示
            if S.show_help:
                # S.frame を表示サイズにリサイズしてヘルプ用ベース画像を作成
                h, w = S.frame.shape[:2]
                min_z = max(DISP_W / w, DISP_H / h)
                zoom = max(S.zoom, min_z)
                nw, nh = int(w * zoom), int(h * zoom)
                resized = cv2.resize(
                    S.frame, (nw, nh), interpolation=cv2.INTER_LINEAR
                )
                mx, my = (nw - DISP_W) // 2, (nh - DISP_H) // 2
                pan_x = max(-mx, min(mx, S.pan_x))
                pan_y = max(-my, min(my, S.pan_y))
                cx, cy = nw // 2 + pan_x, nh // 2 + pan_y
                x1 = max(0, min(cx - DISP_W // 2, nw - DISP_W))
                y1 = max(0, min(cy - DISP_H // 2, nh - DISP_H))
                S.crop_x, S.crop_y = x1, y1
                help_base = resized[y1 : y1 + DISP_H, x1 : x1 + DISP_W]

                help_display = help_base.copy()
                draw_help(help_display)
                cv2.imshow(WND_NAME, help_display)
                # キー入力またはクリックで閉じる。
                # waitKeyEx(0) でブロックするとマウスコールバックで
                # show_help が False になっても復帰しないため、
                # 短いタイムアウトでポーリングする。
                while S.show_help:
                    help_key = cv2.waitKeyEx(30) & 0xFFFFFFFF
                    if help_key != 0xFFFFFFFF and help_key != -1:
                        S.show_help = False
                        break
                    if (
                        cv2.getWindowProperty(WND_NAME, cv2.WND_PROP_VISIBLE)
                        < 1
                    ):
                        S.show_help = False
                        break
                continue

            # フレーム処理
            h, w = S.frame.shape[:2]
            min_z = max(DISP_W / w, DISP_H / h)
            S.zoom = max(S.zoom, min_z)
            nw, nh = int(w * S.zoom), int(h * S.zoom)
            resized = cv2.resize(
                S.frame, (nw, nh), interpolation=cv2.INTER_LINEAR
            )
            mx, my = (nw - DISP_W) // 2, (nh - DISP_H) // 2
            S.pan_x = max(-mx, min(mx, S.pan_x))
            S.pan_y = max(-my, min(my, S.pan_y))
            cx, cy = nw // 2 + S.pan_x, nh // 2 + S.pan_y
            x1 = max(0, min(cx - DISP_W // 2, nw - DISP_W))
            y1 = max(0, min(cy - DISP_H // 2, nh - DISP_H))
            S.crop_x, S.crop_y = x1, y1
            display = resized[y1 : y1 + DISP_H, x1 : x1 + DISP_W]

            # シーク完了適用（成功・失敗どちらでも完了扱いにする）
            if S.seeking and S.seek_done:
                with S._lock:
                    if (
                        S.seek_result_frame != -1
                        and S.seek_result_frame_data is not None
                    ):
                        S.frame = S.seek_result_frame_data
                        S.frame_idx = S.seek_result_frame
                    else:
                        add_info(
                            f"Seek failed: {S.seek_target_frame}",
                            (0, 0, 255),
                        )
                    S.seeking = False
                    S.seek_done = False
                    S.seek_result_frame = -1
                    S.seek_result_frame_data = None
                    S.seek_thread = None

            # 描画
            draw_ui(display)
            draw_seekbar(display)
            draw_status_bar(display)

            if S.seeking:
                sd = display.copy()
                draw_text_with_bg(
                    sd,
                    "SEEKING...",
                    (DISP_W // 2 - 120, DISP_H // 2),
                    font_scale=1.8,
                    color=(0, 0, 255),
                    thickness=3,
                    alpha=0.65,
                    bg_color=(0, 0, 0),
                    padding=8,
                )
                cv2.imshow(WND_NAME, sd)
                cv2.waitKey(1)
            else:
                cv2.imshow(WND_NAME, display)

            key = cv2.waitKeyEx(30) & 0xFFFFFFFF
            if key == 0xFFFFFFFF or key == -1:
                bk = -1
            else:
                bk = key & 0xFF
            # 修飾キーはキーコードに含まれないため Win32 API から取得
            sh, ct = is_shift_down(), is_ctrl_down()
            # 特殊キー（矢印キー等）のベースコード（修飾キーを除く）
            base_key = key & 0xFFFF0000
            if bk != -1 and bk != 255:
                print(f"Key: {key}, base: {bk}, shift: {sh}, ctrl: {ct}")

            if bk == ord("q"):
                break
            if cv2.getWindowProperty(WND_NAME, cv2.WND_PROP_VISIBLE) < 1:
                break
            if bk == ord("/"):
                if not S.show_help:
                    S.show_help = True
                    add_info(
                        "Help opened (press any key or click to close)",
                        (0, 255, 255),
                    )
            elif bk == ord("r"):
                S.zoom = 1.0
                S.pan_x = 0
                S.pan_y = 0
                add_info("Zoom/Pan reset", (255, 255, 0))
            elif bk == ord("g"):
                go_to_frame()
            elif base_key == 2555904:  # Right arrow key (with any modifier)
                st = 100 if ct else (10 if sh else 1)
                t = min(S.frame_idx + st, S.total_frames - 1)
                if start_seek(t):
                    add_info(
                        f"Seek: {S.frame_idx}->{t} (Right, step={st})",
                        (0, 255, 255),
                    )
            elif base_key == 2424832:  # Left arrow key (with any modifier)
                st = 100 if ct else (10 if sh else 1)
                t = max(S.frame_idx - st, 0)
                if start_seek(t):
                    add_info(
                        f"Seek: {S.frame_idx}->{t} (Left, step={st})",
                        (0, 255, 255),
                    )
            elif bk == 32:
                t = min(S.frame_idx + 1, S.total_frames - 1)
                if start_seek(t):
                    add_info(
                        f"Seek: {S.frame_idx}->{t} (Space)", (0, 255, 255)
                    )
            elif bk == 8:
                t = max(S.frame_idx - 1, 0)
                if start_seek(t):
                    add_info(
                        f"Seek: {S.frame_idx}->{t} (Backspace)",
                        (0, 255, 255),
                    )
            elif bk in [ord(str(i)) for i in range(1, len(CATEGORIES) + 1)]:
                ck = chr(bk)
                S.current_category, _ = CATEGORIES[ck]
                S.locale_idx = 0
                S.current_names = get_locale_names(
                    S.current_category, S.locale_idx
                )
                S.name_idx = 0
                if S.current_names:
                    loc = get_locale_name(S.current_category, S.locale_idx)
                    loc_note = f" [{loc}]" if loc else ""
                    add_info(
                        f"Category: {S.current_category}{loc_note} ({len(S.current_names)} items)",
                        (255, 255, 0),
                    )
            elif bk == KEY_NEXT_LOCALE or bk == KEY_PREV_LOCALE:
                if not S.current_category:
                    add_info("No category selected", (0, 0, 255))
                elif not is_multilang(S.current_category):
                    add_info(
                        f"{S.current_category} has no language variants",
                        (0, 0, 255),
                    )
                else:
                    step = -1 if bk == KEY_PREV_LOCALE else 1
                    n_locales = len(get_locales(S.current_category))
                    S.locale_idx = (S.locale_idx + step) % n_locales
                    S.current_names = get_locale_names(
                        S.current_category, S.locale_idx
                    )
                    S.name_idx = 0
                    loc = get_locale_name(S.current_category, S.locale_idx)
                    add_info(
                        f"Language: {loc} ({S.locale_idx + 1}/{n_locales}) "
                        f"- {len(S.current_names)} items",
                        (255, 255, 0),
                    )
            elif bk == ord("n"):
                if (
                    S.current_category
                    and S.name_idx < len(S.current_names) - 1
                ):
                    S.name_idx += 1
                    add_info(
                        f"Next: {S.current_names[S.name_idx]}", (255, 255, 0)
                    )
            elif bk == ord("p"):
                print(f"{S.name_idx} {S.current_names}")
                if S.current_category and S.name_idx > 0:
                    S.name_idx -= 1
                    add_info(
                        f"Prev: {S.current_names[S.name_idx]}", (255, 255, 0)
                    )
            elif bk == 13:
                if S.current_category and S.name_idx < len(S.current_names):
                    x1, y1 = S.sel_start
                    x2, y2 = S.sel_end
                    if x1 != x2 and y1 != y2:
                        x1, x2 = sorted([x1, x2])
                        y1, y2 = sorted([y1, y2])
                        roi = S.frame[y1:y2, x1:x2]
                        out_dir = get_output_dir(
                            S.current_category, S.locale_idx
                        )
                        out_dir.mkdir(parents=True, exist_ok=True)
                        out_path = (
                            out_dir / f"{S.current_names[S.name_idx]}.png"
                        )
                        rg = (
                            cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
                            if len(roi.shape) == 3
                            else roi
                        )
                        cv2.imwrite(str(out_path), rg)
                        print(f"Saved: {out_path}")
                        loc = get_locale_name(
                            S.current_category, S.locale_idx
                        )
                        loc_note = f"/{loc}" if loc else ""
                        add_info(
                            f"Saved: {S.current_category}{loc_note}/{S.current_names[S.name_idx]}",
                            (0, 255, 0),
                        )
                        S.selecting = False
                        S.sel_start = (0, 0)
                        S.sel_end = (0, 0)
                        S.name_idx += 1
                        if S.name_idx < len(S.current_names):
                            add_info(
                                f"Next: {S.current_names[S.name_idx]}",
                                (255, 255, 0),
                            )
                        else:
                            add_info(
                                f"Category {S.current_category} complete!",
                                (0, 255, 0),
                            )
                    else:
                        add_info(
                            "No selection (drag to select ROI)", (0, 0, 255)
                        )

    except Exception as e:
        print(f"Error: {type(e).__name__}: {e}")
    finally:
        # ワーカースレッドの終了を待ってからキャプチャを解放する
        if S.seek_thread is not None and S.seek_thread.is_alive():
            S.seek_thread.join(timeout=2.0)
        with S._cap_lock:
            if S.cap is not None:
                S.cap.release()
                S.cap = None
        cv2.destroyAllWindows()


if __name__ == "__main__":
    main()

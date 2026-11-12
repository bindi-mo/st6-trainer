#!/usr/bin/env python3
"""
Street Fighter 6 Frame Data Scraper & Converter CLI Tool.
Follows the sf6_frame_data_scraper_converter_specifications.md design specification.
"""

import argparse
import json
import os
import re

from bs4 import BeautifulSoup
from playwright.sync_api import sync_playwright

__version__ = "1.6.8"

# Character IDs supported by default (All names in uppercase, vitality dynamically parsed)
DEFAULT_CHARACTERS = [
    {"id": "ryu", "name": "RYU"},
    {"id": "luke", "name": "LUKE"},
    {"id": "jamie", "name": "JAMIE"},
    {"id": "chunli", "name": "CHUN-LI"},
    {"id": "guile", "name": "GUILE"},
    {"id": "kimberly", "name": "KIMBERLY"},
    {"id": "juri", "name": "JURI"},
    {"id": "ken", "name": "KEN"},
    {"id": "blanka", "name": "BLANKA"},
    {"id": "dhalsim", "name": "DHALSIM"},
    {"id": "ehonda", "name": "E.HONDA"},
    {"id": "deejay", "name": "DEE JAY"},
    {"id": "manon", "name": "MANON"},
    {"id": "marisa", "name": "MARISA"},
    {"id": "jp", "name": "JP"},
    {"id": "zangief", "name": "ZANGIEF"},
    {"id": "lily", "name": "LILY"},
    {"id": "cammy", "name": "CAMMY"},
    {"id": "rashid", "name": "RASHID"},
    {"id": "aki", "name": "A.K.I."},
    {"id": "ed", "name": "ED"},
    {"id": "gouki_akuma", "name": "AKUMA"},
    {"id": "vega_mbison", "name": "M.BISON"},
    {"id": "terry", "name": "TERRY"},
    {"id": "mai", "name": "MAI"},
    {"id": "elena", "name": "ELENA"},
    {"id": "sagat", "name": "SAGAT"},
    {"id": "cviper", "name": "C.VIPER"},
    {"id": "alex", "name": "ALEX"},
    {"id": "ingrid", "name": "INGRID"},
    {"id": "yasmine", "name": "YASMINE"},
    {"id": "arjun", "name": "ARJUN"},
]

# Order of table columns matching HTML schema
FIELD_KEYS = [
    "move_name",
    "startup",
    "active",
    "recovery",
    "hit",
    "block",
    "cancel",
    "damage",
    "combo_scaling",
    "drive_gauge_gain_hit",
    "drive_gauge_lose_dguard",
    "drive_gauge_lose_punish",
    "sa_gauge_gain",
    "attribute",
    "miscellaneous",
]

# Controller image file to notation symbol mapping
IMG_MAPPING = {
    "key-u.png": "8",
    "key-d.png": "2",
    "key-l.png": "4",
    "key-r.png": "6",
    "key-ul.png": "7",
    "key-ur.png": "9",
    "key-dl.png": "1",
    "key-dr.png": "3",
    "key-nutral.png": "5",
    "key-plus.png": "+",
    "key-all.png": "FULL",
    "icon_punch.png": "P",
    "icon_punch_l.png": "LP",
    "icon_punch_m.png": "MP",
    "icon_punch_h.png": "HP",
    "icon_kick.png": "K",
    "icon_kick_l.png": "LK",
    "icon_kick_m.png": "MK",
    "icon_kick_h.png": "HK",
    "modern_auto.png": "A",
    "modern_sp.png": "SP",
    "modern_l.png": "L",
    "modern_m.png": "M",
    "modern_h.png": "H",
    "modern_dl.png": "DI",
    "modern_dp.png": "DP",
    "arrow_3.png": ">",
    "key-or.png": "",
    "key-uc.png": "",
    "key-dc.png": "",
    "key-lc.png": "",
    "key-rc.png": "",
    "icon_throw.png": "",
    "key-circle.png": "",
}


class UnmappedImageError(Exception):
    """Error when no string definition corresponding to the image is found."""

    def __init__(self, src: str | None = None):
        self.src = src

        if src is None:
            message = "The text corresponding to the image is not registered in the table."
        elif src == "":
            message = "The image source is an empty string, so mapping is not possible."
        else:
            message = (
                f"The string corresponding to the image '{src}'"
                " is not registered in the table."
            )

        self.msg = message

        super().__init__(message)


# ==============================================================================
# Text & HTML Parsing Helpers
# ==============================================================================


def extract_vitality(soup):
    """
    Extracts vitality strictly from HTML content as a string.
    Primary Target: <p class="frame_attention__n8nO4">Vitality<span>10000</span></p>
    Fallback Target: Generic element search & regex range scan
    Returns None if not found in HTML.
    """
    if not soup:
        return None

    for comment in soup.find_all(
        string=lambda text: (
            isinstance(text, str) and "SCRAPED_VITALITY" in text
        )
    ):
        matches = re.findall(r"\d+", comment)
        if matches:
            numbers = [m for m in matches if 8000 <= int(m) <= 12000]
            if numbers:
                return numbers[0]

    attention_elem = soup.find(class_=re.compile(r"frame_attention"))
    if attention_elem:
        span = attention_elem.find("span")
        if span:
            span_text = re.sub(r"\D", "", span.get_text(strip=True))
            if span_text.isdigit():
                return span_text

    for cell in soup.find_all(["th", "td", "span", "div", "p"]):
        text = cell.get_text(strip=True)
        if "vitality" in text.lower() or "体力" in text:
            matches = re.findall(r"\d+", text)
            if matches:
                numbers = [m for m in matches if 8000 <= int(m) <= 12000]
                if numbers:
                    return numbers[0]

    full_text = soup.get_text()
    matches = re.findall(r"\b(8[5-9]00|9\d{3}|1[0-1]\d{3})\b", full_text)
    if matches:
        return matches[0]

    return None


def extract_cell_text_with_newlines(cell_soup):
    if not cell_soup:
        return ""
    cell_clone = BeautifulSoup(str(cell_soup), "html.parser")
    for br in cell_clone.find_all(["br", "br/"]):
        br.replace_with("\n")
    return cell_clone.get_text().strip()


def extract_active_frame_from_cell(cell_soup):
    if not cell_soup:
        return ""
    p_tag = cell_soup.find("p")
    if p_tag and not cell_soup.find("label"):
        return p_tag.get_text(strip=True)

    label_tag = cell_soup.find("label")
    main_frame = label_tag.get_text(strip=True) if label_tag else ""

    inner_div = cell_soup.find(class_=re.compile(r"frame_inner"))
    if not inner_div:
        ex_div = cell_soup.find(class_=re.compile(r"frame_ex"))
        inner_div = ex_div if ex_div else None

    detail_frame = inner_div.get_text(strip=True) if inner_div else ""

    if not detail_frame and label_tag:
        label_clone = BeautifulSoup(str(cell_soup), "html.parser")
        if label_clone.find("label"):
            label_clone.find("label").extract()
        detail_frame = label_clone.get_text(strip=True)

    if main_frame and detail_frame:
        return f"{main_frame}({detail_frame})"
    elif main_frame:
        return main_frame
    elif p_tag:
        return p_tag.get_text(strip=True)
    else:
        return cell_soup.get_text(strip=True)


# ==============================================================================
# Network Layer (Playwright Fetcher)
# ==============================================================================


def fetch_character_html(char_id, html_dir, page, force=False):
    classic_path = os.path.join(html_dir, f"{char_id}_classic.txt")
    modern_path = os.path.join(html_dir, f"{char_id}_modern.txt")

    need_classic = force or not os.path.exists(classic_path)
    need_modern = force or not os.path.exists(modern_path)

    if not need_classic and not need_modern:
        print(
            f"[FETCH] HTML files for '{char_id}' already exist. Skipping download."
        )
        return

    url = f"https://www.streetfighter.com/6/character/{char_id}/frame"
    print(f"[FETCH] Navigating to: {url}")

    # Use domcontentloaded instead of networkidle to prevent 12s timeout/delay on 2nd+ character loops
    page.goto(url, wait_until="domcontentloaded", timeout=30000)

    try:
        page.locator(
            "#CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll"
        ).click(timeout=2000)
        page.wait_for_timeout(500)
    except Exception:  # ruff: ignore[S110, BLE001]
        pass

    modes = []
    if need_classic:
        modes.append("classic")
    if need_modern:
        modes.append("modern")

    for mode in modes:
        mode_btn_query = f"img[src*='logo-{mode}.png']"
        print(f"[FETCH] Switching to '{mode}' control mode for {char_id}...")

        try:
            page.wait_for_selector(mode_btn_query, timeout=5000)
            page.locator(mode_btn_query).click(force=True)
        except Exception:  # ruff: ignore[BLE001]
            # Fallback direct JS click if selector wait fails
            page.evaluate(
                f'document.querySelector("{mode_btn_query}")?.click()'
            )

        page.wait_for_timeout(1000)

        vitality_str = None
        try:
            attention_locator = page.locator("p[class*='frame_attention']")
            if attention_locator.count() > 0:
                v_text = attention_locator.inner_text()
                match = re.search(r"\d+", v_text)
                if match:
                    # Keep as string directly without int conversion
                    vitality_str = match.group(0)
        except Exception:  # ruff: ignore[S110, BLE001]
            pass

        table_html = ""
        try:
            page.wait_for_selector("table", timeout=5000)
            table_element = page.locator("table").first
            table_html = table_element.evaluate("el => el.outerHTML")
        except Exception:  # ruff: ignore[BLE001]
            table_html = page.content()

        if vitality_str:
            table_html = (
                f"<!-- SCRAPED_VITALITY: {vitality_str} -->\n" + table_html
            )

        filename = os.path.join(html_dir, f"{char_id}_{mode}.txt")
        with open(filename, "w", encoding="utf-8") as f:
            f.write(table_html)
        print(f"[FETCH] Saved table HTML: {filename}")


# ==============================================================================
# Local HTML Parser Layer
# ==============================================================================


def convert_imgs_to_notation(img_tags):
    symbols = []
    for img in img_tags:
        src = img.get("src", "")
        for filename, symbol in IMG_MAPPING.items():
            if filename in src:
                symbols.append(symbol)
                break
        else:
            raise UnmappedImageError(src)
    return "".join(symbols)


def map_raw_symbols_to_format(raw_str):
    if not raw_str:
        return ""
    formatted = raw_str.replace("+", "")
    tokens = re.findall(r"([1-9]+)|([A-Z/]+)", formatted)
    result = []
    for num_part, btn_part in tokens:
        if num_part:
            result.append(num_part)
        elif btn_part:
            result.append(f"[{btn_part}]")
    return "".join(result)


def parse_command_cell_html(td_element, mode):
    if mode == "classic":
        classic_p = td_element.find("p", class_=re.compile(r"frame_classic"))
        target_container = classic_p if classic_p else td_element
        imgs = target_container.find_all("img")
        return map_raw_symbols_to_format(convert_imgs_to_notation(imgs))

    elif mode == "modern":
        modern_p = td_element.find("p", class_=re.compile(r"frame_modern"))
        if not modern_p:
            return {"modern": "", "modern_manual": ""}
        span = modern_p.find("span")
        manual_imgs = span.find_all("img") if span else []
        manual_raw = convert_imgs_to_notation(manual_imgs)
        all_imgs = modern_p.find_all("img")
        modern_imgs = [
            img for img in all_imgs if not (span and img in manual_imgs)
        ]
        modern_raw = convert_imgs_to_notation(modern_imgs)
        return {
            "modern": map_raw_symbols_to_format(modern_raw),
            "modern_manual": map_raw_symbols_to_format(manual_raw),
        }


def parse_local_html_mode(file_path, mode):
    if not os.path.exists(file_path):
        return None, None

    with open(file_path, "r", encoding="utf-8") as f:
        soup = BeautifulSoup(f.read(), "html.parser")

    vitality = extract_vitality(soup)
    moves = []

    tables = soup.find_all("table")
    target_rows = []
    if tables:
        for table in tables:
            target_rows.extend(table.find_all("tr"))
    else:
        target_rows = soup.find_all("tr")

    for row in target_rows:
        row_class = row.get("class", [])
        if any("frame_heading" in c for c in row_class):
            continue

        cells = row.find_all(["th", "td"])
        if len(cells) >= 2:
            move_name_elem = cells[0].find(class_=re.compile(r"frame_arts"))
            move_name = (
                move_name_elem.get_text(strip=True) if move_name_elem else ""
            )

            if not move_name and len(cells) > 0:
                move_name = cells[0].get_text(strip=True)

            cmd_data = parse_command_cell_html(cells[0], mode)

            move_info = {"move_name": move_name, "command": cmd_data}

            if mode == "classic":
                cell_values = {}
                for i, key in enumerate(FIELD_KEYS[1:]):
                    if (i + 1) < len(cells):
                        cell_soup = cells[i + 1]
                        if key == "active":
                            cell_values[key] = extract_active_frame_from_cell(
                                cell_soup
                            )
                        elif key == "miscellaneous":
                            cell_values[key] = (
                                extract_cell_text_with_newlines(cell_soup)
                            )
                        else:
                            cell_values[key] = cell_soup.get_text(strip=True)
                    else:
                        cell_values[key] = ""

                move_info["frame"] = {
                    "startup": cell_values.get("startup", ""),
                    "active": cell_values.get("active", ""),
                    "recovery": cell_values.get("recovery", ""),
                }

                move_info["hit"] = cell_values.get("hit", "")
                move_info["block"] = cell_values.get("block", "")
                move_info["cancel"] = cell_values.get("cancel", "")
                move_info["damage"] = cell_values.get("damage", "")
                move_info["combo_scaling"] = cell_values.get(
                    "combo_scaling", ""
                )

                move_info["drive_gauge"] = {
                    "hit": cell_values.get("drive_gauge_gain_hit", ""),
                    "dguard": cell_values.get("drive_gauge_lose_dguard", ""),
                    "punish": cell_values.get("drive_gauge_lose_punish", ""),
                }

                move_info["sa_gauge_gain"] = cell_values.get(
                    "sa_gauge_gain", ""
                )
                move_info["attribute"] = cell_values.get("attribute", "")
                move_info["miscellaneous"] = cell_values.get(
                    "miscellaneous", ""
                )

            moves.append(move_info)

    return moves, vitality


def process_character_json(
    char_id, char_name, html_dir, json_dir, force=False
):
    json_path = os.path.join(json_dir, f"{char_id}.json")
    if not force and os.path.exists(json_path):
        print(
            f"[CONVERT] JSON file for '{char_id}' already exists ({json_path}). Skipping conversion."
        )
        return True

    classic_path = os.path.join(html_dir, f"{char_id}_classic.txt")
    modern_path = os.path.join(html_dir, f"{char_id}_modern.txt")

    if not os.path.exists(classic_path) or not os.path.exists(modern_path):
        print(
            f"[WARN] HTML file(s) missing for '{char_id}'. Skipping JSON conversion."
        )
        return False

    print(f"[CONVERT] Processing local HTML files for '{char_id}'...")
    classic_moves, classic_vitality = parse_local_html_mode(
        classic_path, "classic"
    )
    modern_moves, modern_vitality = parse_local_html_mode(
        modern_path, "modern"
    )

    vitality_str = (
        classic_vitality if classic_vitality is not None else modern_vitality
    )

    if classic_moves is None or modern_moves is None:
        print(f"[ERROR] Failed to parse HTML for '{char_id}'.")
        return False

    # Build a lookup from modern moves keyed by move_name
    modern_lookup = {}
    for m_move in modern_moves:
        modern_lookup[m_move["move_name"]] = m_move["command"]

    unified_moves = []
    for c_move in classic_moves:
        c_cmd = c_move["command"]
        m_cmd = ""
        m_manual_cmd = ""

        m_data = modern_lookup.get(c_move["move_name"])
        if m_data:
            m_cmd = m_data.get("modern", "")
            m_manual_cmd = m_data.get("modern_manual", "")

        c_move["command"] = {
            "classic": c_cmd,
            "modern": m_cmd,
            "modern_manual": m_manual_cmd,
        }
        unified_moves.append(c_move)

    character_json = {
        "character_id": char_id,
        "character_name": char_name,
        "vitality": vitality_str,
        "moves": unified_moves,
    }

    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(character_json, f, ensure_ascii=False, indent=4)

    print(
        f"[CONVERT] Successfully generated: {json_path} ({len(unified_moves)} moves)"
    )
    return True


def parse_arg():
    parser = argparse.ArgumentParser(
        description=f"SF6 Frame Data Scraper & HTML-to-JSON Converter CLI Tool (v{__version__})",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    parser.add_argument(
        "--fetch",
        action="store_true",
        default=False,
        help="Execute HTML fetching.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        default=False,
        help="Force overwrite HTML fetch and/or JSON conversion.",
    )
    parser.add_argument(
        "-o",
        "--html-dir",
        type=str,
        default="./html_cache",
        help="Directory path for HTML cache.",
    )
    parser.add_argument(
        "-j",
        "--json-dir",
        type=str,
        default="./sf6_json_output",
        help="Directory path for JSON output.",
    )
    parser.add_argument(
        "-c",
        "--character",
        type=str,
        default="all",
        help="Specific character ID or 'all'.",
    )

    return parser.parse_args()


# ==============================================================================
# CLI Orchestrator & Entry Point
# ==============================================================================


def main():
    args = parse_arg()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    html_dir = os.path.abspath(os.path.join(script_dir, args.html_dir))
    json_dir = os.path.abspath(os.path.join(script_dir, args.json_dir))

    os.makedirs(html_dir, exist_ok=True)
    os.makedirs(json_dir, exist_ok=True)

    char_dict = {c["id"].lower(): c.get("name") for c in DEFAULT_CHARACTERS}

    if args.character.lower() == "all":
        target_characters = [
            {"id": c["id"], "name": c.get("name") or c["id"].upper()}
            for c in DEFAULT_CHARACTERS
        ]
    else:
        target_id = args.character.lower()
        matched_name = char_dict.get(target_id) or target_id.upper()
        target_characters = [{"id": target_id, "name": matched_name}]

    missing_html = any(
        not os.path.exists(
            os.path.join(html_dir, f"{char['id']}_classic.txt")
        )
        or not os.path.exists(
            os.path.join(html_dir, f"{char['id']}_modern.txt")
        )
        for char in target_characters
    )

    should_fetch = args.fetch or missing_html
    should_convert = (not args.fetch) or args.force

    if should_fetch:
        print("[INIT] Starting Playwright browser for web fetching...")
        with sync_playwright() as p:
            browser = p.chromium.launch(
                channel="msedge",
                headless=False,
                args=["--disable-blink-features=AutomationControlled"],
            )
            context = browser.new_context(
                user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
                viewport={"width": 1920, "height": 1080},
            )
            page = context.new_page()

            for char in target_characters:
                try:
                    fetch_character_html(
                        char["id"], html_dir, page, force=args.force
                    )
                except Exception as e:  # ruff: ignore[BLE001]
                    print(
                        f"[ERROR] Failed to fetch {char['id']}: {type(e).__name__} - {e}"
                    )

            browser.close()
        print("[FETCH] Web fetching phase complete.")
    else:
        print(
            "[FETCH] HTML files already exist and --fetch not specified. Skipping download."
        )

    if should_convert:
        print("[INIT] Starting local HTML to JSON conversion...")
        for char in target_characters:
            try:
                process_character_json(
                    char["id"],
                    char["name"],
                    html_dir,
                    json_dir,
                    force=args.force,
                )
            except UnmappedImageError as e:
                print(f"[ERROR] {type(e).__name__} - {e}")
                break
            except Exception as e:  # ruff: ignore[BLE001]
                print(
                    f"[ERROR] Failed to convert {char['id']}: {type(e).__name__} - {e}"
                )
        else:
            print("[CONVERT] Conversion phase complete.")
    else:
        print(
            "[CONVERT] --fetch specified without --force. Skipping JSON conversion."
        )


if __name__ == "__main__":
    main()

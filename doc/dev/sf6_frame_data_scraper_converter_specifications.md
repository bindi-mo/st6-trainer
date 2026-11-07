# SF6 Frame Data Scraper & Converter Specification

## 1. Overview

This specification defines the behavior and interfaces of the **Web Fetcher** (which retrieves frame data in HTML format from the official Street Fighter 6 website) and the **JSON Converter** (which parses local HTML files and converts them into a unified data structure).

Script Name: `sf6_framedata_processor.py`

## 2. Path Resolution Specification

All output and reference directories (specified via `-o / --html-dir` and `-j / --json-dir`) are resolved as absolute paths based on the **directory where the script is located (`sf6_framedata_processor.py` directory)**, rather than the current working directory (CWD) from where the command is executed.

## 3. CLI Arguments Specification

Command-line arguments are controlled using `argparse`. Default values are explicitly displayed during help output (`formatter_class=argparse.ArgumentDefaultsHelpFormatter`).

| Argument | Type | Default | Description | 
| ----- | ----- | ----- | ----- | 
| `--fetch` | bool | `False` | Executes HTML fetching. | 
| `--force` | bool | `False` | Forces overwriting HTML fetching (`--fetch --force`) and/or JSON conversion (`--force`). | 
| `-o`, `--html-dir` | path | `./html_cache` | Directory path for saving and referencing HTML cache files (relative to script location). | 
| `-j`, `--json-dir` | path | `./sf6_json_output` | Directory path for generated JSON files (relative to script location). | 
| `-c`, `--character` | string | `all` | Processes a specific character ID (e.g., `luke`, `ryu`), defaulting to all characters. | 

## 4. Execution Control Matrix

Behavior branches based on combinations of CLI flags as follows:

```
                    ┌─────────────────────────┐
                    │      CLI Execution      │
                    └────────────┬────────────┘
                                 │
                 ┌───────────────┴───────────────┐
                 ▼                               ▼
            --fetch specified               --fetch NOT specified
                 │                               │
        ┌────────┴────────┐             ┌────────┴────────┐
        ▼                 ▼             ▼                 ▼
   No --force        --force         No --force        --force
     (Mode 1)          (Mode 2)      (Mode 3: Default)  (Mode 4)
  [Fetch & Convert]  [Force Fetch & Force Convert] [Safe Pipeline] [Force Convert]
```

### Detailed Operations

1. **Mode 1: `--fetch` only**
   * **Purpose**: Download missing HTML caches and perform normal conversion.
   * **Action**:
     * Downloads missing HTML files and converts them to JSON normally.

2. **Mode 2: `--fetch --force`**
   * **Purpose**: Force re-fetch all HTML caches and force regenerate JSON.
   * **Action**:
     * Always performs web fetching, **overwrites** existing HTML files, and **forces JSON conversion/overwrite**.

3. **Mode 3: No flags (Default behavior)**
   * **Purpose**: Safe pipeline protecting existing files.
   * **Action**:
     * **HTML Fetching**: Fetches only if missing.
     * **JSON Conversion**: Converts if missing, skips if `<id>.json` already exists.

4. **Mode 4: `--force` only**
   * **Purpose**: Re-generate JSON exclusively while reusing existing HTML files.
   * **Action**:
     * **HTML Fetching**: Reuses existing HTML files (fetches only if missing).
     * **JSON Conversion**: **Forces overwrite** of existing `<id>.json` files.

## 5. HTML Parsing Specifications

### 5.1 URL and Text Processing Specifications

* Target URLs use the format without language codes: `https://www.streetfighter.com/6/character/{char_id}/frame`
* `<br>` / `<br/>` tags within text cells such as `miscellaneous` are replaced with newline characters `\n` upon extraction.

### 5.2 `active` Frame Parsing Structure

The `<td>` cell containing active frames supports the following two HTML structures:

* **Pattern A (Single Value)**:
  ```html
  <td class="frame_active_frame__6Sovc"><p>21-25</p></td>
  ```
  -> Output: `"21-25"`

* **Pattern B (With Detailed Breakdown)**:
  ```html
  <td class="frame_active_frame__6Sovc">
      <input type="checkbox" id="af_300_69">
      <label for="af_300_69">13-37</label>
      <div class="frame_ex___h3rR">
          <div class="frame_inner__Qf7xV">13-14, 19-20, 25-26, 30-31, 36-37</div>
      </div>
  </td>
  ```
  -> Output: `"13-37(13-14, 19-20, 25-26, 30-31, 36-37)"`

### 5.3 Vitality Extraction Logic

Dynamically extracts the character's vitality value from the HTML content.

* **Target Structure**: Detects elements containing the text `"Vitality"` (such as `<p class="frame_attention__n8nO4">`) and extracts the integer value from their internal (or subsequent) `<span>` element.
* The extracted value is stored in the `vitality` key during JSON output. If not found, it evaluates to `null`.

## 6. JSON Schema Specification

### 6.1 Character Name (`character_name`) Configuration Rules

* `name` definitions inside `DEFAULT_CHARACTERS` (e.g., `"LUKE"`, `"RYU"`, `"CHUN-LI"`) are kept entirely in uppercase.
* When a specific character is specified via the CLI option `-c <character_id>`, the script searches the `DEFAULT_CHARACTERS` list for the corresponding `character_id` and uses its matching `name` for the JSON output's `character_name`.
* If the specified `character_id` does not exist in `DEFAULT_CHARACTERS`, or if no `name` is defined, the `character_id` converted to uppercase (`.upper()`) is set as the `character_name`.

### 6.2 JSON Schema Example

```json
{
    "character_id": "luke",
    "character_name": "LUKE",
    "vitality": 10000,
    "moves": [
        {
            "move_name": "Vulcan Blitz",
            "command": {
                "classic": "236[LP]",
                "modern": "5[SP]",
                "modern_manual": "236[LP]"
            },
            "frame": {
                "startup": "14",
                "active": "14-30(14-15, 29-30)",
                "recovery": "18"
            },
            "hit": "+2",
            "block": "-3",
            "cancel": "SA3",
            "damage": "1000",
            "combo_scaling": "10%",
            "drive_gauge": {
                "hit": "+250",
                "dguard": "-500",
                "punish": "-1000"
            },
            "sa_gauge_gain": "+100",
            "attribute": "Mid",
            "miscellaneous": "*1 Can be canceled from the 4th frame via Drive Rush\n*2 Parry hitbox is extended for as long as the buttons are held"
        }
    ]
}
```
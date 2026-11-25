# Match History & Statistics Guide

Complete reference for browsing past matches, viewing statistics dashboards, and referencing frame data.

---

## Overview

ST6 Trainer automatically caches every scan. The **Match History** sidebar and **Statistics Dashboard** let you:
- Load previous matches instantly (no re-scan)
- View aggregate statistics across matches
- Reference frame data for any character/move

---

## Match History Sidebar

![Match History](screenshots/history-sidebar.png)

### Location

Right pane, top section — **"Match History"**

### Features

| Feature | Description |
|---------|-------------|
| **List cached matches** | All scans from cache, newest first |
| **Load match** | Click any row to load into scanner + LLM context |
| **Match summary** | ID, date, characters, control types, rounds, result |
| **Source video** | Path to original video (if available) |

### Loading a Match

1. Find match in list (sorted by date, newest first)
2. Click the row
3. App loads:
   - Scan results → Scanner panel
   - Match ID → Active match (status bar)
   - Characters/controls → Header controls
   - LLM context → Ready for chat/reports

![Match Loaded](screenshots/history-match-loaded.png)

### Match Summary Columns

| Column | Example | Meaning |
|--------|---------|---------|
| **Match ID** | `a1b2c3d4` | Unique hash (video + config + offsets) |
| **Played At** | `2024-11-20 19:30` | Match timestamp (from video metadata) |
| **My Character** | `Ryu` | Player 1 character |
| **Opponent** | `Ken` | Player 2 character |
| **My Control** | `Classic` | Control type |
| **Rounds** | `3` | Total rounds in match |
| **Result** | `Win / Loss` | Match outcome |
| **Video** | `C:\Videos\match.mp4` | Source path (click to open folder) |

### Cache Persistence

- Cache survives app restarts
- Stored in `%APPDATA%\st6-trainer\scan_cache\`
- Keyed by video SHA-256 + scanner config version
- Manual calibration creates new cache entry (new match ID)

---

## Statistics Dashboard

![Statistics Dashboard](screenshots/stats-dashboard.png)

### Location

Right pane, bottom section — **"Statistics Dashboard"**

### Requirements

- Requires **active match** (loaded from history or fresh scan)
- Shows data for currently loaded match only

### Panels

#### 1. Damage Overview

| Metric | Description |
|--------|-------------|
| **Total Damage Dealt** | Sum of all damage you inflicted |
| **Total Damage Taken** | Sum of all damage you received |
| **Net Damage** | Dealt − Taken |
| **Avg Damage/Round** | Per-round averages |

#### 2. Drive Gauge Usage

| Metric | Description |
|--------|-------------|
| **Total Drive Used** | Blocks consumed (drive rush, impact, parry, etc.) |
| **Burnout Count** | Times you entered burnout |
| **Avg Drive/Round** | Average blocks per round |
| **Drive Efficiency** | Damage dealt per drive block |

#### 3. Round Timing

| Metric | Description |
|--------|-------------|
| **Total Match Time** | Sum of all round durations |
| **Avg Round Time** | Mean round duration |
| **Shortest / Longest Round** | Min/max round times |
| **Time Efficiency** | Damage per second |

#### 4. Input Statistics

| Metric | Description |
|--------|-------------|
| **Total Inputs** | Button presses recorded |
| **Inputs/Second** | APM (actions per minute) equivalent |
| **Unique Moves Used** | Distinct move count (from frame meter) |
| **Most Used Move** | Top move by frequency |

#### 5. System Notifications

| Notification Type | Count | Description |
|-------------------|-------|-------------|
| **Counter Hit** | N | You counter-hit opponent |
| **Punish Counter** | N | You punished a whiff/block |
| **Reversal** | N | Reversal timing detected |
| **Meaty** | N | Meaty setup detected |

---

## Frame Data Reference Modal

![Frame Data Modal](screenshots/frame-data-modal.png)

### Opening the Modal

**From Timeline Nav (left pane):**
1. Scan a video or load a match
2. Timeline shows move names with frame numbers
3. Click any move → **Frame Data Modal opens**

**From Match History (planned):**
- Right-click match → "View Frame Data"

### Modal Contents

| Section | Description |
|---------|-------------|
| **Character** | Selected character (e.g., Ryu) |
| **Control Type** | Classic / Modern |
| **Move Name** | e.g., "c.MK", "Hadoken", "Shoryuken" |
| **Frame Data Table** | Startup, Active, Recovery, Hit Adv, Block Adv |
| **Cancel Info** | Special/Super/Drive Rush cancel windows |
| **Properties** | Armor, Invincibility, Projectile, etc. |
| **Notes** | Community tips, common uses |

### Frame Data Table Columns

| Column | Meaning |
|--------|---------|
| **Startup** | Frames before active (including first active) |
| **Active** | Number of active frames |
| **Recovery** | Frames after active until actionable |
| **Hit Adv** | Frame advantage on hit (+ = you recover first) |
| **Block Adv** | Frame advantage on block |
| **Cancel** | Cancel window (e.g., "4-6F into Special") |

### Data Source

Frame data comes from the **SF6 Frame Data Scraper** (see `dev/sf6_frame_data_scraper_converter_specifications.md`):
- Scraped from official Capcom frame data
- Converted to JSON
- Bundled with app (read-only)

> **Accuracy:** Data matches official values at time of scraping. Verify with patch notes for latest balance changes.

---

## Timeline Navigation

![Timeline Nav](screenshots/timeline-nav.png)

### Location

Left pane, below scanner results — **"Timeline Navigation"**

### Features

| Feature | Description |
|---------|-------------|
| **Frame slider** | Jump to any frame in match |
| **Round markers** | Visual round boundaries |
| **Move list** | Detected moves with frame numbers |
| **Jump to frame** | Click move → scrolls scanner frame list |
| **Open frame data** | Click move → opens Frame Data Modal |

### Using Timeline

1. Scan/load a match
2. Timeline populates with rounds and moves
3. Drag slider or click round marker
4. Scanner frame list scrolls to that frame
5. Click move name → Frame Data Modal

---

## Workflow: Analyzing a Past Match

```
1. Open Match History sidebar
   ↓
2. Find match (by date, character, result)
   ↓
3. Click to load
   ↓
4. Review Scanner results (left pane)
   ↓
5. Check Statistics Dashboard (right pane)
   ↓
6. Use Timeline to find key moments
   ↓
7. Click moves → Frame Data Modal for details
   ↓
8. Start LLM Worker → Chat or Generate Reports
```

---

## Cache Management (from Scanner)

![Cache Management](screenshots/cache-management.png)

### Refreshing Cache List

Click **"Refresh Cache List"** in Scanner → Scan Cache section.

### Clearing Cache

Click **"Clear All Cache"** — **irreversible**.

**When to clear:**
- Disk space low
- Corrupted cache entries
- Scanner config version changed (auto-invalidates)

### Cache Size Estimation

| Match Length | Cache Size |
|--------------|------------|
| 1 round (60s) | ~500 KB |
| 3 rounds (3 min) | ~1.5 MB |
| 5 rounds (5 min) | ~2.5 MB |
| 100 matches | ~150–250 MB |

---

## Exporting Data

### Current Options

| Data | Export Method |
|------|---------------|
| Scan results | Copy from Scanner UI |
| Statistics | Screenshot / manual note |
| Frame data | Copy from Modal |
| Reports | Copy from LLM Chat |

### Planned Exports

- **Match export** → `.json` (full scan + metadata)
- **Statistics export** → `.csv` / `.xlsx`
- **Report export** → `.md` / `.pdf`
- **Frame data export** → `.csv` per character

---

## Troubleshooting History

| Issue | Solution |
|-------|----------|
| Match History empty | Run a scan first; check cache directory exists |
| Match won't load | Cache corrupted → Clear cache, re-scan |
| Statistics show zeros | No active match loaded |
| Frame Data Modal empty | Select character/control type first |
| Timeline not showing | Requires scanned match with frame meter data |

---

## Next Steps

- [Video Scanner Guide](scanner.md) — Create new scans
- [LLM Assistant Guide](llm.md) — Analyze loaded matches
- [Troubleshooting](troubleshooting.md) — Fix history/cache issues
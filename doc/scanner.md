# Video Scanner Guide

Complete reference for the video scanning feature — from video selection to result interpretation and manual calibration.

---

## Overview

The scanner analyzes Street Fighter 6 replay videos and extracts:
- **Per-frame inputs** for both players (button bitmasks)
- **Drive gauge** status (blocks, burnout state, pixel intensity)
- **System notifications** (Counter Hit, Punish Counter, etc.) with confidence
- **Frame meter** data (startup, active, recovery, hit/block advantage)
- **HUD layout** (timer, health bars, drive gauges, notification areas)
- **Metadata** (characters, control types, match ID, offsets)

---

## Supported Video Formats

| Format | Codec Recommendation | Notes |
|--------|---------------------|-------|
| `.mp4` | H.264 / H.265 | Best compatibility |
| `.mkv` | H.264 / H.265 / AV1 | Good for high-quality captures |
| `.avi` | Various | Legacy captures |
| `.mov` | H.264 / ProRes | macOS captures |

**Recommended settings:**
- Resolution: 1920×1080 (1080p) or 1280×720 (720p)
- Frame rate: 60 FPS (preferred) or 30 FPS
- Bitrate: ≥ 10 Mbps for 1080p60

---

## Selecting a Video

![Video Selection](screenshots/scanner-video-selection.png)

1. Click **"Select Video"** in the Scanner section (top-left)
2. File dialog opens — navigate to your replay
3. Select file → **Open**
4. File path displays below the button

**Validation:** The app checks file extension only. Corrupted or non-SF6 videos will fail at scan time with an error.

---

## Scanning Process

![Scan Progress](screenshots/scanner-progress.png)

Click **"Scan Video"** to start. The scan runs in the Rust backend (multi-threaded).

### Progress Indicators

| Indicator | Description |
|-----------|-------------|
| **Frame counter** | `current / total` frames processed |
| **Progress bar** | Visual 0–100% |
| **Percentage** | Numeric progress |
| **Elapsed time** | Seconds since scan started |
| **Remaining time** | Estimated seconds to completion |
| **Finish ETA** | Wall-clock time of estimated completion |

### Scan Stages (Internal)

1. **Video open & probe** — FFmpeg reads metadata
2. **HUD detection** — Locate timer, health bars, drive gauges
3. **Character/control detection** — OCR on character names, control type icons
4. **Frame extraction** — Decode frames, run template matching
5. **Data serialization** — Build `VideoScanResult` JSON

---

## Interpreting Scan Results

![Scan Results](screenshots/scanner-results.png)

### Video Info Grid

| Field | Meaning |
|-------|---------|
| **Video** | Source file path |
| **Resolution** | Detected width × height |
| **FPS** | Frames per second (e.g., 59.94) |
| **Total Frames** | Frame count in video |
| **Duration** | Video length in seconds |
| **Frames Processed** | Frames successfully analyzed |

### Metadata Grid

| Field | Meaning |
|-------|---------|
| **Match ID** | Unique identifier (hash of video + config + offsets) |
| **Character P1** | Player 1 character name |
| **Character P2** | Player 2 character name |
| **Control Type P1** | `classic` or `modern` |
| **Control Type P2** | `classic` or `modern` |
| **Top Y Offset** | Pixels from top for HUD crop |
| **Bottom Y Offset** | Pixels from bottom for HUD crop |
| **Manual Verified** | `true` if user calibrated, `false`/`null` otherwise |

### Frame Data Preview

Shows first 10 frames. Each frame displays:

```
Frame 1245 (20750.0ms)
P1 Drive: 6 blocks | P2 Drive: 4 blocks (BURNOUT)
| Notification: Counter Hit (0.94)
| Meter: S:3 A:2 R:18 HA:+5 BA:+2
```

| Component | Description |
|-----------|-------------|
| **Frame index** | 0-based frame number |
| **Timestamp** | Milliseconds from video start |
| **P1/P2 Drive** | Blocks (0–6), burnout flag, pixel intensity (0–255) |
| **System Notification** | Text + confidence (0.0–1.0) |
| **Frame Meter** | Startup/Active/Recovery frames, hit/block advantage |

> **Note:** Only first 10 frames shown. Full data available in cache for LLM/reports.

---

## Manual Calibration (§2.7 / §7.1)

![Manual Calibration](screenshots/scanner-calibration.png)

### When to Calibrate

- Character names detected incorrectly
- Health bars not aligning with HUD
- Drive gauge blocks not counting correctly
- System notifications missing or false positives

### How It Works

The scanner detects HUD regions automatically. Sometimes the crop offsets (`top_y_offset_px`, `bottom_y_offset_px`) are off by a few pixels due to:
- Video encoding differences (letterboxing, safe areas)
- UI scale settings in SF6
- Aspect ratio variations

### Adjusting Offsets

1. **Top Y Offset** — Moves the top crop boundary down (+) or up (−)
2. **Bottom Y Offset** — Moves the bottom crop boundary up (+) or down (−)
3. Use **±1 px** buttons for fine adjustment
4. Click **"Save Manual Offsets (Verify)"**

### What Happens on Save

1. Backend re-scans with new offsets
2. New `match_id` generated (hash includes offsets)
3. `manual_verified` set to `true`
4. Cache updated with new results
5. UI refreshes with new data

> **Importance:** The new `match_id` prevents confusion with old (incorrect) cached results.

### Verification Indicator

After successful save:
```
✅ Manual offsets verified: match_id will reflect the adjusted offsets.
```

---

## Scan Cache

![Scan Cache](screenshots/scanner-cache.png)

Scans are cached to disk (keyed by video SHA-256 + scanner config version).

### Cache Management

| Action | Description |
|--------|-------------|
| **Refresh Cache List** | Reload cache entries from disk |
| **Clear All Cache** | Delete all cached scans (irreversible) |

### Cache Entry Columns

| Column | Description |
|--------|-------------|
| **Match ID** | Unique scan identifier |
| **P1 / P2** | Character names |
| **Frames** | Total frames processed |
| **Cached At** | Timestamp of cache creation |
| **Size** | Cache file size in KB |

### Cache Location

```
%APPDATA%\st6-trainer\scan_cache\
  ├── <cache_key>.json      # Scan result
  └── <cache_key>.meta.json # Metadata only (for list)
```

---

## Common Scan Issues

| Symptom | Likely Cause | Fix |
|---------|--------------|-----|
| "No video selected" | File dialog cancelled | Re-select video |
| Scan fails immediately | Corrupted/unsupported video | Try re-encoding with HandBrake |
| Wrong characters detected | OCR confusion | Manual calibration |
| Drive gauge always 0 | HUD crop wrong | Adjust Top/Bottom Y offsets |
| No notifications found | Confidence threshold | Check video quality (720p+ required) |
| Scan very slow | No GPU / high-res video | Use 720p, close other apps |

---

## Keyboard Shortcuts (Scanner)

| Key | Action |
|-----|--------|
| `Ctrl+O` | Select Video |
| `Enter` (when video selected) | Start Scan |
| `Escape` | Cancel scan (if running) |

---

## Next Steps

- [LLM Assistant Guide](llm.md) — Analyze scan results with AI
- [Match History & Stats](history.md) — Load cached matches
- [Troubleshooting](troubleshooting.md) — Fix scan problems
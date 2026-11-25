# Quick Start Guide

Get ST6 Trainer running and analyze your first match in 5 minutes.

---

## 1. Installation

### Prerequisites
- **Bun** ≥ 1.0 — `curl -fsSL https://bun.sh/install | bash`
- **Rust** ≥ 1.75 — `rustup update stable`
- **Windows 10/11** (tested on Windows 11)

### Build & Run

```bash
# Clone and enter project
git clone https://github.com/bindi-mo/st6-trainer.git
cd st6-trainer

# Install frontend dependencies
bun install

# Build Rust backend (release mode for performance)
cd src-tauri && cargo build --release && cd ..

# Development mode (hot reload)
bun run tauri dev

# Production build
bun run tauri build
```

> **Note:** First `cargo build --release` takes 2–5 minutes. Subsequent builds are faster.

---

## 2. First Launch

![App Launch](screenshots/app-launch.png)

When the app opens, you'll see:
- **Left pane**: Video Scanner + Timeline
- **Right pane**: Match History + LLM Chat + Reports + Statistics
- **Bottom status bar**: LLM worker status, current model, active match ID

---

## 3. Select a Video

![Select Video](screenshots/select-video.png)

1. Click **"Select Video"** button (top-left)
2. Choose an SF6 replay file:
   - Supported formats: `.mp4`, `.mkv`, `.avi`, `.mov`
   - Recommended: 1080p60, 720p60, or 720p30
   - File must be a **local file** (not a stream URL)
3. The file path appears below the button

> **Tip:** Use replays from SF6's built-in replay system (Battle Hub → Replays → Save Replay).

---

## 4. Scan the Video

![Scan Video](screenshots/scan-video.png)

1. Click **"Scan Video"** (primary button, enabled after video selection)
2. Watch the progress bar:
   - **Frame counter**: Current / Total frames
   - **Percentage**: 0–100%
   - **Elapsed / Remaining time**: Estimated
   - **Finish ETA**: Wall-clock time
3. Wait for completion (typically 10–60 seconds depending on video length)

### Scan Results

![Scan Results](screenshots/scan-results.png)

After scanning, you'll see:
- **Video info**: Resolution, FPS, total frames, duration
- **Metadata**: Match ID, Character P1/P2, Control types, HUD offsets
- **Frame data preview**: First 10 frames with inputs, drive gauge, notifications, frame meter

---

## 5. (Optional) Manual Calibration

![Manual Calibration](screenshots/manual-calibration.png)

If HUD detection seems off:
1. Adjust **Top Y Offset** / **Bottom Y Offset** with ±1 px buttons
2. Click **"Save Manual Offsets (Verify)"**
3. The `manual_verified` flag becomes `true` and Match ID updates

> **When to calibrate:** Character names wrong, health bars misaligned, drive gauge not detected.

---

## 6. Start LLM Worker

![LLM Worker](screenshots/llm-worker.png)

Before chatting or generating reports:
1. In the **LLM Model** panel (right pane):
   - Select a model from the dropdown (download first if empty — see [LLM Guide](llm.md#downloading-models))
   - Click **"Start Worker"**
2. Wait for status: **"Ready"** (green dot)

---

## 7. Chat or Generate Reports

### Chat with LLM
![LLM Chat](screenshots/llm-chat.png)

1. Type a message in the chat input (bottom of LLM Chat section)
2. Press **Enter** to send
3. Response streams in real-time

**Example questions:**
- "Why did I lose round 2?"
- "What's the frame advantage on Ryu's c.MK?"
- "Suggest a punish for Ken's DP on block"

### Generate Reports
![Reports](screenshots/reports.png)

Click one of three report buttons (requires scanned video + ready LLM):

| Report | Purpose |
|--------|---------|
| **Damage Analysis** | Analyze why you took damage — spacing, timing, knowledge gaps |
| **Punish Verification** | Verify optimal punishes for opponent's moves |
| **Training Menu** | Generate practice drills based on match patterns |

Reports appear below the buttons as formatted Markdown.

---

## 8. Explore Match History & Stats

![Match History](screenshots/match-history.png)

- **Match History** (right sidebar): Click any past match to load it
- **Statistics Dashboard** (bottom right): Damage dealt/taken, drive gauge usage, round times
- **Frame Data Modal**: Click a move in timeline to see frame data

---

## ✅ You're Ready!

You've now:
- ✅ Installed and launched ST6 Trainer
- ✅ Scanned your first SF6 replay
- ✅ (Optional) Calibrated HUD offsets
- ✅ Started the LLM worker
- ✅ Chatted with the LLM or generated a report
- ✅ Explored match history and statistics

---

## Next Steps

| Want to... | Read |
|------------|------|
| Understand scanner results in detail | [Video Scanner Guide](scanner.md) |
| Learn LLM features deeply | [LLM Assistant Guide](llm.md) |
| Use match history & frame data | [Match History & Stats Guide](history.md) |
| Fix a problem | [Troubleshooting](troubleshooting.md) |
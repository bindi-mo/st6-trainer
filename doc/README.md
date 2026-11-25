# ST6 Trainer — User Guide

**ST6 Trainer** is a desktop application for analyzing Street Fighter 6 match videos. It scans your replay videos, extracts frame-by-frame input data, drive gauge status, system notifications, and frame meter data — then lets you analyze matches with an integrated LLM assistant.

---

## 📖 Documentation Index

| Guide | Description |
|-------|-------------|
| [Quick Start](quickstart.md) | Get up and running in 5 minutes |
| [Video Scanner](scanner.md) | Select videos, scan, interpret results, manual calibration |
| [LLM Assistant](llm.md) | Model selection, worker management, chat, report generation |
| [Match History & Stats](history.md) | Load past matches, statistics dashboard, frame data reference |
| [Troubleshooting](troubleshooting.md) | Common issues and solutions |

---

## 🎯 What This App Does

1. **Video Scanning** — Drop in an MP4/MKV/AVI/MOV replay; the scanner detects characters, controls, HUD layout, and extracts per-frame data
2. **Manual Calibration** — Fine-tune HUD offsets (±1 px) and mark as verified to prevent mis-identification
3. **LLM Analysis** — Chat with a local LLM about the match; generate three report types:
   - **Damage Analysis** — Why you got hit
   - **Punish Verification** — Confirm optimal punishes
   - **Training Menu** — Drills based on your match patterns
4. **Match History** — Browse cached scans, load previous matches, view statistics
5. **Frame Data Reference** — Look up move frame data for any character/control type

---

## ⚡ Quick Start

```bash
# 1. Install dependencies
bun install
cd src-tauri && cargo build --release

# 2. Run development mode
bun run tauri dev

# 3. Or build for production
bun run tauri build
```

**First run:**
1. Launch the app
2. Click **"Select Video"** and choose a Street Fighter 6 replay
3. Click **"Scan Video"** — wait for progress to complete
4. Review results; optionally adjust **Manual Calibration** offsets
5. Start the **LLM Worker** (download a model if needed)
6. Chat or generate reports!

---

## 🖥️ System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| OS | Windows 10/11 | Windows 11 |
| RAM | 8 GB | 16+ GB |
| GPU | DirectX 11 compatible | NVIDIA RTX 3060+ (for LLM GPU layers) |
| Storage | 5 GB free | 20+ GB (models + cache) |
| Video | 720p30+ SF6 replay | 1080p60+ SF6 replay |

---

## 📁 Project Structure

```
st6-trainer/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── hooks/              # Custom React hooks
│   └── App.tsx             # Main application
├── src-tauri/              # Rust backend (Tauri)
│   ├── src/
│   │   ├── commands.rs     # Tauri command handlers
│   │   ├── scanner/        # Video scanning logic
│   │   ├── llm/            # LLM worker management
│   │   └── cache/          # Scan cache management
│   └── Cargo.toml
├── doc/                    # This documentation
└── package.json
```

---

## 🔗 Related Resources

- [SF6 Video Scanner Specification](dev/sf6_video_scanner_spec_ja.md) (Japanese)
- [Detailed Design Document](dev/sf6_trainer_detailed_design_ja.md) (Japanese)
- [Frame Data Scraper Specs](dev/sf6_frame_data_scraper_converter_specifications.md)

---

## 📝 License

MIT License — see [LICENSE](../LICENSE) for details.
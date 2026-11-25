# Troubleshooting Guide

Common issues and solutions for ST6 Trainer.

---

## Quick Diagnosis

| Symptom | Likely Section |
|---------|----------------|
| App won't start | [Installation Issues](#installation-issues) |
| Video scan fails | [Scanner Issues](#scanner-issues) |
| Wrong characters detected | [Calibration Issues](#calibration-issues) |
| LLM worker won't start | [LLM Worker Issues](#llm-worker-issues) |
| Chat/reports not working | [LLM Runtime Issues](#llm-runtime-issues) |
| Match history empty | [Cache Issues](#cache-issues) |
| Frame data missing | [Frame Data Issues](#frame-data-issues) |
| App crashes/freezes | [Crashes & Freezes](#crashes--freezes) |

---

## Installation Issues

### `bun: command not found`
```bash
# Install Bun
curl -fsSL https://bun.sh/install | bash
# Restart terminal, then verify
bun --version
```

### `cargo: command not found`
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Restart terminal, then verify
cargo --version
```

### `bun install` fails with peer dependency errors
```bash
# Clear cache and retry
bun pm cache rm
rm -rf node_modules
bun install
```

### `cargo build --release` fails
**Common causes:**
- **Outdated Rust:** `rustup update stable`
- **Missing Windows SDK:** Install "Desktop development with C++" in Visual Studio Installer
- **Linker errors:** Ensure `link.exe` in PATH (Developer Command Prompt)

### Tauri dev server won't start
```bash
# Check port 1420/1421 not in use
netstat -ano | findstr :142
# Kill conflicting process or change port in tauri.conf.json
```

---

## Scanner Issues

### "Select Video" button does nothing
- **Cause:** File dialog blocked by antivirus/firewall
- **Fix:** Allow Tauri/Bun in Windows Defender; run as Administrator once

### Scan fails immediately with error
| Error | Cause | Fix |
|-------|-------|-----|
| `Failed to open video` | Corrupted file / unsupported codec | Re-encode with HandBrake (H.264, MP4) |
| `FFmpeg error` | FFmpeg not found / version mismatch | App bundles FFmpeg; restart app |
| `HUD detection failed` | Video not SF6 / wrong resolution | Use 720p+ SF6 replay |
| `OCR failed` | Character names unreadable | Manual calibration (see below) |

### Scan runs but produces wrong results
**Symptoms:** Wrong characters, 0 drive gauge, no notifications

**Fixes:**
1. **Manual Calibration** — Adjust Top/Bottom Y offsets (±1 px) → Save
2. **Check video quality** — Must be ≥ 720p, ≥ 30 FPS, clear HUD
3. **Verify SF6 settings** — UI Scale = 100%, no HUD mods
4. **Re-encode video** — HandBrake: H.264, Constant Quality 20, 60 FPS

### Scan very slow (> 5 min for 3-min video)
- **Cause:** CPU-only decoding, high-res video
- **Fixes:**
  - Use 720p instead of 1080p/4K
  - Close other apps (browser, games)
  - Ensure dedicated GPU used (NVIDIA Control Panel → Preferred GPU)

### "No frames processed" / empty results
- Video may be variable frame rate (VFR)
- **Fix:** Re-encode to constant frame rate (CFR) with HandBrake

---

## Calibration Issues

### Offsets reset after save
- **Cause:** `manual_verified` not set to `true`
- **Fix:** Click **"Save Manual Offsets (Verify)"** button (not just adjust)

### Match ID doesn't change after calibration
- **Cause:** Offsets identical to auto-detected values
- **Fix:** Change by at least ±1 px, then save

### Calibration saved but results still wrong
- **Cause:** HUD regions fundamentally misaligned (e.g., ultrawide monitor)
- **Fix:** Try different video; report issue with video sample

### "Failed to save manual offsets"
- Check write permissions to `%APPDATA%\st6-trainer\scan_cache\`
- Disk full?
- Antivirus blocking write?

---

## LLM Worker Issues

### "No models found" in dropdown
**Cause:** Models directory empty or wrong location

**Fix:**
```bash
# Check directory exists
ls %APPDATA%\st6-trainer\models\

# Should contain .gguf files
# If not, download models (see LLM Guide)
```

### Worker fails to start / "Worker start error"
| Error | Cause | Fix |
|-------|-------|-----|
| `CUDA out of memory` | VRAM full | Reduce `gpu_layers` (edit App.tsx), close GPU apps |
| `Model load failed` | Corrupted .gguf | Re-download model; verify file size |
| `Unsupported model` | Wrong format | Must be `.gguf` (not .bin, .safetensors) |
| `Context size too large` | Model max context < 4096 | Use model with ≥ 4K context |

### Worker starts but shows "Error" state
1. Check console (F12 → Console) for Rust backend logs
2. Common: `ggml_cuda_init` failed → GPU driver issue
3. **Fix:** Update GPU drivers; try CPU-only (`gpu_layers: 0`)

### Worker stuck at "Starting..." for > 60 seconds
- Large model (8B+) on slow disk/CPU
- **Fix:** Use smaller model (3B), SSD for models folder

---

## LLM Runtime Issues

### Chat input disabled / "Send" button grayed out
**Requirements (all must be met):**
- ✅ Worker status = **Ready** (green)
- ✅ Input not empty
- ✅ Not currently streaming

### "Generating..." but no output appears
- **Cause:** Model loading first token (can take 5–15s for large models)
- **Fix:** Wait; check console for `llm-stream-token` events

### Gibberish / repetitive / hallucinated output
| Cause | Fix |
|-------|-----|
| Model too small for task | Use 7B+ model |
| Context overflow | Shorter prompts; worker auto-truncates |
| Bad quantization | Re-download Q4_K_M or higher |
| Prompt confusion | Rephrase; use English for best results |

### Report generation fails
| Error | Fix |
|-------|-----|
| "Scan result not available" | Scan a video first |
| "Worker not ready" | Wait for Ready state |
| "Prompt generation failed" | Check Rust logs; may be missing frame data |

### Cancel button doesn't work
- **Cause:** `llm_cancel` command not reaching worker
- **Fix:** Restart worker; if persistent, restart app

---

## Cache Issues

### Match History empty after scans
1. Check cache directory: `%APPDATA%\st6-trainer\scan_cache\`
2. Should contain `.json` and `.meta.json` files
3. If empty → scans not caching (check disk space, permissions)

### "Failed to load cache entries"
- Corrupted cache file
- **Fix:** Click **"Clear All Cache"**, re-scan videos

### Cache not updating after manual calibration
- Calibration creates **new** cache entry (new match ID)
- Old entry preserved
- **Fix:** Load new match from history (newest first)

### Disk space full from cache
```bash
# Check size
du -sh %APPDATA%\st6-trainer\scan_cache\

# Clear old entries manually or use "Clear All Cache"
```

---

## Frame Data Issues

### Frame Data Modal shows "No data" / empty
**Requirements:**
- Character selected (from scan or dropdown)
- Control type selected (Classic/Modern)
- Move name available (from timeline click)

**Fix:** Load a match → click move in Timeline → modal opens with data

### Wrong frame data for character
- Data sourced from scraper at build time
- May not match latest patch
- **Fix:** Check `dev/sf6_frame_data_scraper_converter_specifications.md` for version

### Modal won't open
- Click move name in **Timeline Navigation** (left pane, below scanner)
- Not from Match History (not yet implemented)

---

## Crashes & Freezes

### App crashes on startup
1. **Check logs:**
   - Dev mode: Console output
   - Release: `%APPDATA%\st6-trainer\logs\`
2. **Common causes:**
   - Corrupted config → Delete `%APPDATA%\st6-trainer\`
   - GPU driver crash → Update drivers
   - Antivirus blocking Tauri → Add exception

### App freezes during scan
- Large video + no progress updates
- **Fix:** Wait 2–3x estimated time; if stuck, kill process, use smaller video

### App freezes during LLM generation
- Model swapping to system RAM (VRAM full)
- **Fix:** Reduce `gpu_layers`, use smaller model

### "Tauri IPC error" / "Invoke failed"
- Frontend ↔ Backend communication broken
- **Fix:** Restart app; if dev mode, restart `bun run tauri dev`

---

## Log Locations

| Log Type | Location |
|----------|----------|
| Frontend (dev) | Browser DevTools → Console |
| Frontend (release) | `%APPDATA%\st6-trainer\logs\frontend.log` |
| Backend (Rust) | `%APPDATA%\st6-trainer\logs\backend.log` |
| Tauri | `%APPDATA%\st6-trainer\logs\tauri.log` |
| Scan cache | `%APPDATA%\st6-trainer\scan_cache\` |
| Models | `%APPDATA%\st6-trainer\models\` |

---

## Getting Help

### Before Reporting

1. **Check this guide** for your symptom
2. **Try the fix** — most issues are covered above
3. **Collect logs** (see locations above)
4. **Note:** OS version, GPU, video specs, model used

### Report Template

```markdown
**Issue:** Brief description
**Steps to reproduce:**
1. ...
2. ...
3. ...

**Expected:** What should happen
**Actual:** What happens (error message, screenshot)

**Environment:**
- OS: Windows 11 23H2
- GPU: RTX 3080 10GB / Driver 560.xx
- Video: 1080p60 MP4 H.264
- Model: llama-3.1-8b-q4_k_m.gguf
- App version: (from About dialog)

**Logs:** (paste relevant lines)
```

### Where to Report

- **GitHub Issues:** https://github.com/bindi-mo/st6-trainer/issues
- **Discussions:** https://github.com/bindi-mo/st6-trainer/discussions

---

## FAQ

### Can I use this on Mac/Linux?
- **Currently:** Windows only (Tauri + FFmpeg + scanner optimized for Windows)
- **Planned:** Linux support; macOS requires code signing changes

### Can I analyze online replays (YouTube, Twitch)?
- **No:** Only local video files
- **Workaround:** Download video first (yt-dlp), then scan

### Does it work with other fighting games?
- **No:** Hardcoded for SF6 HUD layout, characters, frame data
- **Architecture:** Modular — could be adapted

### Is my data sent anywhere?
- **No:** 100% local. No network calls for scanning or LLM.

### Can I use my own frame data?
- **Not yet:** Frame data bundled at build time
- **Planned:** Custom frame data JSON import

### How accurate is the scanner?
- **Characters/Controls:** ~95%+ on clean 1080p60
- **Drive gauge:** ~90% (depends on HUD visibility)
- **Notifications:** ~85% (confidence threshold 0.7)
- **Frame meter:** ~80% (experimental)

---

## Version-Specific Notes

### v0.1.x (Current)
- Manual calibration required for best results
- LLM worker must be restarted for model change
- No report export (copy-paste only)
- Frame data from 2024 scrape

### Known Limitations
- No batch scan (one video at a time)
- No video preview in app
- Timeline requires frame meter data (not all moves detected)
- Statistics dashboard single-match only

---

## Next Steps

- [Video Scanner Guide](scanner.md) — Detailed scanner usage
- [LLM Assistant Guide](llm.md) — LLM configuration
- [Match History & Stats](history.md) — Cache management
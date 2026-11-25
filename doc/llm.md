# LLM Assistant Guide

Complete reference for the integrated LLM (Large Language Model) features — model management, worker lifecycle, chat, and report generation.

---

## Overview

ST6 Trainer includes a **local LLM worker** (llama.cpp-based) that runs entirely on your machine. No data leaves your computer.

### Capabilities

- **Free-form chat** — Ask questions about the match, frame data, strategy
- **Report generation** — Three specialized report templates
- **Context-aware** — Automatically includes scan metadata and frame data

---

## Model Management

![Model Selection](screenshots/llm-model-selection.png)

### Downloading Models

Models are **not bundled** — you must download them.

1. Click the model dropdown (shows "Select a model...")
2. If empty, you'll see: *"No models found. Download models first."*
3. Models are stored in: `%APPDATA%\st6-trainer\models\`

### Recommended Models

| Model | Size | Quality | VRAM (GPU layers=35) | Best For |
|-------|------|---------|---------------------|----------|
| `llama-3.2-3b-instruct-q4_k_m.gguf` | ~2 GB | Good | ~3 GB | Fast chat, basic reports |
| `llama-3.1-8b-instruct-q4_k_m.gguf` | ~5 GB | Better | ~6 GB | Detailed analysis |
| `qwen2.5-7b-instruct-q4_k_m.gguf` | ~4 GB | Excellent | ~5 GB | Multilingual, code |
| `nemotron-3-ultra-q4_k_m.gguf` | ~8 GB | Best | ~10 GB | Complex reasoning |

> **Format:** Only `.gguf` files (quantized) are supported.

### Adding Models

**Option A: Hugging Face (recommended)**
```bash
# Example using huggingface-cli
huggingface-cli download bartowski/Llama-3.2-3B-Instruct-GGUF \
  --local-dir %APPDATA%\st6-trainer\models \
  --include "*.gguf"
```

**Option B: Manual download**
1. Visit [Hugging Face](https://huggingface.co/models?library=gguf&sort=downloads)
2. Search for your preferred model (e.g., "llama-3.2-3b-instruct")
3. Download `.gguf` file (Q4_K_M quantization recommended)
4. Place in `%APPDATA%\st6-trainer\models\`

**Option C: In-app download (planned)**
Future versions will include a model downloader UI.

### Refreshing Model List

After adding models, click the dropdown — it auto-refreshes on panel mount. Or restart the app.

---

## Worker Lifecycle

![Worker States](screenshots/llm-worker-states.png)

### States

| State | Indicator | Meaning |
|-------|-----------|---------|
| **Offline** | ⚪ Gray | Worker not started |
| **Starting** | 🟡 Yellow | Loading model into memory |
| **Ready** | 🟢 Green | Accepting requests |
| **Busy** | 🔵 Blue | Generating response |
| **Error** | 🔴 Red | Failed to start/process |

### Starting the Worker

1. Select a model from dropdown
2. Click **"Start Worker"**
3. Wait for **"Ready"** (10–30 seconds depending on model size)

**Configuration (fixed in current version):**
- Context size: 4096 tokens
- GPU layers: 35 (adjust in `src-tauri/src/llm/worker.rs` if needed)
- Threads: Auto (CPU core count)

### Stopping the Worker

Click **"Stop Worker"** when status is **Ready** or **Busy**.

Effects:
- Frees VRAM/RAM
- Clears chat history
- Resets model dropdown (worker must restart for model change)

> **Tip:** Keep worker running between matches — startup is the slowest part.

---

## Chat Interface

![Chat Interface](screenshots/llm-chat.png)

### Sending Messages

1. Type in the input box (bottom of Chat section)
2. Press **Enter** to send (Shift+Enter for newline)
3. Response streams token-by-token

### Chat Features

| Feature | Description |
|---------|-------------|
| **Streaming** | Real-time token display |
| **Cancel** | Click "Cancel" during generation |
| **History** | Persists while worker runs |
| **Context** | Includes last scan metadata automatically |

### Example Prompts

**Match Analysis:**
```
"Why did I lose round 2 against Ken?"
"What was my biggest mistake in the neutral game?"
"Analyze my drive gauge usage — am I burning too early?"
```

**Frame Data Questions:**
```
"What's the frame advantage on Ryu's c.MK on block?"
"Show me all 5-frame punishes for Luke."
"Is Ken's DP safe on block at max range?"
```

**Strategy:**
```
"Suggest a game plan for Guile vs. Zangief."
"How do I deal with drive rush pressure?"
"What's the optimal punish for whiffed DP?"
```

### Limitations

- **Context window:** 4096 tokens (≈3000 words)
- **Knowledge cutoff:** Model training date
- **No internet access:** Cannot look up current patch notes
- **Hallucination risk:** Verify frame data against official sources

---

## Report Generation

![Reports](screenshots/llm-reports.png)

Three specialized reports — each builds a detailed prompt from your scan data, then streams the LLM response.

### Prerequisites

- ✅ Video scanned (scan results loaded)
- ✅ LLM worker **Ready**
- ✅ Model loaded

### Report Types

#### 1. Damage Analysis Report
**Button:** "Damage Analysis Report"
**Purpose:** Understand why you took damage

**Covers:**
- Neutral game losses (spacing, whiff punishes)
- Defensive gaps (blockstrings, frame traps)
- Resource mismanagement (drive gauge, super)
- Pattern recognition (repeated situations)

**Best used:** After a match you lost or took heavy damage.

#### 2. Punish Verification Report
**Button:** "Punish Verification Report"
**Purpose:** Confirm optimal punishes for opponent's moves

**Covers:**
- Every blocked/whiffed special move by opponent
- Optimal punish per situation (range, meter, drive)
- Frame-perfect vs. practical punishes
- Character-specific interactions

**Best used:** When you missed punishes or want to optimize.

#### 3. Training Menu Report
**Button:** "Training Menu Report"
**Purpose:** Generate practice drills from match patterns

**Covers:**
- Situation-specific drills (anti-air, whiff punish, blockstring gaps)
- Drive gauge management exercises
- Character matchup routines
- Progressive difficulty (easy → tournament)

**Best used:** Before practice sessions.

### Report Output

Reports render as **Markdown** with:
- Headings, tables, code blocks
- Bold/italic emphasis
- Bullet/numbered lists
- Blockquotes for key takeaways

![Report Output](screenshots/llm-report-output.png)

### Saving Reports

Currently: Copy-paste from UI.
Planned: Export to `.md` / `.pdf` buttons.

---

## Advanced: Prompt Templates

Reports use internal prompt templates (in `src-tauri/src/llm/prompts.rs`). You can view/modify by rebuilding.

### Custom Prompts (Workaround)

In chat, you can ask:
```
"Generate a damage analysis report for match <match_id> using this format:
1. Executive Summary
2. Top 3 Damage Sources
3. Per-Round Breakdown
4. Action Items"
```

---

## Performance Tuning

### GPU Acceleration

The worker uses `gpu_layers: 35` by default (in `App.tsx` → `startWorker()`).

**Adjust based on VRAM:**

| VRAM | GPU Layers | Note |
|------|------------|------|
| 4 GB | 20–25 | Conservative |
| 6 GB | 30–35 | Balanced |
| 8 GB | 40–45 | Aggressive |
| 10+ GB | 50+ (full offload) | Maximum speed |

**To change:** Edit `src/App.tsx` line with `gpu_layers: 35` → rebuild.

### CPU Threads

Set via `OMP_NUM_THREADS` env var or `llama.cpp` default (auto = physical cores).

### Quantization Trade-offs

| Quantization | Size | Quality | Speed |
|--------------|------|---------|-------|
| Q2_K | Smallest | Low | Fastest |
| Q4_K_M | **Recommended** | **Good** | **Fast** |
| Q5_K_M | Medium | Better | Medium |
| Q8_0 | Large | Best | Slowest |

---

## Troubleshooting LLM

| Issue | Solution |
|-------|----------|
| "No models found" | Download `.gguf` to `%APPDATA%\st6-trainer\models\` |
| Worker fails to start | Check VRAM; reduce `gpu_layers`; try smaller model |
| "Out of memory" | Close other apps; use Q4_K_M; reduce context size |
| Slow generation | Enable GPU layers; use smaller model; check CPU usage |
| Gibberish output | Model corrupted; re-download; check quantization |
| Chat not sending | Worker must be **Ready**; check input not empty |

---

## Privacy & Security

- **100% local** — No network requests for inference
- **No telemetry** — No usage data collected
- **Model files** — Only read, never modified
- **Scan data** — Passed to LLM as prompt context only

---

## Next Steps

- [Video Scanner Guide](scanner.md) — Prepare scan data for LLM
- [Match History & Stats](history.md) — Load matches for analysis
- [Troubleshooting](troubleshooting.md) — Fix LLM issues
# Scan Speed Optimization & Progress Display Plan

## Problem Definition

1. Video scanning takes a very long time (bottleneck identification and improvement needed)
2. Scan progress (progress, remaining time, estimated finish time) is completely unknown during scanning
3. Standard output (`std::cout`) is performed every frame during scanning, but its meaning is unclear and it is costly

## Current Architecture

- **C++ DLL** (`scanner-cpp/st6_scanner.dll`): Processes video frame by frame using OpenCV. Performs normalization, gauge detection, input tracking, and frame meter reading
- **Rust FFI** (`src-tauri/src/scanner_ffi.rs`): Wraps the DLL and provides an iterator via the `Scanner` structure
- **Tauri command** (`src-tauri/src/commands.rs`): `scan_video` rescans the video from the beginning with **three separate scanner instances** for telemetry / input_events / move_events, and returns all results after storing them in memory
- **Frontend** (`src/App.tsx`): Awaits `invoke("scan_video")` with no progress display until completion

## Identified Bottlenecks (Priority Ordered)

| # | Bottleneck | Location | Reason |
|---|---|---|---|
| 1 | **Three full video rescans** | `commands.rs:scan_video` | Creates `Scanner::new` separately for telemetry/input/move, reading the video three times |
| 2 | **Template image `imread` every frame** | `frame_meter.cpp:read_digit`, `input_tracker.cpp:match_templates` | Reads templates from disk every frame (no cache) |
| 3 | **Heavy OpenCV processing per frame** | `raster_normalizer.cpp:normalize` | `cvtColor`×2, `minMaxLoc`×2, `mean`×2, `resize`, plus `cvtColor` in `detect_content_region` |
| 4 | **Pixel loop fill ratio calculation** | `gauge.cpp:compute_fill_ratio` | C++ double loop (columns × rows) replaceable with OpenCV functions |
| 5 | **`std::cout` every frame** | `raster_normalizer.cpp` each function | Debug output incurs I/O cost; little meaningful information |
| 6 | **Missing progress display** | All layers | Frontend, Rust, and C++ all lack progress callbacks |

## Improvement Plan (Implementation Order)

### Phase 1: Reduce Standard Output (Immediate Effect)

**Target**: `scanner-cpp/raster_normalizer.cpp`

- Remove or make conditional (debug flag) the per-frame `std::cout` in `detect_content_region`, `check_aspect_ratio`, `check_isotropy`, `normalize`
- Keep `std::cerr` only for errors
- Do not output meaningful information (normal frame size, scale, etc.) at all, or output it only once

### Phase 2: Introduce Template Image Caching

**Target**: `scanner-cpp/frame_meter.cpp`, `scanner-cpp/input_tracker.cpp`

- Cache digit templates read with `imread` every frame in `read_digit` — load once at initialization
- Cache direction/button templates loaded with `load_template` every frame in `match_templates` — load once at initialization
- Template paths resolved at `ScannerImpl` or each detector initialization

### Phase 3: Integrate Three Rescans into One Pass (Maximum Effect)

**Target**: `src-tauri/src/commands.rs`, `src-tauri/src/scanner_ffi.rs`

- In `scan_video`, collect telemetry / input_events / move_events **simultaneously with one `Scanner` instance**
- Perform frame load → normalization → gauge detection (telemetry) → input tracking (input event) → frame meter (move event) in a single pass
- Reduces video read cost to one-third

### Phase 4: Add Progress API

**Target**: `scanner-cpp/scanner.cpp`, `scanner-cpp/scanner_api.h`, `src-tauri/src/scanner_ffi.rs`, `src-tauri/src/commands.rs`, `src/App.tsx`

- C++ DLL: Add progress retrieval API including `total_frames` in metadata, or `scanner_get_progress`
- Rust FFI: Add progress retrieval method to `Scanner`
- Tauri command: Emit `app.emit("scan-progress", {...})` within the scan loop (frame_index, total_frames, elapsed_ms, estimated_remaining_ms, estimated_finish_at)
- Frontend: Use `listen("scan-progress")` to display progress bar, remaining time, and estimated finish time

### Phase 5: Lighten OpenCV Processing

**Target**: `scanner-cpp/raster_normalizer.cpp`, `scanner-cpp/gauge.cpp`

- Reduce duplicate `cvtColor`/`minMaxLoc`/`mean` in `normalize` (reuse a single grayscale conversion)
- Replace the pixel double loop in `gauge.cpp:compute_fill_ratio` with OpenCV functions such as `countNonZero` or `reduce`
- Consider changing `INTER_LINEAR` resize to `INTER_AREA` as needed

## Target Files

1. `scanner-cpp/raster_normalizer.cpp` — Standard output reduction, OpenCV processing lightening
2. `scanner-cpp/gauge.cpp` — Fill ratio calculation replacement with OpenCV functions
3. `scanner-cpp/frame_meter.cpp` — Template caching
4. `scanner-cpp/input_tracker.cpp` — Template caching
5. `scanner-cpp/scanner.cpp` — Add fields for progress retrieval
6. `scanner-cpp/scanner_api.h` — Add progress API declarations
7. `src-tauri/src/scanner_ffi.rs` — Add progress methods
8. `src-tauri/src/commands.rs` — Single-pass integrated scan, progress event emission
9. `src/App.tsx` — Progress display UI (progress bar, remaining time, estimated finish time)

## Verification Methods

- `cargo check` (Rust side) — confirm no compilation errors
- `bunx tsc --noEmit` (frontend side) — confirm no type errors
- Measure scan time on real video before and after each phase to confirm reduction
- Confirm frontend correctly displays progress bar, remaining time, and estimated finish time
- Confirm standard output is reduced and I/O cost is lowered

## Estimated Effect

| Improvement Item | Expected Effect |
|---|---|
| Three rescans → single-pass integration | Scan time approx. 1/3 (up to 3x speedup) |
| Template caching | Significant reduction in I/O cost for template matching |
| Standard output reduction | Reduction of per-frame `std::cout` I/O cost |
| OpenCV processing lightening | Shorter processing time per frame |
| Progress display | Significant UX improvement (remaining time and estimated finish time visible) |

## Out of Scope (Deferred for Now)

- Multi-threaded frame parallel processing (VideoCapture thread-safety issues, etc.)

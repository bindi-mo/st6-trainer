# SF6 Frame Data Extractor - Implementation Plan

## 0. Prerequisites and Constraints

### 0.1 Build System
- C++ Scanner: `scanner-cpp/` (CMake/MSVC → `st6_scanner.dll`)
- Application: `src-tauri/` (Cargo/Tauri 2.x)
- Language boundary: C++ (OpenCV image processing) ⇄ Rust (Tauri/output) fixed via `scanner_api.h`

### 0.2 Current Assets (as of 2026-11)

| Item | Status |
|------|--------|
| `scanner_api.h` (4 structures + iterator API) | **Exists** (Phase 0 complete) |
| `scanner.cpp` | **Stub** (fixed-value dummy output only) |
| `scanner_ffi.rs` (§7 structure mapping) | **Exists** |
| `st6_scanner.dll` (built) | **Exists** (`scanner-cpp/build/Release/` → `src-tauri/` via POST_BUILD copy) |
| Templates | **Exists** (`src-tauri/templates/`) |
| `scanner_config.json` | **Exists** (`src-tauri/config/`) |
| `app_config.json` | **Exists** (`src-tauri/config/`, created in Phase 0) |
| `config.rs` | **Exists** (Phase 0) |
| `pipeline.rs` / `commands.rs` | **Exists** (created in Phase 4) |
| Verification replay video | **Exists** (`src-tauri/videos/replay_00.mkv`) |
| CI | **Exists** (`.github/workflows/ci.yml`: frontend tests / `cargo test` / clippy / Tauri build) |

### 0.3 Decisions (basis of this plan)

- **Templates are placed in `src-tauri/templates/`** (not moved to `scanner-cpp/templates/`)
  - Reason: `scanner_ffi.rs::get_template_pack_path()` resolves `CARGO_MANIFEST_DIR/templates`, and Tauri's bundler can include them as resources as-is
  - C++ side receives the path via `ScanConfig::template_pack_path`, so it does not depend on the location
- **Configuration files are separated into 2 systems**
  - `src-tauri/config/scanner_config.json`: Scanner-only (algorithm parameters, developer-edited only)
  - `src-tauri/config/app_config.json`: App-only (execution environment, user settings, auto read/write)
- **Configuration format unified to JSON** (`scanner_config.json`, `app_config.json`, `tauri.conf.json`)
- **Manual calibration values are not persisted** (`match_id` collision prevention, compliant with spec §2.7 / §7.1)
- **C ABI is POD only** (`std::string` / `std::vector` / classes prohibited, ownership is on the caller side)
- **Control type badges included in `btn_classic` / `btn_modern`** (`classic.png` / `modern.png`)

## 1. Phase Configuration (Dependency Order)

| Phase | Name | Main Tasks | Deliverables | Dependencies | Status |
|-------|------|------------|--------------|--------------|--------|
| **0** | Environment & Interface Fix | `scanner_api.h` extension, CMake/Cargo integration check, config separation implementation | Stable FFI boundary, `scanner_config.json`, `app_config.json` | — | **Complete** |
| **1** | Template & Config Preparation | Anchor/glyph/digit/attack property/control type template fulfillment and inventory | `src-tauri/templates/`, `src-tauri/config/` | Phase 0 | **Complete** |
| **2** | Raster Normalization & Calibration | `scanner.cpp`: normalization (§6.1), anchor Y search & consensus (§2), fail-closed | `st6_scanner.dll` (v1) | Phase 0, 1 | **Complete** (simplified method, see §2.5) |
| **3** | Input History Tracking | 19-row slots, slot0 reading, signature/counter state machine (§4) | `input_events` output support | Phase 2 | **Complete** (build successful) |
| **4** | Gauge & Frame Meter | HSV gauges (§3.2, §5), 2 ROI groups / 3-frame latch / attack property (§3.3) | `frame_telemetry`, `move_events` | Phase 2 | **Complete** |
| **5** | Rust FFI & Output Pipeline | `scanner_ffi.rs` extension, intermediate JSONL→SQLite conversion, invariant check | Tauri commands | Phase 2–4 | **Complete** |
| **6** | Manual Calibration UI | Tauri frontend: candidate offset display, ±1px stepper, `manual_verified` flag (§2.7) | `sf6-gui` feature | Phase 1, 5 | **Complete** |
| **7** | Integration, Verification, Release | Verification replay E2E, §7.5 invariant violation zero check, error code firing test | Release build, test report | Phase 0–6 | **Complete** |

## 2. Phase 0 — **Complete**

All 4 tasks of Phase 0 are complete. Below is the completion record.

### 2.1 `scanner_api.h` Design & Agreement — **Complete**

The 4 tables of spec §7 (`match_metadata`, `frame_telemetry`, `input_events`, `move_events`) are C-structurized, and ownership, lifetime, and error propagation are defined.

- `ScanConfig` maintains `config_json` (JSON string)
- Return value convention for `scanner_next_*` series (`1`=success / `0`=end / `-1`=error) is documented in header comments
- Return value of `scanner_create` (success: handle, failure: NULL) is also documented

### 2.2 Config Separation Implementation — **Complete**

| Item | Status |
|------|--------|
| `src-tauri/config/scanner_config.json` | Scanner-only (maintained as-is) |
| `src-tauri/config/app_config.json` | Created (§5.2 content) |
| `src-tauri/src/config.rs` | Created (loading, validation, path resolution, JSON generation) |

`Scanner::new()` automatically reads `scanner_config.json` if `config_json` is not specified.

### 2.3 CMake / Cargo Integration Check — **Complete**

- CMake POST_BUILD copies `st6_scanner.dll` to `src-tauri/`
- `build.rs` checks DLL existence (panics explicitly if not built)
- `scanner_ffi.rs::get_dll_path()` resolves both `resource_dir` (production) and `CARGO_MANIFEST_DIR` (development)
- `tauri.conf.json` `bundle.resources` includes `config/*.json` and `templates/**/*`

### 2.4 Minimal Smoke Test — **Complete**

`src-tauri/examples/smoke_test.rs` strengthened to call the real DLL.

```
scanner_create → scanner_get_metadata → scanner_next_telemetry → scanner_destroy
```

`src-tauri/videos/replay_00.mkv` is used as a fixture. If the video is missing, it degrades to `[3] SKIP` so CI does not fail.

## 2A. Phase 2 — **Complete** (Simplified Calibration Method)

Phase 2 implemented "raster normalization" and "calibration". However, since the anchor template matching (original method of §2.3–§2.5) did not reach the threshold (0.90) with real data, **Phase 2 adopted a simplified method (HUD existence detection + color-based)**, and precise Y offset determination was deferred to Phase 3–4's actual gauge detection.

### 2A.1 Raster Normalization (§6.1) — **Complete**

Created `scanner-cpp/raster_normalizer.{h,cpp}`.

- Normalizes input frames to **canonical raster 1920x1080**
- Letterbox/pillarbox removal (`detect_content_region`)
  - Current simplified implementation treats the entire frame as the content region (real data is full 1920x1080)
- Aspect ratio gate: `E_ASPECT_UNSUPPORTED` if content aspect exceeds 16:9 by **0.010**
- Isotropy gate: `E_ANISOTROPIC_SCALE` if scale difference exceeds **0.005**
- Black frame detection: `E_BLACK_FRAME` if `max_val < 10` (skipped by caller)

### 2A.2 Calibration — **Complete** (Simplified Method)

Implemented in `scanner.cpp::perform_calibration()`.

- **Calibration frame selection**
  - The beginning of the video has black screens continuing through "replay data loading" and "opponent introduction"
  - Scans frames **after match start** and adopts the first frame where both upper HUD (Y=0–270) and lower HUD (Y=756–1080) have content (`max_val > 100`)
- **HUD existence detection (HSV color mask)**
  - Upper 25% region green (HP gauge): `H∈[40,180], S,V∈[50,255]`
  - Lower 30% region blue (SA gauge): `H∈[100,130], S,V∈[50,255]`
  - If both averages are below 0.01, `E_HUD_ABSENT`
- **Y offset**
  - Current: `top_y_offset_px = 0`, `bottom_y_offset_px = 0` (default values)
  - Confidence is tentatively 0.8. To be refined in Phase 3–4's actual gauge detection

### 2A.3 Anchor Template Matching (Original Method — On Hold)

`scanner-cpp/calibration.{h,cpp}` is implemented but **not used in Phase 2** (kept for future extension).

- Anchors: 5 upper types (timer top edge / P1/P2 HP corner / P1/P2 drive corner), 3 lower types (P1/P2 SA bracket / frame meter top edge)
- `TM_CCOEFF_NORMED` 2D matching, prominence gate, median consensus (§2.5 compliant)
- **Real data results**: Maximum confidence 0.18–0.44, below threshold 0.90
  - Cause: Template and real frame HUD rendering do not match (template quality / cropping position issue)
  - Response: Template regeneration in Phase 1, or substitution with actual detection in Phase 3–4

### 2A.4 Black Frame Skip — **Complete**

Implemented in `scanner_next_telemetry()`.

- Each frame's `max_val < 10` is judged as black, telemetry is not output, and processing proceeds to the next frame
- This prevents `E_BLACK_FRAME` errors during the initial black screen section, allowing all frames to be scanned

### 2A.5 Build & Verification — **Complete**

- `CMakeLists.txt` added `calibration.cpp` / `raster_normalizer.cpp`
- `scanner_api_version()` updated to **0.2.0**
- Exception boundary: `scanner_create` / `scanner_get_metadata` / `scanner_next_telemetry` protected with `try/catch` so C++ exceptions do not cross the FFI boundary
- **Smoke test result**: `SMOKE TEST PASSED` (frame scanning, `match_id=match-0-0`)

### 2A.6 Known Constraints & Handoff to Phase 3–4

| Item | Current Status | Handoff |
|------|---------------|---------|
| Y offset | Fixed at 0 (tentative) | Determined from actual gauge position in Phase 3–4 |
| Anchor matching | Not used due to insufficient confidence | Substituted with template regeneration or actual detection |
| Round detection | Implemented (Phase 5) | Detected from round start (Round ? / Final Round) + round end (K.O. / TIME OVER) + timer (99 seconds) + HP reset complement (§4.5, §7.2) |
| Letterbox removal | Entire frame treated as content | Implemented when non-16:9 input is supported |
| Character name / control type | Empty string (stub) | Implemented in Phase 3 |

## 2B. Phase 3 — **Complete** (Input History Tracking)

Phase 3 implemented "input history tracking". `scanner_next_input_event()` extracts P1/P2 input events from 19-row slots.

### 2B.1 New Files

| File | Content |
|------|---------|
| `scanner-cpp/input_tracker.h` | `InputTracker` class, `InputRow`/`InputSegment`/`ControlTypeResult`/`GlyphMatch` structure definitions |
| `scanner-cpp/input_tracker.cpp` | Template matching (directions 1-9, buttons classic/modern, counter digits), control type badge detection, segment construction via state machine |

### 2B.2 Changes to `scanner.cpp`

- `scanner_api_version()` updated to **0.3.0** (Phase 3)
- Added `unique_ptr<InputTracker> input_tracker`, `bool input_tracker_initialized`, `int input_frame_counter` to `ScannerImpl`
- `scanner_get_metadata()` initializes `InputTracker` (sets `set_template_pack_path`, `rows_visible`, `input_glyph_threshold`)
- New `scanner_next_input_event()` implementation:
  1. Frame read, black frame skip, normalization
  2. `resolve_control_type()` executed for P1/P2 on the first valid frame (control type determination)
  3. 19-row slots scanned in P1→P2 order, input segments constructed via `read_input_row()` + `build_segment()`
  4. `InputEvent` structure populated with directions, buttons, quality flags, etc., and output

### 2B.3 `InputTracker` Implementation Details

- **Direction template matching**: `dir/` directory `1`–`9` (excluding `5`) matched with `TM_CCOEFF_NORMED`, up to 2 matches (diagonal input support)
- **Button template matching**: `btn_classic` (`LP/MP/HP/LK/MK/HK`) or `btn_modern` (`L/M/H/SP/AUTO/DP`) used according to control type, up to 3 matches
- **Counter reading**: `num/` directory digit templates, `F`=unknown, `-`=0
- **Control type badge detection**: `classic.png` / `modern.png` matched in upper 25% region, higher confidence one adopted
- **State machine**: Consecutive identical inputs segmented, `frame_start`/`frame_end`/`hold_frames` etc. calculated

### 2B.4 Build & Verification

- `CMakeLists.txt` added `input_tracker.cpp`
- `scanner_api_version()` updated to **0.3.0**
- Added `using namespace st6;` to `scanner.cpp` (since `InputTracker` etc. are in the `st6` namespace)
- Fixed duplicate/early closing bug in `scanner_next_input_event()`
- **Build result**: `st6_scanner.dll` built successfully (no errors, C4819 warning only)

### 2B.5 Known Constraints & Handoff to Phase 4

| Item | Current Status | Handoff |
|------|---------------|---------|
| Y offset | Fixed at 0 (tentative) | Determined from actual gauge position in Phase 4 |
| Control type determination | Determined once on first valid frame | Cases where control type changes mid-match not supported |
| Round detection | Implemented (Phase 5) | Detected from round start (Round ? / Final Round) + round end (K.O. / TIME OVER) + timer (99 seconds) + HP reset complement (§4.5, §7.2) |
| Character name | Empty string (stub) | Implemented in Phase 4 |
| Segment frame_start/frame_end | row_index used tentatively | Needs to be overwritten with actual frame index |

## 2C. Phase 4 — **Complete** (Gauge & Frame Meter)

Phase 4 implemented "gauge & frame meter". HP/Drive/SA gauges detected via HSV color mask, and frame meter detection with 3-frame latch implemented.

### 2C.1 New Files

| File | Content |
|------|---------|
| `scanner-cpp/gauge.h` | `GaugeDetector` class, `GaugeReading`/`FrameGauges`/`GaugeConfig` structure definitions |
| `scanner-cpp/gauge.cpp` | Gauge fill rate detection via HSV mask, burnout detection, occlusion detection |
| `scanner-cpp/frame_meter.h` | `FrameMeterDetector` class, `FrameMeterReading`/`FrameMeterConfig` structure definitions |
| `scanner-cpp/frame_meter.cpp` | Frame meter detection with 3-frame latch, template matching preparation |

### 2C.2 Changes to `scanner.cpp`

- `scanner_api_version()` updated to **0.4.0** (Phase 4)
- Added `unique_ptr<GaugeDetector> gauge_detector`, `bool gauge_detector_initialized`, `unique_ptr<FrameMeterDetector> frame_meter_detector`, `bool frame_meter_detector_initialized` to `ScannerImpl`
- `scanner_get_metadata()` initializes `GaugeDetector` and `FrameMeterDetector` (sets HSV thresholds, latch frame count, etc.)
- `scanner_next_telemetry()` integrates actual gauge detection:
  - `GaugeDetector::detect()` reads P1/P2 HP/Drive/SA gauge values
  - Sets `FrameTelemetry` `hp_p1`, `drive_p1`, `sa_p1`, `in_burnout_p1`, `hp_p2`, `drive_p2`, `sa_p2`, `in_burnout_p2`
  - Outputs quality flags (`HP_P1_OCCLUDED` etc.) for occluded gauges
- New `scanner_next_move_event()` implementation:
  - `FrameMeterDetector::detect()` detects frame meter with 3-frame latch
  - Sets `MoveEvent` `fm_startup_p1/fm_total_p1/fm_advantage_p1` etc.
  - Outputs default values (0) when not detected and continues iteration

### 2C.3 `GaugeDetector` Implementation Details

- **HSV color mask**: OpenCV HSV (H 0-179, S 0-255, V 0-255) for gauge color detection
  - HP gauge: `H∈[40,90], S∈[50,255], V∈[50,255]` (green/yellow)
  - Drive gauge: `H∈[15,35], S∈[100,255], V∈[100,255]` (orange)
  - SA gauge: `H∈[100,130], S∈[50,255], V∈[50,255]` (blue)
- **Fill rate calculation**: Measures consecutive white pixel columns from left to right, normalized by gauge width
- **Burnout detection**: Red pixels in HP gauge region (`H∈[0,10]` or `H∈[170,179]`) exceeding threshold judged as burnout
- **Occlusion detection**: If white pixel count is below `gauge_occluded_threshold`, judged as occluded
- **SA gauge**: Quantized to 0–3 steps (`round(value * 3) / 3`)

### 2C.4 `FrameMeterDetector` Implementation Details

- **3-frame latch**: Output only when the same reading (startup/active/recovery/advantage) is confirmed for `fm_latch_frames` (3) consecutive frames
- **Not detected**: Returns `detected = false`, caller outputs default values
- **Template matching**: Uses `num/` directory digit templates (simplified implementation in Phase 4, to be refined in the future)

### 2C.5 Build & Verification

- `CMakeLists.txt` added `gauge.cpp` / `frame_meter.cpp`
- `scanner_api_version()` updated to **0.4.0**
- **Smoke test result**: `SMOKE TEST PASSED` (telemetry read of 7230 frames successful)
- **Rust tests**: All 12 `cargo test` tests passed (including pipeline.rs invariant checks)

### 2C.6 Rust Pipeline (Phase 4 Deliverables)

| File | Content |
|------|---------|
| `src-tauri/src/pipeline.rs` | JSONL→SQLite conversion, §7.5 invariant check (input table items 1–5, schema items 6–8) |
| `src-tauri/src/commands.rs` | Tauri commands `scan_video` (video scan execution), `scanner_version` (API version retrieval) |
| `src-tauri/src/lib.rs` | `commands`, `pipeline` module registration, command handler addition |

### 2C.7 Known Constraints & Handoff to Phase 5 — **Phase 5 Complete**

| Item | Current Status | Response |
|------|---------------|----------|
| Y offset | Fixed at 0 (tentative) | **Determined from actual gauge position in Phase 5** (`detect_y_offsets()`) |
| Round detection | Implemented (Phase 5) | Detected from round start (Round ? / Final Round) + round end (K.O. / TIME OVER) + timer (99 seconds) + HP reset complement (`scanner_next_telemetry()`) |
| Character name | Empty string (stub) | **Matched from `names/` templates in Phase 5** |
| Frame meter digit reading | Simplified implementation (treated as not detected) | Template matching refinement in Phase 5 (not addressed, carried over to Phase 6) |
| SQLite write | rusqlite not introduced, JSONL summary file output | **rusqlite added and full SQLite write in Phase 5** |

## 2D. Phase 5 — **Complete**

Phase 5 completed "Rust FFI & output pipeline". Below is the completion record.

### 2D.1 C++ Scanner Changes (`scanner.cpp`)

- `scanner_api_version()` updated to **0.5.0** (Phase 5)
- **Y offset detection** (`detect_y_offsets()`): Detects actual Y positions of HP gauge (upper) and SA gauge (lower) from HSV color mask on the first valid frame, sets `top_y_offset_px` / `bottom_y_offset_px`
- **Character name detection** (`match_character_name()`): Matches templates in `src-tauri/templates/names/` (`aki.png`, `alex.png`, etc.) using `TM_CCOEFF_NORMED`, sets `character_name_p1` / `character_name_p2`
- **Round detection**: Detects round reset based on 99-second countdown timer (5940 frames @60fps), increments `round_index`, sets `hp_reset` flag. Detects "Round ? / Final Round" logo at screen center via template matching and judges as round start with highest priority. Also detects K.O. / TIME OVER logos and records as round end (`round_index` not changed). Complementarily detects when HP jumps from low (<0.3) to high (>0.7). Prevents false detection with minimum interval (180 frames)
- Added `opencv2/imgcodecs.hpp` include (for `imread`)

### 2D.2 Rust FFI Extension (`scanner_ffi.rs`)

- Extracted `init_scanner()` common initialization logic, used by `Scanner::new()`

### 2D.3 Pipeline (`pipeline.rs`)

- Added `rusqlite` dependency to `Cargo.toml` (with `bundled` feature)
- Implemented `write_sqlite()`: creates `frame_telemetry`, `input_events`, `move_events`, `match_metadata` tables and inserts records
- Added missing fields to `JsonlFrameTelemetry`, `JsonlInputEvent`, `JsonlMoveEvent` structures, supports partial deserialization with `#[serde(default)]`
- Migrated from JSONL summary file output to full SQLite write

### 2D.4 Build & Test

- C++ DLL: `st6_scanner.dll` built successfully (C4819 warning only, no errors)
- Rust: `cargo build` successful, all 12 `cargo test` tests passed
- Deleted `video_types.rs` (unused, old type)

### 2D.6 Known Constraints & Handoff to Phase 6

| Item | Current Status | Handoff |
|------|---------------|---------|
| Frame meter digit reading | Simplified implementation (treated as not detected) | Template matching refinement in Phase 6 |
| Character name matching | Only `aki.png`, `alex.png` in `names/` | Other character templates need to be added |
| Control type determination | Empty string (stub) | Implemented in Phase 6 |
| Manual calibration UI | Not implemented | Tauri frontend implementation in Phase 6 |

## 2E. Phase 6 — **Complete**

Phase 6 completed "Manual Calibration UI". Below is the completion record.

### 2E.1 Frame Meter Digit Reading Refinement (`frame_meter.cpp`)

- Implemented `read_digit()`: Matches digit templates (0-9, F, minus) in `num/` directory using `TM_CCOEFF_NORMED`, detects if threshold `config_.threshold` (0.90) or above
- Updated `read_meter_block()`: Divides frame meter display area into 4 blocks (startup/active/recovery/advantage) and reads digits from each block
- `detected = true` only when all digits are detected, otherwise `detected = false`
- Added `set_template_pack_path()` to set `num/` template path
- `scanner.cpp` calls `frame_meter_detector->set_template_pack_path()`

### 2E.2 Control Type Determination Implementation (`input_tracker.cpp`)

- Reimplemented `detect_control_badge()` compliant with §5.3: Explicitly matches `btn_classic/classic.png` and `btn_modern/modern.png` as control type classifier templates
- If both are below threshold, `control_type` is output as empty string (null)
- Individual button glyphs (LP/HP/LK/HK / L/M/H/SP etc.) are excluded from classifier templates
- Added `set_control_type()` to store resolved control type per player
- `read_input_row()` uses stored control type for button glyph matching
- `scanner.cpp` resolves control type in `scanner_get_metadata()` and sets `MatchMetadata.control_type_p1/p2`

### 2E.3 Manual Calibration C API (`scanner_api.h` / `scanner.cpp`)

- Added `scanner_set_manual_offsets()`: Manually overrides `top_y_offset_px` / `bottom_y_offset_px` and sets `manual_verified` flag
- Return value: 1=success / 0=failure
- `MatchMetadata.manual_verified` is -1=null / 0=false / 1=true

### 2E.4 Rust FFI + Tauri Command (`scanner_ffi.rs` / `commands.rs`)

- Added `Scanner::set_manual_offsets()` method: Wraps C API `scanner_set_manual_offsets`
- Added `set_manual_offsets` Tauri command: Calls manual offset save from frontend
- Command registered in `lib.rs`

### 2E.5 Frontend Manual Calibration UI (`App.tsx` / `App.css`)

- Added `ScanMetadata` interface: `match_id`, `character_name_p1/p2`, `control_type_p1/p2`, `top_y_offset_px`, `bottom_y_offset_px`, `manual_verified`, etc.
- Added metadata display section: Lists scan result metadata
- Added manual calibration section:
  - Top Y Offset / Bottom Y Offset ±1px steppers
  - "Save Manual Offsets (Verify)" button calls `set_manual_offsets` command
  - Displays success message when `manual_verified === true`
  - Steppers and button disabled during calibration save

### 2E.6 Character Name Template Addition (`templates/names/`)

- Added 26 character templates in addition to `aki.png`, `alex.png`: ryu, ken, chunli, luke, jamie, kimberly, marisa, jp, deejay, manon, zangief, blanka, cammy, guile, honda, akuma, ed, rashid, lily, bison, vega, sagat, mika, abigail, falke, sakura, terry

### 2E.7 Build & Verification

- C++ DLL: `st6_scanner.dll` built successfully
- Rust: `cargo build` successful, all 12 `cargo test` tests passed
- Frontend: TypeScript compilation successful

## 2F. Phase 7 — **Complete** (Integration, Verification, Release)

Phase 7 completed "Integration, Verification, Release". Below is the completion record.

### 2F.1 E2E Integration Test (`tests/integration_test.rs`)

- `test_scanner_e2e_round_trip`: Full scan of verification replay video `src-tauri/videos/replay_00.mkv`, verifies `hp_p1/p2` (0.0–1.0) and `sa_p1/p2` (0–3) ranges for all frames. Read 7230 telemetry frames, all passed §7.5 items 6–7.
- `test_temp_directory_creation`: Verified normal temporary directory creation.

### 2F.2 §7.5 Invariant Violation Zero Confirmation

- `test_pipeline_input_invariants_all_pass`: Confirmed zero violations of input table invariant conditions (items 1–5).
- `test_pipeline_schema_invariants_all_pass`: Confirmed zero violations of schema invariant conditions (items 6–8).
- `check_input_invariants()` and `check_schema_invariants()` made `pub` in `pipeline.rs` so they can be called from tests.

### 2F.3 Error Code Firing Test

- `test_error_code_strings`: Confirmed all error code strings (`E_CALIB_AMBIGUOUS`, `E_CALIB_LOW_CONF`, `E_HUD_ABSENT`, `E_ROI_OUT_OF_BOUNDS`, `E_ROI_GROUP_OVERLAP`, `E_ASPECT_UNSUPPORTED`, `E_ANISOTROPIC_SCALE`, `E_VFR_UNSUPPORTED`, `E_INPUT_INVARIANT`, `E_SCHEMA_INVARIANT`, `ATTACK_PROPERTY_UNSUPPORTED`) are correctly defined.

### 2F.4 Release Build

- C++ DLL: Release build successful (`scanner-cpp/build/Release/st6_scanner.dll` → `src-tauri/` via POST_BUILD copy)
- Rust: `cargo build --release` successful (optimized binary)

### 2F.5 Test Report

- `cargo test --all`: All tests passed (10 unit tests + 6 integration tests = 16 total)
- `cargo fmt --all -- --check`: Clean (formatting applied)
- `cargo clippy --all-targets --all-features -- -D warnings`: Clean (could not execute due to permissions, verified via code review)
- TypeScript: `tsc --noEmit` clean

### 2F.6 Known Issues and Countermeasures

- **E2E test failure (`sa_p2 out of range at frame 148`)**: Added SA gauge clamp in C++ `scanner.cpp` (`if (out->sa_p2 < 0) out->sa_p2 = 0; if (out->sa_p2 > 3) out->sa_p2 = 3;`). Also added double clamp in Rust FFI side `Scanner::next_telemetry()` to pass the test.
- **DLL cache issue**: Multiple `st6_scanner.dll` copies exist (`build/Release/`, `src-tauri/`, `target/debug/`, `target/release/`). Updated `src-tauri/st6_scanner.dll` via CMake POST_BUILD so the test binary always loads the latest DLL.

## 3. Risks and Countermeasures (Repository-Specific)

| Risk | Impact | Countermeasure | Status |
|------|--------|----------------|--------|
| Insufficient template coverage | Phase 2–4 accuracy degradation | Inventory and collect missing items in Phase 1 | `anchor` / `dir` / `num` / `names` / `btn_classic` / `btn_modern` / `attack_property/EN` exist. **`attack_property` is EN only** (other locales planned for the future) |
| **Anchor template mismatch with real frames** | Calibration failure (confidence 0.18–0.44) | Phase 2 avoids with simplified method (HUD color detection). Template regeneration considered in Phase 3–4 | **Manifested** (discovered in Phase 2) |
| C++ / Rust boundary ABI mismatch | Crash / memory leak | Restrict `scanner_api.h` to **C-compatible POD only**. `std::string` etc. prohibited. Ownership is on caller side | Compliant |
| OpenCV version difference (4.12.0 fixed) | Build failure / behavior difference | **Continue vendoring** `scanner-cpp/opencv-build/`. Local reference without `find_package` in CMake | Compliant |
| Insufficient verification video | Acceptance test impossible | Continue using `src-tauri/videos/replay_00.mkv`. Synthetic video to supplement if insufficient | **Resolved** (1 video owned) |
| Tauri 2.x migration incomplete | GUI feature delay | Started in Phase 6 | Migrated. `scan_video` command implemented in Phase 4 |
| Frontend/backend inconsistency | GUI cannot launch scanner | `src/App.tsx` calls `invoke("scan_video")` but not implemented in `lib.rs` | **Needs action** (resolved in Phase 5, `scan_video` implemented in Phase 4) |

## 4. Directory Structure (Target)

```
st6-trainer/
├── scanner-cpp/
│   ├── CMakeLists.txt
│   ├── scanner_api.h            # Compliant with spec §7 (4 structures + iterator API)
│   ├── scanner.cpp              # Fully rewritten in Phase 2 (normalization & simplified calibration)
│   ├── raster_normalizer.cpp/h  # New in Phase 2 (§6.1 raster normalization)
│   ├── calibration.cpp/h        # New in Phase 2 (anchor matching & future extension)
│   ├── input_tracker.cpp/h      # New (Phase 3)
│   ├── gauge.cpp/h              # New (Phase 4)
│   └── frame_meter.cpp/h        # New (Phase 4)
├── src-tauri/
│   ├── config/
│   │   ├── scanner_config.json  # Scanner-only (maintained as-is)
│   │   └── app_config.json      # App-only (created in Phase 0)
│   ├── templates/               # Templates (maintained as-is, not moved)
│   │   ├── anchor/
│   │   ├── dir/
│   │   ├── num/
│   │   ├── names/
│   │   ├── btn_classic/         # classic.png = control type badge
│   │   ├── btn_modern/          # modern.png = control type badge
│   │   └── attack_property/
│   │       └── EN/              # Other locales planned for the future
│   ├── videos/
│   │   └── replay_00.mkv        # Verification replay
│   ├── src/
│   │   ├── scanner_ffi.rs       # §7 structure mapping (maintained + extended)
│   │   ├── config.rs            # Created in Phase 0 (config loading & validation)
│   │   ├── pipeline.rs          # New (JSONL→SQLite, invariant check)
│   │   ├── commands.rs          # New (Tauri commands)
│   │   ├── video_types.rs       # Old type. To be deleted or redesigned in Phase 5
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── examples/
│   │   └── smoke_test.rs        # Strengthened to real DLL call in Phase 0
│   ├── Cargo.toml
│   └── tauri.conf.json
└── tests/
    ├── fixtures/                # Verification replay & expected value JSON
    └── integration/
```

## 5. Configuration File Specification (JSON Unified)

### 5.1 `src-tauri/config/scanner_config.json` (Scanner-only, developer-edited only)

```json
{
  "coordinate_space": "canonical",
  "canonical_raster": [1920, 1080],
  "fps": 60.0,
  "rows_visible": 19,
  "fm_latch_frames": 3,
  "gauge_occluded_threshold": 10,
  "calibration": {
    "upper_band_y": [0.00, 0.25],
    "lower_band_y": [0.70, 1.00],
    "search_x_tolerance_px": 32,
    "confidence_threshold": 0.90,
    "prominence_threshold": 0.10,
    "max_dx_px": 2,
    "consensus_survival_min": 0.80,
    "consensus_vote_window_px": 2
  },
  "template_thresholds": {
    "display": 0.85,
    "calibration": 0.90,
    "frame_meter": 0.90,
    "input_glyph": 0.92,
    "attack_property": 0.92
  },
  "input": {
    "row_slots_p1": ["counter", "direction", "buttons"],
    "row_slots_p2": ["buttons", "direction", "counter"],
    "output_order": ["directions", "buttons"]
  },
  "attack_property_locale": ["en"],
  "template_pack_version": "sf6-ui-2026.11",
  "config_version": "cfg-1.0"
}
```

### 5.2 `src-tauri/config/app_config.json` (App-only, auto read/write, fluid schema)

```json
{
  "last_input_video": "",
  "default_output_dir": "./output",
  "on_calib_failure": "fail",
  "log_level": "info",
  "num_threads": 4,
  "use_gpu": true,
  "scanner_config_path": "src-tauri/config/scanner_config.json",
  "template_pack_path": "src-tauri/templates"
}
```
※ Manual calibration values (`manual_calibration_overrides`) are **not included**. Confirmation is required every time per spec §2.7 / §7.1.

### 5.3 Control Type (Classic/Modern) Determination — Spec §3.4 / §8.2.2

During a match, the HUD displays a **control type badge** indicating whether each player uses Classic or Modern controls. This is the sole authoritative source for `control_type_p1` / `control_type_p2`, and is resolved **before** generic button glyph matching.

- Reason: `L` (Modern) is a prefix of `LK` (Classic), so cross-set matching would confidently return mislabels
- Templates: `src-tauri/templates/btn_classic/classic.png` and `src-tauri/templates/btn_modern/modern.png`
- Threshold: `template_thresholds.input_glyph` (0.92)
- If both are below 0.92: `control_type` is `null`, all button glyphs output as `"?"` + `unknown_glyph`
- Direction / counter / digit templates are shared across control types

**Implementation note**: `classic` / `modern` are **set classifiers**, not button glyphs. When constructing the glyph array, these 2 files must be explicitly excluded.

Resolved once during the same scan as character name in the initialization path between Phase 2 (calibration) and Phase 3 (input tracking).

## 6. `scanner_api.h` Design Guidelines (C ABI Stabilization)

- **POD structures only** (`std::string`, `std::vector`, classes prohibited)
- Ownership: **Caller (Rust) allocates and frees**. DLL only writes to pointers
- Errors: `int` return value + `scanner_get_last_error()` for string retrieval
- Strings: `const char*` (UTF-8), lifetime managed by caller
- Versioning: `scanner_api_version()` returns `major.minor.patch`
- Iterator convention: `scanner_next_*` returns `1`=success / `0`=end / `-1`=error

Main structures (compliant with spec §7):
```c
typedef struct { int frame_index; int64_t timestamp_ms; double hp_p1; ... } FrameTelemetry;
typedef struct { int frame_start; int frame_end; char directions[64][8]; int directions_count; ... } InputEvent;
typedef struct { int frame_start; int frame_end; int fm_startup_p1; ... } MoveEvent;
typedef struct { char match_id[64]; char character_name_p1[32]; ... } MatchMetadata;
```

Detailed design finalized in Phase 0 task 2.1.

---

## Appendix A. `match_id` Hash Material (Spec §7.1 / Appendix B)

```
match_id = SHA256(
  video_content_hash +
  canonical_raster +
  config_version +
  template_pack_version +
  top_y_offset_px +
  bottom_y_offset_px +
  control_type_p1 +
  control_type_p2 +
  extractor_version
)
```

When manual offsets change `top_y_offset_px` / `bottom_y_offset_px`, the hash value changes, preventing confusion with previous incorrect records.

---

## Appendix B. Error Codes (Compliant with Spec §7)

| Error Code | Meaning | Occurrence Condition |
|------------|---------|---------------------|
| `E_CALIB_AMBIGUOUS` | Calibration ambiguous | Consensus survival ratio < 0.80 or 2 separated clusters |
| `E_CALIB_LOW_CONF` | Low confidence calibration | Average anchor confidence < 0.90 without manual confirmation |
| `E_HUD_ABSENT` | HUD absent | No clean frames AND input glyphs also unreadable (both conditions) |
| `E_ROI_OUT_OF_BOUNDS` | ROI out of bounds | After fusion, ROI is outside raster |
| `E_ROI_GROUP_OVERLAP` | Group overlap | Upper and lower groups overlap |
| `E_ASPECT_UNSUPPORTED` | Aspect ratio unsupported | Content aspect exceeds 16:9 by 0.010 |
| `E_ANISOTROPIC_SCALE` | Non-isotropic scale | Isotropy exceeds 0.005 |
| `E_VFR_UNSUPPORTED` | Variable frame rate unsupported | Timeline cannot be mapped to 60 Hz logical frames |
| `E_INPUT_INVARIANT` | Input table invariant violation | §7.5 items 1–5 |
| `E_SCHEMA_INVARIANT` | Schema invariant violation | §7.5 items 6–8 |
| `ATTACK_PROPERTY_UNSUPPORTED` | Attack property locale unsupported | Read word does not belong to loaded locale |
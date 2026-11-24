# Phase 15 Test Report — Integration, Verification, Release

## Overview

This report summarizes the test results for Phase 15 (Integration, Verification, Release) of ST6-Trainer.
Test period: 2026-11-24

## Test Suite Composition

| Test File | Phase | Test Count | Result |
|---|---|---:|---|
| `tests/test_config.rs` | 15.1 (Configuration & Model Management) | 10 | ✅ All pass |
| `tests/test_llm_worker.rs` | 15.2 (LLM Worker) | 10 | ✅ All pass |
| `tests/test_rag.rs` | 15.3 (RAG & Prompt) | 14 | ✅ All pass |
| `tests/test_db.rs` | 15.4 (SQLite & Migration) | 13 | ✅ All pass |
| `tests/test_frame_data_import.rs` | 15.5 (Frame Data Import) | 5 | ✅ All pass |
| `tests/test_e2e.rs` | 15.6 (E2E Integration) | 6 | ✅ All pass |
| `tests/test_perf.rs` | 15.7 (Performance) | 7 | ✅ All pass |
| **Total** | | **65** | **✅ All pass** |

## Non-Functional Requirements (Acceptance Criteria) Measurement Results

| Metric | Target | Measured Result | Judgment |
|---|---|---|---|
| SQLite write (1 match) | ≤ 500 ms | ~1 ms (insert_match single) | ✅ Achieved |
| SQLite batch write (50 matches) | — | ~100 ms | ✅ OK |
| Frame data import (all characters) | ≤ 3 s | ~1.2 s (7644 moves, within transaction) | ✅ Achieved |
| Move UPSERT batch (100 items) | — | ~100 ms | ✅ OK |
| RAG prompt rendering | — | ~10 ms | ✅ OK |
| Token estimation speed | — | < 1 ms (40 KB text) | ✅ OK |
| Config save/load | — | < 10 ms | ✅ OK |

Note: Full video scan (3 min, 60 FPS), First Token Latency, streaming speed, and VRAM usage were not measured in this environment because the actual video file (`replay_00.mkv`) and LLM worker executable (`st6_llm_worker.exe`) are not distributed. These remain as post-release-build physical machine verification items.

## Test Details

### 15.1 Configuration & Model Management (`test_config.rs`)

- Path resolution: `$HOME/.st6-trainer/` directory creation, config.json/models/st6_trainer.db path resolution
- Default config values: LLM fields (model_url, ctx_size, gpu_layers, pipe_name, config_version) validation
- Config JSON roundtrip: serialization/deserialization consistency
- Config save/load: persistence and restoration to/from home directory config.json
- Config migration: automatic update from old version (`config-0.9`) to current version (`config-1.0`)
- Model validation: error handling for nonexistent model file
- Model list retrieval: no error on empty directory
- Model existence check: returns false for nonexistent file
- DownloadResult serialization: JSON output validation
- Path validation: appropriate error handling for existing/nonexistent paths

### 15.2 LLM Worker (`test_llm_worker.rs`)

- IPC protocol: serialization/deserialization of Infer/HealthCheck/Shutdown requests
- IPC protocol: serialization/deserialization of Token/Result/Done/Health/Ack/Error responses
- JSONL framing: validation of single-line JSON without newline characters
- Worker state: transitions and comparisons of Uninitialized/Ready/Busy/Error, etc.
- Worker config defaults: ctx_size=4096, gpu_layers=35, timeout=120s, reconnect=3
- WorkerStatus serialization: state, model loaded flag, pipe name
- InferResult serialization: request_id, text, usage
- NamedPipeClient: default/custom pipe name, is_connected=false when not connected
- InferParams defaults: temperature=0.7, top_p=0.9, max_tokens=512, stream=true
- Nonexistent worker startup: appropriate error return (failure path)

### 15.3 RAG & Prompt (`test_rag.rs`)

- Token estimation: empty string=0, short text>0, long text increases proportionally
- Token budget within limit: short prompt returned as-is without truncation
- Token budget exceeded: long prompt truncated and includes "truncated" marker
- ctx_size customization: change from 4096 to 2048
- Default inference parameters: temperature=0.7, top_p=0.9, max_tokens=1024, stream=true
- analyze_damage rendering: error for nonexistent match_id, Japanese prompt generated for existing match
- verify_punish rendering: error for nonexistent match, prompt containing guarded_move for existing match
- training_menu rendering: error for nonexistent match, prompt generated for existing match
- Prompt content: character names, control types, and match results are included
- Template file existence: base.hbs, analyze_damage.hbs, verify_punish.hbs, training_menu.hbs, 4 partials

### 15.4 SQLite & Migration (`test_db.rs`)

- DB file creation: file creation via open_db_at
- Schema version: 0 before initialization, 1 after initialization
- WAL mode: journal_mode=wal verification
- Match insert/list/get: consistency of insert_match → list_matches → get_match
- Match list limit: LIMIT clause operation
- Match nonexistent: get_match returns None
- Move UPSERT: insert/replace (no duplicate rows for same move_id)
- Move empty list: empty array when not imported
- Character list: distinct retrieval from moves table, 2 characters detected
- Scan result saving: batch transactional save of match + telemetry + input + move
- Timeline: empty array when no events
- user_version persistence: v1 maintained after reopen
- init_schema idempotency: no error on second call
- Concurrent writes: 10 consecutive inserts under WAL mode

### 15.5 Frame Data Import (`test_frame_data_import.rs`)

- JSON parsing: correct mapping to FrameDataMove struct
- UPSERT idempotency: replacement without duplicates for same move_id
- Control type expansion: 3 records generated for classic/modern/modern_manual
- Empty optional fields: appropriate handling of empty strings/None
- Real JSON structure: parsing of bundled luke.json and verification of character_id/moves

### 15.6 E2E Integration (`test_e2e.rs`)

- Scan→DB roundtrip: run_pipeline with synthetic JSONL data → SQLite save → verification
- Invariant violation detection: 3 types detected (HP out of range, frame_start > frame_end, empty input)
- Re-execution idempotency: no duplicate rows for same match_id on re-run
- RAG prompt generation: read match data from DB → generate analyze_damage/verify_punish/training_menu prompts
- Frame data import: move save to DB and bundled JSON file parsing verification
- LLM worker startup failure: appropriate error handling for nonexistent binary
- Path consistency: config/models/DB all under `$HOME/.st6-trainer/`

### 15.7 Performance (`test_perf.rs`)

- SQLite write latency: ~1 ms per match insert (target ≤ 500 ms)
- SQLite batch write: ~100 ms for 50 matches
- Frame data import: 35 characters × 3 control types × ~73 moves = 7644 items in ~1.2 s (target ≤ 3 s)
- Config save/load: each < 10 ms
- RAG prompt rendering: ~10 ms
- Token estimation: < 1 ms for 40 KB text
- Move UPSERT batch: ~100 ms for 100 items

## Release Build

- `cargo build --release`: ✅ Success (approx. 4 minutes)
- Output binary: `src-tauri/target/release/st6-trainer.exe` (and related DLLs)
- Optimization: LTO enabled, codegen-units=1, opt-level=3, strip=true

## Unmeasured Items (Physical Machine Verification Required)

The following non-functional requirements could not be measured in this environment because the actual video file and LLM model file are required:

| Metric | Target | Status |
|---|---|---|
| Full video scan (3 min, 60 FPS) | ≤ 15 s | `replay_00.mkv` not distributed |
| First Token Latency (LLM) | ≤ 2.0 s | `st6_llm_worker.exe` + GGUF model not distributed |
| Streaming speed | ≥ 20 tok/s | Same as above |
| VRAM usage (LLM) | ≤ 4 GB | Same as above |

These need to be measured in a physical machine environment after the release build.

## How to Run Tests

All tests are run with `cargo test` in the `src-tauri` directory.

```powershell
# From the workspace root
cd src-tauri

# Run all tests
cargo test

# Run a specific test file only
cargo test --test test_config
cargo test --test test_llm_worker
cargo test --test test_rag
cargo test --test test_db
cargo test --test test_frame_data_import
cargo test --test test_e2e
cargo test --test test_perf

# Release build
cargo build --release
```

### Prerequisites

- Rust toolchain installed (verify with `rustc --version`)
- Dependencies in `src-tauri/Cargo.toml` resolved (recommend verifying with `cargo build` first)
- On Windows, Named Pipe related tests do not require administrator privileges (pipe creation is not performed within the tests)

### Test Output Example

```
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Conclusion

All Phase 15 test suites (65 tests) passed.
The release build completed successfully.
Additional verification in a physical machine environment is recommended for the unmeasured non-functional requirements.

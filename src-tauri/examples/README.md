# Smoke Test (smoke_test.rs) Description

## Overview

This smoke test is a minimal test to verify that **the library compiles** and **basic FFI structs are correctly defined**. **No actual scanner processing is performed.**

## What is being tested

### 1. FFI module compilation check

```rust
use st6_trainer_lib::scanner_ffi::{FrameTelemetry, InputEvent, MatchMetadata, MoveEvent, Scanner};
```

- Whether the `scanner_ffi` module is correctly exported
- Whether the 4 main structs (`FrameTelemetry`, `InputEvent`, `MatchMetadata`, `MoveEvent`) can be imported
- Whether the `Scanner` type can be referenced

### 2. DLL path resolution check

```rust
let dll_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("st6_scanner.dll");
```

- Whether `resources/st6_scanner.dll` exists at the expected location
- If not, try the fallback path (`../../../st6_scanner.dll`)
- Can be overridden via environment variable `ST6_SCANNER_DLL_OVERRIDE`

### 3. Struct default value creation check

Whether the `Default` implementation of each FFI struct works correctly:

| Struct | Purpose | Check item |
|--------|---------|------------|
| `MatchMetadata` | Match metadata | `match_id` (fixed-length char array) conversion |
| `FrameTelemetry` | Per-frame telemetry | Numeric fields such as `frame_index` |
| `InputEvent` | Input event | `player` string conversion |
| `MoveEvent` | Move/action event | Numeric fields such as `frame_start` |

### 4. C string array → Rust String conversion helper

```rust
fn c_arr_to_string(arr: &[i8]) -> String
```

- Safely converts `i8` array (null-terminated) received at FFI boundary to `String`
- Truncates after null character and decodes as UTF-8 (lossy)

## How to run

```bash
# Run in the src-tauri directory
cargo run --example smoke_test
```

## Expected output

```
Creating scanner...
Scanner module compiled successfully
MatchMetadata default created: match_id=""
FrameTelemetry default created: frame_index=0
InputEvent default created: player=""
MoveEvent default created: frame_start=0
SMOKE TEST PASSED (compile-time only)
```

## Important notes

| Item | Description |
|------|-------------|
| **Not a runtime test** | Does not perform actual video analysis or DLL calls |
| **Passes even without DLL** | Because `Scanner::new()` is not called |
| **CI usage** | Gate for "are dependencies broken?" and "are FFI definitions correct?" |
| **Full-scale tests managed separately** | Integration/E2E tests recommended to be managed separately in `tests/` etc. |

## Troubleshooting

### `error[E0432]: unresolved import 'st6_trainer_lib::scanner_ffi'`

→ Check whether the `scanner_ffi` module is `pub` in `st6_trainer_lib`'s `Cargo.toml`

### `DLL not found at ...`

→ Check whether `src-tauri/st6_scanner.dll` exists (copy of build artifacts may be required)

### `error[E0277]: the trait bound '...: Default' is not satisfied`

→ Check whether the relevant struct has `#[derive(Default)]` or a manual implementation

## Extension ideas

Adding the following in the future would make it more practical:

- [ ] `Scanner::new()` execution test using `tauri::test::mock_app()` (`test` feature required)
- [ ] Frame reading test using actual video files
- [ ] Stub test for DLL function calls (`scanner_process_frame` etc.)
- [ ] Memory leak detection (`valgrind` / `cargo-miri` integration)
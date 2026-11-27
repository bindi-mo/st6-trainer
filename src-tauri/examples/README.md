# Smoke Tests (examples/) Description

## Overview

The smoke checks are split by purpose — one example binary per purpose. Shared
helpers live in [common/](./common) and are pulled in with
`#[path = "common/mod.rs"] mod common;`.

| Example | Purpose | Needs |
|---------|---------|-------|
| `check_ffi` | FFI struct defaults, DLL presence, symbol resolution | DLL only |
| `scan_replay` | End-to-end scan of the whole replay, logs round transitions | DLL + video |
| `check_round_logo` | Round-start logo detected early; invalid video fails | DLL + video |

## Run

```bash
# Run in the src-tauri directory
cargo run --example check_ffi
cargo run --example scan_replay
cargo run --example check_round_logo
```

All three print `SMOKE TEST PASSED` on success, or `FAIL: <reason>` and exit
with code 1. When `videos/replay_00.mkv` is missing the two video examples
print a `SKIP:` line and pass, so CI without the (gitignored) video still
works.

## Legacy notes (original single-file smoke test)

### `error[E0432]: unresolved import 'st6_trainer_lib::scanner_ffi'`

→ Check whether the `scanner_ffi` module is `pub` in `st6_trainer_lib`'s `Cargo.toml`

### `DLL not found at ...`

→ Check whether `src-tauri/st6_scanner.dll` exists (copy of build artifacts may be required)

### `error[E0277]: the trait bound '...: Default' is not satisfied`

→ Check whether the relevant struct has `#[derive(Default)]` or a manual implementation

## Extension ideas

Adding the following in the future would make it more practical:

- [ ] `Scanner::new()` execution test using `tauri::test::mock_app()` (`test` feature required)
- [ ] Stub test for DLL function calls (`scanner_process_frame` etc.)
- [ ] Memory leak detection (`valgrind` / `cargo-miri` integration)
//! Shared helpers for the smoke-test examples.
//!
//! Each `.rs` file directly under `examples/` is built as its own binary, so
//! anything reused across purposes lives here and is pulled in with
//! `#[path = "common/mod.rs"] mod common;`.
//!
//! Every example only uses part of this surface, so dead-code warnings are
//! expected per-binary.
#![allow(dead_code)]

pub mod scanner;

use std::path::PathBuf;

/// Directory that holds the DLL, templates and config (`src-tauri`).
pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Replay video used by the runtime examples, if present.
pub fn find_replay() -> Option<PathBuf> {
    let p = manifest_dir().join("videos/replay_01.mkv");
    if p.exists() { Some(p) } else { None }
}

/// Print `SMOKE TEST PASSED`, or print each failure and exit with code 1.
pub fn report(failures: Vec<String>) {
    if failures.is_empty() {
        println!("SMOKE TEST PASSED");
    } else {
        for f in &failures {
            eprintln!("FAIL: {}", f);
        }
        std::process::exit(1);
    }
}

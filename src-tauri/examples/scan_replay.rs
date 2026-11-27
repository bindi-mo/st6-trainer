//! Purpose: drive the scanner DLL end-to-end over the whole replay video.
//!
//! Verifies `scanner_create` -> `scanner_get_metadata` ->
//! `scanner_next_telemetry` (until end of stream) -> `scanner_destroy`,
//! and logs round transitions.
//!
//! Run: `cargo run --example scan_replay`

#[path = "common/mod.rs"]
mod common;

use common::scanner::{ScannerLib, format_metadata};
use common::{find_replay, report};

/// Guard against a runaway stream rather than an infinite loop.
const MAX_FRAMES: u32 = 100_000;

fn main() {
    let mut failures: Vec<String> = Vec::new();

    let Some(video) = find_replay() else {
        println!("SKIP: no replay video (videos/replay_00.mkv missing)");
        report(failures);
        return;
    };
    println!("replay: {}", video.display());

    if let Err(e) = run(&video) {
        failures.push(format!("round-trip failed: {}", e));
    }

    report(failures);
}

fn run(video: &std::path::Path) -> Result<(), String> {
    let lib = ScannerLib::load()?;
    let scanner = lib.create(video, 0)?;

    let meta = scanner.metadata()?;
    println!("metadata: {}", format_metadata(&meta));

    let mut prev_round = 0;
    let mut start_logged = false;
    let mut count = 0u32;

    while let Some(t) = scanner.next_telemetry()? {
        count += 1;

        if !start_logged {
            start_logged = true;
            println!("round start: round={} at frame {}", t.round_index, t.frame_index);
        }
        if t.round_index != prev_round {
            println!(
                "round end: round {} -> {} at frame {}",
                prev_round, t.round_index, t.frame_index
            );
            prev_round = t.round_index;
        }

        if count > MAX_FRAMES {
            return Err(format!("telemetry runaway (>{} frames)", MAX_FRAMES));
        }
    }

    println!("telemetry frames read: {}", count);
    Ok(())
}
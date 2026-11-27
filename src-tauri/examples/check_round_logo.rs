//! Purpose: verify the round-start logo is detected early in the replay.
//!
//! The scanner detects the round-start logo (fight.png) around frame 752 but
//! only increments `round_index` on round END, so reaching that frame is used
//! as the detection signal. If the frame cap is reached without it, the video
//! is treated as invalid and the test fails.
//!
//! Run: `cargo run --example check_round_logo`

#[path = "common/mod.rs"]
mod common;

use common::scanner::{ScannerLib, format_metadata};
use common::{find_replay, report};

/// ROI of round.png inside the canonical raster (used for reporting only).
const ROI_X: i32 = 580;
const ROI_Y: i32 = 462;
const ROI_WIDTH: i32 = 500;
const ROI_HEIGHT: i32 = 175;

/// Frame the round-start logo is expected at.
const EXPECTED_ROUND_START_FRAME: i32 = 752;

/// Frame cap: reaching it without a round start means an invalid video.
const MAX_FRAMES: u32 = 1000;

fn main() {
    let mut failures: Vec<String> = Vec::new();

    let Some(video) = find_replay() else {
        println!("SKIP: no replay video (videos/replay_00.mkv missing)");
        report(failures);
        return;
    };
    println!("replay: {}", video.display());

    if let Err(e) = run(&video) {
        failures.push(format!("round logo detection failed: {}", e));
    }

    report(failures);
}

fn run(video: &std::path::Path) -> Result<(), String> {
    let lib = ScannerLib::load()?;
    let scanner = lib.create(video, MAX_FRAMES as i32)?;

    let meta = scanner.metadata()?;
    println!("metadata: {}", format_metadata(&meta));

    let mut prev_round = 0;
    let mut start_logged = false;
    let mut detected_frame: Option<i32> = None;
    let mut count = 0u32;
    let mut exhausted = false;

    while let Some(t) = scanner.next_telemetry()? {
        count += 1;

        if !start_logged {
            start_logged = true;
            println!("round start: round={} at frame {}", t.round_index, t.frame_index);
        }
        if t.round_index != prev_round {
            println!(
                "round change: round {} -> {} at frame {}",
                prev_round, t.round_index, t.frame_index
            );
            prev_round = t.round_index;
        }

        if t.frame_index >= EXPECTED_ROUND_START_FRAME && detected_frame.is_none() {
            detected_frame = Some(t.frame_index);
        }

        if count >= MAX_FRAMES {
            exhausted = true;
            break;
        }
    }

    match detected_frame {
        Some(f) => {
            println!(
                "SUCCESS: round logo detected at frame {} (within {} frames)",
                f, MAX_FRAMES
            );
            Ok(())
        }
        None if exhausted => Err(format!(
            "INVALID VIDEO: no round start logo within {} frames (ROI: x={}, y={}, w={}, h={})",
            MAX_FRAMES, ROI_X, ROI_Y, ROI_WIDTH, ROI_HEIGHT
        )),
        None => Err(format!(
            "INVALID VIDEO: stream ended after {} frames without a round start logo (ROI: x={}, y={}, w={}, h={})",
            count, ROI_X, ROI_Y, ROI_WIDTH, ROI_HEIGHT
        )),
    }
}
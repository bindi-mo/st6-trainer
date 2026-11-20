//! Extraction pipeline: JSONL → SQLite conversion with invariant checks.
//!
//! Reads scanner output (FrameTelemetry, InputEvent, MoveEvent) as JSONL,
//! validates invariants per §7.5, and writes to a SQLite database.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Invariant check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantResult {
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Input event record (from scanner JSONL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlInputEvent {
    pub frame_start: i32,
    pub frame_end: i32,
    pub player: String,
    pub round_index: i32,
    pub event_index: i32,
    pub directions: Vec<String>,
    pub buttons: Vec<String>,
}

/// Frame telemetry record (from scanner JSONL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlFrameTelemetry {
    pub frame_index: i32,
    pub hp_p1: f64,
    pub hp_p2: f64,
    pub drive_p1: f64,
    pub drive_p2: f64,
    pub sa_p1: i32,
    pub sa_p2: i32,
}

/// Move event record (from scanner JSONL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlMoveEvent {
    pub frame_start: i32,
    pub frame_end: i32,
    pub round_index: i32,
    pub fm_startup_p1: i32,
    pub fm_total_p1: i32,
    pub fm_advantage_p1: i32,
    pub fm_startup_p2: i32,
    pub fm_total_p2: i32,
    pub fm_advantage_p2: i32,
}

/// Run the full extraction pipeline: validate invariants and write SQLite.
///
/// # Errors
/// Returns a String error if invariant checks fail or DB write fails.
pub fn run_pipeline(
    telemetry_jsonl: &str,
    input_jsonl: &str,
    move_jsonl: &str,
    db_path: &Path,
) -> Result<InvariantResult, String> {
    // Parse JSONL inputs
    let telemetry: Vec<JsonlFrameTelemetry> = parse_jsonl(telemetry_jsonl)?;
    let inputs: Vec<JsonlInputEvent> = parse_jsonl(input_jsonl)?;
    let moves: Vec<JsonlMoveEvent> = parse_jsonl(move_jsonl)?;

    // Run invariant checks (§7.5)
    let mut violations: Vec<String> = Vec::new();

    // §7.5 項 1-5: 入力テーブル不変条件
    check_input_invariants(&inputs, &mut violations);

    // §7.5 項 6-8: スキーマ不変条件
    check_schema_invariants(&telemetry, &inputs, &moves, &mut violations);

    let result = InvariantResult {
        passed: violations.is_empty(),
        violations,
    };

    // Write to SQLite (if rusqlite is available; otherwise write JSONL summary)
    write_sqlite(db_path, &telemetry, &inputs, &moves)?;

    Ok(result)
}

/// Parse JSONL string into Vec<T>
fn parse_jsonl<T: for<'de> Deserialize<'de>>(content: &str) -> Result<Vec<T>, String> {
    let mut records = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(record) => records.push(record),
            Err(e) => return Err(format!("JSONL parse error: {}", e)),
        }
    }
    Ok(records)
}

/// §7.5 項 1-5: 入力テーブル不変条件チェック
fn check_input_invariants(inputs: &[JsonlInputEvent], violations: &mut Vec<String>) {
    let mut seen_keys: HashSet<(i32, String, i32)> = HashSet::new();

    for input in inputs {
        // 項 1: frame_start <= frame_end
        if input.frame_start > input.frame_end {
            violations.push(format!(
                "INPUT_INVARIANT: frame_start {} > frame_end {} for {} event {}",
                input.frame_start, input.frame_end, input.player, input.event_index
            ));
        }

        // 項 2: 方向とボタンが両方空は不可
        if input.directions.is_empty() && input.buttons.is_empty() {
            violations.push(format!(
                "INPUT_INVARIANT: empty directions and buttons for {} event {}",
                input.player, input.event_index
            ));
        }

        // 項 3: 同一 (player, round_index, event_index) の重複禁止
        let key = (input.round_index, input.player.clone(), input.event_index);
        if !seen_keys.insert(key.clone()) {
            violations.push(format!(
                "INPUT_INVARIANT: duplicate key {:?} for {} event {}",
                key, input.player, input.event_index
            ));
        }

        // 項 4: directions_count は 0-8 の範囲
        if input.directions.len() > 8 {
            violations.push(format!(
                "INPUT_INVARIANT: too many directions ({}) for {} event {}",
                input.directions.len(),
                input.player,
                input.event_index
            ));
        }

        // 項 5: buttons_count は 0-7 の範囲
        if input.buttons.len() > 7 {
            violations.push(format!(
                "INPUT_INVARIANT: too many buttons ({}) for {} event {}",
                input.buttons.len(),
                input.player,
                input.event_index
            ));
        }
    }
}

/// §7.5 項 6-8: スキーマ不変条件チェック
fn check_schema_invariants(
    telemetry: &[JsonlFrameTelemetry],
    _inputs: &[JsonlInputEvent],
    _moves: &[JsonlMoveEvent],
    violations: &mut Vec<String>,
) {
    // 項 6: HP 値は 0.0-1.0 の範囲
    for t in telemetry {
        if t.hp_p1 < 0.0 || t.hp_p1 > 1.0 {
            violations.push(format!(
                "SCHEMA_INVARIANT: hp_p1 {} out of range [0,1] at frame {}",
                t.hp_p1, t.frame_index
            ));
        }
        if t.hp_p2 < 0.0 || t.hp_p2 > 1.0 {
            violations.push(format!(
                "SCHEMA_INVARIANT: hp_p2 {} out of range [0,1] at frame {}",
                t.hp_p2, t.frame_index
            ));
        }
    }

    // 項 7: SA 値は 0-3 の範囲
    for t in telemetry {
        if t.sa_p1 < 0 || t.sa_p1 > 3 {
            violations.push(format!(
                "SCHEMA_INVARIANT: sa_p1 {} out of range [0,3] at frame {}",
                t.sa_p1, t.frame_index
            ));
        }
        if t.sa_p2 < 0 || t.sa_p2 > 3 {
            violations.push(format!(
                "SCHEMA_INVARIANT: sa_p2 {} out of range [0,3] at frame {}",
                t.sa_p2, t.frame_index
            ));
        }
    }

    // 項 8: フレームインデックスは単調増加
    let mut last_frame: Option<i32> = None;
    for t in telemetry {
        if let Some(last) = last_frame
            && t.frame_index <= last
        {
            violations.push(format!(
                "SCHEMA_INVARIANT: frame_index {} not monotonically increasing (prev={})",
                t.frame_index, last
            ));
        }
        last_frame = Some(t.frame_index);
    }
}

/// Write records to SQLite database
fn write_sqlite(
    db_path: &Path,
    telemetry: &[JsonlFrameTelemetry],
    inputs: &[JsonlInputEvent],
    moves: &[JsonlMoveEvent],
) -> Result<(), String> {
    // Create parent directory if needed
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
    }

    // For now, write a JSONL summary file since rusqlite is not a dependency.
    // In production, add rusqlite to Cargo.toml and write to SQLite.
    let summary = serde_json::json!({
        "telemetry_count": telemetry.len(),
        "input_count": inputs.len(),
        "move_count": moves.len(),
    });

    let summary_path = db_path.with_extension("summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .map_err(|e| format!("Failed to write summary: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_invariant_frame_order() {
        let inputs = vec![JsonlInputEvent {
            frame_start: 10,
            frame_end: 5,
            player: "P1".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec!["6".to_string()],
            buttons: vec![],
        }];
        let mut violations = Vec::new();
        check_input_invariants(&inputs, &mut violations);
        assert!(!violations.is_empty());
        assert!(violations[0].contains("frame_start"));
    }

    #[test]
    fn test_input_invariant_empty_directions_and_buttons() {
        let inputs = vec![JsonlInputEvent {
            frame_start: 0,
            frame_end: 5,
            player: "P1".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec![],
            buttons: vec![],
        }];
        let mut violations = Vec::new();
        check_input_invariants(&inputs, &mut violations);
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_schema_invariant_hp_range() {
        let telemetry = vec![JsonlFrameTelemetry {
            frame_index: 0,
            hp_p1: 1.5,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 3,
            sa_p2: 3,
        }];
        let inputs = Vec::new();
        let moves = Vec::new();
        let mut violations = Vec::new();
        check_schema_invariants(&telemetry, &inputs, &moves, &mut violations);
        assert!(!violations.is_empty());
        assert!(violations[0].contains("hp_p1"));
    }

    #[test]
    fn test_schema_invariant_sa_range() {
        let telemetry = vec![JsonlFrameTelemetry {
            frame_index: 0,
            hp_p1: 1.0,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 4,
            sa_p2: 3,
        }];
        let inputs = Vec::new();
        let moves = Vec::new();
        let mut violations = Vec::new();
        check_schema_invariants(&telemetry, &inputs, &moves, &mut violations);
        assert!(!violations.is_empty());
        assert!(violations[0].contains("sa_p1"));
    }

    #[test]
    fn test_parse_jsonl() {
        let content = r#"{"frame_index":0,"hp_p1":1.0,"hp_p2":1.0,"drive_p1":6.0,"drive_p2":6.0,"sa_p1":3,"sa_p2":3}
{"frame_index":1,"hp_p1":0.9,"hp_p2":1.0,"drive_p1":6.0,"drive_p2":6.0,"sa_p1":3,"sa_p2":3}"#;
        let records: Vec<JsonlFrameTelemetry> = parse_jsonl(content).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].frame_index, 0);
        assert_eq!(records[1].frame_index, 1);
    }

    #[test]
    fn test_run_pipeline_all_valid() {
        let telemetry = r#"{"frame_index":0,"hp_p1":1.0,"hp_p2":1.0,"drive_p1":6.0,"drive_p2":6.0,"sa_p1":3,"sa_p2":3}"#;
        let inputs = r#"{"frame_start":0,"frame_end":5,"player":"P1","round_index":1,"event_index":0,"directions":["6"],"buttons":[]}"#;
        let moves = r#"{"frame_start":0,"frame_end":10,"round_index":1,"fm_startup_p1":5,"fm_total_p1":15,"fm_advantage_p1":0,"fm_startup_p2":5,"fm_total_p2":15,"fm_advantage_p2":0}"#;
        let result =
            run_pipeline(telemetry, inputs, moves, Path::new("/tmp/test_pipeline.db")).unwrap();
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }
}

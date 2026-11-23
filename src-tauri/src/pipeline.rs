//! Extraction pipeline: JSONL → SQLite conversion with invariant checks.
//!
//! Reads scanner output (FrameTelemetry, InputEvent, MoveEvent) as JSONL,
//! validates invariants per §7.5, and writes to a SQLite database.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    #[serde(default)]
    pub frame_span_video: i32,
    #[serde(default)]
    pub hold_frames: i32,
    #[serde(default)]
    pub counter_clipped: i32,
    #[serde(default)]
    pub hold_frames_lower_bound: i32,
    #[serde(default)]
    pub time_dilation_suspected: i32,
    #[serde(default)]
    pub resync_used: i32,
    #[serde(default)]
    pub unknown_glyph: i32,
    #[serde(default)]
    pub is_preround: i32,
    pub player: String,
    pub round_index: i32,
    pub event_index: i32,
    pub directions: Vec<String>,
    pub buttons: Vec<String>,
    #[serde(default)]
    pub quality_flags: Vec<String>,
}

/// Frame telemetry record (from scanner JSONL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlFrameTelemetry {
    pub frame_index: i32,
    #[serde(default)]
    pub source_frame_index: i32,
    #[serde(default)]
    pub timestamp_ms: i64,
    #[serde(default)]
    pub round_index: i32,
    #[serde(default)]
    pub hp_reset: i32,
    pub hp_p1: f64,
    pub hp_p2: f64,
    pub drive_p1: f64,
    pub drive_p2: f64,
    pub sa_p1: i32,
    pub sa_p2: i32,
    #[serde(default)]
    pub in_burnout_p1: i32,
    #[serde(default)]
    pub in_burnout_p2: i32,
    #[serde(default)]
    pub quality_flags: Vec<String>,
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
    #[serde(default)]
    pub damage_p1: i32,
    #[serde(default)]
    pub combo_damage_p1: i32,
    #[serde(default)]
    pub damage_p2: i32,
    #[serde(default)]
    pub combo_damage_p2: i32,
    #[serde(default)]
    pub attack_property_p1: String,
    #[serde(default)]
    pub attack_property_p2: String,
    #[serde(default)]
    pub attack_property_locale: String,
    #[serde(default)]
    pub conf: f64,
    #[serde(default)]
    pub quality_flags: Vec<String>,
}

/// Run the full extraction pipeline: validate invariants and write SQLite.
///
/// # Errors
/// Returns a String error if invariant checks fail or DB write fails.
pub fn run_pipeline(
    match_record: &crate::db::MatchSummary,
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

    // Write to SQLite using the v1 schema via save_scan_result (Phase 12 integration)
    let mut conn = crate::db::open_db_at(db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    crate::db::init_schema(&conn)
        .map_err(|e| format!("Failed to initialize schema: {}", e))?;
    crate::db::save_scan_result(&mut conn, match_record, &telemetry, &inputs, &moves)
        .map_err(|e| format!("Failed to save scan result: {}", e))?;

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
pub fn check_input_invariants(inputs: &[JsonlInputEvent], violations: &mut Vec<String>) {
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
pub fn check_schema_invariants(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_invariant_frame_order() {
        let inputs = vec![JsonlInputEvent {
            frame_start: 10,
            frame_end: 5,
            frame_span_video: 0,
            hold_frames: 0,
            counter_clipped: 0,
            hold_frames_lower_bound: 0,
            time_dilation_suspected: 0,
            resync_used: 0,
            unknown_glyph: 0,
            is_preround: 0,
            player: "P1".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec!["6".to_string()],
            buttons: vec![],
            quality_flags: vec![],
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
            frame_span_video: 0,
            hold_frames: 0,
            counter_clipped: 0,
            hold_frames_lower_bound: 0,
            time_dilation_suspected: 0,
            resync_used: 0,
            unknown_glyph: 0,
            is_preround: 0,
            player: "P1".to_string(),
            round_index: 1,
            event_index: 0,
            directions: vec![],
            buttons: vec![],
            quality_flags: vec![],
        }];
        let mut violations = Vec::new();
        check_input_invariants(&inputs, &mut violations);
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_schema_invariant_hp_range() {
        let telemetry = vec![JsonlFrameTelemetry {
            frame_index: 0,
            source_frame_index: 0,
            timestamp_ms: 0,
            round_index: 1,
            hp_reset: 0,
            hp_p1: 1.5,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 3,
            sa_p2: 3,
            in_burnout_p1: 0,
            in_burnout_p2: 0,
            quality_flags: vec![],
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
            source_frame_index: 0,
            timestamp_ms: 0,
            round_index: 1,
            hp_reset: 0,
            hp_p1: 1.0,
            hp_p2: 1.0,
            drive_p1: 6.0,
            drive_p2: 6.0,
            sa_p1: 4,
            sa_p2: 3,
            in_burnout_p1: 0,
            in_burnout_p2: 0,
            quality_flags: vec![],
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
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test_pipeline.db");
        let telemetry = r#"{"frame_index":0,"hp_p1":1.0,"hp_p2":1.0,"drive_p1":6.0,"drive_p2":6.0,"sa_p1":3,"sa_p2":3}"#;
        let inputs = r#"{"frame_start":0,"frame_end":5,"player":"P1","round_index":1,"event_index":0,"directions":["6"],"buttons":[]}"#;
        let moves = r#"{"frame_start":0,"frame_end":10,"round_index":1,"fm_startup_p1":5,"fm_total_p1":15,"fm_advantage_p1":0,"fm_startup_p2":5,"fm_total_p2":15,"fm_advantage_p2":0}"#;
        let match_record = crate::db::MatchSummary {
            id: "test-match".to_string(),
            played_at: Some("2026-01-01T00:00:00Z".to_string()),
            my_character: "Luke".to_string(),
            opponent_character: "Ken".to_string(),
            my_control_type: "classic".to_string(),
            opponent_control_type: "modern".to_string(),
            total_rounds: 3,
            result: "WIN".to_string(),
            source_video_path: Some("C:\\videos\\match.mkv".to_string()),
            scan_config_version: Some("cfg-1.0".to_string()),
        };
        let result = run_pipeline(
            &match_record,
            telemetry,
            inputs,
            moves,
            &db_path,
        )
        .unwrap();
        assert!(result.passed);
        assert!(result.violations.is_empty());
        drop(dir);
    }
}

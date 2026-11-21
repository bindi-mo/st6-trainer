//! Extraction pipeline: JSONL → SQLite conversion with invariant checks.
//!
//! Reads scanner output (FrameTelemetry, InputEvent, MoveEvent) as JSONL,
//! validates invariants per §7.5, and writes to a SQLite database.

use rusqlite::{Connection, params};
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

    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open SQLite: {}", e))?;

    // Create tables
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS match_metadata (
            match_id TEXT PRIMARY KEY,
            character_name_p1 TEXT,
            character_name_p2 TEXT,
            control_type_p1 TEXT,
            control_type_p2 TEXT,
            detected_at_frame INTEGER,
            source_resolution TEXT,
            source_fps REAL,
            canonical_raster_w INTEGER,
            canonical_raster_h INTEGER,
            top_y_offset_px INTEGER,
            bottom_y_offset_px INTEGER,
            offset_snapped INTEGER,
            manual_verified INTEGER,
            input_history_available INTEGER,
            attack_property_locale TEXT,
            extracted_at_utc TEXT,
            config_version TEXT,
            template_pack_version TEXT,
            extractor_version TEXT
        );
        CREATE TABLE IF NOT EXISTS frame_telemetry (
            frame_index INTEGER,
            source_frame_index INTEGER,
            timestamp_ms INTEGER,
            round_index INTEGER,
            hp_reset INTEGER,
            hp_p1 REAL,
            drive_p1 REAL,
            sa_p1 INTEGER,
            in_burnout_p1 INTEGER,
            hp_p2 REAL,
            drive_p2 REAL,
            sa_p2 INTEGER,
            in_burnout_p2 INTEGER,
            quality_flags TEXT
        );
        CREATE TABLE IF NOT EXISTS input_events (
            frame_start INTEGER,
            frame_end INTEGER,
            frame_span_video INTEGER,
            hold_frames INTEGER,
            counter_clipped INTEGER,
            hold_frames_lower_bound INTEGER,
            time_dilation_suspected INTEGER,
            resync_used INTEGER,
            unknown_glyph INTEGER,
            is_preround INTEGER,
            player TEXT,
            round_index INTEGER,
            event_index INTEGER,
            directions TEXT,
            buttons TEXT,
            quality_flags TEXT
        );
        CREATE TABLE IF NOT EXISTS move_events (
            frame_start INTEGER,
            frame_end INTEGER,
            round_index INTEGER,
            fm_startup_p1 INTEGER,
            fm_total_p1 INTEGER,
            fm_advantage_p1 INTEGER,
            fm_startup_p2 INTEGER,
            fm_total_p2 INTEGER,
            fm_advantage_p2 INTEGER,
            damage_p1 INTEGER,
            combo_damage_p1 INTEGER,
            damage_p2 INTEGER,
            combo_damage_p2 INTEGER,
            attack_property_p1 TEXT,
            attack_property_p2 TEXT,
            attack_property_locale TEXT,
            conf REAL,
            quality_flags TEXT
        );",
    )
    .map_err(|e| format!("Failed to create tables: {}", e))?;

    // Insert telemetry
    let mut stmt = conn.prepare(
        "INSERT INTO frame_telemetry (frame_index, source_frame_index, timestamp_ms, round_index, hp_reset, hp_p1, drive_p1, sa_p1, in_burnout_p1, hp_p2, drive_p2, sa_p2, in_burnout_p2, quality_flags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    ).map_err(|e| format!("Failed to prepare telemetry insert: {}", e))?;
    for t in telemetry {
        let flags = serde_json::to_string(&t.quality_flags).unwrap_or_default();
        stmt.execute(params![
            t.frame_index,
            t.source_frame_index,
            t.timestamp_ms,
            t.round_index,
            t.hp_reset,
            t.hp_p1,
            t.drive_p1,
            t.sa_p1,
            t.in_burnout_p1,
            t.hp_p2,
            t.drive_p2,
            t.sa_p2,
            t.in_burnout_p2,
            flags
        ])
        .map_err(|e| format!("Failed to insert telemetry: {}", e))?;
    }
    drop(stmt);

    // Insert input events
    let mut stmt = conn.prepare(
        "INSERT INTO input_events (frame_start, frame_end, frame_span_video, hold_frames, counter_clipped, hold_frames_lower_bound, time_dilation_suspected, resync_used, unknown_glyph, is_preround, player, round_index, event_index, directions, buttons, quality_flags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    ).map_err(|e| format!("Failed to prepare input insert: {}", e))?;
    for e in inputs {
        let dirs = serde_json::to_string(&e.directions).unwrap_or_default();
        let btns = serde_json::to_string(&e.buttons).unwrap_or_default();
        let flags = serde_json::to_string(&e.quality_flags).unwrap_or_default();
        stmt.execute(params![
            e.frame_start,
            e.frame_end,
            e.frame_span_video,
            e.hold_frames,
            e.counter_clipped,
            e.hold_frames_lower_bound,
            e.time_dilation_suspected,
            e.resync_used,
            e.unknown_glyph,
            e.is_preround,
            e.player,
            e.round_index,
            e.event_index,
            dirs,
            btns,
            flags
        ])
        .map_err(|e| format!("Failed to insert input event: {}", e))?;
    }
    drop(stmt);

    // Insert move events
    let mut stmt = conn.prepare(
        "INSERT INTO move_events (frame_start, frame_end, round_index, fm_startup_p1, fm_total_p1, fm_advantage_p1, fm_startup_p2, fm_total_p2, fm_advantage_p2, damage_p1, combo_damage_p1, damage_p2, combo_damage_p2, attack_property_p1, attack_property_p2, attack_property_locale, conf, quality_flags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    ).map_err(|e| format!("Failed to prepare move insert: {}", e))?;
    for m in moves {
        let flags = serde_json::to_string(&m.quality_flags).unwrap_or_default();
        stmt.execute(params![
            m.frame_start,
            m.frame_end,
            m.round_index,
            m.fm_startup_p1,
            m.fm_total_p1,
            m.fm_advantage_p1,
            m.fm_startup_p2,
            m.fm_total_p2,
            m.fm_advantage_p2,
            m.damage_p1,
            m.combo_damage_p1,
            m.damage_p2,
            m.combo_damage_p2,
            m.attack_property_p1,
            m.attack_property_p2,
            m.attack_property_locale,
            m.conf,
            flags
        ])
        .map_err(|e| format!("Failed to insert move event: {}", e))?;
    }
    drop(stmt);

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
        let telemetry = r#"{"frame_index":0,"hp_p1":1.0,"hp_p2":1.0,"drive_p1":6.0,"drive_p2":6.0,"sa_p1":3,"sa_p2":3}"#;
        let inputs = r#"{"frame_start":0,"frame_end":5,"player":"P1","round_index":1,"event_index":0,"directions":["6"],"buttons":[]}"#;
        let moves = r#"{"frame_start":0,"frame_end":10,"round_index":1,"fm_startup_p1":5,"fm_total_p1":15,"fm_advantage_p1":0,"fm_startup_p2":5,"fm_total_p2":15,"fm_advantage_p2":0}"#;
        let result =
            run_pipeline(telemetry, inputs, moves, Path::new("/tmp/test_pipeline.db")).unwrap();
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }
}

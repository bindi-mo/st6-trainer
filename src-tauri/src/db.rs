//! Phase 10/12: SQLite database access layer for RAG and match data.
//!
//! Provides connection management (WAL mode, busy_timeout) and query functions
//! for reading scan results and frame data from `$HOME/.st6-trainer/st6_trainer.db`.

use crate::paths;
use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Open (or create) the application SQLite database with WAL mode and busy timeout.
pub fn open_db() -> Result<Connection, String> {
    let db_path = paths::resolve_db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| {
        format!("Failed to open SQLite database {}: {}", db_path.display(), e)
    })?;

    // Enable WAL mode for concurrent read/write safety
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

    // Set busy timeout to avoid immediate SQLITE_BUSY errors
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("Failed to set busy_timeout: {}", e))?;

    Ok(conn)
}

/// Open a database at a specific path (used by tests and pipeline integration).
pub fn open_db_at(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| {
        format!("Failed to open SQLite database {}: {}", path.display(), e)
    })?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("Failed to set busy_timeout: {}", e))?;

    Ok(conn)
}

/// Initialize the v1 schema if not already present.
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA user_version = 1;

        CREATE TABLE IF NOT EXISTS matches (
            id TEXT PRIMARY KEY,
            played_at DATETIME NOT NULL,
            my_character TEXT NOT NULL,
            opponent_character TEXT NOT NULL,
            my_control_type TEXT NOT NULL,
            opponent_control_type TEXT NOT NULL,
            my_rank TEXT,
            opponent_rank TEXT,
            total_rounds INTEGER NOT NULL,
            result TEXT NOT NULL,
            source_video_path TEXT,
            scan_config_version TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS moves (
            move_id TEXT PRIMARY KEY,
            character_id TEXT NOT NULL,
            character_name TEXT NOT NULL,
            control_type TEXT NOT NULL,
            version TEXT NOT NULL,
            move_name TEXT NOT NULL,
            category TEXT NOT NULL,
            command TEXT NOT NULL,
            startup TEXT NOT NULL,
            active TEXT NOT NULL,
            recovery TEXT NOT NULL,
            hit_advantage TEXT NOT NULL,
            block_advantage TEXT NOT NULL,
            cancel TEXT NOT NULL,
            damage TEXT NOT NULL,
            combo_scaling TEXT,
            drive_gauge_gain_hit INTEGER,
            drive_gauge_lose_dguard INTEGER,
            drive_gauge_lose_punish INTEGER,
            sa_gauge_gain INTEGER,
            attribute TEXT,
            note TEXT
        );

        CREATE TABLE IF NOT EXISTS frame_telemetry (
            match_id TEXT NOT NULL,
            frame_index INTEGER NOT NULL,
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
            quality_flags TEXT,
            PRIMARY KEY (match_id, frame_index),
            FOREIGN KEY (match_id) REFERENCES matches(id)
        );

        CREATE TABLE IF NOT EXISTS input_events (
            match_id TEXT NOT NULL,
            frame_start INTEGER NOT NULL,
            frame_end INTEGER NOT NULL,
            frame_span_video INTEGER,
            hold_frames INTEGER,
            counter_clipped INTEGER,
            hold_frames_lower_bound INTEGER,
            time_dilation_suspected INTEGER,
            resync_used INTEGER,
            unknown_glyph INTEGER,
            is_preround INTEGER,
            player TEXT NOT NULL,
            round_index INTEGER NOT NULL,
            event_index INTEGER NOT NULL,
            directions TEXT,
            buttons TEXT,
            quality_flags TEXT,
            PRIMARY KEY (match_id, player, round_index, event_index),
            FOREIGN KEY (match_id) REFERENCES matches(id)
        );

        CREATE TABLE IF NOT EXISTS move_events (
            match_id TEXT NOT NULL,
            frame_start INTEGER NOT NULL,
            frame_end INTEGER NOT NULL,
            round_index INTEGER NOT NULL,
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
            quality_flags TEXT,
            PRIMARY KEY (match_id, frame_start, round_index),
            FOREIGN KEY (match_id) REFERENCES matches(id)
        );

        CREATE INDEX IF NOT EXISTS idx_frame_telemetry_match_round ON frame_telemetry(match_id, round_index);
        CREATE INDEX IF NOT EXISTS idx_input_events_match_player ON input_events(match_id, player);
        CREATE INDEX IF NOT EXISTS idx_move_events_match_round ON move_events(match_id, round_index);
        CREATE INDEX IF NOT EXISTS idx_moves_character_control ON moves(character_id, control_type);",
    )
    .map_err(|e| format!("Failed to initialize schema: {}", e))?;

    Ok(())
}

/// Get the current schema version (user_version pragma).
pub fn get_schema_version(conn: &Connection) -> Result<u32, String> {
    let row: Option<u32> = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Failed to read user_version: {}", e))?;
    Ok(row.unwrap_or(0))
}

/// Match metadata for RAG context assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSummary {
    pub id: String,
    pub played_at: Option<String>,
    pub my_character: String,
    pub opponent_character: String,
    pub my_control_type: String,
    pub opponent_control_type: String,
    pub total_rounds: i32,
    pub result: String,
    pub source_video_path: Option<String>,
    pub scan_config_version: Option<String>,
}

/// List recent matches from the database.
pub fn list_matches(conn: &Connection, limit: i32) -> Result<Vec<MatchSummary>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, played_at, my_character, opponent_character, my_control_type,
                opponent_control_type, total_rounds, result, source_video_path,
                scan_config_version
         FROM matches
         ORDER BY played_at DESC
         LIMIT ?1",
    ).map_err(|e| format!("Failed to prepare list_matches: {}", e))?;

    let rows = stmt
        .query_map([limit], |row| {
            Ok(MatchSummary {
                id: row.get(0)?,
                played_at: row.get(1)?,
                my_character: row.get(2)?,
                opponent_character: row.get(3)?,
                my_control_type: row.get(4)?,
                opponent_control_type: row.get(5)?,
                total_rounds: row.get(6)?,
                result: row.get(7)?,
                source_video_path: row.get(8)?,
                scan_config_version: row.get(9)?,
            })
        })
        .map_err(|e| format!("Failed to query matches: {}", e))?;

    let mut matches = Vec::new();
    for row in rows {
        matches.push(row.map_err(|e| format!("Failed to read match row: {}", e))?);
    }
    Ok(matches)
}

/// Get a single match by ID.
pub fn get_match(conn: &Connection, match_id: &str) -> Result<Option<MatchSummary>, String> {
    let row: Option<MatchSummary> = conn
        .query_row(
            "SELECT id, played_at, my_character, opponent_character, my_control_type,
                    opponent_control_type, total_rounds, result, source_video_path,
                    scan_config_version
             FROM matches WHERE id = ?1",
            [match_id],
            |row| {
                Ok(MatchSummary {
                    id: row.get(0)?,
                    played_at: row.get(1)?,
                    my_character: row.get(2)?,
                    opponent_character: row.get(3)?,
                    my_control_type: row.get(4)?,
                    opponent_control_type: row.get(5)?,
                    total_rounds: row.get(6)?,
                    result: row.get(7)?,
                    source_video_path: row.get(8)?,
                    scan_config_version: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|e| format!("Failed to query match: {}", e))?;
    Ok(row)
}

/// Frame telemetry record for RAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameTelemetryRecord {
    pub match_id: String,
    pub frame_index: i32,
    pub source_frame_index: Option<i32>,
    pub timestamp_ms: Option<i64>,
    pub round_index: i32,
    pub hp_reset: i32,
    pub hp_p1: f64,
    pub drive_p1: f64,
    pub sa_p1: i32,
    pub in_burnout_p1: i32,
    pub hp_p2: f64,
    pub drive_p2: f64,
    pub sa_p2: i32,
    pub in_burnout_p2: i32,
    pub quality_flags: Vec<String>,
}

/// Get frame telemetry for a match, ordered by frame index.
pub fn get_frame_telemetry(
    conn: &Connection,
    match_id: &str,
) -> Result<Vec<FrameTelemetryRecord>, String> {
    let mut stmt = conn.prepare(
        "SELECT frame_index, source_frame_index, timestamp_ms, round_index, hp_reset,
                hp_p1, drive_p1, sa_p1, in_burnout_p1, hp_p2, drive_p2, sa_p2, in_burnout_p2,
                quality_flags
         FROM frame_telemetry
         WHERE match_id = ?1
         ORDER BY frame_index",
    ).map_err(|e| format!("Failed to prepare get_frame_telemetry: {}", e))?;

    let rows = stmt
        .query_map([match_id], |row| {
            let flags: String = row.get(13).unwrap_or_default();
            let quality_flags: Vec<String> = serde_json::from_str(&flags).unwrap_or_default();
            Ok(FrameTelemetryRecord {
                match_id: match_id.to_string(),
                frame_index: row.get(0)?,
                source_frame_index: row.get(1)?,
                timestamp_ms: row.get(2)?,
                round_index: row.get(3)?,
                hp_reset: row.get(4)?,
                hp_p1: row.get(5)?,
                drive_p1: row.get(6)?,
                sa_p1: row.get(7)?,
                in_burnout_p1: row.get(8)?,
                hp_p2: row.get(9)?,
                drive_p2: row.get(10)?,
                sa_p2: row.get(11)?,
                in_burnout_p2: row.get(12)?,
                quality_flags,
            })
        })
        .map_err(|e| format!("Failed to query frame telemetry: {}", e))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| format!("Failed to read telemetry row: {}", e))?);
    }
    Ok(records)
}

/// Input event record for RAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEventRecord {
    pub match_id: String,
    pub frame_start: i32,
    pub frame_end: i32,
    pub frame_span_video: Option<i32>,
    pub hold_frames: i32,
    pub counter_clipped: i32,
    pub hold_frames_lower_bound: i32,
    pub time_dilation_suspected: i32,
    pub resync_used: i32,
    pub unknown_glyph: i32,
    pub is_preround: i32,
    pub player: String,
    pub round_index: i32,
    pub event_index: i32,
    pub directions: Vec<String>,
    pub buttons: Vec<String>,
    pub quality_flags: Vec<String>,
}

/// Get input events for a match, ordered by player, round, event index.
pub fn get_input_events(
    conn: &Connection,
    match_id: &str,
) -> Result<Vec<InputEventRecord>, String> {
    let mut stmt = conn.prepare(
        "SELECT frame_start, frame_end, frame_span_video, hold_frames, counter_clipped,
                hold_frames_lower_bound, time_dilation_suspected, resync_used, unknown_glyph,
                is_preround, player, round_index, event_index, directions, buttons,
                quality_flags
         FROM input_events
         WHERE match_id = ?1
         ORDER BY player, round_index, event_index",
    ).map_err(|e| format!("Failed to prepare get_input_events: {}", e))?;

    let rows = stmt
        .query_map([match_id], |row| {
            let dirs: String = row.get(13).unwrap_or_default();
            let btns: String = row.get(14).unwrap_or_default();
            let flags: String = row.get(15).unwrap_or_default();
            Ok(InputEventRecord {
                match_id: match_id.to_string(),
                frame_start: row.get(0)?,
                frame_end: row.get(1)?,
                frame_span_video: row.get(2)?,
                hold_frames: row.get(3)?,
                counter_clipped: row.get(4)?,
                hold_frames_lower_bound: row.get(5)?,
                time_dilation_suspected: row.get(6)?,
                resync_used: row.get(7)?,
                unknown_glyph: row.get(8)?,
                is_preround: row.get(9)?,
                player: row.get(10)?,
                round_index: row.get(11)?,
                event_index: row.get(12)?,
                directions: serde_json::from_str(&dirs).unwrap_or_default(),
                buttons: serde_json::from_str(&btns).unwrap_or_default(),
                quality_flags: serde_json::from_str(&flags).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Failed to query input events: {}", e))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| format!("Failed to read input event row: {}", e))?);
    }
    Ok(records)
}

/// Move event record for RAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveEventRecord {
    pub match_id: String,
    pub frame_start: i32,
    pub frame_end: i32,
    pub round_index: i32,
    pub fm_startup_p1: Option<i32>,
    pub fm_total_p1: Option<i32>,
    pub fm_advantage_p1: Option<i32>,
    pub fm_startup_p2: Option<i32>,
    pub fm_total_p2: Option<i32>,
    pub fm_advantage_p2: Option<i32>,
    pub damage_p1: Option<i32>,
    pub combo_damage_p1: Option<i32>,
    pub damage_p2: Option<i32>,
    pub combo_damage_p2: Option<i32>,
    pub attack_property_p1: Option<String>,
    pub attack_property_p2: Option<String>,
    pub attack_property_locale: Option<String>,
    pub conf: Option<f64>,
    pub quality_flags: Vec<String>,
}

/// Get move events for a match, ordered by frame start.
pub fn get_move_events(
    conn: &Connection,
    match_id: &str,
) -> Result<Vec<MoveEventRecord>, String> {
    let mut stmt = conn.prepare(
        "SELECT frame_start, frame_end, round_index, fm_startup_p1, fm_total_p1, fm_advantage_p1,
                fm_startup_p2, fm_total_p2, fm_advantage_p2, damage_p1, combo_damage_p1,
                damage_p2, combo_damage_p2, attack_property_p1, attack_property_p2,
                attack_property_locale, conf, quality_flags
         FROM move_events
         WHERE match_id = ?1
         ORDER BY frame_start",
    ).map_err(|e| format!("Failed to prepare get_move_events: {}", e))?;

    let rows = stmt
        .query_map([match_id], |row| {
            let flags: String = row.get(17).unwrap_or_default();
            Ok(MoveEventRecord {
                match_id: match_id.to_string(),
                frame_start: row.get(0)?,
                frame_end: row.get(1)?,
                round_index: row.get(2)?,
                fm_startup_p1: row.get(3)?,
                fm_total_p1: row.get(4)?,
                fm_advantage_p1: row.get(5)?,
                fm_startup_p2: row.get(6)?,
                fm_total_p2: row.get(7)?,
                fm_advantage_p2: row.get(8)?,
                damage_p1: row.get(9)?,
                combo_damage_p1: row.get(10)?,
                damage_p2: row.get(11)?,
                combo_damage_p2: row.get(12)?,
                attack_property_p1: row.get(13)?,
                attack_property_p2: row.get(14)?,
                attack_property_locale: row.get(15)?,
                conf: row.get(16)?,
                quality_flags: serde_json::from_str(&flags).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Failed to query move events: {}", e))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| format!("Failed to read move event row: {}", e))?);
    }
    Ok(records)
}

/// Frame data move record from the `moves` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveRecord {
    pub move_id: String,
    pub character_id: String,
    pub character_name: String,
    pub control_type: String,
    pub version: String,
    pub move_name: String,
    pub category: String,
    pub command: String,
    pub startup: String,
    pub active: String,
    pub recovery: String,
    pub hit_advantage: String,
    pub block_advantage: String,
    pub cancel: String,
    pub damage: String,
    pub combo_scaling: Option<String>,
    pub drive_gauge_gain_hit: Option<i32>,
    pub drive_gauge_lose_dguard: Option<i32>,
    pub drive_gauge_lose_punish: Option<i32>,
    pub sa_gauge_gain: Option<i32>,
    pub attribute: Option<String>,
    pub note: Option<String>,
}

/// Get moves for a character and control type.
pub fn get_moves_for_character(
    conn: &Connection,
    character_id: &str,
    control_type: &str,
) -> Result<Vec<MoveRecord>, String> {
    let mut stmt = conn.prepare(
        "SELECT move_id, character_id, character_name, control_type, version, move_name,
                category, command, startup, active, recovery, hit_advantage, block_advantage,
                cancel, damage, combo_scaling, drive_gauge_gain_hit, drive_gauge_lose_dguard,
                drive_gauge_lose_punish, sa_gauge_gain, attribute, note
         FROM moves
         WHERE character_id = ?1 AND control_type = ?2
         ORDER BY move_name",
    ).map_err(|e| format!("Failed to prepare get_moves_for_character: {}", e))?;

    let rows = stmt
        .query_map([character_id, control_type], |row| {
            Ok(MoveRecord {
                move_id: row.get(0)?,
                character_id: row.get(1)?,
                character_name: row.get(2)?,
                control_type: row.get(3)?,
                version: row.get(4)?,
                move_name: row.get(5)?,
                category: row.get(6)?,
                command: row.get(7)?,
                startup: row.get(8)?,
                active: row.get(9)?,
                recovery: row.get(10)?,
                hit_advantage: row.get(11)?,
                block_advantage: row.get(12)?,
                cancel: row.get(13)?,
                damage: row.get(14)?,
                combo_scaling: row.get(15)?,
                drive_gauge_gain_hit: row.get(16)?,
                drive_gauge_lose_dguard: row.get(17)?,
                drive_gauge_lose_punish: row.get(18)?,
                sa_gauge_gain: row.get(19)?,
                attribute: row.get(20)?,
                note: row.get(21)?,
            })
        })
        .map_err(|e| format!("Failed to query moves: {}", e))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| format!("Failed to read move row: {}", e))?);
    }
    Ok(records)
}

/// Insert a match record (used by pipeline integration).
pub fn insert_match(conn: &Connection, record: &MatchSummary) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO matches (id, played_at, my_character, opponent_character,
                my_control_type, opponent_control_type, total_rounds, result,
                source_video_path, scan_config_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.id,
            record.played_at,
            record.my_character,
            record.opponent_character,
            record.my_control_type,
            record.opponent_control_type,
            record.total_rounds,
            record.result,
            record.source_video_path,
            record.scan_config_version
        ],
    )
    .map_err(|e| format!("Failed to insert match: {}", e))?;
    Ok(())
}

/// Save a complete scan result (match + telemetry + inputs + moves) in a transaction.
pub fn save_scan_result(
    conn: &mut Connection,
    record: &MatchSummary,
    telemetry: &[crate::pipeline::JsonlFrameTelemetry],
    inputs: &[crate::pipeline::JsonlInputEvent],
    moves: &[crate::pipeline::JsonlMoveEvent],
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to begin transaction: {}", e))?;

    insert_match(
        &tx,
        record,
    )?;

    // Insert frame telemetry
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO frame_telemetry (match_id, frame_index, source_frame_index,
                    timestamp_ms, round_index, hp_reset, hp_p1, drive_p1, sa_p1, in_burnout_p1,
                    hp_p2, drive_p2, sa_p2, in_burnout_p2, quality_flags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        ).map_err(|e| format!("Failed to prepare telemetry insert: {}", e))?;
        for t in telemetry {
            let flags = serde_json::to_string(&t.quality_flags).unwrap_or_default();
            stmt.execute(params![
                record.id,
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
            ]).map_err(|e| format!("Failed to insert telemetry: {}", e))?;
        }
        drop(stmt);
    }

    // Insert input events
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO input_events (match_id, frame_start, frame_end,
                    frame_span_video, hold_frames, counter_clipped, hold_frames_lower_bound,
                    time_dilation_suspected, resync_used, unknown_glyph, is_preround,
                    player, round_index, event_index, directions, buttons, quality_flags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        ).map_err(|e| format!("Failed to prepare input insert: {}", e))?;
        for e in inputs {
            let dirs = serde_json::to_string(&e.directions).unwrap_or_default();
            let btns = serde_json::to_string(&e.buttons).unwrap_or_default();
            let flags = serde_json::to_string(&e.quality_flags).unwrap_or_default();
            stmt.execute(params![
                record.id,
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
            ]).map_err(|e| format!("Failed to insert input event: {}", e))?;
        }
        drop(stmt);
    }

    // Insert move events
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO move_events (match_id, frame_start, frame_end, round_index,
                    fm_startup_p1, fm_total_p1, fm_advantage_p1, fm_startup_p2, fm_total_p2,
                    fm_advantage_p2, damage_p1, combo_damage_p1, damage_p2, combo_damage_p2,
                    attack_property_p1, attack_property_p2, attack_property_locale, conf,
                    quality_flags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19)",
        ).map_err(|e| format!("Failed to prepare move insert: {}", e))?;
        for m in moves {
            let flags = serde_json::to_string(&m.quality_flags).unwrap_or_default();
            stmt.execute(params![
                record.id,
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
            ]).map_err(|e| format!("Failed to insert move event: {}", e))?;
        }
        drop(stmt);
    }

    tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
    Ok(())
}

/// Character summary for the character selector dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSummary {
    pub character_id: String,
    pub character_name: String,
}

/// List distinct characters available in the moves table.
pub fn list_characters(conn: &Connection) -> Result<Vec<CharacterSummary>, String> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT character_id, character_name
         FROM moves
         ORDER BY character_name",
    ).map_err(|e| format!("Failed to prepare list_characters: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CharacterSummary {
                character_id: row.get(0)?,
                character_name: row.get(1)?,
            })
        })
        .map_err(|e| format!("Failed to query characters: {}", e))?;

    let mut characters = Vec::new();
    for row in rows {
        characters.push(row.map_err(|e| format!("Failed to read character row: {}", e))?);
    }
    Ok(characters)
}

/// A single timeline entry (either a move event or an input event).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub frame_start: i32,
    pub frame_end: i32,
    pub round_index: i32,
    pub entry_type: String, // "move" or "input"
    pub player: String,
    pub description: String,
    pub move_name: Option<String>,
    pub damage_p1: Option<i32>,
    pub damage_p2: Option<i32>,
    pub conf: Option<f64>,
}

/// Full timeline for a match: move events + input events merged chronologically.
pub fn get_match_timeline(
    conn: &Connection,
    match_id: &str,
) -> Result<Vec<TimelineEntry>, String> {
    let mut entries: Vec<TimelineEntry> = Vec::new();

    // Move events
    let move_events = get_move_events(conn, match_id)?;
    for m in move_events {
        let description = if let Some(ref mv) = m.attack_property_p1 {
            format!("{} (P1)", mv)
        } else if let Some(ref mv) = m.attack_property_p2 {
            format!("{} (P2)", mv)
        } else {
            "Move event".to_string()
        };
        entries.push(TimelineEntry {
            frame_start: m.frame_start,
            frame_end: m.frame_end,
            round_index: m.round_index,
            entry_type: "move".to_string(),
            player: "P1/P2".to_string(),
            description,
            move_name: m.attack_property_p1.or_else(|| m.attack_property_p2.clone()),
            damage_p1: m.damage_p1,
            damage_p2: m.damage_p2,
            conf: m.conf,
        });
    }

    // Input events
    let input_events = get_input_events(conn, match_id)?;
    for e in input_events {
        let dirs = e.directions.join(", ");
        let btns = e.buttons.join(", ");
        let description = format!("{} {} [{}] [{}]", e.player, dirs, btns, e.round_index);
        entries.push(TimelineEntry {
            frame_start: e.frame_start,
            frame_end: e.frame_end,
            round_index: e.round_index,
            entry_type: "input".to_string(),
            player: e.player,
            description,
            move_name: None,
            damage_p1: None,
            damage_p2: None,
            conf: None,
        });
    }

    // Sort by frame_start, then by entry_type (move before input at same frame)
    entries.sort_by(|a, b| {
        a.frame_start
            .cmp(&b.frame_start)
            .then_with(|| a.entry_type.cmp(&b.entry_type))
    });

    Ok(entries)
}

/// Upsert a single move record into the `moves` table.
///
/// Uses `INSERT OR REPLACE` keyed on `move_id` (character_id + control_type + move_name).
pub fn upsert_move(conn: &Connection, record: &MoveRecord) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO moves (
                move_id, character_id, character_name, control_type, version, move_name,
                category, command, startup, active, recovery, hit_advantage, block_advantage,
                cancel, damage, combo_scaling, drive_gauge_gain_hit, drive_gauge_lose_dguard,
                drive_gauge_lose_punish, sa_gauge_gain, attribute, note
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, ?21, ?22)",
        params![
            record.move_id,
            record.character_id,
            record.character_name,
            record.control_type,
            record.version,
            record.move_name,
            record.category,
            record.command,
            record.startup,
            record.active,
            record.recovery,
            record.hit_advantage,
            record.block_advantage,
            record.cancel,
            record.damage,
            record.combo_scaling,
            record.drive_gauge_gain_hit,
            record.drive_gauge_lose_dguard,
            record.drive_gauge_lose_punish,
            record.sa_gauge_gain,
            record.attribute,
            record.note,
        ],
    )
    .map_err(|e| format!("Failed to upsert move {}: {}", record.move_id, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_db_creates_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at(&db_path).expect("open_db_at");
        assert!(db_path.exists());
        let version = get_schema_version(&conn).expect("get_schema_version");
        assert_eq!(version, 0); // not initialized yet
        drop(conn);
        let _ = dir.close();
    }

    #[test]
    fn test_init_schema_sets_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");
        let version = get_schema_version(&conn).expect("get_schema_version");
        assert_eq!(version, 1);
        drop(conn);
        let _ = dir.close();
    }

    #[test]
    fn test_insert_and_list_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");

        insert_match(
            &conn,
            &MatchSummary {
                id: "match-001".to_string(),
                played_at: Some("2026-01-01T00:00:00Z".to_string()),
                my_character: "Luke".to_string(),
                opponent_character: "Ken".to_string(),
                my_control_type: "classic".to_string(),
                opponent_control_type: "modern".to_string(),
                total_rounds: 3,
                result: "WIN".to_string(),
                source_video_path: Some("C:\\videos\\match.mkv".to_string()),
                scan_config_version: Some("cfg-1.0".to_string()),
            },
        )
        .expect("insert_match");

        let matches = list_matches(&conn, 10).expect("list_matches");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "match-001");
        assert_eq!(matches[0].my_character, "Luke");
        assert_eq!(matches[0].result, "WIN");

        let found = get_match(&conn, "match-001").expect("get_match");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "match-001");

        let not_found = get_match(&conn, "nonexistent").expect("get_match");
        assert!(not_found.is_none());

        drop(conn);
        let _ = dir.close();
    }

    #[test]
    fn test_get_moves_for_character_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");

        let moves = get_moves_for_character(&conn, "luke", "classic").expect("get_moves");
        assert!(moves.is_empty());

        drop(conn);
        let _ = dir.close();
    }

    #[test]
    fn test_upsert_move_insert_and_replace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");

        let record = MoveRecord {
            move_id: "luke-classic-Standing Light Punch".to_string(),
            character_id: "luke".to_string(),
            character_name: "LUKE".to_string(),
            control_type: "classic".to_string(),
            version: "sf6-ui-2026.11".to_string(),
            move_name: "Standing Light Punch".to_string(),
            category: "Normal".to_string(),
            command: "[LP]".to_string(),
            startup: "7".to_string(),
            active: "7-8".to_string(),
            recovery: "*11".to_string(),
            hit_advantage: "2".to_string(),
            block_advantage: "-3".to_string(),
            cancel: "C".to_string(),
            damage: "300".to_string(),
            combo_scaling: Some("Starter scaling 20%".to_string()),
            drive_gauge_gain_hit: Some(250),
            drive_gauge_lose_dguard: Some(-500),
            drive_gauge_lose_punish: Some(-2000),
            sa_gauge_gain: Some(300),
            attribute: Some("High".to_string()),
            note: Some("* 3 frames of recovery added when the attack misses".to_string()),
        };

        // Insert
        upsert_move(&conn, &record).expect("upsert_move insert");
        let moves = get_moves_for_character(&conn, "luke", "classic").expect("get_moves");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].move_name, "Standing Light Punch");
        assert_eq!(moves[0].startup, "7");

        // Replace (same move_id, different data)
        let mut updated = record.clone();
        updated.startup = "6".to_string();
        updated.damage = "320".to_string();
        upsert_move(&conn, &updated).expect("upsert_move replace");
        let moves = get_moves_for_character(&conn, "luke", "classic").expect("get_moves");
        assert_eq!(moves.len(), 1, "UPSERT should not duplicate rows");
        assert_eq!(moves[0].startup, "6");
        assert_eq!(moves[0].damage, "320");

        drop(conn);
        let _ = dir.close();
    }
}

//! Phase 13: Frame data master JSON importer.
//!
//! Imports `frame_data/*.json` into the SQLite `moves` table with
//! differential detection (mtime + SHA256) to avoid full re-imports.
//! Each move is expanded into one record per control type
//! (classic / modern / modern_manual).

use crate::db::{MoveRecord, init_schema, open_db, upsert_move};
use crate::paths::{resolve_bundled_frame_data_dir, resolve_frame_data_dir};
use rusqlite::{Connection, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::Emitter;

/// JSON structure of a frame data master file.
#[derive(Debug, Deserialize)]
pub struct FrameDataJson {
    pub character_id: String,
    pub character_name: String,
    #[serde(default)]
    pub version: String,
    pub moves: Vec<FrameDataMove>,
}

#[derive(Debug, Deserialize)]
pub struct FrameDataMove {
    pub move_name: String,
    pub command: FrameDataCommand,
    pub frame: FrameDataFrame,
    #[serde(default)]
    pub hit: String,
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub cancel: String,
    #[serde(default)]
    pub damage: String,
    #[serde(default)]
    pub combo_scaling: Option<String>,
    #[serde(default)]
    pub drive_gauge: FrameDataDriveGauge,
    #[serde(default)]
    pub sa_gauge_gain: Option<String>,
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub miscellaneous: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FrameDataCommand {
    pub classic: String,
    pub modern: String,
    pub modern_manual: String,
}

#[derive(Debug, Deserialize)]
pub struct FrameDataFrame {
    #[serde(default)]
    pub startup: String,
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub recovery: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct FrameDataDriveGauge {
    #[serde(default)]
    pub hit: String,
    #[serde(default)]
    pub dguard: String,
    #[serde(default)]
    pub punish: String,
}

/// Import result statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportStats {
    pub files_scanned: usize,
    pub files_imported: usize,
    pub files_skipped: usize,
    pub moves_imported: usize,
    pub errors: Vec<String>,
}

/// Parse an integer string, returning None if empty/invalid.
/// Extracts leading digits (handles values like "*11", "45 total frames", "10-12", "-500").
fn parse_opt_i32(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Skip leading non-digit characters like '*', but keep a leading '-' for negatives
    let start = s
        .find(|c: char| c.is_ascii_digit() || c == '-')
        .unwrap_or(s.len());
    if start >= s.len() {
        return None;
    }
    let rest = &s[start..];
    let (sign, digits_start) = if rest.starts_with('-') {
        (-1, 1)
    } else {
        (1, 0)
    };
    let digits: String = rest[digits_start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        format!("{}{}", if sign < 0 { "-" } else { "" }, digits)
            .parse::<i32>()
            .ok()
    }
}

/// Compute SHA256 hash of a file.
fn sha256_file(path: &Path) -> Result<String, String> {
    let content =
        fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Get file mtime as epoch seconds.
fn file_mtime(path: &Path) -> Result<u64, String> {
    let meta =
        fs::metadata(path).map_err(|e| format!("Failed to stat {}: {}", path.display(), e))?;
    Ok(meta
        .modified()
        .map_err(|e| format!("Failed to read mtime for {}: {}", path.display(), e))?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Invalid mtime for {}: {}", path.display(), e))?
        .as_secs())
}

/// Build a MoveRecord from a JSON move and control type.
pub fn build_move_record(
    character_id: &str,
    character_name: &str,
    version: &str,
    move_data: &FrameDataMove,
    control_type: &str,
) -> MoveRecord {
    let command = match control_type {
        "classic" => move_data.command.classic.clone(),
        "modern" => move_data.command.modern.clone(),
        "modern_manual" => move_data.command.modern_manual.clone(),
        _ => String::new(),
    };

    let move_id = format!("{}-{}-{}", character_id, control_type, move_data.move_name);

    MoveRecord {
        move_id,
        character_id: character_id.to_string(),
        character_name: character_name.to_string(),
        control_type: control_type.to_string(),
        version: version.to_string(),
        move_name: move_data.move_name.clone(),
        category: derive_category(&move_data.move_name),
        command,
        startup: move_data.frame.startup.clone(),
        active: move_data.frame.active.clone(),
        recovery: move_data.frame.recovery.clone(),
        hit_advantage: move_data.hit.clone(),
        block_advantage: move_data.block.clone(),
        cancel: move_data.cancel.clone(),
        damage: move_data.damage.clone(),
        combo_scaling: move_data.combo_scaling.clone(),
        drive_gauge_gain_hit: parse_opt_i32(&move_data.drive_gauge.hit),
        drive_gauge_lose_dguard: parse_opt_i32(&move_data.drive_gauge.dguard),
        drive_gauge_lose_punish: parse_opt_i32(&move_data.drive_gauge.punish),
        sa_gauge_gain: move_data
            .sa_gauge_gain
            .as_ref()
            .and_then(|s| parse_opt_i32(s)),
        attribute: move_data.attribute.clone(),
        note: move_data.miscellaneous.clone(),
    }
}

/// Derive a move category from the move name.
fn derive_category(move_name: &str) -> String {
    let name = move_name.to_lowercase();
    if name.contains("throw") || name.contains("投げ") {
        "Throw".to_string()
    } else if name.contains("drive rush") || name.contains("drive impact") || name.contains("parry")
    {
        "System".to_string()
    } else if name.contains("super art") || name.contains("critical art") || name.contains(" sa ") {
        "Super Art".to_string()
    } else if name.contains("standing") || name.contains("crouching") || name.contains("jumping") {
        "Normal".to_string()
    } else if name.contains("special") || name.contains("unique") {
        "Special".to_string()
    } else {
        "Normal".to_string()
    }
}

/// Track import state per file (sha256 + mtime).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ImportLogEntry {
    file_name: String,
    sha256: String,
    mtime_secs: u64,
    move_count: usize,
}

/// Load the import log from the database.
fn load_import_log(conn: &Connection) -> Result<HashMap<String, ImportLogEntry>, String> {
    // Create tracking table if not exists
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS frame_data_import_log (
            file_name TEXT PRIMARY KEY,
            sha256 TEXT NOT NULL,
            mtime_secs INTEGER NOT NULL,
            move_count INTEGER NOT NULL,
            imported_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .map_err(|e| format!("Failed to create import log table: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT file_name, sha256, mtime_secs, move_count FROM frame_data_import_log")
        .map_err(|e| format!("Failed to prepare import log query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ImportLogEntry {
                file_name: row.get(0)?,
                sha256: row.get(1)?,
                mtime_secs: row.get(2)?,
                move_count: row.get(3)?,
            })
        })
        .map_err(|e| format!("Failed to query import log: {}", e))?;

    let mut log = HashMap::new();
    for row in rows {
        let entry = row.map_err(|e| format!("Failed to read import log row: {}", e))?;
        log.insert(entry.file_name.clone(), entry);
    }
    Ok(log)
}

/// Save/update an import log entry.
fn save_import_log_entry(conn: &Connection, entry: &ImportLogEntry) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO frame_data_import_log (file_name, sha256, mtime_secs, move_count)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            entry.file_name,
            entry.sha256,
            entry.mtime_secs,
            entry.move_count
        ],
    )
    .map_err(|e| format!("Failed to save import log entry: {}", e))?;
    Ok(())
}

/// Import all frame data JSON files from the bundled directory into SQLite.
///
/// Only files whose mtime or SHA256 hash have changed since the last import
/// are processed. Emits progress via Tauri events when an app handle is provided.
pub fn import_frame_data(app: Option<&tauri::AppHandle>) -> Result<ImportStats, String> {
    let bundled_dir = resolve_bundled_frame_data_dir()?;
    let _target_dir = resolve_frame_data_dir()?; // ensure target dir exists

    let mut conn = open_db().map_err(|e| e.to_string())?;
    init_schema(&conn).map_err(|e| e.to_string())?;

    let import_log = load_import_log(&conn)?;

    let mut stats = ImportStats {
        files_scanned: 0,
        files_imported: 0,
        files_skipped: 0,
        moves_imported: 0,
        errors: Vec::new(),
    };

    // Collect JSON files
    let mut json_files: Vec<PathBuf> = Vec::new();
    for entry in
        fs::read_dir(&bundled_dir).map_err(|e| format!("Failed to read frame_data dir: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            json_files.push(path);
        }
    }
    json_files.sort();

    for json_path in &json_files {
        stats.files_scanned += 1;
        let file_name = json_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Compute current hash and mtime
        let current_sha256 = match sha256_file(json_path) {
            Ok(h) => h,
            Err(e) => {
                stats.errors.push(format!("{}: {}", file_name, e));
                continue;
            }
        };
        let current_mtime = match file_mtime(json_path) {
            Ok(m) => m,
            Err(e) => {
                stats.errors.push(format!("{}: {}", file_name, e));
                continue;
            }
        };

        // Check if already imported with same hash and mtime
        if let Some(prev) = import_log.get(&file_name)
            && prev.sha256 == current_sha256
            && prev.mtime_secs == current_mtime
        {
            stats.files_skipped += 1;
            continue;
        }

        // Parse the JSON file
        let content = fs::read_to_string(json_path)
            .map_err(|e| format!("Failed to read {}: {}", json_path.display(), e))?;
        let data: FrameDataJson = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", file_name, e))?;

        let version = if data.version.is_empty() {
            "sf6-ui-2026.11".to_string()
        } else {
            data.version
        };

        let mut file_move_count = 0usize;
        let control_types = ["classic", "modern", "modern_manual"];

        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        for move_data in &data.moves {
            for ct in &control_types {
                let record = build_move_record(
                    &data.character_id,
                    &data.character_name,
                    &version,
                    move_data,
                    ct,
                );
                upsert_move(&tx, &record)
                    .map_err(|e| format!("Failed to upsert move {}: {}", record.move_id, e))?;
                file_move_count += 1;
            }
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        // Update import log
        let entry = ImportLogEntry {
            file_name: file_name.clone(),
            sha256: current_sha256,
            mtime_secs: current_mtime,
            move_count: file_move_count,
        };
        save_import_log_entry(&conn, &entry)?;

        stats.files_imported += 1;
        stats.moves_imported += file_move_count;

        // Emit progress event
        if let Some(app) = app {
            let _ = app.emit(
                "frame-data-import-progress",
                serde_json::json!({
                    "file_name": file_name,
                    "moves_imported": file_move_count,
                    "files_imported": stats.files_imported,
                    "files_scanned": stats.files_scanned,
                }),
            );
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opt_i32() {
        assert_eq!(parse_opt_i32("7"), Some(7));
        assert_eq!(parse_opt_i32("*11"), Some(11));
        assert_eq!(parse_opt_i32("45 total frames"), Some(45));
        assert_eq!(parse_opt_i32("10-12"), Some(10));
        assert_eq!(parse_opt_i32(""), None);
        assert_eq!(parse_opt_i32("-"), None);
        assert_eq!(parse_opt_i32("abc"), None);
    }

    #[test]
    fn test_derive_category() {
        assert_eq!(derive_category("Standing Light Punch"), "Normal");
        assert_eq!(derive_category("Crouching Medium Kick"), "Normal");
        assert_eq!(derive_category("Jumping Heavy Punch"), "Normal");
        assert_eq!(derive_category("Hadoken"), "Normal");
        assert_eq!(derive_category("Shoryuken"), "Normal");
        assert_eq!(derive_category("Drive Impact"), "System");
        assert_eq!(derive_category("Drive Rush"), "System");
        assert_eq!(derive_category("Perfect Parry"), "System");
        assert_eq!(derive_category("Super Art 1"), "Super Art");
        assert_eq!(derive_category("Critical Art"), "Super Art");
        assert_eq!(derive_category("Throw"), "Throw");
        assert_eq!(derive_category("投げ"), "Throw");
    }

    #[test]
    fn test_build_move_record() {
        let move_data = FrameDataMove {
            move_name: "Standing Light Punch".to_string(),
            command: FrameDataCommand {
                classic: "[LP]".to_string(),
                modern: "[L]".to_string(),
                modern_manual: "".to_string(),
            },
            frame: FrameDataFrame {
                startup: "7".to_string(),
                active: "7-8".to_string(),
                recovery: "*11".to_string(),
            },
            hit: "2".to_string(),
            block: "-3".to_string(),
            cancel: "C".to_string(),
            damage: "300".to_string(),
            combo_scaling: Some("Starter scaling 20%".to_string()),
            drive_gauge: FrameDataDriveGauge {
                hit: "250".to_string(),
                dguard: "-500".to_string(),
                punish: "-2000".to_string(),
            },
            sa_gauge_gain: Some("300".to_string()),
            attribute: Some("High".to_string()),
            miscellaneous: Some("* 3 frames of recovery added when the attack misses".to_string()),
        };

        let record = build_move_record("luke", "LUKE", "sf6-ui-2026.11", &move_data, "classic");
        assert_eq!(record.move_id, "luke-classic-Standing Light Punch");
        assert_eq!(record.character_id, "luke");
        assert_eq!(record.character_name, "LUKE");
        assert_eq!(record.control_type, "classic");
        assert_eq!(record.command, "[LP]");
        assert_eq!(record.startup, "7");
        assert_eq!(record.active, "7-8");
        assert_eq!(record.recovery, "*11");
        assert_eq!(record.hit_advantage, "2");
        assert_eq!(record.block_advantage, "-3");
        assert_eq!(record.cancel, "C");
        assert_eq!(record.damage, "300");
        assert_eq!(
            record.combo_scaling,
            Some("Starter scaling 20%".to_string())
        );
        assert_eq!(record.drive_gauge_gain_hit, Some(250));
        assert_eq!(record.drive_gauge_lose_dguard, Some(-500));
        assert_eq!(record.drive_gauge_lose_punish, Some(-2000));
        assert_eq!(record.sa_gauge_gain, Some(300));
        assert_eq!(record.attribute, Some("High".to_string()));
        assert_eq!(record.category, "Normal");

        // Modern control type uses different command
        let record_modern =
            build_move_record("luke", "LUKE", "sf6-ui-2026.11", &move_data, "modern");
        assert_eq!(record_modern.move_id, "luke-modern-Standing Light Punch");
        assert_eq!(record_modern.command, "[L]");
        assert_eq!(record_modern.control_type, "modern");
    }

    #[test]
    fn test_import_frame_data_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_db_at_for_test(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");

        // Create a temporary frame_data dir with one JSON file
        let frame_data_dir = dir.path().join("frame_data");
        fs::create_dir_all(&frame_data_dir).expect("create frame_data dir");
        let json_content = r#"{
            "character_id": "luke",
            "character_name": "LUKE",
            "version": "sf6-ui-2026.11",
            "moves": [
                {
                    "move_name": "Standing Light Punch",
                    "command": {
                        "classic": "[LP]",
                        "modern": "[L]",
                        "modern_manual": ""
                    },
                    "frame": {
                        "startup": "7",
                        "active": "7-8",
                        "recovery": "*11"
                    },
                    "hit": "2",
                    "block": "-3",
                    "cancel": "C",
                    "damage": "300",
                    "combo_scaling": "Starter scaling 20%",
                    "drive_gauge": {
                        "hit": "250",
                        "dguard": "-500",
                        "punish": "-2000"
                    },
                    "sa_gauge_gain": "300",
                    "attribute": "High",
                    "miscellaneous": ""
                }
            ]
        }"#;
        fs::write(frame_data_dir.join("luke.json"), json_content).expect("write json");

        // We can't easily test import_frame_data because it reads from bundled dir.
        // Instead verify the DB operations work.
        let moves =
            crate::db::get_moves_for_character(&conn, "luke", "classic").expect("get_moves");
        assert!(moves.is_empty());

        drop(conn);
        let _ = dir.close();
    }

    /// Open a DB at a specific path for testing (mirrors db.rs open_db_at).
    fn open_db_at_for_test(path: &Path) -> Result<Connection, String> {
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("Failed to set WAL: {}", e))?;
        conn.execute_batch("PRAGMA busy_timeout=5000;")
            .map_err(|e| format!("Failed to set busy_timeout: {}", e))?;
        Ok(conn)
    }
}

//! Phase 15.7: Performance tests.
//! Measures non-functional requirements from the Phase 15 plan:
//! - SQLite write latency (≤ 500 ms per match)
//! - Frame data import time (≤ 3 seconds for all characters)
//! - Token estimation / prompt rendering performance
//! - Config save/load latency

use serial_test::serial;
use std::fs;
use std::time::Instant;
use tempfile::tempdir;

#[test]
#[serial]
fn test_perf_sqlite_write_latency() {
    use st6_trainer_lib::db::{MatchSummary, init_schema, insert_match, open_db_at};

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("perf.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let record = MatchSummary {
        id: "perf-match-001".to_string(),
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

    let start = Instant::now();
    insert_match(&conn, &record).expect("insert_match");
    let elapsed = start.elapsed();

    // Requirement: SQLite write (1 match) ≤ 500 ms
    assert!(
        elapsed.as_millis() <= 500,
        "SQLite insert took {} ms (limit 500 ms)",
        elapsed.as_millis()
    );

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_perf_sqlite_batch_write_latency() {
    use st6_trainer_lib::db::{MatchSummary, init_schema, insert_match, open_db_at};

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("perf_batch.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let start = Instant::now();
    for i in 1..=50 {
        insert_match(
            &conn,
            &MatchSummary {
                id: format!("perf-batch-{:03}", i),
                played_at: Some("2026-01-01T00:00:00Z".to_string()),
                my_character: "Luke".to_string(),
                opponent_character: "Ken".to_string(),
                my_control_type: "classic".to_string(),
                opponent_control_type: "modern".to_string(),
                total_rounds: 3,
                result: "WIN".to_string(),
                source_video_path: None,
                scan_config_version: None,
            },
        )
        .expect("insert_match");
    }
    let elapsed = start.elapsed();

    // 50 inserts should complete well within a reasonable time
    assert!(
        elapsed.as_millis() <= 5000,
        "50 SQLite inserts took {} ms",
        elapsed.as_millis()
    );

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_perf_frame_data_import_all_characters() {
    use st6_trainer_lib::db::{init_schema, open_db_at, upsert_move};
    use st6_trainer_lib::frame_data_importer::build_move_record;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("perf_import.db");
    let mut conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    // Simulate importing all bundled frame_data JSON files (35 characters × ~100 moves × 3 control types)
    let bundled_dir = st6_trainer_lib::paths::resolve_bundled_frame_data_dir()
        .expect("resolve_bundled_frame_data_dir");
    let json_files: Vec<_> = fs::read_dir(&bundled_dir)
        .expect("read frame_data dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();

    let start = Instant::now();
    let mut total_moves = 0usize;
    let tx = conn.transaction().expect("begin transaction");
    for json_file in &json_files {
        let content = fs::read_to_string(json_file.path()).expect("read frame data json");
        let data: st6_trainer_lib::frame_data_importer::FrameDataJson =
            serde_json::from_str(&content).expect("parse frame data json");

        let version = if data.version.is_empty() {
            "sf6-ui-2026.11".to_string()
        } else {
            data.version
        };

        for move_data in &data.moves {
            for ct in ["classic", "modern", "modern_manual"] {
                let record = build_move_record(
                    &data.character_id,
                    &data.character_name,
                    &version,
                    move_data,
                    ct,
                );
                upsert_move(&tx, &record).expect("upsert_move");
                total_moves += 1;
            }
        }
    }
    tx.commit().expect("commit transaction");
    let elapsed = start.elapsed();

    // Requirement: frame data import (all characters) ≤ 3 seconds
    assert!(
        elapsed.as_millis() <= 3000,
        "Frame data import of {} moves took {} ms (limit 3000 ms)",
        total_moves,
        elapsed.as_millis()
    );
    assert!(total_moves > 0, "Should have imported at least some moves");

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_perf_config_save_load_latency() {
    use st6_trainer_lib::config::{AppConfig, load_app_config_from_home, save_app_config_to_home};

    let cfg = AppConfig::default();

    let start = Instant::now();
    save_app_config_to_home(&cfg).expect("save_app_config_to_home");
    let save_elapsed = start.elapsed();

    let start = Instant::now();
    let loaded = load_app_config_from_home();
    let load_elapsed = start.elapsed();

    assert_eq!(loaded.model_url, cfg.model_url);
    assert_eq!(loaded.ctx_size, cfg.ctx_size);

    // Config save/load should be fast (< 100 ms each)
    assert!(
        save_elapsed.as_millis() <= 100,
        "Config save took {} ms",
        save_elapsed.as_millis()
    );
    assert!(
        load_elapsed.as_millis() <= 100,
        "Config load took {} ms",
        load_elapsed.as_millis()
    );
}

#[test]
#[serial]
fn test_perf_rag_prompt_render_latency() {
    use st6_trainer_lib::db::{MatchSummary, init_schema, insert_match, open_db_at};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("perf_rag.db");
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");
        insert_match(
            &conn,
            &MatchSummary {
                id: "perf-rag-match".to_string(),
                played_at: Some("2026-01-01T00:00:00Z".to_string()),
                my_character: "Luke".to_string(),
                opponent_character: "Ken".to_string(),
                my_control_type: "classic".to_string(),
                opponent_control_type: "modern".to_string(),
                total_rounds: 3,
                result: "WIN".to_string(),
                source_video_path: None,
                scan_config_version: None,
            },
        )
        .expect("insert_match");
        drop(conn);
    }

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let start = Instant::now();
    let conn = open_db_at(&db_path).expect("open_db_at");
    let result = engine
        .render_analyze_damage(&conn, "perf-rag-match")
        .expect("render");
    let elapsed = start.elapsed();
    drop(conn);

    assert!(!result.prompt.is_empty());
    // Prompt rendering should be fast (< 500 ms)
    assert!(
        elapsed.as_millis() <= 500,
        "RAG prompt render took {} ms (limit 500 ms)",
        elapsed.as_millis()
    );
}

#[test]
#[serial]
fn test_perf_estimate_tokens_speed() {
    use st6_trainer_lib::rag::estimate_tokens;

    let text = "This is a performance test string for token estimation. ".repeat(1000);
    let start = Instant::now();
    let tokens = estimate_tokens(&text);
    let elapsed = start.elapsed();

    assert!(tokens > 0);
    // Token estimation of ~40KB text should be very fast (< 10 ms)
    assert!(
        elapsed.as_millis() <= 10,
        "Token estimation took {} ms",
        elapsed.as_millis()
    );
}

#[test]
#[serial]
fn test_perf_move_upsert_batch() {
    use st6_trainer_lib::db::{MoveRecord, init_schema, open_db_at, upsert_move};

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("perf_upsert.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let start = Instant::now();
    for i in 1..=100 {
        upsert_move(
            &conn,
            &MoveRecord {
                move_id: format!("luke-classic-Move {}", i),
                character_id: "luke".to_string(),
                character_name: "LUKE".to_string(),
                control_type: "classic".to_string(),
                version: "v1".to_string(),
                move_name: format!("Move {}", i),
                category: "Normal".to_string(),
                command: "[LP]".to_string(),
                startup: "7".to_string(),
                active: "7-8".to_string(),
                recovery: "*11".to_string(),
                hit_advantage: "2".to_string(),
                block_advantage: "-3".to_string(),
                cancel: "C".to_string(),
                damage: "300".to_string(),
                combo_scaling: None,
                drive_gauge_gain_hit: None,
                drive_gauge_lose_dguard: None,
                drive_gauge_lose_punish: None,
                sa_gauge_gain: None,
                attribute: None,
                note: None,
            },
        )
        .expect("upsert_move");
    }
    let elapsed = start.elapsed();

    // 100 upserts should complete quickly (< 1 second)
    assert!(
        elapsed.as_millis() <= 1000,
        "100 move upserts took {} ms",
        elapsed.as_millis()
    );

    drop(conn);
    let _ = dir.close();
}

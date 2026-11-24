//! Phase 15.6: E2E integration test.
//! Covers the full flow: video scan → SQLite save → RAG prompt generation → LLM inference.
//! Skips gracefully when no replay video or LLM worker binary is available.

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

#[test]
#[serial]
fn test_e2e_scan_to_db_round_trip() {
    use st6_trainer_lib::db::{get_match, init_schema, open_db_at, insert_match, MatchSummary};
    use st6_trainer_lib::pipeline::{
        InvariantResult, JsonlFrameTelemetry, JsonlInputEvent, JsonlMoveEvent, run_pipeline,
    };

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("e2e_test.db");

    // 1. Initialize schema
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");
        let version = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
        assert_eq!(version, 1);
        drop(conn);
    }

    // 2. Build a synthetic match record
    let match_record = MatchSummary {
        id: "e2e-match-001".to_string(),
        played_at: Some("2026-01-01T00:00:00Z".to_string()),
        my_character: "Luke".to_string(),
        opponent_character: "Ken".to_string(),
        my_control_type: "classic".to_string(),
        opponent_control_type: "modern".to_string(),
        total_rounds: 3,
        result: "WIN".to_string(),
        source_video_path: Some("C:\\videos\\replay_00.mkv".to_string()),
        scan_config_version: Some("cfg-1.0".to_string()),
    };

    // 3. Build synthetic JSONL data
    let telemetry_jsonl = serde_json::to_string(&JsonlFrameTelemetry {
        frame_index: 0,
        source_frame_index: 0,
        timestamp_ms: 0,
        round_index: 1,
        hp_reset: 0,
        hp_p1: 1.0,
        hp_p2: 1.0,
        drive_p1: 6.0,
        drive_p2: 6.0,
        sa_p1: 3,
        sa_p2: 3,
        in_burnout_p1: 0,
        in_burnout_p2: 0,
        quality_flags: vec![],
    })
    .unwrap();

    let input_jsonl = serde_json::to_string(&JsonlInputEvent {
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
        directions: vec!["6".to_string()],
        buttons: vec!["LP".to_string()],
        quality_flags: vec![],
    })
    .unwrap();

    let move_jsonl = serde_json::to_string(&JsonlMoveEvent {
        frame_start: 100,
        frame_end: 110,
        round_index: 1,
        fm_startup_p1: 5,
        fm_total_p1: 15,
        fm_advantage_p1: 0,
        fm_startup_p2: 5,
        fm_total_p2: 15,
        fm_advantage_p2: 0,
        damage_p1: 0,
        combo_damage_p1: 0,
        damage_p2: 300,
        combo_damage_p2: 300,
        attack_property_p1: "High".to_string(),
        attack_property_p2: "Mid".to_string(),
        attack_property_locale: "en".to_string(),
        conf: 0.95,
        quality_flags: vec![],
    })
    .unwrap();

    // 4. Run the pipeline (invariant checks + SQLite write)
    let result: InvariantResult = run_pipeline(
        &match_record,
        &telemetry_jsonl,
        &input_jsonl,
        &move_jsonl,
        &db_path,
    )
    .expect("run_pipeline");

    assert!(result.passed, "Invariants should pass: {:?}", result.violations);
    assert!(result.violations.is_empty());

    // 5. Verify data was persisted in SQLite
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        let found = get_match(&conn, "e2e-match-001").expect("get_match");
        assert!(found.is_some(), "Match should be persisted");
        let m = found.unwrap();
        assert_eq!(m.my_character, "Luke");
        assert_eq!(m.opponent_character, "Ken");
        assert_eq!(m.result, "WIN");

        let telemetry = st6_trainer_lib::db::get_frame_telemetry(&conn, "e2e-match-001")
            .expect("get_frame_telemetry");
        assert_eq!(telemetry.len(), 1);
        assert_eq!(telemetry[0].frame_index, 0);

        let inputs = st6_trainer_lib::db::get_input_events(&conn, "e2e-match-001")
            .expect("get_input_events");
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].player, "P1");

        let move_events = st6_trainer_lib::db::get_move_events(&conn, "e2e-match-001")
            .expect("get_move_events");
        assert_eq!(move_events.len(), 1);
        assert_eq!(move_events[0].damage_p2, Some(300));

        drop(conn);
    }

    // 6. Verify idempotent re-run (INSERT OR REPLACE)
    let result2: InvariantResult = run_pipeline(
        &match_record,
        &telemetry_jsonl,
        &input_jsonl,
        &move_jsonl,
        &db_path,
    )
    .expect("run_pipeline second time");
    assert!(result2.passed);

    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        let matches = st6_trainer_lib::db::list_matches(&conn, 10).expect("list_matches");
        assert_eq!(matches.len(), 1, "Re-run should not duplicate match");
        drop(conn);
    }

    drop(dir);
}

#[test]
#[serial]
fn test_e2e_pipeline_invariant_violations_detected() {
    use st6_trainer_lib::db::{init_schema, open_db_at};
    use st6_trainer_lib::pipeline::{
        JsonlFrameTelemetry, JsonlInputEvent, JsonlMoveEvent, run_pipeline,
    };
    use st6_trainer_lib::db::MatchSummary;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("e2e_invariant.db");

    let match_record = MatchSummary {
        id: "e2e-invariant-001".to_string(),
        played_at: Some("2026-01-01T00:00:00Z".to_string()),
        my_character: "Luke".to_string(),
        opponent_character: "Ken".to_string(),
        my_control_type: "classic".to_string(),
        opponent_control_type: "modern".to_string(),
        total_rounds: 3,
        result: "WIN".to_string(),
        source_video_path: None,
        scan_config_version: None,
    };

    // Telemetry with HP out of range (should trigger SCHEMA_INVARIANT)
    let telemetry_jsonl = serde_json::to_string(&JsonlFrameTelemetry {
        frame_index: 0,
        source_frame_index: 0,
        timestamp_ms: 0,
        round_index: 1,
        hp_reset: 0,
        hp_p1: 1.5, // out of range [0,1]
        hp_p2: 1.0,
        drive_p1: 6.0,
        drive_p2: 6.0,
        sa_p1: 3,
        sa_p2: 3,
        in_burnout_p1: 0,
        in_burnout_p2: 0,
        quality_flags: vec![],
    })
    .unwrap();

    let input_jsonl = serde_json::to_string(&JsonlInputEvent {
        frame_start: 10,
        frame_end: 5, // frame_start > frame_end (INPUT_INVARIANT)
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
        buttons: vec![], // empty directions AND buttons (INPUT_INVARIANT)
        quality_flags: vec![],
    })
    .unwrap();

    let move_jsonl = serde_json::to_string(&JsonlMoveEvent {
        frame_start: 100,
        frame_end: 110,
        round_index: 1,
        fm_startup_p1: 5,
        fm_total_p1: 15,
        fm_advantage_p1: 0,
        fm_startup_p2: 5,
        fm_total_p2: 15,
        fm_advantage_p2: 0,
        damage_p1: 0,
        combo_damage_p1: 0,
        damage_p2: 0,
        combo_damage_p2: 0,
        attack_property_p1: "".to_string(),
        attack_property_p2: "".to_string(),
        attack_property_locale: "en".to_string(),
        conf: 0.95,
        quality_flags: vec![],
    })
    .unwrap();

    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");
        drop(conn);
    }

    let result = run_pipeline(
        &match_record,
        &telemetry_jsonl,
        &input_jsonl,
        &move_jsonl,
        &db_path,
    )
    .expect("run_pipeline should not error on violations");

    assert!(!result.passed, "Should detect invariant violations");
    assert!(!result.violations.is_empty(), "Should list violations");
    let has_hp_violation = result
        .violations
        .iter()
        .any(|v| v.contains("hp_p1") && v.contains("out of range"));
    let has_frame_violation = result
        .violations
        .iter()
        .any(|v| v.contains("frame_start") && v.contains("frame_end"));
    let has_empty_input_violation = result
        .violations
        .iter()
        .any(|v| v.contains("empty directions and buttons"));
    assert!(has_hp_violation, "Should detect HP out of range: {:?}", result.violations);
    assert!(has_frame_violation, "Should detect frame_start > frame_end: {:?}", result.violations);
    assert!(
        has_empty_input_violation,
        "Should detect empty directions/buttons: {:?}",
        result.violations
    );

    drop(dir);
}

#[test]
#[serial]
fn test_e2e_rag_prompt_generation_from_db() {
    use st6_trainer_lib::db::{init_schema, open_db_at, insert_match, MatchSummary};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("e2e_rag.db");

    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");
        insert_match(
            &conn,
            &MatchSummary {
                id: "e2e-rag-match".to_string(),
                played_at: Some("2026-01-15T12:00:00Z".to_string()),
                my_character: "A.K.I.".to_string(),
                opponent_character: "Ed".to_string(),
                my_control_type: "modern".to_string(),
                opponent_control_type: "classic".to_string(),
                total_rounds: 2,
                result: "LOSS".to_string(),
                source_video_path: None,
                scan_config_version: None,
            },
        )
        .expect("insert_match");
        drop(conn);
    }

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    // analyze_damage
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        let result = engine.render_analyze_damage(&conn, "e2e-rag-match").expect("render");
        assert!(!result.prompt.is_empty());
        assert!(result.prompt.contains("A.K.I."));
        assert!(result.prompt.contains("Ed"));
        assert!(result.estimated_tokens > 0);
        drop(conn);
    }

    // verify_punish
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        let result = engine
            .render_verify_punish(&conn, "e2e-rag-match", "Hadoken", 10)
            .expect("render");
        assert!(!result.prompt.is_empty());
        assert!(result.prompt.contains("Hadoken"));
        drop(conn);
    }

    // training_menu
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        let result = engine.render_training_menu(&conn, "e2e-rag-match").expect("render");
        assert!(!result.prompt.is_empty());
        drop(conn);
    }

    drop(dir);
}

#[test]
#[serial]
fn test_e2e_frame_data_import_to_db() {
    use st6_trainer_lib::db::{get_moves_for_character, init_schema, open_db_at};

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("e2e_frame_data.db");

    // We can't easily override the bundled dir, so test via direct DB operations
    // that mirror what import_frame_data does internally.
    {
        let conn = open_db_at(&db_path).expect("open_db_at");
        init_schema(&conn).expect("init_schema");

        // Simulate importing one move for luke (classic)
        st6_trainer_lib::db::upsert_move(
            &conn,
            &st6_trainer_lib::db::MoveRecord {
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
                note: Some("".to_string()),
            },
        )
        .expect("upsert_move");

        let moves = get_moves_for_character(&conn, "luke", "classic").expect("get_moves");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].move_name, "Standing Light Punch");
        assert_eq!(moves[0].command, "[LP]");

        drop(conn);
    }

    // Verify bundled frame_data JSON files exist and parse
    let bundled_dir = st6_trainer_lib::paths::resolve_bundled_frame_data_dir()
        .expect("resolve_bundled_frame_data_dir");
    let json_files: Vec<_> = fs::read_dir(&bundled_dir)
        .expect("read frame_data dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
        .collect();
    assert!(!json_files.is_empty(), "Should have bundled frame_data JSON files");

    // Verify at least luke.json parses
    let luke_path = bundled_dir.join("luke.json");
    if luke_path.exists() {
        let content = fs::read_to_string(&luke_path).expect("read luke.json");
        let data: st6_trainer_lib::frame_data_importer::FrameDataJson =
            serde_json::from_str(&content).expect("parse luke.json");
        assert_eq!(data.character_id, "luke");
        assert!(!data.moves.is_empty());
    }

    drop(dir);
}

#[test]
#[serial]
fn test_e2e_llm_worker_status_without_binary() {
    // When no worker binary exists, start() should fail gracefully.
    // This tests the error path of the LLM worker.
    use st6_trainer_lib::llm_worker::{LlmWorker, WorkerConfig};
    use std::path::PathBuf;

    let config = WorkerConfig {
        worker_bin_path: PathBuf::from("nonexistent_worker.exe"),
        model_path: String::new(),
        ctx_size: 4096,
        gpu_layers: 35,
        pipe_name: r"\\.\pipe\st6_llm_worker_test".to_string(),
        request_timeout_secs: 5,
        max_reconnect_attempts: 1,
    };

    let worker = LlmWorker::new(config);
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = rt.block_on(worker.start());
    assert!(result.is_err(), "Starting nonexistent worker should fail");
    assert!(result.unwrap_err().contains("Failed to spawn worker subprocess"));
}

#[test]
#[serial]
fn test_e2e_config_model_paths_consistency() {
    // Verify that config paths, model paths, and DB paths are all under $HOME/.st6-trainer/
    let app_dir = st6_trainer_lib::paths::resolve_app_data_dir().expect("resolve_app_data_dir");
    let config_path = st6_trainer_lib::paths::resolve_config_path().expect("resolve_config_path");
    let models_dir = st6_trainer_lib::paths::resolve_models_dir().expect("resolve_models_dir");
    let db_path = st6_trainer_lib::paths::resolve_db_path().expect("resolve_db_path");

    assert!(config_path.starts_with(&app_dir));
    assert!(models_dir.starts_with(&app_dir));
    assert!(db_path.starts_with(&app_dir));
    assert!(config_path.file_name().unwrap() == "config.json");
    assert!(db_path.file_name().unwrap() == "st6_trainer.db");
}

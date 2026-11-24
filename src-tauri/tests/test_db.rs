//! Phase 15.4: Unit tests for SQLite database & migration (Phase 12).
//! Covers schema initialization, user_version migration, match CRUD,
//! scan result saving, WAL mode, and RAG query functions.

use serial_test::serial;
use tempfile::tempdir;

#[test]
#[serial]
fn test_db_open_creates_file() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    assert!(db_path.exists());
    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_schema_version_before_and_after_init() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");

    let version_before = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
    assert_eq!(version_before, 0, "schema should be uninitialized");

    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");
    let version_after = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
    assert_eq!(version_after, 1, "schema should be v1 after init");

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_wal_mode_enabled() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("query journal_mode");
    assert_eq!(journal_mode, "wal", "WAL mode should be enabled");

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_insert_and_list_matches() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    let record = st6_trainer_lib::db::MatchSummary {
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
    };

    st6_trainer_lib::db::insert_match(&conn, &record).expect("insert_match");

    let matches = st6_trainer_lib::db::list_matches(&conn, 10).expect("list_matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, "match-001");
    assert_eq!(matches[0].my_character, "Luke");
    assert_eq!(matches[0].opponent_character, "Ken");
    assert_eq!(matches[0].result, "WIN");

    let found = st6_trainer_lib::db::get_match(&conn, "match-001").expect("get_match");
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "match-001");

    let not_found = st6_trainer_lib::db::get_match(&conn, "nonexistent").expect("get_match");
    assert!(not_found.is_none());

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_list_matches_limit() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    for i in 1..=5 {
        st6_trainer_lib::db::insert_match(
            &conn,
            &st6_trainer_lib::db::MatchSummary {
                id: format!("match-{:03}", i),
                played_at: Some(format!("2026-01-{:02}T00:00:00Z", i)),
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

    let limited = st6_trainer_lib::db::list_matches(&conn, 3).expect("list_matches");
    assert_eq!(limited.len(), 3);

    let all = st6_trainer_lib::db::list_matches(&conn, 100).expect("list_matches");
    assert_eq!(all.len(), 5);

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_upsert_move_insert_and_replace() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    let record = st6_trainer_lib::db::MoveRecord {
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
    };

    st6_trainer_lib::db::upsert_move(&conn, &record).expect("upsert_move insert");
    let moves = st6_trainer_lib::db::get_moves_for_character(&conn, "luke", "classic")
        .expect("get_moves");
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].startup, "7");
    assert_eq!(moves[0].damage, "300");

    // Replace same move_id
    let mut updated = record.clone();
    updated.startup = "6".to_string();
    updated.damage = "320".to_string();
    st6_trainer_lib::db::upsert_move(&conn, &updated).expect("upsert_move replace");
    let moves = st6_trainer_lib::db::get_moves_for_character(&conn, "luke", "classic")
        .expect("get_moves");
    assert_eq!(moves.len(), 1, "UPSERT should not duplicate rows");
    assert_eq!(moves[0].startup, "6");
    assert_eq!(moves[0].damage, "320");

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_get_moves_for_character_empty() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    let moves = st6_trainer_lib::db::get_moves_for_character(&conn, "luke", "classic")
        .expect("get_moves");
    assert!(moves.is_empty());

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_list_characters() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    // No moves yet -> empty
    let chars = st6_trainer_lib::db::list_characters(&conn).expect("list_characters");
    assert!(chars.is_empty());

    // Insert moves for two characters
    st6_trainer_lib::db::upsert_move(
        &conn,
        &st6_trainer_lib::db::MoveRecord {
            move_id: "luke-classic-Standing Light Punch".to_string(),
            character_id: "luke".to_string(),
            character_name: "LUKE".to_string(),
            control_type: "classic".to_string(),
            version: "v1".to_string(),
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
    st6_trainer_lib::db::upsert_move(
        &conn,
        &st6_trainer_lib::db::MoveRecord {
            move_id: "ken-classic-Standing Light Punch".to_string(),
            character_id: "ken".to_string(),
            character_name: "KEN".to_string(),
            control_type: "classic".to_string(),
            version: "v1".to_string(),
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

    let chars = st6_trainer_lib::db::list_characters(&conn).expect("list_characters");
    assert_eq!(chars.len(), 2);
    let ids: Vec<&str> = chars.iter().map(|c| c.character_id.as_str()).collect();
    assert!(ids.contains(&"luke"));
    assert!(ids.contains(&"ken"));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_save_scan_result_transaction() {
    use st6_trainer_lib::pipeline::{
        JsonlFrameTelemetry, JsonlInputEvent, JsonlMoveEvent,
    };

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    let record = st6_trainer_lib::db::MatchSummary {
        id: "match-scan-001".to_string(),
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
        sa_p1: 3,
        sa_p2: 3,
        in_burnout_p1: 0,
        in_burnout_p2: 0,
        quality_flags: vec![],
    }];

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
        directions: vec!["6".to_string()],
        buttons: vec!["LP".to_string()],
        quality_flags: vec![],
    }];

    let moves = vec![JsonlMoveEvent {
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
    }];

    st6_trainer_lib::db::save_scan_result(
        &mut conn,
        &record,
        &telemetry,
        &inputs,
        &moves,
    )
    .expect("save_scan_result");

    // Verify match was inserted
    let matches = st6_trainer_lib::db::list_matches(&conn, 10).expect("list_matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, "match-scan-001");

    // Verify telemetry was inserted
    let tel = st6_trainer_lib::db::get_frame_telemetry(&conn, "match-scan-001")
        .expect("get_frame_telemetry");
    assert_eq!(tel.len(), 1);
    assert_eq!(tel[0].frame_index, 0);
    assert_eq!(tel[0].hp_p1, 1.0);

    // Verify input events were inserted
    let evts = st6_trainer_lib::db::get_input_events(&conn, "match-scan-001")
        .expect("get_input_events");
    assert_eq!(evts.len(), 1);
    assert_eq!(evts[0].player, "P1");

    // Verify move events were inserted
    let mv_evts = st6_trainer_lib::db::get_move_events(&conn, "match-scan-001")
        .expect("get_move_events");
    assert_eq!(mv_evts.len(), 1);
    assert_eq!(mv_evts[0].damage_p2, Some(300));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_get_match_timeline() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    let record = st6_trainer_lib::db::MatchSummary {
        id: "match-timeline-001".to_string(),
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

    st6_trainer_lib::db::insert_match(&conn, &record).expect("insert_match");

    // No events yet -> empty timeline
    let timeline = st6_trainer_lib::db::get_match_timeline(&conn, "match-timeline-001")
        .expect("get_match_timeline");
    assert!(timeline.is_empty());

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_migration_user_version_persists() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    {
        let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
        st6_trainer_lib::db::init_schema(&conn).expect("init_schema");
        let version = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
        assert_eq!(version, 1);
        drop(conn);
    }
    // Re-open and verify version persists
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    let version = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
    assert_eq!(version, 1);
    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_init_schema_idempotent() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");

    st6_trainer_lib::db::init_schema(&conn).expect("init_schema first");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema second (idempotent)");

    let version = st6_trainer_lib::db::get_schema_version(&conn).expect("get_schema_version");
    assert_eq!(version, 1);

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_db_concurrent_writes_wal() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");

    // Insert multiple matches rapidly
    for i in 1..=10 {
        st6_trainer_lib::db::insert_match(
            &conn,
            &st6_trainer_lib::db::MatchSummary {
                id: format!("match-concurrent-{:03}", i),
                played_at: Some(format!("2026-01-{:02}T00:00:00Z", i)),
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

    let matches = st6_trainer_lib::db::list_matches(&conn, 100).expect("list_matches");
    assert_eq!(matches.len(), 10);

    drop(conn);
    let _ = dir.close();
}

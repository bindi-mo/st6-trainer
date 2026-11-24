//! Phase 15.3: Unit tests for RAG & context assembly (Phase 10).
//! Covers template rendering, token budget management, and prompt generation.

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

#[test]
#[serial]
fn test_rag_estimate_tokens() {
    use st6_trainer_lib::rag::estimate_tokens;

    assert_eq!(estimate_tokens(""), 0);
    assert!(estimate_tokens("Hello world") > 0);
    assert!(estimate_tokens("こんにちは") > 0);
    // Longer text should produce more tokens
    assert!(estimate_tokens("This is a longer sentence with more words.") > estimate_tokens("Hi"));
}

#[test]
#[serial]
fn test_rag_fit_to_budget_under_limit() {
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");
    let short = "Short prompt";
    let result = engine.fit_to_budget(short);
    assert_eq!(result, short);
}

#[test]
#[serial]
fn test_rag_fit_to_budget_over_limit() {
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::with_ctx_size(
        RagEngine::new(&template_dir).expect("RagEngine::new"),
        10,
    );
    let long = "This is a very long prompt that exceeds the token budget significantly. It contains enough text that truncation should produce a shorter result than the original input string because the truncation marker itself is relatively short compared to this long input.";
    let result = engine.fit_to_budget(long);
    assert!(result.len() < long.len());
    assert!(result.contains("切り詰め"));
}

#[test]
#[serial]
fn test_rag_with_ctx_size() {
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");
    assert_eq!(engine.ctx_size(), 4096);

    let custom = RagEngine::new(&template_dir).expect("RagEngine::new").with_ctx_size(2048);
    assert_eq!(custom.ctx_size(), 2048);
}

#[test]
#[serial]
fn test_rag_default_infer_params() {
    use st6_trainer_lib::rag::default_infer_params;

    let params = default_infer_params();
    assert_eq!(params.temperature, Some(0.7));
    assert_eq!(params.top_p, Some(0.9));
    assert_eq!(params.max_tokens, Some(1024));
    assert_eq!(params.stream, Some(true));
}

#[test]
#[serial]
fn test_rag_render_analyze_damage_with_empty_db() {
    use st6_trainer_lib::db::{init_schema, open_db_at};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    // Match does not exist -> should return error
    let result = engine.render_analyze_damage(&conn, "nonexistent-match");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Match not found"));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_render_verify_punish_with_empty_db() {
    use st6_trainer_lib::db::{init_schema, open_db_at};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine.render_verify_punish(&conn, "nonexistent-match", "Hadoken", 10);
    assert!(result.is_err());

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_render_training_menu_with_empty_db() {
    use st6_trainer_lib::db::{init_schema, open_db_at};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine.render_training_menu(&conn, "nonexistent-match");
    assert!(result.is_err());

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_render_analyze_damage_with_match() {
    use st6_trainer_lib::db::{insert_match, init_schema, open_db_at, MatchSummary};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    insert_match(
        &conn,
        &MatchSummary {
            id: "match-rag-001".to_string(),
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

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine.render_analyze_damage(&conn, "match-rag-001").expect("render_analyze_damage");
    assert!(!result.prompt.is_empty());
    assert_eq!(result.template_name, "analyze_damage");
    assert!(result.estimated_tokens > 0);
    // Prompt should contain match context
    assert!(result.prompt.contains("Luke"));
    assert!(result.prompt.contains("Ken"));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_render_verify_punish_with_match() {
    use st6_trainer_lib::db::{insert_match, init_schema, open_db_at, MatchSummary};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    insert_match(
        &conn,
        &MatchSummary {
            id: "match-rag-002".to_string(),
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

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine
        .render_verify_punish(&conn, "match-rag-002", "Hadoken", 10)
        .expect("render_verify_punish");
    assert!(!result.prompt.is_empty());
    assert_eq!(result.template_name, "verify_punish");
    assert!(result.estimated_tokens > 0);
    assert!(result.prompt.contains("Hadoken"));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_render_training_menu_with_match() {
    use st6_trainer_lib::db::{insert_match, init_schema, open_db_at, MatchSummary};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    insert_match(
        &conn,
        &MatchSummary {
            id: "match-rag-003".to_string(),
            played_at: Some("2026-01-01T00:00:00Z".to_string()),
            my_character: "Luke".to_string(),
            opponent_character: "Ken".to_string(),
            my_control_type: "classic".to_string(),
            opponent_control_type: "modern".to_string(),
            total_rounds: 3,
            result: "LOSS".to_string(),
            source_video_path: None,
            scan_config_version: None,
        },
    )
    .expect("insert_match");

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine.render_training_menu(&conn, "match-rag-003").expect("render_training_menu");
    assert!(!result.prompt.is_empty());
    assert_eq!(result.template_name, "training_menu");
    assert!(result.estimated_tokens > 0);

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_prompt_contains_match_context() {
    use st6_trainer_lib::db::{insert_match, init_schema, open_db_at, MatchSummary};
    use st6_trainer_lib::rag::RagEngine;
    use std::path::PathBuf;

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = open_db_at(&db_path).expect("open_db_at");
    init_schema(&conn).expect("init_schema");

    insert_match(
        &conn,
        &MatchSummary {
            id: "match-ctx-001".to_string(),
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

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let engine = RagEngine::new(&template_dir).expect("RagEngine::new");

    let result = engine.render_analyze_damage(&conn, "match-ctx-001").expect("render");
    assert!(result.prompt.contains("A.K.I."));
    assert!(result.prompt.contains("Ed"));
    assert!(result.prompt.contains("modern"));
    assert!(result.prompt.contains("classic"));
    assert!(result.prompt.contains("LOSS"));

    drop(conn);
    let _ = dir.close();
}

#[test]
#[serial]
fn test_rag_template_files_exist() {
    use std::path::PathBuf;

    let template_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prompts");
    assert!(template_dir.join("base.hbs").exists());
    assert!(template_dir.join("analyze_damage.hbs").exists());
    assert!(template_dir.join("verify_punish.hbs").exists());
    assert!(template_dir.join("training_menu.hbs").exists());
    assert!(template_dir.join("partials/match_context.hbs").exists());
    assert!(template_dir.join("partials/frame_data.hbs").exists());
    assert!(template_dir.join("partials/move_event.hbs").exists());
    assert!(template_dir.join("partials/input_sequence.hbs").exists());
}

#[test]
#[serial]
fn test_rag_cleanup_temp_db_file() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let conn = st6_trainer_lib::db::open_db_at(&db_path).expect("open_db_at");
    st6_trainer_lib::db::init_schema(&conn).expect("init_schema");
    drop(conn);
    // TempDir drop cleans up the directory
    let _ = dir.close();
    assert!(!db_path.exists(), "temp db should be cleaned up");
}

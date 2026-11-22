//! Phase 10: Structured RAG & Context Assembly.
//!
//! Reads scan results from SQLite, assembles structured context data, renders
//! handlebars templates, and generates LLM prompts for three use cases:
//! - analyze_damage: Analysis of the Causes of Being Hit
//! - verify_punish: Verification of Guaranteed Counterattacks
//! - training_menu: Training Plan Proposal
//!
//! Includes token budget management to keep prompts within ctx_size limits.

use crate::db;
use crate::llm_protocol::InferParams;
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

/// Prompt generation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResult {
    pub prompt: String,
    pub estimated_tokens: usize,
    pub template_name: String,
}

/// Context data for match analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchContext {
    pub match_id: String,
    pub played_at: Option<String>,
    pub my_character: String,
    pub opponent_character: String,
    pub my_control_type: String,
    pub opponent_control_type: String,
    pub total_rounds: i32,
    pub result: String,
    pub device: Option<String>,
    pub rounds: Vec<RoundContext>,
}

/// Per-round context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundContext {
    pub round_index: i32,
    pub hp_p1_start: f64,
    pub hp_p1_end: f64,
    pub hp_p2_start: f64,
    pub hp_p2_end: f64,
    pub round_result: String,
}

/// Damage event context for analyze_damage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageEvent {
    pub frame_start: i32,
    pub frame_end: i32,
    pub round_index: i32,
    pub opponent_move: String,
    pub opponent_character: String,
    pub my_state: String,
    pub my_inputs: Vec<String>,
    pub frame_disadvantage: i32,
    pub damage: i32,
}

/// Punish verification context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunishContext {
    pub guarded_move: String,
    pub opponent_character: String,
    pub guard_frame: i32,
    pub block_advantage: i32,
    pub my_character: String,
    pub my_control_type: String,
    pub opponent_control_type: String,
    pub candidate_moves: Vec<CandidateMove>,
}

/// Candidate move for punish verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateMove {
    pub move_name: String,
    pub startup: String,
    pub block_advantage: String,
    pub damage: String,
    pub is_punishable: bool,
    pub reach: String,
    pub cancel: String,
}

/// Training menu context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMenuContext {
    pub issues: Vec<TrainingIssue>,
    pub menus: Vec<TrainingMenu>,
    pub estimated_time: String,
    pub recommended_sets: i32,
    pub device: Option<String>,
}

/// Training issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingIssue {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub related_moves: String,
}

/// Training menu item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMenu {
    pub menu_name: String,
    pub purpose: String,
    pub method: String,
    pub repetitions: String,
    pub success_criteria: String,
    pub device_notes: String,
}

/// RAG engine: loads templates, assembles context, renders prompts.
pub struct RagEngine {
    handlebars: Handlebars<'static>,
    ctx_size: u32,
}

impl RagEngine {
    /// Create a new RagEngine, loading all templates from the prompts directory.
    pub fn new(template_dir: &PathBuf) -> Result<Self, String> {
        let mut handlebars = Handlebars::new();

        // Register base template
        let base_path = template_dir.join("base.hbs");
        let base = std::fs::read_to_string(&base_path)
            .map_err(|e| format!("Failed to read base.hbs: {}", e))?;
        handlebars
            .register_template_string("base", base)
            .map_err(|e| format!("Failed to compile base.hbs: {}", e))?;

        // Register partials
        let partials_dir = template_dir.join("partials");
        for entry in std::fs::read_dir(&partials_dir)
            .map_err(|e| format!("Failed to read partials dir: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "hbs") {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
                handlebars
                    .register_partial(&name, &content)
                    .map_err(|e| format!("Failed to register partial {}: {}", name, e))?;
            }
        }

        // Register use-case templates
        for template_name in ["analyze_damage", "verify_punish", "training_menu"] {
            let tpl_path = template_dir.join(format!("{}.hbs", template_name));
            if tpl_path.exists() {
                let content = std::fs::read_to_string(&tpl_path)
                    .map_err(|e| format!("Failed to read {}.hbs: {}", template_name, e))?;
                handlebars
                    .register_template_string(template_name, content)
                    .map_err(|e| format!("Failed to compile {}.hbs: {}", template_name, e))?;
            }
        }

        Ok(Self {
            handlebars,
            ctx_size: 4096,
        })
    }

    /// Set the context size for token budget management.
    pub fn with_ctx_size(mut self, ctx_size: u32) -> Self {
        self.ctx_size = ctx_size;
        self
    }

    /// Get current context size.
    pub fn ctx_size(&self) -> u32 {
        self.ctx_size
    }

    /// Load match context from SQLite for a given match ID.
    pub fn load_match_context(
        conn: &rusqlite::Connection,
        match_id: &str,
    ) -> Result<MatchContext, String> {
        let match_summary = db::get_match(conn, match_id)?
            .ok_or_else(|| format!("Match not found: {}", match_id))?;

        // Build round contexts from frame telemetry (first/last HP per round)
        let telemetry = db::get_frame_telemetry(conn, match_id)?;
        let rounds = build_round_contexts(&telemetry);

        Ok(MatchContext {
            match_id: match_summary.id,
            played_at: match_summary.played_at,
            my_character: match_summary.my_character,
            opponent_character: match_summary.opponent_character,
            my_control_type: match_summary.my_control_type,
            opponent_control_type: match_summary.opponent_control_type,
            total_rounds: match_summary.total_rounds,
            result: match_summary.result,
            device: None, // TODO: from config or match metadata
            rounds,
        })
    }

    /// Load frame data moves for a character/control type from SQLite.
    pub fn load_frame_data(
        conn: &rusqlite::Connection,
        character_id: &str,
        control_type: &str,
    ) -> Result<Vec<db::MoveRecord>, String> {
        db::get_moves_for_character(conn, character_id, control_type)
    }

    /// Render the analyze_damage prompt.
    pub fn render_analyze_damage(
        &self,
        conn: &rusqlite::Connection,
        match_id: &str,
    ) -> Result<PromptResult, String> {
        let ctx = Self::load_match_context(conn, match_id)?;

        // Get damage events from move events (where damage was dealt to my character)
        let move_events = db::get_move_events(conn, match_id)?;
        let damage_events = build_damage_events(&move_events, &ctx.my_character);

        let frame_data = Self::load_frame_data(conn, &ctx.my_character, &ctx.my_control_type)?;

        let data = json!({
            "match_id": ctx.match_id,
            "played_at": ctx.played_at,
            "my_character": ctx.my_character,
            "opponent_character": ctx.opponent_character,
            "my_control_type": ctx.my_control_type,
            "opponent_control_type": ctx.opponent_control_type,
            "total_rounds": ctx.total_rounds,
            "result": ctx.result,
            "device": ctx.device,
            "rounds": ctx.rounds,
            "damage_events": damage_events,
            "frame_data": frame_data,
        });

        let prompt = self.render_template("analyze_damage", &data)?;
        let estimated_tokens = estimate_tokens(&prompt);

        Ok(PromptResult {
            prompt,
            estimated_tokens,
            template_name: "analyze_damage".to_string(),
        })
    }

    /// Render the verify_punish prompt.
    pub fn render_verify_punish(
        &self,
        conn: &rusqlite::Connection,
        match_id: &str,
        guarded_move: &str,
        guard_frame: i32,
    ) -> Result<PromptResult, String> {
        let ctx = Self::load_match_context(conn, match_id)?;

        // Get frame data for my character to find candidate punish moves
        let frame_data = Self::load_frame_data(conn, &ctx.my_character, &ctx.my_control_type)?;

        // Determine block advantage from the guarded move's frame data
        let block_advantage = parse_block_advantage(&frame_data, guarded_move);

        // Build candidate moves: moves with startup <= |block_advantage| (punishable)
        let candidate_moves: Vec<CandidateMove> = frame_data
            .iter()
            .filter(|m| {
                // Only consider moves that can punish (startup <= punish window)
                let startup: i32 = m.startup.parse().unwrap_or(999);
                startup <= block_advantage.abs()
            })
            .map(|m| CandidateMove {
                move_name: m.move_name.clone(),
                startup: m.startup.clone(),
                block_advantage: m.block_advantage.clone(),
                damage: m.damage.clone(),
                is_punishable: true,
                reach: m.attribute.clone().unwrap_or_else(|| "Unknown".to_string()),
                cancel: m.cancel.clone(),
            })
            .take(5)
            .collect();

        let data = json!({
            "match_id": ctx.match_id,
            "played_at": ctx.played_at,
            "my_character": ctx.my_character,
            "opponent_character": ctx.opponent_character,
            "my_control_type": ctx.my_control_type,
            "opponent_control_type": ctx.opponent_control_type,
            "total_rounds": ctx.total_rounds,
            "result": ctx.result,
            "device": ctx.device,
            "rounds": ctx.rounds,
            "guarded_move": guarded_move,
            "opponent_character": ctx.opponent_character,
            "guard_frame": guard_frame,
            "block_advantage": block_advantage,
            "candidate_moves": candidate_moves,
        });

        let prompt = self.render_template("verify_punish", &data)?;
        let estimated_tokens = estimate_tokens(&prompt);

        Ok(PromptResult {
            prompt,
            estimated_tokens,
            template_name: "verify_punish".to_string(),
        })
    }

    /// Render the training_menu prompt.
    pub fn render_training_menu(
        &self,
        conn: &rusqlite::Connection,
        match_id: &str,
    ) -> Result<PromptResult, String> {
        let ctx = Self::load_match_context(conn, match_id)?;

        // Get move events to identify issues (punishable moves whiffed, etc.)
        let move_events = db::get_move_events(conn, match_id)?;
        let issues = build_training_issues(&move_events, &ctx);

        // Get frame data for menu generation
        let _frame_data = Self::load_frame_data(conn, &ctx.my_character, &ctx.my_control_type)?;

        // Build training menus from issues
        let menus: Vec<TrainingMenu> = issues
            .iter()
            .map(|issue| TrainingMenu {
                menu_name: format!("{} 対策", issue.title),
                purpose: issue.description.clone(),
                method: format!("{} の反復練習", issue.related_moves),
                repetitions: "10回 × 3セット".to_string(),
                success_criteria: "成功率 80% 以上".to_string(),
                device_notes: ctx.device.clone().unwrap_or_else(|| "パッド".to_string()),
            })
            .collect();

        let data = json!({
            "match_id": ctx.match_id,
            "played_at": ctx.played_at,
            "my_character": ctx.my_character,
            "opponent_character": ctx.opponent_character,
            "my_control_type": ctx.my_control_type,
            "opponent_control_type": ctx.opponent_control_type,
            "total_rounds": ctx.total_rounds,
            "result": ctx.result,
            "device": ctx.device,
            "rounds": ctx.rounds,
            "issues": issues,
            "menus": menus,
            "estimated_time": "15分",
            "recommended_sets": 3,
        });

        let prompt = self.render_template("training_menu", &data)?;
        let estimated_tokens = estimate_tokens(&prompt);

        Ok(PromptResult {
            prompt,
            estimated_tokens,
            template_name: "training_menu".to_string(),
        })
    }

    /// Render a named template with JSON data.
    fn render_template(&self, name: &str, data: &serde_json::Value) -> Result<String, String> {
        self.handlebars
            .render(name, data)
            .map_err(|e| format!("Template render error ({}): {}", name, e))
    }

    /// Check if the prompt fits within the token budget, truncating if needed.
    pub fn fit_to_budget(&self, prompt: &str) -> String {
        let estimated = estimate_tokens(prompt);
        if estimated <= self.ctx_size as usize {
            return prompt.to_string();
        }

        // Simple truncation: keep the beginning and end, cut the middle
        let target = self.ctx_size as usize * 3 / 4; // leave room for completion
        let half = target / 2;
        let chars = prompt.chars().collect::<Vec<_>>();
        if chars.len() <= target {
            return prompt.to_string();
        }
        let start: String = chars.iter().take(half).collect();
        let end: String = chars.iter().rev().take(half).collect();
        format!(
            "{}\n\n[... コンテキストがトークン予算により切り詰められました ...]\n\n{}",
            start, end
        )
    }
}

/// Build round contexts from frame telemetry (first/last frame per round).
fn build_round_contexts(telemetry: &[db::FrameTelemetryRecord]) -> Vec<RoundContext> {
    let mut rounds: HashMap<i32, (f64, f64, f64, f64)> = HashMap::new();

    for t in telemetry {
        let entry = rounds
            .entry(t.round_index)
            .or_insert((t.hp_p1, t.hp_p1, t.hp_p2, t.hp_p2));
        // entry.0/2 = start HP (first frame seen, kept from or_insert)
        // entry.1/3 = end HP (last frame seen, overwritten each iteration)
        entry.1 = t.hp_p1;
        entry.3 = t.hp_p2;
    }

    let mut result: Vec<RoundContext> = rounds
        .into_iter()
        .map(|(round_index, (p1_start, p1_end, p2_start, p2_end))| {
            let round_result = if p1_end < p2_start {
                "WIN"
            } else if p2_end < p1_start {
                "LOSS"
            } else {
                "UNKNOWN"
            };
            RoundContext {
                round_index,
                hp_p1_start: p1_start,
                hp_p1_end: p1_end,
                hp_p2_start: p2_start,
                hp_p2_end: p2_end,
                round_result: round_result.to_string(),
            }
        })
        .collect();

    result.sort_by_key(|r| r.round_index);
    result
}

/// Build damage events from move events (moves that hit my character).
fn build_damage_events(
    move_events: &[db::MoveEventRecord],
    _my_character: &str,
) -> Vec<DamageEvent> {
    let mut events = Vec::new();

    for m in move_events {
        // Damage to my character (P2 if I'm P1, etc.) - simplified: look at damage_p2
        if let Some(damage) = m.damage_p2 {
            if damage > 0 {
                events.push(DamageEvent {
                    frame_start: m.frame_start,
                    frame_end: m.frame_end,
                    round_index: m.round_index,
                    opponent_move: m
                        .attack_property_p2
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string()),
                    opponent_character: "Opponent".to_string(),
                    my_state: "Guard/Hit".to_string(),
                    my_inputs: Vec::new(),
                    frame_disadvantage: m.fm_advantage_p2.unwrap_or(0),
                    damage,
                });
            }
        }
    }

    events
}

/// Build training issues from move events.
fn build_training_issues(
    move_events: &[db::MoveEventRecord],
    _ctx: &MatchContext,
) -> Vec<TrainingIssue> {
    let mut issues = Vec::new();

    // Look for moves with negative advantage (punishable)
    for m in move_events {
        if let Some(adv) = m.fm_advantage_p1 {
            if adv < -5 {
                issues.push(TrainingIssue {
                    title: format!("フレーム不利技 (ラウンド {})", m.round_index),
                    description: format!(
                        "発生 {}F の技が {}F 不利でした。ガード後反撃を避けるための対策が必要です。",
                        m.fm_startup_p1.unwrap_or(0),
                        adv.abs()
                    ),
                    priority: "高".to_string(),
                    related_moves: m.attack_property_p1.clone().unwrap_or_else(|| "Unknown".to_string()),
                });
            }
        }
    }

    // If no specific issues, add a general one
    if issues.is_empty() {
        issues.push(TrainingIssue {
            title: "基本コンボ練習".to_string(),
            description: "安定したコンボを決めるための基礎練習".to_string(),
            priority: "中".to_string(),
            related_moves: "全技".to_string(),
        });
    }

    issues
}

/// Parse block advantage from frame data for a specific move.
fn parse_block_advantage(frame_data: &[db::MoveRecord], move_name: &str) -> i32 {
    for m in frame_data {
        if m.move_name == move_name {
            return m.block_advantage.parse().unwrap_or(0);
        }
    }
    0
}

/// Estimate token count from a string (rough approximation: 1 token ≈ 4 chars for Japanese).
pub fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: Japanese text averages ~2 chars per token, English ~4 chars per token
    // Use a conservative estimate of 2.5 chars per token
    (text.len() as f64 / 2.5).ceil() as usize
}

/// Default prompt parameters for LLM inference.
pub fn default_infer_params() -> InferParams {
    InferParams {
        temperature: Some(0.7),
        top_p: Some(0.9),
        max_tokens: Some(1024),
        stream: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::path::PathBuf;

    #[test]
    fn test_estimate_tokens() {
        let tokens = estimate_tokens("Hello world");
        assert!(tokens > 0);
        // Short text should be at least 1 token
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_fit_to_budget_under_limit() {
        let engine = RagEngine::new(&PathBuf::from("prompts")).unwrap();
        let short = "Short prompt";
        let result = engine.fit_to_budget(short);
        assert_eq!(result, short);
    }

    #[test]
    fn test_fit_to_budget_over_limit() {
        let engine =
            RagEngine::with_ctx_size(RagEngine::new(&PathBuf::from("prompts")).unwrap(), 10);
        let long = "This is a very long prompt that exceeds the token budget significantly. It contains enough text that truncation should produce a shorter result than the original input string because the truncation marker itself is relatively short compared to this long input.";
        let result = engine.fit_to_budget(long);
        assert!(result.len() < long.len());
        assert!(result.contains("切り詰め"));
    }

    #[test]
    fn test_build_round_contexts() {
        let telemetry = vec![
            db::FrameTelemetryRecord {
                match_id: "m".to_string(),
                frame_index: 0,
                source_frame_index: Some(0),
                timestamp_ms: Some(0),
                round_index: 1,
                hp_reset: 0,
                hp_p1: 1.0,
                drive_p1: 6.0,
                sa_p1: 3,
                in_burnout_p1: 0,
                hp_p2: 1.0,
                drive_p2: 6.0,
                sa_p2: 3,
                in_burnout_p2: 0,
                quality_flags: vec![],
            },
            db::FrameTelemetryRecord {
                match_id: "m".to_string(),
                frame_index: 100,
                source_frame_index: Some(100),
                timestamp_ms: Some(1667),
                round_index: 1,
                hp_reset: 0,
                hp_p1: 0.7,
                drive_p1: 6.0,
                sa_p1: 3,
                in_burnout_p1: 0,
                hp_p2: 1.0,
                drive_p2: 6.0,
                sa_p2: 3,
                in_burnout_p2: 0,
                quality_flags: vec![],
            },
        ];
        let rounds = build_round_contexts(&telemetry);
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].round_index, 1);
        assert_eq!(rounds[0].hp_p1_start, 1.0);
        assert_eq!(rounds[0].hp_p1_end, 0.7);
    }

    #[test]
    fn test_build_damage_events() {
        let move_events = vec![db::MoveEventRecord {
            match_id: "m".to_string(),
            frame_start: 100,
            frame_end: 110,
            round_index: 1,
            fm_startup_p1: Some(5),
            fm_total_p1: Some(15),
            fm_advantage_p1: Some(0),
            fm_startup_p2: Some(5),
            fm_total_p2: Some(15),
            fm_advantage_p2: Some(0),
            damage_p1: Some(0),
            combo_damage_p1: Some(0),
            damage_p2: Some(300),
            combo_damage_p2: Some(300),
            attack_property_p1: Some("High".to_string()),
            attack_property_p2: Some("Mid".to_string()),
            attack_property_locale: Some("en".to_string()),
            conf: Some(0.95),
            quality_flags: vec![],
        }];
        let events = build_damage_events(&move_events, "Luke");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].damage, 300);
    }

    #[test]
    fn test_default_infer_params() {
        let params = default_infer_params();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.top_p, Some(0.9));
        assert_eq!(params.max_tokens, Some(1024));
        assert_eq!(params.stream, Some(true));
    }
}

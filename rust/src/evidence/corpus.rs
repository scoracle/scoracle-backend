//! Shared entity lookup, transfer-heat, identity-card, and dedupe primitives.

use crate::evidence::trajectory::DEFAULT_TRAJECTORY;
use crate::studio::insider::HeatItem;
use anyhow::{bail, Context, Result};
use sqlx::PgPool;

/// Bounds the transfer rumors shown to the model as the entity's "transfer temperature".
/// `COGNITION_MAX_HEAT_ITEMS` overrides the default.
/// Read once via OnceLock: corpus is a shared primitive with no config handle, and the value
/// must not change between a prompt build and its ledger row.
fn max_heat_items() -> i64 {
    static MAX: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("COGNITION_MAX_HEAT_ITEMS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(6)
    })
}

/// load_transfer_heat returns the entity's hottest active transfer/trade rumors (latest
/// per counterparty, heat > 0, model-vetted), naming the counterparty. The `is_rumor IS
/// TRUE` gate is applied after picking the latest row per counterparty, so a newer
/// cleared/unknown verdict supersedes an older TRUE. The
/// current-week freshness gate plus the shared cooling-off retirement rule keeps very
/// old false positives from grounding prompts forever — mirrors the /transfers card
/// read path.
pub async fn load_transfer_heat(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<HeatItem>> {
    let query = if entity_type == "team" {
        r#"
        SELECT counterparty, heat, stage, direction, summary, confidence FROM (
            SELECT DISTINCT ON (tr.player_id)
                   p.name AS counterparty, tr.team_id, pci.team_id AS current_team_id,
                   tr.heat, tr.is_rumor,
                   COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
                   COALESCE(tr.model_summary,'') AS summary, tr.confidence::float8 AS confidence,
                   tr.generated_at
            FROM transfer_rumors tr
            JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
            LEFT JOIN public.player_current_identity pci ON pci.player_id = tr.player_id AND pci.sport = tr.sport
            WHERE tr.team_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
              AND tr.generated_at > NOW() - INTERVAL '7 days'
              AND (COALESCE(tr.trajectory, $4) <> 'cooling_off'
                   OR COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) > NOW() - INTERVAL '3 days')
            ORDER BY tr.player_id, tr.generated_at DESC
        ) latest
        WHERE heat > 0 AND is_rumor IS TRUE
          AND NOT (
              (current_team_id IS NOT NULL AND current_team_id = team_id AND direction = 'incoming')
              OR (current_team_id IS NOT NULL AND current_team_id <> team_id AND direction = 'outgoing')
          )
        ORDER BY heat DESC LIMIT $3
        "#
    } else {
        r#"
        SELECT counterparty, heat, stage, direction, summary, confidence FROM (
            SELECT DISTINCT ON (tr.team_id)
                   t.name AS counterparty, tr.team_id, pci.team_id AS current_team_id,
                   tr.heat, tr.is_rumor,
                   COALESCE(tr.stage,'') AS stage, COALESCE(tr.direction,'') AS direction,
                   COALESCE(tr.model_summary,'') AS summary, tr.confidence::float8 AS confidence,
                   tr.generated_at
            FROM transfer_rumors tr
            JOIN teams t ON t.id = tr.team_id AND t.sport = tr.sport
            LEFT JOIN public.player_current_identity pci ON pci.player_id = tr.player_id AND pci.sport = tr.sport
            WHERE tr.player_id = $1 AND tr.sport = $2 AND tr.heat IS NOT NULL
              AND tr.generated_at > NOW() - INTERVAL '7 days'
              AND (COALESCE(tr.trajectory, $4) <> 'cooling_off'
                   OR COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) > NOW() - INTERVAL '3 days')
            ORDER BY tr.team_id, tr.generated_at DESC
        ) latest
        WHERE heat > 0 AND is_rumor IS TRUE
          AND NOT (
              (current_team_id IS NOT NULL AND current_team_id = team_id AND direction = 'incoming')
              OR (current_team_id IS NOT NULL AND current_team_id <> team_id AND direction = 'outgoing')
          )
        ORDER BY heat DESC LIMIT $3
        "#
    };

    // heat is int2 → scan as i16 (matches Go scanning into `int`, widened below).
    let rows: Vec<(String, i16, String, String, String, Option<f64>)> = sqlx::query_as(query)
        .bind(entity_id)
        .bind(sport)
        .bind(max_heat_items())
        .bind(DEFAULT_TRAJECTORY)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load transfer heat {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(counterparty, heat, stage, direction, summary, confidence)| HeatItem {
                counterparty,
                heat: heat as i32,
                stage,
                direction,
                summary,
                confidence,
            },
        )
        .collect())
}

/// lookup_entity_name resolves the display name for the prompt. An empty/missing name is an
/// error, so the work item fails and retries rather than generating against a blank prompt.
pub async fn lookup_entity_name(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<String> {
    let query = if entity_type == "player" {
        "SELECT name FROM players WHERE id = $1 AND sport = $2"
    } else {
        "SELECT name FROM teams WHERE id = $1 AND sport = $2"
    };
    let name: String = sqlx::query_scalar(query)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("lookup {entity_type}/{entity_id} ({sport})"))?;
    if name.is_empty() {
        bail!("empty name for {entity_type}/{entity_id} ({sport})");
    }
    Ok(name)
}

/// dedupe_i64 removes duplicates preserving first-seen order. Mirrors `dedupeInt64`.
pub fn dedupe_i64(input: Vec<i64>) -> Vec<i64> {
    let mut out = Vec::with_capacity(input.len());
    let mut seen = std::collections::HashSet::with_capacity(input.len());
    for v in input {
        if seen.insert(v) {
            out.push(v);
        }
    }
    out
}

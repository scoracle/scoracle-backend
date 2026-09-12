//! Shared entity lookup, transfer-heat, identity-card, and dedupe primitives.

use crate::trajectory::DEFAULT_TRAJECTORY;
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

/// One active, vetted transfer rumor naming its counterparty.
#[derive(Clone, Debug)]
pub struct HeatItem {
    pub counterparty: String,
    pub heat: i32,
    pub stage: String,
    pub direction: String,
    pub summary: String,
    pub confidence: Option<f64>,
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

/// write_heat_lines renders heat bullets:
///   `- <counterparty> — heat <heat>[, <direction>][, <stage>][ (confidence 0.N)][ — "<summary>"]`
///
/// All downstream prompts share this transfer-heat line format.
pub fn write_heat_lines(b: &mut String, heat: &[HeatItem]) {
    for h in heat {
        let mut line = format!("- {} — heat {}", h.counterparty, h.heat);
        if !h.direction.is_empty() {
            line.push_str(", ");
            line.push_str(&h.direction);
        }
        if !h.stage.is_empty() {
            line.push_str(", ");
            line.push_str(&h.stage);
        }
        if let Some(c) = h.confidence {
            line.push_str(&format!(" (confidence {c:.1})"));
        }
        if !h.summary.is_empty() {
            // The summary is written ≤240 bytes and single-sentence; fold any stray newline so
            // one rumor stays one prompt bullet.
            line.push_str(" — \"");
            line.push_str(&h.summary.replace(['\n', '\r'], " "));
            line.push('"');
        }
        b.push_str(&line);
        b.push('\n');
    }
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

/// Framing that tells the model to reconcile dated house records with current reporting.
pub const IDENTITY_CARD_FRAMING: &str = "Your entity, per our house records — a dated snapshot, not gospel. Reporting may have moved past it: when a story and these records disagree, weigh recency and credibility and write what you judge true.";

/// load_identity_card renders the entity's house-record identity line for the prompts: who
/// this is, where they play, and (teams) the coach on record. Prompt-only and deliberately
/// OUTSIDE every input_hash — identity context frames the read, it is not evidence, and a
/// records sync must not reopen a fleet (the same treatment as relational memory).
///
/// `None` when the entity is unknown — an absent card is honest; an empty one is noise.
pub async fn load_identity_card(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> anyhow::Result<Option<String>> {
    use sqlx::Row;
    let sport_uc = sport.to_uppercase();
    let card = match entity_type {
        "team" => {
            let row = sqlx::query(
                r#"
                SELECT t.name, t.city, t.country, t.venue_name, t.conference, t.division,
                       l.name AS league_name, l.country AS league_country,
                       (SELECT pe.full_name FROM public.persons pe
                         WHERE pe.sport = t.sport AND pe.team_id = t.id AND pe.kind = 'coach'
                         ORDER BY pe.created_at DESC LIMIT 1) AS coach
                  FROM public.teams t
                  LEFT JOIN public.leagues l ON l.id = t.league_id AND l.sport = t.sport
                 WHERE t.id = $1 AND t.sport = $2
                "#,
            )
            .bind(entity_id)
            .bind(&sport_uc)
            .fetch_optional(pool)
            .await?;
            row.map(|r| {
                let name: String = r.get("name");
                let mut line = name;
                if let Some(league) = r.get::<Option<String>, _>("league_name") {
                    line.push_str(&format!(" — {league}"));
                    if let Some(c) = r.get::<Option<String>, _>("league_country") {
                        line.push_str(&format!(" ({c})"));
                    }
                } else if let Some(conf) = r.get::<Option<String>, _>("conference") {
                    line.push_str(&format!(" — {conf}"));
                    if let Some(div) = r.get::<Option<String>, _>("division") {
                        line.push_str(&format!(" {div}"));
                    }
                }
                if let (Some(venue), Some(city)) = (
                    r.get::<Option<String>, _>("venue_name"),
                    r.get::<Option<String>, _>("city"),
                ) {
                    line.push_str(&format!(". Home: {venue}, {city}"));
                }
                if let Some(coach) = r.get::<Option<String>, _>("coach") {
                    line.push_str(&format!(". Coach on record: {coach}"));
                }
                line.push('.');
                line
            })
        }
        "player" => {
            let row = sqlx::query(
                r#"
                SELECT p.name, p.nationality,
                       t.name AS team_name,
                       l.name AS league_name,
                       (SELECT ps.position FROM public.player_stats ps
                         WHERE ps.player_id = p.id AND ps.sport = p.sport
                           AND ps.position IS NOT NULL
                         ORDER BY ps.season DESC LIMIT 1) AS position
                  FROM public.players p
                  LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
                  LEFT JOIN public.leagues l ON l.id = COALESCE(p.league_id, t.league_id) AND l.sport = p.sport
                 WHERE p.id = $1 AND p.sport = $2
                "#,
            )
            .bind(entity_id)
            .bind(&sport_uc)
            .fetch_optional(pool)
            .await?;
            row.map(|r| {
                let name: String = r.get("name");
                let mut line = name;
                if let Some(pos) = r.get::<Option<String>, _>("position") {
                    line.push_str(&format!(" — {pos}"));
                }
                if let Some(team) = r.get::<Option<String>, _>("team_name") {
                    line.push_str(&format!(", on record at {team}"));
                    if let Some(league) = r.get::<Option<String>, _>("league_name") {
                        line.push_str(&format!(" ({league})"));
                    }
                }
                if let Some(nat) = r.get::<Option<String>, _>("nationality") {
                    line.push_str(&format!(". Nationality: {nat}"));
                }
                line.push('.');
                line
            })
        }
        _ => None,
    };
    Ok(card.map(|c| format!("{IDENTITY_CARD_FRAMING}\n- {c}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_lines_render_summary_and_confidence_when_present() {
        let heat = vec![
            HeatItem {
                counterparty: "Lakers".to_string(),
                heat: 80,
                stage: "advanced_talks".to_string(),
                direction: "incoming".to_string(),
                summary: "Lakers pursuing a\nwing upgrade per ESPN".to_string(),
                confidence: Some(0.75),
            },
            // Bare item (a pre-Phase-1 row with no model_summary): the original line shape.
            HeatItem {
                counterparty: "Heat".to_string(),
                heat: 40,
                stage: String::new(),
                direction: String::new(),
                summary: String::new(),
                confidence: None,
            },
        ];
        let mut b = String::new();
        write_heat_lines(&mut b, &heat);
        assert_eq!(
            b,
            "- Lakers — heat 80, incoming, advanced_talks (confidence 0.8) — \"Lakers pursuing a wing upgrade per ESPN\"\n\
             - Heat — heat 40\n"
        );
    }
}

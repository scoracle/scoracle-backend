//! Scrub stage handler — the news ID-gate as a `pipeline_work` stage (Plan §8, L6 option (i)).
//!
//! Ports `go/internal/ml/news_scrub.go::ScrubArticle` to a `StageHandler`: claim an ARTICLE-keyed
//! work item → load the article + its candidate links (with identity cards) → force-keep the primary
//! (confidence ≥ 1.0, the entity the article was fetched for) and run the ASYMMETRIC `resolve_set`
//! gate (Plan §8) on the secondary fuzzy guesses → write `news_article_entities.vetted`. That write
//! fires the mig-103 `AFTER UPDATE OF vetted` trigger, which enqueues the per-entity derive stages
//! (narratives/vibe/transfers) exactly as today — so scrub joins the queue without changing the
//! downstream contract. Terminal: the handler enqueues nothing itself (the trigger does).
//!
//! The gate spends Gemma only on the ambiguous band; the auto-keeps skip it (the ~50% GPU win). The
//! proxy never auto-drops (the L5 shadow proved that loses non-redundant truth), so every exclusion
//! is the model's — fail-closed when the model won't commit.

use crate::harness::{Candidate, EntityType, Harness, IdentityCard};
use crate::route::Role;
use crate::stage::StageHandler;
use crate::work::{Item, Stage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::Row;
use std::collections::HashMap;

/// ScrubHandler drains the article-keyed `scrub` stage.
pub struct ScrubHandler;

impl ScrubHandler {
    pub fn new() -> Self {
        ScrubHandler
    }
}

impl Default for ScrubHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// One entity currently linked to an article (the fuzzy matcher's guess) + its identity card.
struct ScrubCandidate {
    entity_type: EntityType,
    entity_id: i32,
    name: String,
    nationality: String,
    current_club: String,
    position: String,
    confidence: f64,
}

#[async_trait]
impl StageHandler for ScrubHandler {
    fn stage(&self) -> Stage {
        Stage::Scrub
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        // The scrub item is article-keyed: entity_type='article', entity_id=news_articles.id.
        let article_id = item.entity_id;
        let sport = item.sport.to_uppercase();

        let row = sqlx::query(
            "SELECT title, COALESCE(description, '') AS description FROM news_articles WHERE id = $1",
        )
        .bind(article_id)
        .fetch_optional(&hx.pool)
        .await
        .context("load article")?;
        let Some(row) = row else {
            return Ok(()); // article vanished → nothing to scrub
        };
        let title: String = row.get("title");
        let description: String = row.get("description");

        let cands = load_candidates(hx, article_id, &sport).await?;
        if cands.is_empty() {
            return Ok(());
        }

        // Force-keep the primary (confidence ≥ 1.0); the asymmetric gate vets the secondaries.
        let context = article_text(&title, &description);
        let secondaries: Vec<Candidate> = cands
            .iter()
            .filter(|c| c.confidence < 1.0)
            .map(to_candidate)
            .collect();
        let resolutions = if secondaries.is_empty() {
            Vec::new()
        } else {
            hx.resolve_set(Role::EmotionalNews, &context, &secondaries)
                .await
                .context("resolve_set gate")?
        };
        let kept_secondary: HashMap<(EntityType, i32), bool> = resolutions
            .iter()
            .map(|r| ((r.entity_type, r.entity_id), r.kept))
            .collect();

        // A verdict for every link: primary kept by rule, secondary by the gate (default-drop on a
        // missing verdict — the conservative call, mirroring the Go fail-closed).
        let entity_types: Vec<String> = cands
            .iter()
            .map(|c| c.entity_type.as_str().to_string())
            .collect();
        let entity_ids: Vec<i32> = cands.iter().map(|c| c.entity_id).collect();
        let relevants: Vec<bool> = cands
            .iter()
            .map(|c| {
                c.confidence >= 1.0
                    || kept_secondary
                        .get(&(c.entity_type, c.entity_id))
                        .copied()
                        .unwrap_or(false)
            })
            .collect();

        apply_verdicts(
            hx,
            article_id,
            &sport,
            &entity_types,
            &entity_ids,
            &relevants,
        )
        .await
    }
}

fn article_text(title: &str, description: &str) -> String {
    if description.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {description}")
    }
}

fn to_candidate(c: &ScrubCandidate) -> Candidate {
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Candidate {
        entity_type: c.entity_type,
        entity_id: c.entity_id,
        name: c.name.clone(),
        identity: IdentityCard {
            nationality: opt(&c.nationality),
            current_club: opt(&c.current_club),
            position: opt(&c.position),
        },
    }
}

/// load_candidates returns every entity linked to the article with its identity card — the Rust port
/// of `news_scrub.go::loadCandidates` (current club from `player_current_team`, position from the
/// latest stats row). `match_confidence` is cast `::float8` (sqlx has no numeric→f64 without the
/// decimal feature — the L5 landmine).
async fn load_candidates(
    hx: &Harness,
    article_id: i64,
    sport: &str,
) -> Result<Vec<ScrubCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT nae.entity_type, nae.entity_id,
               COALESCE(p.name, t.name, '')                  AS name,
               COALESCE(p.nationality, '')                   AS nationality,
               COALESCE(ct.name, '')                         AS current_club,
               COALESCE(NULLIF(pos.position, 'Unknown'), '') AS position,
               nae.match_confidence::float8                  AS confidence
        FROM news_article_entities nae
        LEFT JOIN players p ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN teams   t ON nae.entity_type = 'team'   AND t.id = nae.entity_id AND t.sport = nae.sport
        LEFT JOIN public.player_current_team pct ON nae.entity_type = 'player' AND pct.player_id = nae.entity_id AND pct.sport = nae.sport
        LEFT JOIN teams ct ON ct.id = pct.team_id AND ct.sport = nae.sport
        LEFT JOIN LATERAL (
            SELECT ps.position FROM player_stats ps
            WHERE ps.player_id = nae.entity_id AND ps.sport = nae.sport
            ORDER BY ps.season DESC NULLS LAST LIMIT 1
        ) pos ON nae.entity_type = 'player'
        WHERE nae.article_id = $1 AND nae.sport = $2
        ORDER BY nae.match_confidence DESC, nae.entity_type, nae.entity_id
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .fetch_all(&hx.pool)
    .await
    .context("load candidates")?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let et: String = r.get("entity_type");
        let Some(entity_type) = EntityType::from_db_str(&et) else {
            continue; // unknown entity_type → skip (defensive)
        };
        out.push(ScrubCandidate {
            entity_type,
            entity_id: r.get("entity_id"),
            name: r.get("name"),
            nationality: r.get("nationality"),
            current_club: r.get("current_club"),
            position: r.get("position"),
            confidence: r.get("confidence"),
        });
    }
    Ok(out)
}

/// apply_verdicts records the vetted call on every link of the article in ONE UPDATE (over unnest'd
/// parallel arrays) — the Rust port of `news_scrub.go::applyVerdicts`. The whole article's scrub lands
/// in a single statement, so the mig-103 per-row trigger firings all see the article's final vetted
/// state and the constant-payload `pg_notify` de-dups to one wake-up per article. Non-destructive
/// (mig 083): dropped links stay, flagged `vetted=false`.
async fn apply_verdicts(
    hx: &Harness,
    article_id: i64,
    sport: &str,
    entity_types: &[String],
    entity_ids: &[i32],
    relevants: &[bool],
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE news_article_entities n
           SET vetted = v.relevant, scrubbed_at = NOW()
          FROM unnest($2::text[], $3::int[], $4::bool[]) AS v(entity_type, entity_id, relevant)
         WHERE n.article_id = $1 AND n.sport = $5
           AND n.entity_type = v.entity_type AND n.entity_id = v.entity_id
        "#,
    )
    .bind(article_id)
    .bind(entity_types)
    .bind(entity_ids)
    .bind(relevants)
    .bind(sport)
    .execute(&hx.pool)
    .await
    .context("apply verdicts")?;
    Ok(())
}

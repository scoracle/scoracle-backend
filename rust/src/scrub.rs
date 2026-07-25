//! Scrub stage handler — the news ID-gate as a `pipeline_work` stage.
//!
//! It claims an ARTICLE-keyed work item, loads the article plus candidate links with identity
//! cards, force-keeps exact links (confidence >= 1.0), runs the asymmetric `resolve_set` gate on
//! primary team candidates from the broad RSS funnel, and writes `news_article_entities.vetted`.
//! Lower-confidence co-mentions are preserved as scrubbed-but-unvetted tokens for Article Reader,
//! which has the full publisher text and can promote only the co-mentions that are actually
//! material. A vetted write fires the SQL trigger that enqueues downstream per-entity work.
//! Terminal: the handler enqueues nothing itself.
//!
//! The gate spends local-model time only on the ambiguous band; the auto-keeps skip it. The proxy
//! never auto-drops (the L5 shadow proved that loses non-redundant truth), so every exclusion is the
//! model's — fail-closed when the model won't commit.

use crate::harness::{Candidate, EntityType, Harness, IdentityCard};
use crate::route::Role;
use crate::stage::StageHandler;
use crate::work::{self, Item, Stage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::Row;
use std::collections::HashMap;

/// Go marks broad team RSS primaries at 0.95. Lower-confidence links are co-mention
/// candidates; when a primary exists, Article Reader owns their full-text verdict.
const PRIMARY_TEAM_CONFIDENCE_FLOOR: f64 = 0.95;

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
    /// The link's settled verdict, or `None` when nobody has ruled yet. `Some(_)` means this link
    /// is DONE — Candle already adjudicated it, or Article Reader promoted/rejected a co-mention.
    /// Settled links are excluded from the adjudication set so a re-enqueue never re-pays the
    /// model for a verdict already on disk; they still count toward the novelty gate's scope.
    vetted: Option<bool>,
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
            "SELECT title, COALESCE(description, '') AS description, COALESCE(source, '') AS source FROM news_articles WHERE id = $1",
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
        let source: String = row.get("source");

        let cands = load_candidates(hx, article_id, &sport).await?;
        if cands.is_empty() {
            return Ok(());
        }

        // Force-keep exact links (confidence ≥ 1.0). When the Go funnel has a primary team
        // candidate (0.95), Candle only decides that primary article relevance; lower-confidence
        // co-mentions are preserved for Article Reader's full-text verdict. If an old repair item
        // has no primary-like candidate, keep the old behavior and adjudicate every candidate here.
        let context = crate::novelty::article_text(&title, &description);
        let has_primary_like = cands
            .iter()
            .any(|c| c.confidence >= PRIMARY_TEAM_CONFIDENCE_FLOOR);
        // A settled link (`vetted IS NOT NULL`) is never re-adjudicated. `pipeline_work` rows are
        // DELETED on completion, so the queue's idempotency key vanishes once an article is
        // scrubbed — any later team that adds a link re-enqueues the whole article. Without this
        // filter that re-enqueue re-sent every already-decided link to the model, re-paying GPU
        // for verdicts already on disk. Settled links still feed the novelty gate's scope below.
        let scrub_idxs: Vec<usize> = cands
            .iter()
            .enumerate()
            .filter(|(_, c)| !has_primary_like || c.confidence >= PRIMARY_TEAM_CONFIDENCE_FLOOR)
            .filter(|(_, c)| c.vetted.is_none())
            .map(|(idx, _)| idx)
            .collect();
        let deferred_idxs: Vec<usize> = if has_primary_like {
            cands
                .iter()
                .enumerate()
                .filter(|(_, c)| c.confidence < PRIMARY_TEAM_CONFIDENCE_FLOOR)
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        };

        let secondaries: Vec<Candidate> = scrub_idxs
            .iter()
            .map(|&idx| &cands[idx])
            .filter(|c| c.confidence < 1.0)
            .map(to_candidate)
            .collect();
        let gate = if secondaries.is_empty() {
            crate::resolve::ResolveSetOutcome::default()
        } else {
            hx.resolve_set(Role::EmotionalNews, &context, &secondaries)
                .await
                .context("resolve_set gate")?
        };
        let resolutions = &gate.resolutions;
        let kept_secondary: HashMap<(EntityType, i32), bool> = resolutions
            .iter()
            .map(|r| ((r.entity_type, r.entity_id), r.kept))
            .collect();

        // A verdict for every scrub-owned link: exact links are kept by rule, primary team
        // candidates by Candle (default-drop on a missing verdict). Deferred co-mentions stay
        // vetted=NULL and get only scrubbed_at stamped, so maintenance will not requeue them.
        let entity_types: Vec<String> = scrub_idxs
            .iter()
            .map(|&idx| cands[idx].entity_type.as_str().to_string())
            .collect();
        let entity_ids: Vec<i32> = scrub_idxs.iter().map(|&idx| cands[idx].entity_id).collect();
        let relevants: Vec<bool> = scrub_idxs
            .iter()
            .map(|&idx| {
                let c = &cands[idx];
                c.confidence >= 1.0
                    || kept_secondary
                        .get(&(c.entity_type, c.entity_id))
                        .copied()
                        .unwrap_or(false)
            })
            .collect();
        let deferred_entity_types: Vec<String> = deferred_idxs
            .iter()
            .map(|&idx| cands[idx].entity_type.as_str().to_string())
            .collect();
        let deferred_entity_ids: Vec<i32> = deferred_idxs
            .iter()
            .map(|&idx| cands[idx].entity_id)
            .collect();

        // Admission counts THIS pass's keeps plus any link already settled as kept on a previous
        // pass. Without the second clause a re-enqueue that has nothing new to adjudicate would
        // read as "not admitted" and hand the novelty gate an empty scope, changing dedupe.
        let article_admitted = relevants.iter().any(|&kept| kept)
            || cands.iter().any(|c| c.vetted == Some(true));

        // The vetted membership after this scrub plus deferred co-mentions — the novelty gate's
        // conservative scope. Co-mentions are not consumer-visible until Article Reader promotes
        // them, but including them here avoids suppressing an article that may be unique for a
        // co-mentioned player/team once the full text is read.
        let vetted_entities: Vec<(EntityType, i32)> = if article_admitted {
            cands
                .iter()
                .enumerate()
                .filter_map(|(idx, c)| {
                    if let Some(pos) = scrub_idxs.iter().position(|&scrub_idx| scrub_idx == idx) {
                        relevants[pos].then_some((c.entity_type, c.entity_id))
                    } else if deferred_idxs.contains(&idx) {
                        Some((c.entity_type, c.entity_id))
                    } else {
                        // Settled on an earlier pass (scrub_idxs skipped it): honour the verdict
                        // on disk so the gate's scope is identical to a first-pass scrub.
                        (c.vetted == Some(true)).then_some((c.entity_type, c.entity_id))
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        apply_scrub_outcomes(
            hx,
            article_id,
            &sport,
            &entity_types,
            &entity_ids,
            &relevants,
            &deferred_entity_types,
            &deferred_entity_ids,
        )
        .await?;

        if !deferred_idxs.is_empty() && article_admitted {
            work::enqueue(
                &hx.pool,
                &Item {
                    stage: Stage::ArticleRead,
                    entity_type: "article".to_string(),
                    entity_id: article_id,
                    sport: sport.clone(),
                    input_version: Some(article_read_co_mentions_input_version(
                        &cands,
                        &deferred_idxs,
                    )),
                    attempts: 0,
                },
            )
            .await?;
        }

        // Source-aware novelty gate (Cognition Phase 2): suppress a near-dup repost (same outlet, or
        // near-verbatim syndication) of recent canonical coverage; cross-outlet corroboration passes
        // through. Reuses the relevance gate's context embedding when it ran. Writes `duplicate_of`
        // only — membership is untouched, so no derive re-fires.
        crate::novelty::gate(
            hx,
            crate::novelty::ArticleNovelty {
                article_id,
                sport: &sport,
                source: &source,
                context: &context,
                context_vector: gate.context_vector.as_ref(),
                vetted_entities: &vetted_entities,
            },
        )
        .await
        .context("novelty gate")?;
        Ok(())
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

fn article_read_co_mentions_input_version(cands: &[ScrubCandidate], idxs: &[usize]) -> String {
    let mut parts: Vec<String> = idxs
        .iter()
        .map(|&idx| {
            let c = &cands[idx];
            format!(
                "{}:{}:{}",
                c.entity_type.as_str(),
                c.entity_id,
                c.confidence
            )
        })
        .collect();
    parts.sort();
    format!(
        "arcm:{}:{}",
        parts.len(),
        crate::util::hash_components(&parts.join(","))
    )
}

/// load_candidates returns every entity linked to the article with its identity card — the Rust port
/// of `news_scrub.go::loadCandidates` (current club and position from `player_current_identity`).
/// `match_confidence` is cast `::float8` (sqlx has no numeric→f64 without the
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
               COALESCE(NULLIF(pci.position, 'Unknown'), '') AS position,
               nae.match_confidence::float8                  AS confidence,
               nae.vetted                                    AS vetted
        FROM news_article_entities nae
        LEFT JOIN players p ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN teams   t ON nae.entity_type = 'team'   AND t.id = nae.entity_id AND t.sport = nae.sport
        LEFT JOIN public.player_current_identity pci ON nae.entity_type = 'player' AND pci.player_id = nae.entity_id AND pci.sport = nae.sport
        LEFT JOIN teams ct ON ct.id = pci.team_id AND ct.sport = nae.sport
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
            vetted: r.get("vetted"),
        });
    }
    Ok(out)
}

/// apply_scrub_outcomes records Candle-owned verdicts and marks deferred co-mentions as handed
/// to Article Reader. Kept/rejected primary writes use `vetted`; deferred co-mentions keep
/// `vetted=NULL` but get `scrubbed_at=NOW()` so the maintenance sweep does not treat them as
/// unprocessed Candle work.
async fn apply_scrub_outcomes(
    hx: &Harness,
    article_id: i64,
    sport: &str,
    entity_types: &[String],
    entity_ids: &[i32],
    relevants: &[bool],
    deferred_entity_types: &[String],
    deferred_entity_ids: &[i32],
) -> Result<()> {
    if !entity_types.is_empty() {
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
        .context("apply scrub verdicts")?;
    }
    if !deferred_entity_types.is_empty() {
        sqlx::query(
            r#"
            UPDATE news_article_entities n
               SET scrubbed_at = COALESCE(scrubbed_at, NOW())
              FROM unnest($2::text[], $3::int[]) AS v(entity_type, entity_id)
             WHERE n.article_id = $1 AND n.sport = $4
               AND n.entity_type = v.entity_type AND n.entity_id = v.entity_id
               AND n.vetted IS NULL
            "#,
        )
        .bind(article_id)
        .bind(deferred_entity_types)
        .bind(deferred_entity_ids)
        .bind(sport)
        .execute(&hx.pool)
        .await
        .context("defer co-mentions to article_read")?;
    }
    Ok(())
}

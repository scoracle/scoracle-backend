//! Graph evidence preparation and claim-fenced publication. Model interpretation lives in Studio.
use crate::application::models::Models;
use crate::application::queue::work::{self, Item};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::graph::{
    Assignment, GraphArticle, GraphCandidate, GraphExtraction, GraphPerson, GraphRelation,
    GRAPH_PROMPT_VERSION,
};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Extracted, Generation, GenerationCall, Studio};
use crate::util::hash_components;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

const GRAPH_LEDGER: LedgerSpec = LedgerSpec {
    stage: "graph",
    lens: "graph",
    role: Role::EmotionalNews,
    product_table: "narrative_events",
    output_contract_version: "graph-extraction-v1",
};

/// load_graph_article_context loads one article + its Editor-linked entities as the
/// closed candidate list (players with identity-card descriptors, teams by name) — the
/// shared deterministic prefix of the probe, the eval lens, and the stage handler.
/// `Ok(None)` when the article is missing or has no linked entities (nothing to extract
/// against — the fail-closed empty path).
pub async fn load_graph_article_context(
    pool: &PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Option<(GraphArticle, Vec<GraphCandidate>)>> {
    // `duplicate_of IS NULL` makes a stale queue row or a
    // hand-enqueued repair fall through the same `Ok(None)` path as a missing article rather than
    // spending a model call on something the dedup sweep already suppressed.
    // The article's context text prefers the Editor's evidence blurb and falls back to the
    // RSS description, which is usually title-adjacent duplication.
    let row = sqlx::query(
        r#"
        SELECT COALESCE(a.source, 'unknown'), a.published_at::date::text, a.title,
               COALESCE(
                   NULLIF(TRIM(er.read ->> 'evidence_blurb'), ''),
                   a.description,
                   ''
               )
        FROM news_articles a
        LEFT JOIN editor_reads er
               ON er.article_id = a.id AND er.status = 'success'
        WHERE a.id = $1 AND a.duplicate_of IS NULL
        "#,
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await
    .context("load graph article")?;
    let Some(row) = row else { return Ok(None) };
    let article = GraphArticle {
        source: row.get(0),
        published: row.get::<Option<String>, _>(1).unwrap_or_default(),
        title: row.get(2),
        description: row.get(3),
    };

    let cand_rows = sqlx::query(
        r#"
        SELECT e.entity_type, e.entity_id,
               COALESCE(p.name, t.name, pp.full_name, '?') AS name,
               COALESCE(ct.name, '') AS current_club
        FROM news_article_entities e
        LEFT JOIN players p ON e.entity_type='player' AND p.id=e.entity_id AND p.sport=e.sport
        LEFT JOIN teams t ON e.entity_type='team' AND t.id=e.entity_id AND t.sport=e.sport
        LEFT JOIN persons pp ON e.entity_type='person' AND pp.id=e.entity_id AND pp.sport=e.sport
        LEFT JOIN player_current_identity pci
               ON e.entity_type='player' AND pci.player_id=e.entity_id AND pci.sport=e.sport
        LEFT JOIN teams ct ON ct.id=pci.team_id AND ct.sport=e.sport
        WHERE e.article_id=$1 AND e.sport=$2
        ORDER BY e.entity_type, e.entity_id
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .context("load graph candidates")?;
    if cand_rows.is_empty() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for r in cand_rows {
        let entity_type: String = r.get(0);
        let entity_id: i32 = r.get(1);
        let name: String = r.get(2);
        let descriptor =
            crate::evidence::memories::load_identity_record(pool, &entity_type, entity_id, sport)
                .await?
                .unwrap_or_else(|| format!("{name} ({entity_type}; records unavailable)"));
        candidates.push(GraphCandidate {
            entity_type,
            entity_id,
            descriptor,
        });
    }
    Ok(Some((article, candidates)))
}

/// build_graph_input_components is the canonical debounce pre-image: the article's
/// material text plus the sorted vetted-candidate identity list. Same canonical-JSON
/// discipline as the other stages; hashed into `graph_extractions.input_hash`.
pub fn build_graph_input_components(
    article: &GraphArticle,
    candidates: &[GraphCandidate],
) -> String {
    let mut cands: Vec<String> = candidates
        .iter()
        .map(|c| format!("{}:{}", c.entity_type, c.entity_id))
        .collect();
    cands.sort();
    serde_json::json!({
        "candidates": cands,
        "description": article.description,
        "title": article.title,
    })
    .to_string()
}

/// GraphHandler drains the durable `graph` stage: load the article + vetted candidates,
/// debounce on the material hash (bookkeeping row in `graph_extractions`), extract, and
/// write `narrative_events` plus person candidates with
/// idempotent evidence accumulation (the mention PK makes re-extraction a no-op bump).
/// Fail-closed replies record a `failed_closed` bookkeeping row — same material never
/// re-hammers the GPU — and write no events.
pub struct GraphHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

impl GraphHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
        Self { pool, models }
    }
}

enum Prepared {
    Unchanged,
    Read {
        input_hash: String,
        extracted: Box<Extracted<GraphExtraction>>,
    },
}

async fn prepare(pool: &sqlx::PgPool, models: &Models, item: &Item) -> Result<Prepared> {
    ensure!(item.entity_type == "article", "graph requires an article");
    let article_id = item.entity_id;
    let sport = item.sport.to_uppercase();
    let Some((article, candidates)) = load_graph_article_context(pool, article_id, &sport).await?
    else {
        debug!(article_id, sport = %sport, "graph: no article or no vetted candidates");
        return Ok(Prepared::Unchanged);
    };

    let input_components = build_graph_input_components(&article, &candidates);
    let input_hash = hash_components(&input_components);
    // Debounce on the bookkeeping row: same material AND same prompt contract ⇒ the
    // extraction already ran (including a fail-closed one — never re-hammer the GPU
    // on identical bytes). A prompt bump re-extracts everything once, like rating.
    if read_is_current(pool, article_id, &input_hash).await? {
        debug!(
            article_id,
            "graph: debounce-skip, material + contract unchanged"
        );
        return Ok(Prepared::Unchanged);
    }

    let model = models.router.for_role(Role::EmotionalNews);
    let extracted = Studio::new(model.as_ref())
        .extract_graph(&Assignment {
            article,
            candidates,
        })
        .await?;
    Ok(match extracted {
        Some(extracted) => Prepared::Read {
            input_hash,
            extracted: Box::new(extracted),
        },
        None => Prepared::Unchanged,
    })
}

#[async_trait]
impl StudioPlugin for GraphHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::studio::fleet::GRAPH
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let prepared = prepare(pool, models, item).await?;
        commit_claimed(pool, item, &prepared).await
    }
}

/// Product rows carry required article/model/contract provenance. Diagnostic generation
/// logging is best-effort after commit and can never change the completion result.
async fn commit_claimed(pool: &PgPool, item: &Item, prepared: &Prepared) -> Result<PluginOutcome> {
    let mut tx = pool.begin().await?;
    if !work::lock_claim(&mut tx, item).await? {
        tx.rollback().await?;
        return Ok(PluginOutcome::Superseded);
    }
    let mut event_ids = Vec::new();
    if let Prepared::Read {
        input_hash,
        extracted,
    } = prepared
    {
        let article_id = item.entity_id;
        let sport = item.sport.to_uppercase();
        let model = &extracted.model;
        let (outcome, relations, persons) = extraction_parts(extracted);
        for relation in relations {
            event_ids.push(upsert_event(&mut tx, article_id, &sport, model, relation).await?);
        }
        for person in persons {
            accumulate_person(&mut tx, article_id, &sport, model, person).await?;
        }
        sqlx::query(
            r#"
            INSERT INTO graph_extractions
                (article_id, sport, input_hash, prompt_version, model_version,
                 parser_outcome, relations_n, persons_n, extracted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT (article_id) DO UPDATE SET
                sport = EXCLUDED.sport, input_hash = EXCLUDED.input_hash,
                prompt_version = EXCLUDED.prompt_version,
                model_version = EXCLUDED.model_version,
                parser_outcome = EXCLUDED.parser_outcome,
                relations_n = EXCLUDED.relations_n, persons_n = EXCLUDED.persons_n,
                extracted_at = NOW()
            "#,
        )
        .bind(article_id)
        .bind(&sport)
        .bind(input_hash)
        .bind(GRAPH_PROMPT_VERSION)
        .bind(model)
        .bind(outcome)
        .bind(relations.len() as i32)
        .bind(persons.len() as i32)
        .execute(&mut *tx)
        .await
        .context("upsert graph_extractions")?;
    }
    ensure!(
        work::complete_in_transaction(&mut tx, item).await?,
        "graph claim changed during publication"
    );
    tx.commit().await?;
    if let Prepared::Read {
        input_hash,
        extracted,
    } = prepared
    {
        record_diagnostics(pool, item, input_hash, extracted, event_ids).await;
    }
    Ok(PluginOutcome::Committed)
}

fn extraction_parts(
    extracted: &Extracted<GraphExtraction>,
) -> (&'static str, &[GraphRelation], &[GraphPerson]) {
    match &extracted.value {
        None => ("failed_closed", &[], &[]),
        Some(g) => ("extracted", &g.relations, &g.persons),
    }
}

async fn record_diagnostics(
    pool: &PgPool,
    item: &Item,
    input_hash: &str,
    extracted: &Extracted<GraphExtraction>,
    event_ids: Vec<i64>,
) {
    // Diagnostics are optional and must not turn a committed publication into a retry.
    let Ok(entity_id_i32) = i32::try_from(item.entity_id) else {
        return;
    };
    let article_id = item.entity_id;
    let sport = item.sport.to_uppercase();
    let model = extracted.model.clone();
    let (outcome, relations, persons) = extraction_parts(extracted);
    let generation = Generation::called(
        (),
        model,
        GRAPH_PROMPT_VERSION,
        vec![article_id],
        Some(input_hash.to_string()),
        GenerationCall::from(extracted),
    );
    insert_generation_ledger_best_effort(
            pool,
            &generation,
            GRAPH_LEDGER,
            LedgerEvent {
                entity_type: "article",
                entity_id: entity_id_i32,
                sport: &sport,
                pair_entity: None,
                trigger_type: "periodic",
                trigger_payload: serde_json::json!({}),
                product_row_ids: event_ids,
                included_evidence: serde_json::json!({
                    "relations_n": relations.len(),
                    "persons": persons.iter().map(|p| format!("{} [{}]", p.name, p.kind)).collect::<Vec<_>>(),
                }),
                excluded_evidence: serde_json::json!({
                    "parser_outcome": outcome,
                }),
                context_budget: generation.context_budget(serde_json::json!({
                    "num_predict": 768,
                })),
                parser_outcome: outcome,
            },
        )
        .await;
}

async fn upsert_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    article_id: i64,
    sport: &str,
    model: &str,
    r: &GraphRelation,
) -> Result<i64> {
    let row = sqlx::query(
        r#"
        INSERT INTO narrative_events
            (sport, subject_type, subject_id, predicate, object_type, object_id,
             sentiment, confidence, article_id, event_date, source, model_version,
             prompt_version, origin)
        SELECT $1, $2, $3, $4, $5, $6, $7::float8::numeric(3,2), $8, $9,
               COALESCE(a.published_at, NOW()), a.source, $10, $11, 'extraction'
        FROM news_articles a WHERE a.id = $9
        -- Origin joins the dedupe key so an extraction event and a junction
        -- verdict for the same (article, pair, predicate) coexist, never clobber.
        ON CONFLICT (article_id, sport, subject_type, subject_id, predicate,
                     COALESCE(object_type, ''), COALESCE(object_id, 0), origin)
        DO UPDATE SET sentiment = EXCLUDED.sentiment, confidence = EXCLUDED.confidence,
                      model_version = EXCLUDED.model_version,
                      prompt_version = EXCLUDED.prompt_version, extracted_at = NOW()
        RETURNING id
        "#,
    )
    .bind(sport)
    .bind(&r.subject_type)
    .bind(r.subject_id)
    .bind(&r.predicate)
    .bind(r.object_type.as_deref())
    .bind(r.object_id)
    .bind(r.sentiment)
    .bind(&r.confidence)
    .bind(article_id)
    .bind(model)
    .bind(GRAPH_PROMPT_VERSION)
    .fetch_one(&mut **tx)
    .await
    .context("upsert narrative_event")?;
    Ok(row.get("id"))
}

/// accumulate_person resolves-or-creates the candidate person and, ONLY when this
/// article is a NEW mention (the mention PK), bumps the evidence counters. Same-name
/// active rows win the resolve; provider-dupe merges stay a later data-layer pass.
async fn accumulate_person(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    article_id: i64,
    sport: &str,
    model: &str,
    p: &GraphPerson,
) -> Result<i32> {
    let person_id: i32 = sqlx::query_scalar(
        r#"
        WITH existing AS (
            SELECT id FROM narrative_persons
            WHERE sport = $1 AND lower(name) = lower($2) AND merged_into IS NULL
            ORDER BY (status = 'active') DESC, mention_count DESC
            LIMIT 1
        ), ins AS (
            INSERT INTO narrative_persons
                (sport, kind, name, team_id, status, first_seen_at, last_seen_at, model_version)
            SELECT $1, $3, $2, $4, 'candidate',
                   COALESCE((SELECT published_at FROM news_articles WHERE id = $5), NOW()),
                   COALESCE((SELECT published_at FROM news_articles WHERE id = $5), NOW()),
                   $6
            WHERE NOT EXISTS (SELECT 1 FROM existing)
            RETURNING id
        )
        SELECT id FROM existing UNION ALL SELECT id FROM ins
        "#,
    )
    .bind(sport)
    .bind(&p.name)
    .bind(&p.kind)
    .bind(p.team_context_id)
    .bind(article_id)
    .bind(model)
    .fetch_one(&mut **tx)
    .await
    .context("resolve/insert narrative_person")?;

    let new_mention: Option<i32> = sqlx::query_scalar(
        "INSERT INTO narrative_person_mentions (article_id, person_id, sport, team_context_id)
         VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING RETURNING person_id",
    )
    .bind(article_id)
    .bind(person_id)
    .bind(sport)
    // Promotion aggregates these per-mention team votes
    // for the team-token consistency gate. NULL when this article tied the person to no
    // listed team.
    .bind(p.team_context_id)
    .fetch_optional(&mut **tx)
    .await
    .context("insert person mention")?;

    if new_mention.is_some() {
        // Counter bump only on a NEW mention (idempotent re-extraction). The
        // distinct_sources recount runs AFTER the mention insert committed its
        // statement, so the fresh row is visible.
        sqlx::query(
            r#"
            UPDATE narrative_persons p SET
                mention_count = p.mention_count + 1,
                distinct_sources = (
                    SELECT count(DISTINCT a.source)
                    FROM narrative_person_mentions m
                    JOIN news_articles a ON a.id = m.article_id
                    WHERE m.person_id = p.id AND a.source IS NOT NULL),
                last_seen_at = GREATEST(
                    COALESCE(p.last_seen_at, to_timestamp(0)),
                    COALESCE((SELECT published_at FROM news_articles WHERE id = $2), NOW())),
                team_id = COALESCE(p.team_id, $3),
                updated_at = NOW()
            WHERE p.id = $1
            "#,
        )
        .bind(person_id)
        .bind(article_id)
        .bind(p.team_context_id)
        .execute(&mut **tx)
        .await
        .context("bump person evidence")?;
    }
    Ok(person_id)
}

#[cfg(test)]
mod tests;

async fn read_is_current(pool: &PgPool, article_id: i64, input_hash: &str) -> Result<bool> {
    let prior: Option<(String, String)> = sqlx::query_as(
        "SELECT input_hash, prompt_version FROM graph_extractions WHERE article_id = $1",
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await
    .context("graph debounce read")?;
    Ok(prior
        .as_ref()
        .is_some_and(|(h, v)| h == input_hash && v == GRAPH_PROMPT_VERSION))
}

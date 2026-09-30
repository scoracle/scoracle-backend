//! Graph evidence preparation and claim-fenced publication. Model interpretation lives in Studio.
use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work;
use crate::application::queue::work::Item;
use crate::evidence::news::slice_quote;
use crate::plugins::graph::cognition::{
    Assignment, GraphArticle, GraphCandidate, GraphExtraction, GraphPerson, GraphRelation,
    GRAPH_PROMPT_VERSION,
};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Extracted, Generation, GenerationCall, Studio};
use crate::util::hash_components;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tracing::debug;

mod fixture;

const GRAPH_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::graph::manifest::MANIFEST.id.as_str(),
    stage: "graph",
    lens: "graph",
    role: crate::plugins::graph::manifest::ROUTE,
    product_table: "narrative_events",
    output_contract_version: "graph-extraction-v1",
};

/// load_graph_article_context loads one article plus resolved links and Harvester
/// name-match candidates (players with identity-card descriptors, teams by name) — the
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
    // Exact publisher context wins for Harvester articles; historical Editor
    // evidence remains readable until its old claims have drained.
    let row = sqlx::query(
        r#"
        SELECT COALESCE(a.source, 'unknown'), a.published_at::date::text,
               COALESCE(h.headline, a.title),
               COALESCE(
                   h.context_text,
                   NULLIF(TRIM(er.read ->> 'evidence_blurb'), ''),
                   a.description,
                   ''
               ), h.body_sha256, h.context_start, h.context_end, a.full_text,
               a.title
        FROM news_articles a
        LEFT JOIN LATERAL (
            SELECT c.headline, c.context_text, c.body_sha256,
                   c.context_start, c.context_end
            FROM public.harvester_classifications c
            WHERE c.article_id=a.id AND c.sport=$2
            ORDER BY c.created_at DESC, c.id DESC
            LIMIT 1
        ) h ON true
        LEFT JOIN editor_reads er
               ON er.article_id = a.id AND er.status = 'success'
        WHERE a.id = $1 AND a.duplicate_of IS NULL
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .context("load graph article")?;
    let Some(row) = row else { return Ok(None) };
    let context_hash: Option<String> = row.get(4);
    if let Some(hash) = context_hash {
        let body: String = row
            .get::<Option<String>, _>(7)
            .context("Graph Harvester article has no retained publisher body")?;
        let start: i32 = row.get(5);
        let end: i32 = row.get(6);
        let source_text: String = row.get(3);
        let headline: String = row.get(2);
        let title: String = row.get(8);
        ensure!(
            start >= 0
                && end >= start
                && hex::encode(Sha256::digest(body.as_bytes())) == hash
                && body.get(start as usize..end as usize) == Some(source_text.as_str())
                && headline == title,
            "Graph Harvester source hash or byte range drift"
        );
    }
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
        FROM (
            SELECT article_id, entity_type, entity_id, sport
              FROM public.news_article_entities
             WHERE article_id=$1 AND sport=$2
            UNION
            SELECT article_id, entity_type, entity_id, sport
              FROM public.harvester_entity_mentions
             WHERE article_id=$1 AND sport=$2
        ) e
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
    models: ExecutionCapabilities,
}

impl GraphHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
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

async fn prepare(
    pool: &sqlx::PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<Prepared> {
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

    let model = models.inference(crate::plugins::graph::manifest::ROUTE)?;
    let extracted = crate::plugins::graph::cognition::extract_graph(
        &Studio::new(model.as_ref()),
        &Assignment {
            article,
            candidates,
        },
    )
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
        &crate::plugins::graph::manifest::MANIFEST
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
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
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
            event_ids.push(
                upsert_event(
                    publication.transaction(),
                    article_id,
                    &sport,
                    model,
                    relation,
                )
                .await?,
            );
        }
        for person in persons {
            accumulate_person(publication.transaction(), article_id, &sport, model, person).await?;
            nominate_harvester_person(publication.transaction(), article_id, &sport, person)
                .await?;
        }
        if let Some(graph) = extracted.value.as_ref() {
            fixture::review(
                &mut **publication.transaction(),
                article_id,
                &sport,
                input_hash,
                model,
                &graph.final_result_line,
            )
            .await?;
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
        .execute(&mut **publication.transaction())
        .await
        .context("upsert graph_extractions")?;
    }
    publication.commit_final().await?;
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

/// Graph owns the unknown-person handoff for Harvester articles. A model-suggested
/// name only becomes an Investigator candidate when it is independently anchored
/// to the exact publisher headline or hash-verified opening. This is an investigation request,
/// never an authoritative article/entity link.
async fn nominate_harvester_person(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    article_id: i64,
    sport: &str,
    person: &GraphPerson,
) -> Result<()> {
    let row = sqlx::query(
        "SELECT c.context_text,c.context_start,c.context_end,c.body_sha256,a.full_text, \
                c.headline,a.title, \
                strpos(' ' || public.nrm(c.context_text) || ' ', \
                       ' ' || public.nrm($3) || ' ') > 0 AS name_in_opening, \
                strpos(' ' || public.nrm(c.headline) || ' ', \
                       ' ' || public.nrm($3) || ' ') > 0 AS name_in_headline \
         FROM public.harvester_classifications c \
         JOIN public.news_articles a ON a.id=c.article_id \
         WHERE c.article_id=$1 AND c.sport=$2 \
         ORDER BY c.created_at DESC,c.id DESC LIMIT 1",
    )
    .bind(article_id)
    .bind(sport)
    .bind(&person.name)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let opening: String = row.get("context_text");
    let body: String = row
        .get::<Option<String>, _>("full_text")
        .context("Harvester Graph nomination has no retained publisher body")?;
    let start: i32 = row.get("context_start");
    let end: i32 = row.get("context_end");
    let hash: String = row.get("body_sha256");
    let headline: String = row.get("headline");
    let article_title: String = row.get("title");
    ensure!(
        start >= 0
            && end >= start
            && hex::encode(Sha256::digest(body.as_bytes())) == hash
            && body.get(start as usize..end as usize) == Some(opening.as_str())
            && headline == article_title,
        "Harvester Graph nomination source hash or byte range drift"
    );
    let source_span = if row.get::<bool, _>("name_in_opening") {
        &opening
    } else if row.get::<bool, _>("name_in_headline") {
        &headline
    } else {
        return Ok(());
    };
    let Some(quote) = slice_quote(source_span, &person.name) else {
        return Ok(());
    };
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM public.entity_name_surfaces \
         WHERE sport=$1 AND norm=public.nrm($2))",
    )
    .bind(sport)
    .bind(&person.name)
    .fetch_one(&mut **tx)
    .await?;
    if known {
        return Ok(());
    }
    let candidate: (i64, String, i32) = sqlx::query_as(
        "INSERT INTO public.entity_candidates \
         (idempotency_key,norm_name,kind_hint,sport,state,first_seen_at,last_seen_at) \
         VALUES(lower($1) || ':' || public.nrm($2),public.nrm($2),'person',$1,'pending',NOW(),NOW()) \
         ON CONFLICT (idempotency_key) DO UPDATE SET \
           last_seen_at=NOW(), \
           state=CASE WHEN public.entity_candidates.state NOT IN ('pending','accepted') \
                       AND public.entity_candidates.decided_at < NOW()-interval '30 days' \
                      THEN 'pending' ELSE public.entity_candidates.state END \
         RETURNING id,state,mention_count",
    )
    .bind(sport)
    .bind(&person.name)
    .fetch_one(&mut **tx)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO public.candidate_mentions(candidate_id,article_id,quote,observed_at) \
         VALUES($1,$2,$3,NOW()) ON CONFLICT (candidate_id,article_id) DO NOTHING",
    )
    .bind(candidate.0)
    .bind(article_id)
    .bind(quote)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let mention_count = if inserted {
        sqlx::query_scalar::<_, i32>(
            "UPDATE public.entity_candidates SET mention_count=mention_count+1 WHERE id=$1 \
             RETURNING mention_count",
        )
        .bind(candidate.0)
        .fetch_one(&mut **tx)
        .await?
    } else {
        candidate.2
    };
    if candidate.1 == "pending" && (person.team_context_id.is_some() || mention_count >= 2) {
        work::enqueue(
            &mut **tx,
            &Item {
                stage: crate::plugins::investigator::manifest::TASK,
                entity_type: "candidate".into(),
                entity_id: candidate.0,
                sport: sport.into(),
                input_version: None,
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
    }
    Ok(())
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

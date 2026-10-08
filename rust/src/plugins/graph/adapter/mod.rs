//! Source-bound investigation nominations and fixture review receipts.
//! Generated relations and roles never become canonical facts.
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work;
use crate::harness::queue::work::Item;
use crate::harness::{Extracted, Generation, GenerationCall, Studio};
use crate::plugins::graph::cognition::{
    Assignment, GraphArticle, GraphCandidate, GraphExtraction, GraphPerson, GRAPH_PROMPT_VERSION,
};
use crate::plugins::graph::quote::slice_quote;
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
    product_table: "graph_extractions",
    output_contract_version: "graph-extraction-v1",
};

/// Read complete, current native acquisition; model measurements grant no factual authority.
async fn load_source(
    conn: &mut sqlx::PgConnection,
    article_id: i64,
    sport: &str,
) -> Result<Option<crate::plugins::classifier::Source>> {
    let row = sqlx::query("SELECT s.source::text,s.body_sha256 FROM public.classifier_sources s JOIN public.news_articles a ON a.id=s.article_id WHERE s.article_id=$1 AND s.sport=$2 AND a.duplicate_of IS NULL AND s.discovery_version=public.classifier_discovery_version($1,$2) ORDER BY s.id DESC LIMIT 1")
        .bind(article_id).bind(sport).fetch_optional(&mut *conn).await?;
    let Some(row) = row else { return Ok(None) };
    let source: crate::plugins::classifier::Source = serde_json::from_str(row.get(0))?;
    ensure!(
        source.article_id == article_id
            && !source.body.trim().is_empty()
            && hex::encode(Sha256::digest(source.body.as_bytes())) == row.get::<String, _>(1),
        "Graph source snapshot hash mismatch"
    );
    ensure!(
        crate::plugins::classifier::identity::candidates(conn, sport, &source).await?
            == source.provenance["identity_candidates"],
        "Graph canonical candidates changed"
    );
    Ok(Some(source))
}

pub async fn load_graph_article_context(
    pool: &PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Option<(GraphArticle, Vec<GraphCandidate>)>> {
    let source = load_source(&mut *pool.acquire().await?, article_id, sport)
        .await?
        .context("Graph requires current complete Classifier acquisition")?;
    let article = GraphArticle {
        source: source.source.clone(),
        published: source.published_at.clone().unwrap_or_default(),
        title: source.provenance["title"]
            .as_str()
            .unwrap_or_default()
            .into(),
        description: source.body.clone(),
    };
    let mut candidates = Vec::new();
    for c in source.provenance["identity_candidates"]
        .as_array()
        .context("Graph native identities")?
    {
        let entity_type = c["entity_type"]
            .as_str()
            .context("Graph candidate kind")?
            .to_owned();
        let entity_id = i32::try_from(c["entity_id"].as_i64().context("Graph candidate ID")?)?;
        let descriptor =
            crate::tools::meta::load_identity_record(pool, &entity_type, entity_id, sport)
                .await?
                .context("Graph canonical identity disappeared")?;
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
        .map(|c| format!("{}:{}:{}", c.entity_type, c.entity_id, c.descriptor))
        .collect();
    cands.sort();
    serde_json::json!({
        "candidates": cands,
        "description": article.description,
        "title": article.title,
        "source": article.source,
        "published": article.published,
    })
    .to_string()
}

/// Drain source-bound nominations and review receipts; generated relations never
/// become canonical facts. Publication rechecks source material and the work lease.
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
    if let Prepared::Read {
        input_hash,
        extracted,
    } = prepared
    {
        let article_id = item.entity_id;
        let sport = item.sport.to_uppercase();
        let model = &extracted.model;
        // Hold the source stable and revalidate after inference, before any side effect.
        sqlx::query("SELECT id FROM news_articles WHERE id=$1 FOR SHARE NOWAIT")
            .bind(article_id)
            .fetch_optional(&mut **publication.transaction())
            .await?;
        sqlx::query("SELECT p.article_id FROM public.harvester_query_provenance p JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport WHERE p.article_id=$1 AND p.sport=$2 FOR SHARE OF p,t NOWAIT")
            .bind(article_id).bind(&sport).fetch_all(&mut **publication.transaction()).await?;
        sqlx::query("SELECT id FROM public.classifier_sources WHERE article_id=$1 AND sport=$2 FOR SHARE NOWAIT")
            .bind(article_id).bind(&sport).fetch_all(&mut **publication.transaction()).await?;
        let source = load_source(&mut **publication.transaction(), article_id, &sport)
            .await?
            .context("Graph source no longer current")?;
        crate::plugins::classifier::identity::validate(publication.transaction(), &sport, &source)
            .await?;
        let current = load_graph_article_context(pool, article_id, &sport)
            .await?
            .context("Graph source is no longer eligible")?;
        ensure!(
            hash_components(&build_graph_input_components(&current.0, &current.1)) == *input_hash,
            "Graph source or candidate material changed during inference"
        );
        let (outcome, persons) = extraction_parts(extracted);
        for person in persons {
            nominate_person(publication.transaction(), article_id, &sport, person).await?;
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
        .bind(0_i32)
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
        record_diagnostics(pool, item, input_hash, extracted).await;
    }
    Ok(PluginOutcome::Committed)
}

fn extraction_parts(extracted: &Extracted<GraphExtraction>) -> (&'static str, &[GraphPerson]) {
    match &extracted.value {
        None => ("failed_closed", &[]),
        Some(g) => ("extracted", &g.persons),
    }
}

async fn record_diagnostics(
    pool: &PgPool,
    item: &Item,
    input_hash: &str,
    extracted: &Extracted<GraphExtraction>,
) {
    // Diagnostics are optional and must not turn a committed publication into a retry.
    let Ok(entity_id_i32) = i32::try_from(item.entity_id) else {
        return;
    };
    let article_id = item.entity_id;
    let sport = item.sport.to_uppercase();
    let model = extracted.model.clone();
    let (outcome, persons) = extraction_parts(extracted);
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
                product_row_ids: vec![],
                included_evidence: serde_json::json!({
                    "relations_n": 0,
                    "persons": persons.iter().map(|p| format!("{} [{}]", p.name, p.kind)).collect::<Vec<_>>(),
                }),
                excluded_evidence: serde_json::json!({
                    "parser_outcome": outcome,
                    "relations": "unavailable: no evaluated relation extractor",
                }),
                context_budget: generation.context_budget(serde_json::json!({
                    "num_predict": 768,
                })),
                parser_outcome: outcome,
            },
        )
        .await;
}

/// Exact source names become investigation requests, never authoritative identities.
async fn nominate_person(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    article_id: i64,
    sport: &str,
    person: &GraphPerson,
) -> Result<()> {
    let source = load_source(&mut **tx, article_id, sport)
        .await?
        .context("Graph nomination source disappeared")?;
    let headline = source.provenance["title"].as_str().unwrap_or_default();
    let mut quote = None;
    for text in [&source.body, headline] {
        let matches: bool = sqlx::query_scalar(
            "SELECT strpos(' ' || public.nrm($1) || ' ', ' ' || public.nrm($2) || ' ') > 0",
        )
        .bind(text)
        .bind(&person.name)
        .fetch_one(&mut **tx)
        .await?;
        if matches {
            quote = slice_quote(text, &person.name);
            if quote.is_some() {
                break;
            }
        }
    }
    let Some(quote) = quote else { return Ok(()) };
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
    if inserted {
        sqlx::query_scalar::<_, i32>(
            "UPDATE public.entity_candidates SET mention_count=mention_count+1 WHERE id=$1 \
             RETURNING mention_count",
        )
        .bind(candidate.0)
        .fetch_one(&mut **tx)
        .await?;
    }
    if candidate.1 == "pending" {
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

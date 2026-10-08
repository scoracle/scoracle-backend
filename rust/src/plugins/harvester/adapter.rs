//! Claim-fenced acquisition and source-context publication for Harvester.
//! Character delivery is a separate cutover step; pending assignments remain
//! queryable until a character adapter owns their final disposition.
use super::cognition::Article;
use super::context::{self, HarvestContext, HeadlineGate};
use super::policy::CHARACTER_ROUTES;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::tools::WebBroker;
use crate::plugins::harvester::decision::DecisionModel;
use crate::tools::fetch::{count_words, domain_of, ArticleHttpStatus, FetchedArticle};
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashSet;
use std::sync::Arc;

pub const DELIVERY_HELD_REASON: &str = "delivery_held";

fn parse_delivery_characters(raw: &str) -> Result<HashSet<&'static str>> {
    let mut enabled = HashSet::new();
    for name in raw
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let route = CHARACTER_ROUTES
            .iter()
            .find(|route| route.delivery_name == name)
            .ok_or_else(|| {
                anyhow::anyhow!("unknown HARVESTER_DELIVERY_CHARACTERS name {name:?}")
            })?;
        ensure!(
            enabled.insert(route.delivery_name),
            "duplicate Harvester delivery character {name:?}"
        );
    }
    Ok(enabled)
}

fn delivery_characters() -> Result<HashSet<&'static str>> {
    match std::env::var("HARVESTER_DELIVERY_CHARACTERS") {
        Ok(raw) => parse_delivery_characters(&raw),
        Err(std::env::VarError::NotPresent) => Ok(HashSet::new()),
        Err(error) => Err(error.into()),
    }
}

pub struct HarvesterHandler {
    pool: PgPool,
    model: Arc<dyn DecisionModel>,
}

struct Source {
    url: String,
    publisher_url: Option<String>,
    title: String,
    source: String,
    published_at: Option<String>,
    retained_body: Option<String>,
    duplicate_of: Option<i64>,
}

struct Query {
    entity_type: String,
    entity_id: i32,
    name: String,
    sport: String,
    feed_rank: Option<i32>,
}

impl HarvesterHandler {
    pub fn new(pool: PgPool, model: Arc<dyn DecisionModel>) -> Self {
        Self { pool, model }
    }
}

async fn load_source(pool: &PgPool, article_id: i64) -> Result<Option<Source>> {
    let row = sqlx::query(
        "SELECT a.url, h.final_url AS publisher_url, a.title, COALESCE(a.source, '') AS source, \
         a.published_at::text AS published_at, \
         a.full_text, a.duplicate_of \
         FROM public.news_articles a LEFT JOIN public.harvester_acquisitions h ON h.article_id=a.id \
         WHERE a.id = $1",
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await
    .context("load Harvester article")?;
    Ok(row.map(|r| Source {
        url: r.get("url"),
        publisher_url: r.get("publisher_url"),
        title: r.get("title"),
        source: r.get("source"),
        published_at: r.get("published_at"),
        retained_body: r.get("full_text"),
        duplicate_of: r.get("duplicate_of"),
    }))
}

async fn load_queries(pool: &PgPool, article_id: i64, sport: &str) -> Result<Vec<Query>> {
    let rows = sqlx::query(
        "SELECT p.entity_type, p.entity_id, p.sport, p.feed_rank, t.name \
         FROM public.harvester_query_provenance p \
         LEFT JOIN public.teams t ON p.entity_type = 'team' AND t.id = p.entity_id AND t.sport = p.sport \
         WHERE p.article_id = $1 AND p.sport = $2 \
         ORDER BY p.entity_type, p.entity_id",
    )
    .bind(article_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .context("load all Google query entities")?;
    let mut queries = Vec::new();
    for row in rows {
        let entity_type: String = row.get("entity_type");
        ensure!(
            entity_type == "team",
            "unsupported Harvester query entity type"
        );
        let name: Option<String> = row.get("name");
        let name = name.context("query team is missing from teams")?;
        ensure!(!name.trim().is_empty(), "query team has no name");
        queries.push(Query {
            entity_type,
            entity_id: row.get("entity_id"),
            name,
            sport: row.get("sport"),
            feed_rank: row.get("feed_rank"),
        });
    }
    ensure!(
        !queries.is_empty(),
        "Harvester article has no query provenance"
    );
    Ok(queries)
}

fn article(source: &Source, query: &Query, article_id: i64, body: &str) -> Article {
    Article {
        article_id,
        title: source.title.clone(),
        source: source.source.clone(),
        url: source.url.clone(),
        published_at: source.published_at.clone(),
        feed_rank: query.feed_rank,
        body: body.to_owned(),
        hypothesis: EntityMeta {
            name: query.name.clone(),
            entity_type: query.entity_type.clone(),
            entity_id: query.entity_id,
            sport: query.sport.clone(),
        },
    }
}

fn clean_body(body: &str) -> String {
    body.replace('\0', "")
}

fn has_usable_paragraph_opening(body: &str) -> bool {
    body.contains("\n\n") && count_words(&super::cognition::first_paragraphs(body, 3).text) >= 30
}

async fn record_acquisition(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    status: &str,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    body: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let hash = body.map(|s| hex::encode(Sha256::digest(s.as_bytes())));
    let bytes = body.map(|s| i32::try_from(s.len())).transpose()?;
    sqlx::query(
        "INSERT INTO public.harvester_acquisitions \
         (article_id, status, final_url, final_domain, body_sha256, body_bytes, attempts, last_error) \
         VALUES ($1,$2,$3,$4,$5,$6,1,$7) \
         ON CONFLICT (article_id) DO UPDATE SET \
         status=EXCLUDED.status, final_url=COALESCE(EXCLUDED.final_url,harvester_acquisitions.final_url), \
         final_domain=COALESCE(EXCLUDED.final_domain,harvester_acquisitions.final_domain), \
         body_sha256=COALESCE(EXCLUDED.body_sha256,harvester_acquisitions.body_sha256), \
         body_bytes=COALESCE(EXCLUDED.body_bytes,harvester_acquisitions.body_bytes), \
         attempts=harvester_acquisitions.attempts+1, last_error=EXCLUDED.last_error, updated_at=NOW()",
    )
    .bind(article_id)
    .bind(status)
    .bind(final_url)
    .bind(final_domain)
    .bind(hash)
    .bind(bytes)
    .bind(error)
    .execute(&mut **tx)
    .await
    .context("store Harvester acquisition state")?;
    Ok(())
}

async fn store_headline_gates(
    tx: &mut Transaction<'_, Postgres>,
    source: &Source,
    article_id: i64,
    gates: &[HeadlineGate],
) -> Result<()> {
    for gate in gates {
        ensure!(
            gate.article_id == article_id && gate.headline == source.title,
            "headline gate source changed"
        );
        sqlx::query(
            "INSERT INTO public.harvester_headline_gates \
             (article_id,entity_type,entity_id,sport,contract_version,headline,input_hash, \
              model_revision,choice,admitted,policy_version,read_threshold, \
              request,answer,model_provenance,raw_response) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
             ON CONFLICT DO NOTHING",
        )
        .bind(article_id)
        .bind(&gate.hypothesis.entity_type)
        .bind(gate.hypothesis.entity_id)
        .bind(&gate.hypothesis.sport)
        .bind(context::HEADLINE_CONTRACT)
        .bind(&gate.headline)
        .bind(&gate.input_hash)
        .bind(&gate.model_revision)
        .bind(if gate.relevance_probability() >= 0.5 {
            "relevant"
        } else {
            "irrelevant"
        })
        .bind(gate.admits_reading())
        .bind(context::HEADLINE_POLICY)
        .bind(context::HEADLINE_READ_THRESHOLD)
        .bind(serde_json::to_value(&gate.request)?)
        .bind(serde_json::to_value(&gate.response.answers["relevance"])?)
        .bind(&gate.response.provenance)
        .bind(&gate.response.raw_response)
        .execute(&mut **tx)
        .await
        .context("store Harvester headline gate")?;
    }
    Ok(())
}

/// Keep unique exact name-surface matches visible in the headline or delivered
/// publisher opening as candidate identity evidence. Graph must see the same
/// text that grounded the match. This does not write authoritative links.
async fn record_identity_candidates(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    sport: &str,
    headline: &str,
    body: &str,
    opening: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM public.harvester_entity_mentions WHERE article_id=$1 AND sport=$2")
        .bind(article_id)
        .bind(sport)
        .execute(&mut **tx)
        .await?;
    let body_sha256 = hex::encode(Sha256::digest(body.as_bytes()));
    sqlx::query(
        r#"
        WITH unique_surfaces AS (
            SELECT sport, norm, min(entity_type) AS entity_type, min(entity_id) AS entity_id
              FROM public.entity_name_surfaces
             WHERE sport=$2 AND length(norm) >= 8
             GROUP BY sport, norm
            HAVING count(DISTINCT (entity_type, entity_id)) = 1
        ), source_text AS (
            SELECT ' ' || public.nrm($3 || ' ' || $4) || ' ' AS norm
        )
        INSERT INTO public.harvester_entity_mentions
            (article_id, entity_type, entity_id, sport, matched_norm, body_sha256)
        SELECT $1, s.entity_type, s.entity_id, s.sport, s.norm, $5
          FROM unique_surfaces s CROSS JOIN source_text t
         WHERE strpos(t.norm, ' ' || s.norm || ' ') > 0
        ON CONFLICT (article_id, entity_type, entity_id, sport, matched_norm)
        DO UPDATE SET body_sha256=EXCLUDED.body_sha256
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .bind(headline)
    .bind(opening)
    .bind(body_sha256)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Promote only unambiguous canonical names in the delivered source opening.
/// Aliases and Google query membership remain candidate evidence, not links.
async fn record_resolved_links(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    sport: &str,
    opening: &str,
) -> Result<()> {
    // A replay is a fresh source reading for this article/sport. Replace the
    // shared index together with Harvester's provenance rows, so a changed
    // opening cannot leave stale authoritative links behind.
    sqlx::query("DELETE FROM public.news_article_entities WHERE article_id=$1 AND sport=$2")
        .bind(article_id)
        .bind(sport)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM public.harvester_resolved_links WHERE article_id=$1 AND sport=$2")
        .bind(article_id)
        .bind(sport)
        .execute(&mut **tx)
        .await?;
    let opening_sha256 = hex::encode(Sha256::digest(opening.as_bytes()));
    sqlx::query(
        r#"
        INSERT INTO public.harvester_resolved_links
            (article_id,entity_type,entity_id,sport,matched_norm,body_sha256,
             opening_sha256,resolution_method)
        SELECT DISTINCT ON (m.article_id,m.entity_type,m.entity_id,m.sport)
               m.article_id,m.entity_type,m.entity_id,m.sport,m.matched_norm,
               m.body_sha256,$3,'unique_canonical_name_v1'
          FROM public.harvester_entity_mentions m
          JOIN public.entity_name_surfaces s
            ON s.entity_type=m.entity_type AND s.entity_id=m.entity_id
           AND s.sport=m.sport AND s.norm=m.matched_norm
         WHERE m.article_id=$1 AND m.sport=$2 AND s.surface_kind='name'
           AND NOT EXISTS (
               SELECT 1 FROM public.entity_name_surfaces other
                WHERE other.sport=s.sport AND other.norm=s.norm
                  AND (other.entity_type,other.entity_id)<>(s.entity_type,s.entity_id)
           )
         ORDER BY m.article_id,m.entity_type,m.entity_id,m.sport,
                  length(m.matched_norm) DESC,m.matched_norm
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .bind(opening_sha256)
    .execute(&mut **tx)
    .await?;
    // The shared graph and Investigator maintenance jobs consume this generic
    // authoritative link table. Publish only links that passed Harvester's
    // exact-source canonical-name resolution, within the same fenced claim.
    sqlx::query(
        "INSERT INTO public.news_article_entities (article_id,entity_type,entity_id,sport) \
         SELECT article_id,entity_type,entity_id,sport \
           FROM public.harvester_resolved_links WHERE article_id=$1 AND sport=$2 \
         ON CONFLICT (article_id,entity_type,entity_id,sport) DO NOTHING",
    )
    .bind(article_id)
    .bind(sport)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn record_unresolved_names(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    sport: &str,
    headline: &str,
    body: &str,
    opening: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM public.harvester_unresolved_names WHERE article_id=$1 AND sport=$2")
        .bind(article_id)
        .bind(sport)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "WITH source_text AS ( \
             SELECT ' ' || public.nrm($3 || ' ' || $4) || ' ' AS norm \
         ), matched AS ( \
             SELECT s.norm,s.entity_type,s.entity_id,s.surface_kind \
               FROM public.entity_name_surfaces s CROSS JOIN source_text t \
              WHERE s.sport=$2 AND length(s.norm)>=8 \
                AND strpos(t.norm,' ' || s.norm || ' ')>0 \
         ), reviewed AS ( \
             SELECT norm, \
                    CASE WHEN count(DISTINCT (entity_type,entity_id))>1 \
                         THEN 'ambiguous_surface' ELSE 'alias_only' END AS reason, \
                    jsonb_agg(jsonb_build_object( \
                        'entity_type',entity_type,'entity_id',entity_id, \
                        'surface_kind',surface_kind) \
                        ORDER BY entity_type,entity_id,surface_kind) AS candidates \
               FROM matched GROUP BY norm \
              HAVING count(DISTINCT (entity_type,entity_id))>1 \
                  OR bool_and(surface_kind<>'name') \
         ) \
         INSERT INTO public.harvester_unresolved_names \
             (article_id,sport,matched_norm,reason,candidates,body_sha256,opening_sha256) \
         SELECT $1,$2,norm,reason,candidates,$5,$6 FROM reviewed",
    )
    .bind(article_id)
    .bind(sport)
    .bind(headline)
    .bind(opening)
    .bind(hex::encode(Sha256::digest(body.as_bytes())))
    .bind(hex::encode(Sha256::digest(opening.as_bytes())))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Use the same source fence for failure receipts and successful publication.
async fn lock_source(
    tx: &mut Transaction<'_, Postgres>,
    article_id: i64,
    source: &Source,
) -> Result<sqlx::postgres::PgRow> {
    // Another owner may update the row while inference is in flight. The claim
    // fence protects pipeline_work; this row lock protects source identity.
    let row = sqlx::query(
        "SELECT url, title, COALESCE(source, '') AS source, published_at::text AS published_at, \
         full_text, duplicate_of FROM public.news_articles WHERE id=$1 FOR UPDATE",
    )
    .bind(article_id)
    .fetch_one(&mut **tx)
    .await?;
    let url: String = row.get("url");
    let title: String = row.get("title");
    ensure!(
        url == source.url
            && title == source.title
            && row.get::<String, _>("source") == source.source
            && row.get::<Option<String>, _>("published_at") == source.published_at,
        "article changed during Harvester run"
    );
    Ok(row)
}

async fn record_retryable_error(
    pool: &PgPool,
    item: &Item,
    source: &Source,
    gates: &[HeadlineGate],
    status: &str,
    error: &str,
    body: Option<&str>,
    final_url: Option<&str>,
    final_domain: Option<&str>,
) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    lock_source(publication.transaction(), item.entity_id, source).await?;
    store_headline_gates(publication.transaction(), source, item.entity_id, gates).await?;
    record_acquisition(
        publication.transaction(),
        item.entity_id,
        status,
        final_url,
        final_domain,
        body,
        Some(error),
    )
    .await?;
    publication.commit_progress().await?;
    anyhow::bail!("Harvester {status}: {error}")
}

fn http_article_status(error: &anyhow::Error) -> Option<&ArticleHttpStatus> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ArticleHttpStatus>())
}

fn acquisition_failure_status(error: &anyhow::Error) -> &'static str {
    if http_article_status(error).is_some_and(ArticleHttpStatus::is_access_denied) {
        "blocked"
    } else {
        "retryable_error"
    }
}

async fn publish(
    pool: &PgPool,
    item: &Item,
    source: &Source,
    body: Option<&str>,
    fetched: Option<&FetchedArticle>,
    gates: &[HeadlineGate],
    contexts: &[HarvestContext],
) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let tx = publication.transaction();
    let row = lock_source(tx, item.entity_id, source).await?;
    let duplicate_of: Option<i64> = row.get("duplicate_of");
    if duplicate_of.is_some() {
        record_acquisition(tx, item.entity_id, "duplicate", None, None, None, None).await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    store_headline_gates(tx, source, item.entity_id, gates).await?;
    if body.is_none() {
        ensure!(
            !gates.is_empty() && gates.iter().all(|gate| !gate.admits_reading()),
            "publisher fetch skipped without a complete negative headline gate"
        );
        ensure!(
            contexts.is_empty(),
            "negative headline gate has source contexts"
        );
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    let body = body.context("publication missing publisher text")?;
    ensure!(
        contexts.len() == gates.iter().filter(|gate| gate.admits_reading()).count(),
        "publisher contexts do not match headline-positive entities"
    );
    let current_body: Option<String> = row.get("full_text");
    ensure!(
        current_body
            .as_deref()
            .is_none_or(|text| text == body || source.retained_body.as_deref() == Some(text)),
        "retained publisher body changed during Harvester run"
    );
    sqlx::query("UPDATE public.news_articles SET full_text=$2 WHERE id=$1 AND full_text IS DISTINCT FROM $2")
        .bind(item.entity_id)
        .bind(body)
        .execute(&mut **tx)
        .await?;
    record_acquisition(
        tx,
        item.entity_id,
        "acquired",
        fetched.map(|f| f.final_url.as_str()),
        fetched.and_then(|f| f.final_domain.as_deref()),
        Some(body),
        None,
    )
    .await?;
    let shadow_mode = std::env::var("HARVESTER_SHADOW_MODE").as_deref() == Ok("1");
    let delivery = if shadow_mode {
        HashSet::new()
    } else {
        delivery_characters()?
    };
    // Identity extraction is grounded in a publisher opening that passed the
    // entity gate. Unrelated Google results cannot nominate graph identities.
    if !shadow_mode {
        if let Some(relevant_context) = contexts.iter().find(|c| {
            c.predicate_scores
                .get("article_relevance")
                .is_some_and(|p| *p >= crate::plugins::editor::prompt::RELEVANCE_THRESHOLD)
        }) {
            let opening = relevant_context.context.text.as_str();
            record_identity_candidates(
                tx,
                item.entity_id,
                &item.sport,
                &source.title,
                body,
                opening,
            )
            .await?;
            record_resolved_links(tx, item.entity_id, &item.sport, opening).await?;
            record_unresolved_names(
                tx,
                item.entity_id,
                &item.sport,
                &source.title,
                body,
                opening,
            )
            .await?;
        }
    }
    for context in contexts {
        let query = article_for_context(source, context, body);
        context.verify_against(&query)?;
        let distributions = serde_json::to_value(&context.predicate_scores)?;
        let row = sqlx::query(
            "INSERT INTO public.harvester_classifications \
             (article_id, entity_type, entity_id, sport, contract_version, model_revision, entity_choice, \
              body_sha256, headline, model_input_start, model_input_end, model_input_text, \
              context_start, context_end, context_text, distributions, model_provenance) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT (article_id,entity_type,entity_id,sport,contract_version,model_revision,body_sha256) \
             DO UPDATE SET id=harvester_classifications.id \
             WHERE (harvester_classifications.headline,harvester_classifications.entity_choice, \
                    harvester_classifications.model_input_start,harvester_classifications.model_input_end, \
                    harvester_classifications.model_input_text,harvester_classifications.context_start, \
                    harvester_classifications.context_end,harvester_classifications.context_text, \
                    harvester_classifications.distributions, \
                    harvester_classifications.model_provenance - 'relevance' - 'theme_passes') \
               IS NOT DISTINCT FROM \
                   (EXCLUDED.headline,EXCLUDED.entity_choice,EXCLUDED.model_input_start, \
                    EXCLUDED.model_input_end,EXCLUDED.model_input_text,EXCLUDED.context_start, \
                    EXCLUDED.context_end,EXCLUDED.context_text,EXCLUDED.distributions, \
                    EXCLUDED.model_provenance - 'relevance' - 'theme_passes') \
             RETURNING id",
        )
        .bind(context.article_id)
        .bind(&context.hypothesis.entity_type)
        .bind(context.hypothesis.entity_id)
        .bind(&context.hypothesis.sport)
        .bind(&context.contract_version)
        .bind(&context.model_revision)
        .bind(&context.entity_choice)
        .bind(&context.body_sha256)
        .bind(&context.headline)
        .bind(i32::try_from(context.model_input.start)?)
        .bind(i32::try_from(context.model_input.end)?)
        .bind(&context.model_input.text)
        .bind(i32::try_from(context.context.start)?)
        .bind(i32::try_from(context.context.end)?)
        .bind(&context.context.text)
        .bind(distributions)
        .bind(&context.model_provenance)
        .fetch_optional(&mut **tx).await.context("store exact Harvester context")?
        .context("existing Harvester receipt differs from current evidence or decisions; a new receipt revision is required")?;
        let classification_id: i64 = row.get("id");
        if shadow_mode {
            continue;
        }
        sqlx::query(
            "UPDATE public.harvester_assignments d \
             SET status='redundant', reason='superseded by newer source context', updated_at=NOW() \
             FROM public.harvester_classifications old \
             WHERE d.classification_id=old.id AND d.status='pending' AND old.id<>$1 \
               AND old.article_id=$2 AND old.entity_type=$3 AND old.entity_id=$4 AND old.sport=$5",
        )
        .bind(classification_id)
        .bind(context.article_id)
        .bind(&context.hypothesis.entity_type)
        .bind(context.hypothesis.entity_id)
        .bind(&context.hypothesis.sport)
        .execute(&mut **tx)
        .await?;
        if context.entity_choice != "relevant" {
            continue;
        }
        if context
            .recommended_characters
            .iter()
            .any(|id| id == crate::plugins::insider::manifest::MANIFEST.id.as_str())
        {
            sqlx::query(
                r#"
            INSERT INTO public.harvester_insider_pairs
                (classification_id,subject_type,subject_id)
            SELECT $1, subject.entity_type, subject.entity_id
              FROM public.harvester_entity_mentions subject
             WHERE subject.article_id=$2 AND subject.sport=$3
               AND (subject.entity_type='player' OR
                    (subject.entity_type='person' AND EXISTS (
                        SELECT 1 FROM public.persons p
                         WHERE p.id=subject.entity_id AND p.sport=subject.sport
                           AND p.kind='coach')))
            ON CONFLICT DO NOTHING
            "#,
            )
            .bind(classification_id)
            .bind(context.article_id)
            .bind(&context.hypothesis.sport)
            .execute(&mut **tx)
            .await?;
        }
        let work_version = format!("{}:c{classification_id}", context.contract_version);
        if delivery.contains("insider")
            && context
                .recommended_characters
                .iter()
                .any(|id| id == crate::plugins::insider::manifest::MANIFEST.id.as_str())
        {
            let subjects: Vec<(String, i32)> = sqlx::query_as(
                "SELECT subject_type,subject_id FROM public.harvester_insider_pairs \
                 WHERE classification_id=$1 ORDER BY subject_type,subject_id",
            )
            .bind(classification_id)
            .fetch_all(&mut **tx)
            .await?;
            for (entity_type, entity_id) in subjects {
                crate::harness::queue::work::enqueue(
                    &mut **tx,
                    &Item {
                        stage: crate::plugins::insider::manifest::TASK,
                        entity_type,
                        entity_id: i64::from(entity_id),
                        sport: context.hypothesis.sport.clone(),
                        input_version: Some(work_version.clone()),
                        attempts: 0,
                        claim_token: None,
                    },
                )
                .await?;
            }
        }
        // Eligibility comes from plugin policy. The queue owns durable dispatch.
        for route in CHARACTER_ROUTES.iter().filter(|route| {
            context
                .recommended_characters
                .iter()
                .any(|id| id == route.destination.id.as_str())
        }) {
            let enabled = delivery.contains(route.delivery_name);
            sqlx::query(
                "INSERT INTO public.harvester_assignments(classification_id,plugin_id,reason) \
                 VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(classification_id)
            .bind(route.destination.id.as_str())
            .bind((!enabled).then_some(DELIVERY_HELD_REASON))
            .execute(&mut **tx)
            .await?;
            if enabled {
                crate::harness::queue::work::enqueue(
                    &mut **tx,
                    &Item {
                        stage: route.destination.task,
                        entity_type: context.hypothesis.entity_type.clone(),
                        entity_id: i64::from(context.hypothesis.entity_id),
                        sport: context.hypothesis.sport.clone(),
                        input_version: Some(work_version.clone()),
                        attempts: 0,
                        claim_token: None,
                    },
                )
                .await?;
                if route.delivery_name == "influencer" {
                    sqlx::query("UPDATE pipeline_work SET available_at=GREATEST(available_at,NOW()+INTERVAL '60 seconds')
                        WHERE stage='vibe' AND entity_type=$1 AND entity_id=$2 AND sport=$3 AND status='pending'")
                        .bind(&context.hypothesis.entity_type).bind(context.hypothesis.entity_id)
                        .bind(&context.hypothesis.sport).execute(&mut **tx).await?;
                }
            }
        }
    }
    // Graph receives exact name-surface candidates as well as historical
    // resolved links, then makes its own evidence decision.
    if shadow_mode
        || !contexts.iter().any(|c| {
            c.predicate_scores
                .get("article_relevance")
                .is_some_and(|p| *p >= crate::plugins::editor::prompt::RELEVANCE_THRESHOLD)
        })
    {
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    let has_identity_candidates: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.news_article_entities WHERE article_id=$1 AND sport=$2) \
         OR EXISTS (SELECT 1 FROM public.harvester_entity_mentions WHERE article_id=$1 AND sport=$2)",
    )
    .bind(item.entity_id)
    .bind(&item.sport)
    .fetch_one(&mut **tx)
    .await?;
    if has_identity_candidates {
        crate::harness::queue::work::enqueue(
            &mut **tx,
            &Item {
                stage: crate::plugins::graph::manifest::TASK,
                entity_type: "article".into(),
                entity_id: item.entity_id,
                sport: item.sport.clone(),
                input_version: Some(format!(
                    "g:{}",
                    hex::encode(Sha256::digest(body.as_bytes()))
                )),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
    }
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}

fn article_for_context(source: &Source, context: &HarvestContext, body: &str) -> Article {
    let query = Query {
        entity_type: context.hypothesis.entity_type.clone(),
        entity_id: context.hypothesis.entity_id,
        name: context.hypothesis.name.clone(),
        sport: context.hypothesis.sport.clone(),
        feed_rank: context.feed_rank,
    };
    article(source, &query, context.article_id, body)
}

#[async_trait]
impl StudioPlugin for HarvesterHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &super::manifest::MANIFEST
    }

    fn scheduled_operations(&self) -> Vec<Arc<dyn crate::harness::plugin::ScheduledOperation>> {
        super::maintenance::operations(self.pool.clone())
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        item.require_claim_token()?;
        ensure!(
            item.stage == super::manifest::TASK && item.entity_type == "article",
            "Harvester requires an article claim"
        );
        let source = load_source(&self.pool, item.entity_id)
            .await?
            .context("claimed article disappeared")?;
        if source.duplicate_of.is_some() {
            return publish(&self.pool, item, &source, None, None, &[], &[]).await;
        }
        let queries = load_queries(&self.pool, item.entity_id, &item.sport).await?;
        let mut gates = Vec::with_capacity(queries.len());
        for query in &queries {
            let headline = article(&source, query, item.entity_id, "");
            gates.push(context::classify_headline(self.model.as_ref(), &headline).await?);
        }
        publish_headlines(&self.pool, item, &source, &gates).await
    }
}

fn editor_input_version(gates: &[HeadlineGate]) -> String {
    let inputs: Vec<_> = gates
        .iter()
        .map(|g| (&g.hypothesis, &g.input_hash, &g.model_revision))
        .collect();
    format!(
        "editor:{}",
        crate::util::hash_components(
            &serde_json::to_string(&inputs).expect("headline identities serialize")
        )
    )
}

async fn publish_headlines(
    pool: &PgPool,
    item: &Item,
    source: &Source,
    gates: &[HeadlineGate],
) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let tx = publication.transaction();
    lock_source(tx, item.entity_id, source).await?;
    store_headline_gates(tx, source, item.entity_id, gates).await?;
    if gates.iter().any(HeadlineGate::admits_reading) {
        crate::harness::queue::work::enqueue(
            &mut **tx,
            &Item {
                stage: crate::plugins::editor::manifest::TASK,
                entity_type: "article".into(),
                entity_id: item.entity_id,
                sport: item.sport.clone(),
                input_version: Some(editor_input_version(gates)),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
    }
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}

async fn load_headline_gates(
    pool: &PgPool,
    source: &Source,
    queries: &[Query],
    article_id: i64,
) -> Result<Vec<HeadlineGate>> {
    let mut gates = Vec::with_capacity(queries.len());
    for query in queries {
        let headline = article(source, query, article_id, "");
        let request = super::prompt::prepare(&headline)?;
        let input_hash = crate::util::hash_components(&serde_json::to_string(&request)?);
        let row = sqlx::query("SELECT model_revision, request, answer, model_provenance, raw_response FROM public.harvester_headline_gates WHERE article_id=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4 AND contract_version=$5 AND input_hash=$6 AND policy_version=$7 ORDER BY created_at DESC, model_revision DESC LIMIT 1")
            .bind(article_id).bind(&query.entity_type).bind(query.entity_id).bind(&query.sport)
            .bind(context::HEADLINE_CONTRACT).bind(&input_hash).bind(context::HEADLINE_POLICY).fetch_one(pool).await?;
        let gate = HeadlineGate {
            article_id,
            headline: source.title.clone(),
            hypothesis: headline.hypothesis,
            request: serde_json::from_value(row.get("request"))?,
            input_hash,
            model_revision: row.get("model_revision"),
            response: super::decision::DecisionResponse {
                answers: std::collections::BTreeMap::from([(
                    "relevance".into(),
                    serde_json::from_value(row.get("answer"))?,
                )]),
                provenance: row.get("model_provenance"),
                raw_response: row.get("raw_response"),
            },
        };
        gate.verify_against(&article(source, query, article_id, ""))?;
        gates.push(gate);
    }
    Ok(gates)
}

/// Shared acquisition/publication plumbing; Editor owns this stage's model and policy.
pub(crate) async fn execute_editor(
    pool: &PgPool,
    model: &dyn DecisionModel,
    web: &Arc<WebBroker>,
    manifest: &'static PluginManifest,
    item: &Item,
) -> Result<PluginOutcome> {
    item.require_claim_token()?;
    ensure!(
        item.stage == crate::plugins::editor::manifest::TASK && item.entity_type == "article",
        "Editor requires an article claim"
    );
    let source = load_source(pool, item.entity_id)
        .await?
        .context("claimed article disappeared")?;
    if source.duplicate_of.is_some() {
        return publish(pool, item, &source, None, None, &[], &[]).await;
    }
    let queries = load_queries(pool, item.entity_id, &item.sport).await?;
    let gates = load_headline_gates(pool, &source, &queries, item.entity_id).await?;
    ensure!(
        item.input_version.as_deref() == Some(editor_input_version(&gates).as_str()),
        "Editor intake changed; rerun headline screening before reading"
    );
    if gates.iter().all(|g| !g.admits_reading()) {
        return publish(pool, item, &source, None, None, &gates, &[]).await;
    }
    let web = web.scope(pool, manifest);
    let (body, fetched) = if let Some(body) = source
        .retained_body
        .as_deref()
        .filter(|x| has_usable_paragraph_opening(x))
    {
        (clean_body(body), None)
    } else {
        let fetch_url = source.publisher_url.as_deref().unwrap_or(&source.url);
        match web.fetch_curated_article(fetch_url).await {
            Ok(fetched) => (clean_body(&fetched.text), Some(fetched)),
            Err(error) => {
                let final_url = http_article_status(&error).map(|status| status.final_url.as_str());
                let final_domain = final_url.and_then(domain_of);
                return record_retryable_error(
                    pool,
                    item,
                    &source,
                    &gates,
                    acquisition_failure_status(&error),
                    &format!("{error:#}"),
                    None,
                    final_url,
                    final_domain.as_deref(),
                )
                .await;
            }
        }
    };
    if count_words(&body) < 20 {
        return record_retryable_error(
            pool,
            item,
            &source,
            &gates,
            "low_content",
            "publisher returned too little article text",
            Some(&body),
            fetched.as_ref().map(|article| article.final_url.as_str()),
            fetched
                .as_ref()
                .and_then(|article| article.final_domain.as_deref()),
        )
        .await;
    }
    if !has_usable_paragraph_opening(&body) {
        return record_retryable_error(
            pool,
            item,
            &source,
            &gates,
            "low_content",
            "publisher opening lacks thirty words across preserved paragraphs",
            Some(&body),
            fetched.as_ref().map(|article| article.final_url.as_str()),
            fetched
                .as_ref()
                .and_then(|article| article.final_domain.as_deref()),
        )
        .await;
    }
    let mut contexts = Vec::with_capacity(queries.len());
    for (query, gate) in queries.iter().zip(&gates) {
        if !gate.admits_reading() {
            continue;
        }
        let article = article(&source, query, item.entity_id, &body);
        match context::classify_after_headline(model, &article, gate).await {
            Ok(context) => contexts.push(context),
            Err(error) => {
                return record_retryable_error(
                    pool,
                    item,
                    &source,
                    &gates,
                    "classification_error",
                    &format!("{error:#}"),
                    Some(&body),
                    fetched.as_ref().map(|article| article.final_url.as_str()),
                    fetched
                        .as_ref()
                        .and_then(|article| article.final_domain.as_deref()),
                )
                .await
            }
        }
    }
    publish(
        pool,
        item,
        &source,
        Some(&body),
        fetched.as_ref(),
        &gates,
        &contexts,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::queue::work;
    use crate::plugins::harvester::decision::{
        DecisionRequest, DecisionResponse, ProbabilityAnswer,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    struct AllDeliveryForTest(Option<std::ffi::OsString>, Option<std::ffi::OsString>);

    impl AllDeliveryForTest {
        fn new() -> Self {
            let previous = std::env::var_os("HARVESTER_DELIVERY_CHARACTERS");
            std::env::set_var(
                "HARVESTER_DELIVERY_CHARACTERS",
                "journalist,influencer,insider,scout",
            );
            let shadow = std::env::var_os("HARVESTER_SHADOW_MODE");
            std::env::remove_var("HARVESTER_SHADOW_MODE");
            Self(previous, shadow)
        }
    }

    impl Drop for AllDeliveryForTest {
        fn drop(&mut self) {
            if let Some(previous) = &self.1 {
                std::env::set_var("HARVESTER_SHADOW_MODE", previous);
            } else {
                std::env::remove_var("HARVESTER_SHADOW_MODE");
            }
            if let Some(previous) = &self.0 {
                std::env::set_var("HARVESTER_DELIVERY_CHARACTERS", previous);
            } else {
                std::env::remove_var("HARVESTER_DELIVERY_CHARACTERS");
            }
        }
    }

    #[test]
    fn delivery_character_gate_accepts_only_named_unique_characters() {
        assert!(parse_delivery_characters("").unwrap().is_empty());
        let selected = parse_delivery_characters("journalist, scout").unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.contains("journalist"));
        assert!(selected.contains("scout"));
        assert!(parse_delivery_characters("journalist,journalist").is_err());
        assert!(parse_delivery_characters("editor").is_err());
    }

    #[test]
    fn retained_body_needs_substantive_paragraphs_before_delivery() {
        let prose = "The club confirmed that the captain returned to training today after the medical staff completed their assessment. ";
        let body = format!("{}\n\n{}\n\n{}", prose, prose, prose);
        assert!(has_usable_paragraph_opening(&body));
        assert!(!has_usable_paragraph_opening(&body.replace("\n\n", " ")));
        assert!(!has_usable_paragraph_opening(
            "Share\n\nFollow us\n\nPopular news"
        ));
    }

    #[test]
    fn access_denials_are_visible_without_turning_timeouts_into_blocks() {
        for code in [401, 402, 403, 451] {
            let error = anyhow::Error::new(ArticleHttpStatus {
                status: reqwest::StatusCode::from_u16(code).unwrap(),
                final_url: "https://publisher.example/article".into(),
            })
            .context("fetch curated publisher article");
            assert_eq!(acquisition_failure_status(&error), "blocked");
            assert_eq!(
                http_article_status(&error).unwrap().final_url,
                "https://publisher.example/article"
            );
        }
        let rate_limit = anyhow::Error::new(ArticleHttpStatus {
            status: reqwest::StatusCode::TOO_MANY_REQUESTS,
            final_url: "https://publisher.example/article".into(),
        });
        assert_eq!(acquisition_failure_status(&rate_limit), "retryable_error");
        assert_eq!(
            acquisition_failure_status(&anyhow::anyhow!("publisher timeout")),
            "retryable_error"
        );
    }

    struct SmokeLaya {
        themes: &'static [&'static str],
    }

    #[async_trait]
    impl DecisionModel for SmokeLaya {
        async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
            let answers = request
                .questions
                .keys()
                .map(|key| {
                    let choice = if key == "relevance"
                        || key == "article_relevance"
                        || self.themes.contains(&key.as_str())
                    {
                        "relevant"
                    } else {
                        "irrelevant"
                    };
                    (
                        key.clone(),
                        ProbabilityAnswer {
                            probability: if choice == "relevant" { 0.8 } else { 0.2 },
                        },
                    )
                })
                .collect();
            Ok(DecisionResponse {
                answers,
                provenance: json!({
                    "model": "smoke-laya", "revision": "fixture-r1", "adapter": "test", "device": "cpu",
                    "coverage": request.questions.keys().map(|key| (key.clone(), json!({"truncated": false})))
                        .collect::<BTreeMap<_, _>>()
                }),
                raw_response: serde_json::Value::Null,
            })
        }
    }

    struct HeadlineRejectLaya;

    #[async_trait]
    impl DecisionModel for HeadlineRejectLaya {
        async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
            ensure!(request.questions.len() == 1 && request.questions.contains_key("relevance"));
            ensure!(!request.state.is_empty());
            ensure!(!request.state.contains("Publisher opening"));
            Ok(DecisionResponse {
                answers: BTreeMap::from([(
                    "relevance".into(),
                    ProbabilityAnswer { probability: 0.2 },
                )]),
                provenance: json!({"model":"smoke-laya","revision":"fixture-r1",
                    "adapter":"test","device":"cpu",
                    "coverage":{"relevance":{"truncated":false}}}),
                raw_response: serde_json::Value::Null,
            })
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL with migration 286"]
    async fn negative_headline_commits_gate_without_fetch_or_source_classification() -> Result<()> {
        const SPORT: &str = "ZZ_HARVESTER_HEADLINE";
        const ARTICLE: i64 = 9_690_210;
        const TEAM: i32 = 9_690_211;
        let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap()).await?;
        sqlx::query("DELETE FROM public.pipeline_work WHERE stage='harvester' AND entity_id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO public.sports(id,display_name,current_season) \
             VALUES($1,'Headline gate smoke',2026) ON CONFLICT DO NOTHING",
        )
        .bind(SPORT)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO public.teams(id,sport,name) \
             VALUES($1,$2,'Headline Gate Test Club') ON CONFLICT DO NOTHING",
        )
        .bind(TEAM)
        .bind(SPORT)
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,source,title,description,feed_rank) \
             VALUES($1,'headline-gate-smoke-9690210','https://invalid.example.test/no-fetch', \
                    'Example Wire','Unrelated city council budget meeting','Thin Google description',1)")
            .bind(ARTICLE).execute(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_query_provenance \
             (article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',$2,$3,1)",
        )
        .bind(ARTICLE)
        .bind(TEAM)
        .bind(SPORT)
        .execute(&pool)
        .await?;
        work::enqueue(
            &pool,
            &Item {
                stage: super::super::manifest::TASK,
                entity_type: "article".into(),
                entity_id: ARTICLE,
                sport: SPORT.into(),
                input_version: Some("harvest-context-v7:headline-smoke".into()),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        let claimed = work::claim(&pool, super::super::manifest::TASK, 1)
            .await?
            .remove(0);
        ensure!(claimed.entity_id == ARTICLE, "test claimed another article");
        let handler = HarvesterHandler::new(pool.clone(), Arc::new(HeadlineRejectLaya));
        assert_eq!(handler.execute(&claimed).await?, PluginOutcome::Committed);
        let (choice, state, probability): (String, String, f64) = sqlx::query_as(
            "SELECT choice,request->>'state',(answer->>'noul')::float8 \
             FROM public.harvester_headline_gates WHERE article_id=$1 AND contract_version=$2",
        )
        .bind(ARTICLE)
        .bind(context::HEADLINE_CONTRACT)
        .fetch_one(&pool)
        .await?;
        assert_eq!(choice, "irrelevant");
        assert_eq!(state, "Unrelated city council budget meeting");
        assert!(!state.contains("Thin Google description"));
        assert_eq!(probability, 0.2);
        let acquisition: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_acquisitions WHERE article_id=$1",
        )
        .bind(ARTICLE)
        .fetch_one(&pool)
        .await?;
        let classifications: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_classifications WHERE article_id=$1",
        )
        .bind(ARTICLE)
        .fetch_one(&pool)
        .await?;
        let work_left: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.pipeline_work WHERE stage='harvester' AND entity_id=$1",
        )
        .bind(ARTICLE)
        .fetch_one(&pool)
        .await?;
        assert_eq!((acquisition, classifications, work_left), (0, 0, 0));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL with migration 269"]
    async fn selective_routes_and_delivery_controls_remain_independent() -> Result<()> {
        let _delivery = AllDeliveryForTest::new();
        const SPORT: &str = "ZZ_HARVESTER_ROUTES";
        const TEAM: i32 = 9_690_310;
        const ARTICLE: i64 = 9_690_311;
        let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap()).await?;
        sqlx::query("INSERT INTO public.sports(id,display_name,current_season) VALUES($1,'Routing test',2026) ON CONFLICT DO NOTHING")
            .bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.teams(id,sport,name) VALUES($1,$2,'Routing Test Club') ON CONFLICT DO NOTHING")
            .bind(TEAM).bind(SPORT).execute(&pool).await?;
        let body = "The club returned to training on Monday morning after its weekend match. The coaching staff supervised the session at the training ground.\n\nThe players worked together before the next scheduled match later this week.\n\nThe session ended at noon.";
        let cases: &[(&[&str], &str, &[(&str, bool)], &[&str])] = &[
            (
                &["narrative"],
                "",
                &[
                    ("scoracle.character.narrative", true),
                    ("scoracle.character.vibe", true),
                ],
                &[],
            ),
            (
                &["narrative", "fitness"],
                "influencer,scout",
                &[
                    ("scoracle.character.narrative", true),
                    ("scoracle.character.vibe", false),
                    ("scoracle.character.rating", false),
                ],
                &["rating", "vibe"],
            ),
            (
                &[],
                "journalist,influencer,insider,scout",
                &[("scoracle.character.vibe", false)],
                &["vibe"],
            ),
        ];
        for (index, (themes, enabled, expected_receipts, expected_tasks)) in
            cases.iter().enumerate()
        {
            let article_id = ARTICLE + index as i64;
            sqlx::query("DELETE FROM public.pipeline_work WHERE sport=$1")
                .bind(SPORT)
                .execute(&pool)
                .await?;
            sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
                .bind(article_id)
                .execute(&pool)
                .await?;
            sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,title,source,full_text) VALUES($1,$2,'https://example.test/routes','Routing Test Club update','Example Wire',$3)")
                .bind(article_id).bind(format!("routing-test-{article_id}")).bind(body).execute(&pool).await?;
            sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',$2,$3,1)")
                .bind(article_id).bind(TEAM).bind(SPORT).execute(&pool).await?;
            work::enqueue(
                &pool,
                &Item {
                    stage: super::super::manifest::TASK,
                    entity_type: "article".into(),
                    entity_id: article_id,
                    sport: SPORT.into(),
                    input_version: Some(context::CONTRACT.into()),
                    attempts: 0,
                    claim_token: None,
                },
            )
            .await?;
            let claim = work::claim(&pool, super::super::manifest::TASK, 1)
                .await?
                .remove(0);
            assert_eq!(claim.entity_id, article_id);
            std::env::set_var("HARVESTER_DELIVERY_CHARACTERS", enabled);
            let handler = HarvesterHandler::new(pool.clone(), Arc::new(SmokeLaya { themes }));
            assert_eq!(handler.execute(&claim).await?, PluginOutcome::Committed);
            let editor_claim = work::claim(&pool, crate::plugins::editor::manifest::TASK, 1)
                .await?
                .remove(0);
            assert_eq!(editor_claim.entity_id, article_id);
            let editor = crate::plugins::editor::EditorHandler::new(
                pool.clone(),
                Arc::new(SmokeLaya { themes }),
                Arc::new(WebBroker::new(0)?),
            );
            assert_eq!(
                editor.execute(&editor_claim).await?,
                PluginOutcome::Committed
            );
            let receipts: Vec<(String, bool)> = sqlx::query_as(
                "SELECT d.plugin_id,COALESCE(d.reason=$2,false) FROM public.harvester_assignments d JOIN public.harvester_classifications c ON c.id=d.classification_id WHERE c.article_id=$1 ORDER BY d.plugin_id",
            ).bind(article_id).bind(DELIVERY_HELD_REASON).fetch_all(&pool).await?;
            assert_eq!(
                receipts,
                expected_receipts
                    .iter()
                    .map(|(id, held)| (id.to_string(), *held))
                    .collect::<Vec<_>>()
            );
            let tasks: Vec<String> = sqlx::query_scalar(
                "SELECT stage FROM public.pipeline_work WHERE sport=$1 AND stage IN ('narratives','vibe','transfers','rating') ORDER BY stage",
            ).bind(SPORT).fetch_all(&pool).await?;
            assert_eq!(tasks, *expected_tasks);
            for (plugin, held) in *expected_receipts {
                let delivered =
                    super::super::delivery::load_for_character(&pool, plugin, "team", TEAM, SPORT)
                        .await?;
                assert_eq!(delivered.len(), usize::from(!held));
                if !held {
                    assert_eq!(delivered[0].article_id, article_id);
                    assert_eq!(delivered[0].context, body);
                }
            }
            sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
                .bind(article_id)
                .execute(&pool)
                .await?;
        }
        sqlx::query("DELETE FROM public.pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id=$1 AND sport=$2")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        Ok(())
    }
}

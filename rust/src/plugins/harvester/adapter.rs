//! Claim-fenced acquisition and source-context publication for Harvester.
//! Character delivery is a separate cutover step; pending assignments remain
//! queryable until a character adapter owns their final disposition.
use super::cognition::Article;
use super::context::{self, HarvestContext, HeadlineGate};
use super::policy::CHARACTER_ROUTES;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::application::tools::{ToolLedger, WebBroker};
use crate::evidence::fetch::{count_words, domain_of, ArticleHttpStatus, FetchedArticle};
use crate::plugins::meta::EntityMeta;
use crate::studio::decision::DecisionModel;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
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
    web: Arc<WebBroker>,
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
    pub fn new(pool: PgPool, model: Arc<dyn DecisionModel>, web: Arc<WebBroker>) -> Self {
        Self { pool, model, web }
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
        if let Some(relevant_context) = contexts.iter().find(|c| c.entity_choice == "relevant") {
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
              FROM public.harvester_resolved_links team
              JOIN public.harvester_resolved_links subject
                ON subject.article_id=team.article_id AND subject.sport=team.sport
             WHERE team.article_id=$2 AND team.sport=$3
               AND team.entity_type='team' AND team.entity_id=$4
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
            .bind(context.hypothesis.entity_id)
            .execute(&mut **tx)
            .await?;
        }
        let work_version = format!("{}:c{classification_id}", context.contract_version);
        sqlx::query(
            "UPDATE public.harvester_insider_wraps \
             SET status='superseded', \
                 product_ref=COALESCE(product_ref,'{}'::jsonb) || \
                   jsonb_build_object('reason','new_source_revision','superseded_by',$3::text), \
                 updated_at=now() \
             WHERE team_id=$1 AND sport=$2 AND status='pending' AND work_version<>$3",
        )
        .bind(context.hypothesis.entity_id)
        .bind(&context.hypothesis.sport)
        .bind(&work_version)
        .execute(&mut **tx)
        .await?;
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
                crate::application::queue::work::enqueue(
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
            }
        }
    }
    // Graph receives exact name-surface candidates as well as historical
    // resolved links, then makes its own evidence decision.
    if shadow_mode || !contexts.iter().any(|c| c.entity_choice == "relevant") {
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
        crate::application::queue::work::enqueue(
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

    fn scheduled_operations(&self) -> Vec<Arc<dyn crate::studio::plugin::ScheduledOperation>> {
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
        if gates.iter().all(|gate| !gate.admits_reading()) {
            return publish(&self.pool, item, &source, None, None, &gates, &[]).await;
        }
        let ledger = ToolLedger::new();
        let web = self.web.scope(&self.pool, self.manifest(), &ledger);
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
                    let final_url =
                        http_article_status(&error).map(|status| status.final_url.as_str());
                    let final_domain = final_url.and_then(domain_of);
                    return record_retryable_error(
                        &self.pool,
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
                &self.pool,
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
                &self.pool,
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
            match context::classify_after_headline(self.model.as_ref(), &article, gate).await {
                Ok(context) => contexts.push(context),
                Err(error) => {
                    return record_retryable_error(
                        &self.pool,
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
            &self.pool,
            item,
            &source,
            Some(&body),
            fetched.as_ref(),
            &gates,
            &contexts,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::queue::work;
    use crate::studio::decision::{DecisionRequest, DecisionResponse, ProbabilityAnswer};
    use crate::studio::model::{GenerateOptions, GenerateResult, Inference};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;

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

    struct SmokeVibe;
    struct SmokeInsider;
    struct SmokeScout;

    #[async_trait]
    impl Inference for SmokeScout {
        async fn generate(
            &self,
            prompt: &str,
            options: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let response = if prompt.contains("Choose one approved phrasing") {
                let slots = options
                    .format_schema
                    .as_ref()
                    .and_then(|schema| schema["properties"]["choices"]["minItems"].as_u64())
                    .ok_or_else(|| anyhow::anyhow!("missing palette slot count"))?;
                json!({"choices": vec![0; slots as usize]}).to_string()
            } else if prompt.contains("Morgan Example was ruled out with a knee injury") {
                ensure!(prompt.contains("Exact publisher opening (unchanged)"));
                r#"{"kind":"availability","evidence_quote":"Morgan Example was ruled out with a knee injury"}"#.to_string()
            } else if prompt.contains("Morgan Example joined Harvester Test Club") {
                ensure!(prompt.contains("Exact publisher opening (unchanged)"));
                r#"{"kind":"roster","evidence_quote":"Morgan Example joined Harvester Test Club"}"#
                    .to_string()
            } else if prompt.contains("Entity: team Harvester Test Club")
                || prompt.contains("Entity: team Another Test Club")
            {
                ensure!(prompt.contains("Exact publisher opening (unchanged)"));
                r#"{"kind":"performance","evidence_quote":"Morgan Example recorded a season-high 20 points in the last match."}"#.to_string()
            } else {
                ensure!(prompt.contains("Exact publisher opening (unchanged)"));
                r#"{"kind":"none","evidence_quote":""}"#.to_string()
            };
            Ok((
                GenerateResult {
                    response: response.clone(),
                    thinking: String::new(),
                    model: "smoke-scout".into(),
                    total_duration: Duration::from_millis(5),
                    prompt_eval_count: 10,
                    eval_count: 8,
                    completion_reason: Some("stop".into()),
                    raw_response_body: response,
                },
                json!({"source_prompt": prompt}),
            ))
        }
        fn model(&self) -> &str {
            "smoke-scout"
        }
        fn request_body(&self, prompt: &str, _options: &GenerateOptions) -> serde_json::Value {
            json!({"source_prompt": prompt})
        }
    }

    #[async_trait]
    impl Inference for SmokeInsider {
        async fn generate(
            &self,
            prompt: &str,
            options: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let response = if prompt.contains("Choose one approved phrasing") {
                let slots = options
                    .format_schema
                    .as_ref()
                    .and_then(|schema| schema["properties"]["choices"]["minItems"].as_u64())
                    .context("missing Insider score palette slot count")?;
                json!({"choices": vec![0; slots as usize]}).to_string()
            } else {
                ensure!(prompt.contains("Exact publisher opening (unchanged)"));
                ensure!(prompt.contains("Morgan Example"));
                let subject = if prompt.contains("Resolved subject: player Taylor Sample") {
                    "Taylor Sample"
                } else {
                    "Morgan Example"
                };
                format!(
                    "{{\"is_rumor\":true,\"subject\":\"{subject}\",\"stage\":\"advanced_talks\",\"evidence_quote\":\"Harvester Test Club is in talks to sign Morgan Example and Taylor Sample this week.\"}}"
                )
            };
            Ok((
                GenerateResult {
                    response: response.clone(),
                    thinking: String::new(),
                    model: "smoke-insider".into(),
                    total_duration: Duration::from_millis(5),
                    prompt_eval_count: 10,
                    eval_count: 8,
                    completion_reason: Some("stop".into()),
                    raw_response_body: response,
                },
                json!({"source_prompt": prompt}),
            ))
        }
        fn model(&self) -> &str {
            "smoke-insider"
        }
        fn request_body(&self, prompt: &str, _options: &GenerateOptions) -> serde_json::Value {
            json!({"source_prompt": prompt})
        }
    }

    #[async_trait]
    impl Inference for SmokeVibe {
        async fn generate(
            &self,
            prompt: &str,
            options: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            ensure!(
                prompt.contains("Supporters cheered the announcement at the ground")
                    || prompt.contains("supporters cheered the plans for the next community event")
            );
            ensure!(!prompt.contains("Thin RSS description"));
            let reaction_gate = options
                .format_schema
                .as_ref()
                .is_some_and(|schema| schema["properties"].get("has_reaction").is_some());
            let response = if reaction_gate {
                if prompt.contains("Supporters cheered the announcement at the ground")
                    && prompt.contains("Entity: team Harvester Test Club")
                {
                    r#"{"has_reaction":true,"evidence_quote":"Supporters cheered the announcement at the ground."}"#
                } else if prompt
                    .contains("supporters cheered the plans for the next community event")
                    && prompt.contains("Entity: team Harvester Test Club")
                {
                    r#"{"has_reaction":true,"evidence_quote":"supporters cheered the plans for the next community event"}"#
                } else {
                    r#"{"has_reaction":false,"evidence_quote":""}"#
                }
            } else if prompt.contains("Entity: team Harvester Test Club") {
                r#"{"score":73,"headline":"Supporters welcome the event","body":"Supporters cheered the community announcement at the ground."}"#
            } else {
                "null"
            };
            Ok((
                GenerateResult {
                    response: response.into(),
                    thinking: String::new(),
                    model: "smoke-vibe".into(),
                    total_duration: Duration::from_millis(5),
                    prompt_eval_count: 10,
                    eval_count: 8,
                    completion_reason: Some("stop".into()),
                    raw_response_body: response.into(),
                },
                json!({"source_prompt": prompt}),
            ))
        }

        fn model(&self) -> &str {
            "smoke-vibe"
        }

        fn request_body(&self, prompt: &str, _options: &GenerateOptions) -> serde_json::Value {
            json!({"source_prompt": prompt})
        }
    }

    #[async_trait]
    impl DecisionModel for SmokeLaya {
        async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
            let answers = request
                .questions
                .keys()
                .map(|key| {
                    let choice = if key == "relevance" || self.themes.contains(&key.as_str()) {
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
        let handler = HarvesterHandler::new(
            pool.clone(),
            Arc::new(HeadlineRejectLaya),
            Arc::new(WebBroker::new(0)?),
        );
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
            // Selected material stays held when delivery is disabled.
            (
                &["narrative"],
                "",
                &[("scoracle.character.narrative", true)],
                &[],
            ),
            // Enabling Influencer cannot create eligibility; disabling Journalist
            // cannot erase its pending source receipt.
            (
                &["narrative", "fitness"],
                "influencer,scout",
                &[
                    ("scoracle.character.narrative", true),
                    ("scoracle.character.rating", false),
                ],
                &["rating"],
            ),
            // Even fully enabled delivery cannot route a negative theme pass.
            (&[], "journalist,influencer,insider,scout", &[], &[]),
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
            let handler = HarvesterHandler::new(
                pool.clone(),
                Arc::new(SmokeLaya { themes }),
                Arc::new(WebBroker::new(0)?),
            );
            assert_eq!(handler.execute(&claim).await?, PluginOutcome::Committed);
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

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL with migration 269"]
    async fn claimed_harvester_smoke_keeps_editor_and_routes_source_context() -> Result<()> {
        let _delivery = AllDeliveryForTest::new();
        const SPORT: &str = "ZZ_HARVESTER_SMOKE";
        const ARTICLE: i64 = 9_690_101;
        const SECOND_ARTICLE: i64 = 9_690_104;
        const AVAILABILITY_ARTICLE: i64 = 9_690_108;
        const ROSTER_ARTICLE: i64 = 9_690_109;
        const TEAM: i32 = 9_690_102;
        const OTHER_TEAM: i32 = 9_690_103;
        const PLAYER: i32 = 9_690_105;
        const PLAYER_TWO: i32 = 9_690_106;
        let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap()).await?;
        sqlx::query("DELETE FROM public.harvester_insider_identity_reviews WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.harvester_insider_wraps WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.insider_scores WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.stat_summaries WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query(
            "DELETE FROM public.player_availability WHERE sport=$1 AND source_article_id=$2",
        )
        .bind(SPORT)
        .bind(AVAILABILITY_ARTICLE)
        .execute(&pool)
        .await?;
        sqlx::query(
            "DELETE FROM public.transfer_identity_applications WHERE sport=$1 AND player_id=$2",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .execute(&pool)
        .await?;
        sqlx::query("DELETE FROM public.team_stats WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.transfer_rumors WHERE team_id=$1 AND sport=$2")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(SECOND_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(AVAILABILITY_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ROSTER_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id=$1 AND sport=$2")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id=$1 AND sport=$2")
            .bind(OTHER_TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.players WHERE id=$1 AND sport=$2")
            .bind(PLAYER)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.players WHERE id=$1 AND sport=$2")
            .bind(PLAYER_TWO)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.sports(id,display_name,current_season) VALUES($1,'Harvester smoke',2026) ON CONFLICT DO NOTHING")
            .bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.teams(id,sport,name) VALUES($1,$2,'Harvester Test Club')")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.teams(id,sport,name) VALUES($1,$2,'Another Test Club')")
            .bind(OTHER_TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.team_stats(team_id,sport,season,league_id,stats,rating_score,rating_breakdown) VALUES($1,$2,2026,0,'{}'::jsonb,68,$3)")
            .bind(TEAM).bind(SPORT)
            .bind(json!([{"label":"Scoring","measure":"points","value":20.0,"z":1.0,"pct":80.0,"sign":1,"in_comp":true}]))
            .execute(&pool).await?;
        sqlx::query("INSERT INTO public.players(id,sport,name) VALUES($1,$2,'Morgan Example')")
            .bind(PLAYER)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.players(id,sport,name) VALUES($1,$2,'Taylor Sample')")
            .bind(PLAYER_TWO)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',$1,$2,public.nrm('Harvester Test Club'),'name') ON CONFLICT DO NOTHING")
            .bind(TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',$1,$2,public.nrm('Another Test Club'),'name') ON CONFLICT DO NOTHING")
            .bind(OTHER_TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',$1,$2,public.nrm('community event'),'alias') ON CONFLICT DO NOTHING")
            .bind(OTHER_TEAM).bind(SPORT).execute(&pool).await?;
        for team_id in [TEAM, OTHER_TEAM] {
            sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',$1,$2,public.nrm('Supporters'),'alias') ON CONFLICT DO NOTHING")
                .bind(team_id).bind(SPORT).execute(&pool).await?;
        }
        sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('player',$1,$2,public.nrm('Morgan Example'),'name') ON CONFLICT DO NOTHING")
            .bind(PLAYER).bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('player',$1,$2,public.nrm('Taylor Sample'),'name') ON CONFLICT DO NOTHING")
            .bind(PLAYER_TWO).bind(SPORT).execute(&pool).await?;
        let body = "Harvester Test Club is in talks to sign Morgan Example and Taylor Sample this week.\n\nSupporters cheered the announcement at the ground.\n\nMorgan Example recorded a season-high 20 points in the last match.\n\nThis final paragraph mentions Another Test Club outside the delivered publisher opening.";
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,source,title,description,full_text,feed_rank) VALUES($1,$2,$3,$4,$5,$6,$7,1)")
            .bind(ARTICLE).bind("harvester-smoke-9690101").bind("https://example.test/harvester-smoke")
            .bind("Example Wire").bind("Harvester Test Club announces community event")
            .bind("Thin RSS description").bind(body).execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',$2,$3,1)")
            .bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',$2,$3,2)")
            .bind(ARTICLE).bind(OTHER_TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_insider_wraps \
             (team_id,sport,work_version,entity_type,entity_id) \
             VALUES($1,$2,'harvest-context-v1:old','team',$1)",
        )
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
                input_version: Some("harvest-context-v1:q2".into()),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        let claimed = work::claim(&pool, super::super::manifest::TASK, 1)
            .await?
            .remove(0);
        let handler = HarvesterHandler::new(
            pool.clone(),
            Arc::new(SmokeLaya {
                themes: &[
                    "narrative",
                    "emotional_charge",
                    "player_move",
                    "contract",
                    "staffing",
                    "performance",
                    "fitness",
                ],
            }),
            Arc::new(WebBroker::new(0)?),
        );
        // Source attribution must not change between preparation and publication.
        let source = load_source(&pool, ARTICLE).await?.unwrap();
        for mutation in [
            "UPDATE public.news_articles SET source='Changed publisher' WHERE id=$1",
            "UPDATE public.news_articles SET source='Example Wire',published_at=NOW() WHERE id=$1",
        ] {
            sqlx::query(mutation).bind(ARTICLE).execute(&pool).await?;
            let error = publish(&pool, &claimed, &source, None, None, &[], &[])
                .await
                .unwrap_err();
            assert!(error.to_string().contains("article changed"));
            let error = record_retryable_error(
                &pool,
                &claimed,
                &source,
                &[],
                "classification_error",
                "synthetic failure",
                None,
                None,
                None,
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("article changed"));
        }
        sqlx::query("UPDATE public.news_articles SET published_at=NULL WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        std::env::set_var("HARVESTER_SHADOW_MODE", "1");
        assert_eq!(handler.execute(&claimed).await?, PluginOutcome::Committed);
        let shadow_assignments: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_assignments d JOIN public.harvester_classifications c ON c.id=d.classification_id WHERE c.article_id=$1"
        ).bind(ARTICLE).fetch_one(&pool).await?;
        assert_eq!(shadow_assignments, 0);
        std::env::remove_var("HARVESTER_SHADOW_MODE");
        // Publish the same retained evidence twice under fresh claims. Unique
        // classification/assignment receipts must keep delivery idempotent.
        for _ in 0..2 {
            work::enqueue(&pool, &claimed).await?;
            let live_claim = work::claim(&pool, super::super::manifest::TASK, 1)
                .await?
                .remove(0);
            assert_eq!(
                handler.execute(&live_claim).await?,
                PluginOutcome::Committed
            );
            assert_eq!(
                handler.execute(&live_claim).await?,
                PluginOutcome::Superseded
            );
        }
        // The uniqueness key includes body/checkpoint, but not headline or attribution.
        // Same-body revisions must not silently bind new decisions to an old receipt.
        for mutation in [
            "UPDATE public.news_articles SET title='Changed headline with unchanged body' WHERE id=$1",
            "UPDATE public.news_articles SET source='Changed publisher' WHERE id=$1",
        ] {
            sqlx::query(mutation).bind(ARTICLE).execute(&pool).await?;
            work::enqueue(&pool, &claimed).await?;
            let revised_claim = work::claim(&pool, super::super::manifest::TASK, 1).await?.remove(0);
            let error = handler.execute(&revised_claim).await.unwrap_err();
            assert!(error.to_string().contains("existing Harvester receipt differs"));
            sqlx::query("UPDATE public.news_articles SET title='Harvester Test Club announces community event',source='Example Wire' WHERE id=$1")
                .bind(ARTICLE).execute(&pool).await?;
            // Failure rolls back receipt effects and leaves the claim usable.
            assert_eq!(handler.execute(&revised_claim).await?, PluginOutcome::Committed);
        }
        let old_wrap: (String, serde_json::Value) = sqlx::query_as(
            "SELECT status,product_ref FROM public.harvester_insider_wraps \
             WHERE team_id=$1 AND sport=$2 AND work_version='harvest-context-v1:old'",
        )
        .bind(TEAM)
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
        assert_eq!(old_wrap.0, "superseded");
        assert_eq!(old_wrap.1["reason"], "new_source_revision");
        let classifications: Vec<(i32, String, String, String)> = sqlx::query_as(
            "SELECT entity_id,entity_choice,context_text,headline FROM public.harvester_classifications WHERE article_id=$1 ORDER BY entity_id"
        ).bind(ARTICLE).fetch_all(&pool).await?;
        assert_eq!(classifications.len(), 2);
        assert_eq!(
            classifications.iter().map(|row| row.0).collect::<Vec<_>>(),
            vec![TEAM, OTHER_TEAM]
        );
        for (_, entity_choice, context, headline) in classifications {
            assert_eq!(entity_choice, "relevant");
            assert_eq!(headline, "Harvester Test Club announces community event");
            assert!(body.contains(&context));
        }
        let assignments: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_assignments d JOIN public.harvester_classifications c ON c.id=d.classification_id WHERE c.article_id=$1 AND d.status='pending'"
        ).bind(ARTICLE).fetch_one(&pool).await?;
        assert_eq!(assignments, 8);
        let insider_pairs: Vec<(i32, i32)> = sqlx::query_as(
            "SELECT c.entity_id,p.subject_id FROM public.harvester_insider_pairs p \
             JOIN public.harvester_classifications c ON c.id=p.classification_id \
             WHERE c.article_id=$1 ORDER BY c.entity_id,p.subject_id",
        )
        .bind(ARTICLE)
        .fetch_all(&pool)
        .await?;
        assert_eq!(insider_pairs, vec![(TEAM, PLAYER), (TEAM, PLAYER_TWO)]);
        let mentions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_entity_mentions WHERE article_id=$1 AND entity_type='team' AND entity_id=$2"
        ).bind(ARTICLE).bind(TEAM).fetch_one(&pool).await?;
        assert_eq!(mentions, 1);
        let alias_mentions: Vec<String> = sqlx::query_scalar(
            "SELECT matched_norm FROM public.harvester_entity_mentions WHERE article_id=$1 AND entity_type='team' AND entity_id=$2 ORDER BY matched_norm"
        ).bind(ARTICLE).bind(OTHER_TEAM).fetch_all(&pool).await?;
        assert_eq!(alias_mentions, vec!["community event"]);
        let resolved_links: Vec<i32> = sqlx::query_scalar(
            "SELECT entity_id FROM public.harvester_resolved_links WHERE article_id=$1 AND sport=$2 ORDER BY entity_id"
        ).bind(ARTICLE).bind(SPORT).fetch_all(&pool).await?;
        assert_eq!(resolved_links, vec![TEAM, PLAYER, PLAYER_TWO]);
        let shared_links: Vec<i32> = sqlx::query_scalar(
            "SELECT entity_id FROM public.news_article_entities WHERE article_id=$1 AND sport=$2 ORDER BY entity_id"
        ).bind(ARTICLE).bind(SPORT).fetch_all(&pool).await?;
        assert_eq!(shared_links, resolved_links);
        // A fresh Harvester reading replaces an obsolete shared link instead
        // of leaving a former Editor verdict to pollute graph maintenance.
        let mut link_replay = pool.begin().await?;
        sqlx::query("INSERT INTO public.news_article_entities(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
            .bind(ARTICLE).bind(OTHER_TEAM).bind(SPORT).execute(&mut *link_replay).await?;
        let opening = crate::plugins::harvester::cognition::first_paragraphs(body, 3);
        record_resolved_links(&mut link_replay, ARTICLE, SPORT, &opening.text).await?;
        link_replay.commit().await?;
        let shared_after_replay: Vec<i32> = sqlx::query_scalar(
            "SELECT entity_id FROM public.news_article_entities WHERE article_id=$1 AND sport=$2 ORDER BY entity_id"
        ).bind(ARTICLE).bind(SPORT).fetch_all(&pool).await?;
        assert_eq!(shared_after_replay, resolved_links);
        let unresolved_names: Vec<(String, String)> = sqlx::query_as(
            "SELECT matched_norm,reason FROM public.harvester_unresolved_names \
             WHERE article_id=$1 AND sport=$2 ORDER BY matched_norm",
        )
        .bind(ARTICLE)
        .bind(SPORT)
        .fetch_all(&pool)
        .await?;
        assert_eq!(
            unresolved_names,
            vec![
                ("community event".into(), "alias_only".into()),
                ("supporters".into(), "ambiguous_surface".into()),
            ]
        );
        let (heat, news_ids): (Option<i16>, Vec<i64>) = sqlx::query_as(
            "SELECT heat,news_ids FROM public.compute_harvester_transfer_heat($1,$2,$3,'player')",
        )
        .bind(TEAM)
        .bind(PLAYER)
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
        assert!(heat.is_some_and(|value| value > 0));
        assert_eq!(news_ids, vec![ARTICLE]);
        let transfer_candidates =
            crate::plugins::insider::adapter::load_harvester_source_candidates(
                &pool, ARTICLE, TEAM, SPORT,
            )
            .await?;
        assert_eq!(transfer_candidates.len(), 2);
        assert_eq!(transfer_candidates[0].player_id, PLAYER);
        assert_eq!(transfer_candidates[0].player_name, "Morgan Example");
        assert_eq!(transfer_candidates[1].player_id, PLAYER_TWO);
        assert_eq!(transfer_candidates[1].player_name, "Taylor Sample");
        assert!(
            crate::plugins::insider::adapter::load_harvester_source_candidates(
                &pool, ARTICLE, OTHER_TEAM, SPORT,
            )
            .await?
            .is_empty()
        );
        let editor_work: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.pipeline_work WHERE stage='editor' AND entity_id=$1",
        )
        .bind(ARTICLE as i32)
        .fetch_one(&pool)
        .await?;
        assert_eq!(editor_work, 0);
        let journalist_work: i64 = sqlx::query_scalar("SELECT count(*) FROM public.pipeline_work WHERE stage='narratives' AND entity_id IN ($1,$2) AND sport=$3")
            .bind(TEAM).bind(OTHER_TEAM).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(journalist_work, 2);
        let influencer_work: i64 = sqlx::query_scalar("SELECT count(*) FROM public.pipeline_work WHERE stage='vibe' AND entity_id IN ($1,$2) AND sport=$3")
            .bind(TEAM).bind(OTHER_TEAM).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(influencer_work, 2);
        let scout_work: i64 = sqlx::query_scalar("SELECT count(*) FROM public.pipeline_work WHERE stage='rating' AND entity_id IN ($1,$2) AND sport=$3")
            .bind(TEAM).bind(OTHER_TEAM).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(scout_work, 2);
        // Two verified sources for one entity must produce two dispositions.
        let second_body = "Harvester Test Club supporters cheered the plans for the next community event. The organizers invited local families. The club expects a large turnout.";
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,source,title,full_text) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(SECOND_ARTICLE).bind("harvester-smoke-9690104")
            .bind("https://example.test/harvester-smoke-second").bind("Example Wire")
            .bind("Harvester Test Club plans another event").bind(second_body)
            .execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',$2,$3,1)")
            .bind(SECOND_ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await?;
        let second_classification: i64 = sqlx::query_scalar(
            "INSERT INTO public.harvester_classifications \
             (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
              body_sha256,headline,model_input_start,model_input_end,model_input_text, \
              context_start,context_end,context_text,distributions,model_provenance) \
             VALUES ($1,'team',$2,$3,'harvest-context-v1','smoke-laya','relevant', \
                     $4,'Harvester Test Club plans another event',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb) RETURNING id"
        ).bind(SECOND_ARTICLE).bind(TEAM).bind(SPORT)
            .bind(hex::encode(Sha256::digest(second_body.as_bytes())))
            .bind(second_body.len() as i32).bind(second_body)
            .fetch_one(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_assignments(classification_id,plugin_id) VALUES($1,$2)",
        )
        .bind(second_classification)
        .bind(crate::plugins::influencer::manifest::MANIFEST.id.as_str())
        .execute(&pool)
        .await?;
        work::enqueue(
            &pool,
            &Item {
                stage: crate::plugins::influencer::manifest::TASK,
                entity_type: "team".into(),
                entity_id: i64::from(TEAM),
                sport: SPORT.into(),
                input_version: Some(format!("harvest-context-v1:c{second_classification}")),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        for (index, claimed) in work::claim(&pool, crate::plugins::influencer::manifest::TASK, 2)
            .await?
            .into_iter()
            .enumerate()
        {
            if index == 0 {
                let mut stale = claimed.clone();
                stale.claim_token = Some("00000000-0000-0000-0000-000000000001".into());
                assert_eq!(
                    crate::plugins::influencer::adapter::harvester::execute_with_backend(
                        &pool, &SmokeVibe, 4096, &stale
                    )
                    .await?,
                    PluginOutcome::Superseded
                );
            }
            let outcome = crate::plugins::influencer::adapter::harvester::execute_with_backend(
                &pool, &SmokeVibe, 4096, &claimed,
            )
            .await?;
            if claimed.entity_id == i64::from(TEAM) {
                assert!(matches!(outcome, PluginOutcome::Deferred { .. }));
                let still_pending: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM public.harvester_assignments WHERE classification_id=$1 AND status='pending'"
                ).bind(second_classification).fetch_one(&pool).await?;
                assert_eq!(still_pending, 1);
                assert!(work::defer(&pool, &claimed, Duration::ZERO, "next source").await?);
            } else {
                assert_eq!(outcome, PluginOutcome::Committed);
            }
        }
        let resumed = work::claim(&pool, crate::plugins::influencer::manifest::TASK, 1)
            .await?
            .remove(0);
        assert_eq!(resumed.entity_id, i64::from(TEAM));
        assert_eq!(
            crate::plugins::influencer::adapter::harvester::execute_with_backend(
                &pool, &SmokeVibe, 4096, &resumed
            )
            .await?,
            PluginOutcome::Committed
        );
        let second_status: String = sqlx::query_scalar(
            "SELECT status FROM public.harvester_assignments WHERE classification_id=$1",
        )
        .bind(second_classification)
        .fetch_one(&pool)
        .await?;
        assert_eq!(second_status, "used");
        let vibes: Vec<(i32, String, serde_json::Value)> = sqlx::query_as(
            "SELECT c.entity_id,d.status,d.product_ref FROM public.harvester_assignments d \
             JOIN public.harvester_classifications c ON c.id=d.classification_id \
             WHERE c.article_id=$1 AND d.plugin_id=$2 ORDER BY c.entity_id",
        )
        .bind(ARTICLE)
        .bind(crate::plugins::influencer::manifest::MANIFEST.id.as_str())
        .fetch_all(&pool)
        .await?;
        assert_eq!(vibes.len(), 2);
        assert_eq!(vibes[0].0, TEAM);
        assert_eq!(vibes[0].1, "used");
        assert!(vibes[0].2["vibe_score_id"].as_i64().is_some());
        assert_eq!(vibes[1].0, OTHER_TEAM);
        assert_eq!(vibes[1].1, "abstained");
        assert!(vibes[1].2["vibe_score_id"].is_null());
        for (_, _, provenance) in vibes {
            assert_eq!(provenance["article_id"], ARTICLE);
            assert_eq!(provenance["model_version"], "smoke-vibe");
            assert!(matches!(
                provenance["prompt_version"].as_str(),
                Some("vibe-source-v2" | "vibe-source-reaction-v2")
            ));
            assert!(provenance["input_hash"].as_str().is_some());
        }
        let insider_work = work::claim(&pool, crate::plugins::insider::manifest::TASK, 2).await?;
        assert_eq!(insider_work.len(), 2);
        for (index, claimed) in insider_work.into_iter().enumerate() {
            if index == 0 {
                let mut stale = claimed.clone();
                stale.claim_token = Some("00000000-0000-0000-0000-000000000002".into());
                assert_eq!(
                    crate::plugins::insider::adapter::harvester::execute_with_backend(
                        &pool,
                        &SmokeInsider,
                        4096,
                        &stale,
                    )
                    .await?,
                    PluginOutcome::Superseded
                );
            }
            let outcome = crate::plugins::insider::adapter::harvester::execute_with_backend(
                &pool,
                &SmokeInsider,
                4096,
                &claimed,
            )
            .await?;
            if claimed.entity_id == i64::from(TEAM) {
                assert!(matches!(outcome, PluginOutcome::Deferred { .. }));
                let pending_pairs: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM public.harvester_insider_pairs p \
                     JOIN public.harvester_classifications c ON c.id=p.classification_id \
                     WHERE c.article_id=$1 AND c.entity_id=$2 AND p.status='pending'",
                )
                .bind(ARTICLE)
                .bind(TEAM)
                .fetch_one(&pool)
                .await?;
                assert_eq!(pending_pairs, 1);
                assert!(work::defer(&pool, &claimed, Duration::ZERO, "next transfer pair").await?);
            } else {
                assert!(matches!(outcome, PluginOutcome::Deferred { .. }));
                assert!(work::defer(&pool, &claimed, Duration::ZERO, "Insider wraps").await?);
            }
        }
        let resumed_insider = work::claim(&pool, crate::plugins::insider::manifest::TASK, 1)
            .await?
            .remove(0);
        assert_eq!(resumed_insider.entity_id, i64::from(TEAM));
        assert!(matches!(
            crate::plugins::insider::adapter::harvester::execute_with_backend(
                &pool,
                &SmokeInsider,
                4096,
                &resumed_insider,
            )
            .await?,
            PluginOutcome::Deferred { .. }
        ));
        assert!(work::defer(&pool, &resumed_insider, Duration::ZERO, "Insider wraps").await?);
        for _ in 0..12 {
            let claimed = work::claim(&pool, crate::plugins::insider::manifest::TASK, 2).await?;
            if claimed.is_empty() {
                break;
            }
            for item in claimed {
                let outcome = crate::plugins::insider::adapter::harvester::execute_with_backend(
                    &pool,
                    &SmokeInsider,
                    4096,
                    &item,
                )
                .await?;
                if matches!(outcome, PluginOutcome::Deferred { .. }) {
                    assert!(work::defer(&pool, &item, Duration::ZERO, "next Insider wrap").await?);
                } else {
                    assert_eq!(outcome, PluginOutcome::Committed);
                }
            }
        }
        let pending_wraps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_insider_wraps WHERE sport=$1 AND status='pending'"
        ).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(pending_wraps, 0);
        let identity_reviews: Vec<(String, String)> = sqlx::query_as(
            "SELECT status,reason FROM public.harvester_insider_identity_reviews \
             WHERE sport=$1 ORDER BY transfer_rumor_id",
        )
        .bind(SPORT)
        .fetch_all(&pool)
        .await?;
        assert_eq!(
            identity_reviews,
            vec![
                ("skipped".into(), "missing_identity_threshold".into()),
                ("skipped".into(), "missing_identity_threshold".into())
            ]
        );
        let scored_wraps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_insider_wraps WHERE sport=$1 AND status='scored'"
        ).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(scored_wraps, 3);
        let skipped_wraps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_insider_wraps WHERE sport=$1 AND status='skipped'"
        ).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(skipped_wraps, 1);
        let unfinished_insider_work: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.pipeline_work WHERE sport=$1 AND stage='transfers' \
             AND status IN ('pending','running')",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
        assert_eq!(unfinished_insider_work, 0);
        let insider_statuses: Vec<(i32, String)> = sqlx::query_as(
            "SELECT c.entity_id,d.status FROM public.harvester_assignments d \
             JOIN public.harvester_classifications c ON c.id=d.classification_id \
             WHERE c.article_id=$1 AND d.plugin_id=$2 ORDER BY c.entity_id",
        )
        .bind(ARTICLE)
        .bind(crate::plugins::insider::manifest::MANIFEST.id.as_str())
        .fetch_all(&pool)
        .await?;
        assert_eq!(
            insider_statuses,
            vec![(TEAM, "used".into()), (OTHER_TEAM, "abstained".into())]
        );
        let pair_statuses: Vec<(i32, String)> = sqlx::query_as(
            "SELECT p.subject_id,p.status FROM public.harvester_insider_pairs p \
             JOIN public.harvester_classifications c ON c.id=p.classification_id \
             WHERE c.article_id=$1 AND c.entity_id=$2 ORDER BY p.subject_id",
        )
        .bind(ARTICLE)
        .bind(TEAM)
        .fetch_all(&pool)
        .await?;
        assert_eq!(
            pair_statuses,
            vec![(PLAYER, "rumor".into()), (PLAYER_TWO, "rumor".into())]
        );
        let rumor_source_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT unnest(input_news_ids) FROM public.transfer_rumors \
             WHERE team_id=$1 AND player_id IN ($2,$3) AND sport=$4 AND trigger_type='harvester' AND is_rumor=true ORDER BY player_id"
        ).bind(TEAM).bind(PLAYER).bind(PLAYER_TWO).bind(SPORT).fetch_all(&pool).await?;
        assert_eq!(rumor_source_ids, vec![ARTICLE, ARTICLE]);
        let junction_subjects: Vec<i32> = sqlx::query_scalar(
            "SELECT subject_id FROM public.narrative_events \
             WHERE sport=$1 AND article_id=$2 AND origin='junction' \
             ORDER BY subject_id",
        )
        .bind(SPORT)
        .bind(ARTICLE)
        .fetch_all(&pool)
        .await?;
        assert_eq!(junction_subjects, vec![PLAYER, PLAYER_TWO]);
        let scout_claims = work::claim(&pool, crate::plugins::scout::manifest::TASK, 2).await?;
        assert_eq!(scout_claims.len(), 2);
        for claimed in scout_claims {
            assert_eq!(
                crate::plugins::scout::adapter::harvester::execute_with_backend(
                    &pool,
                    &SmokeScout,
                    4096,
                    &claimed,
                )
                .await?,
                PluginOutcome::Committed
            );
        }
        let scout_statuses: Vec<(i32, String, serde_json::Value)> = sqlx::query_as(
            "SELECT c.entity_id,d.status,d.product_ref FROM public.harvester_assignments d \
             JOIN public.harvester_classifications c ON c.id=d.classification_id \
             WHERE c.article_id=$1 AND d.plugin_id=$2 ORDER BY c.entity_id",
        )
        .bind(ARTICLE)
        .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
        .fetch_all(&pool)
        .await?;
        assert_eq!(scout_statuses[0].0, TEAM);
        assert_eq!(scout_statuses[0].1, "used");
        assert_eq!(scout_statuses[0].2["source_role"], "trigger_only");
        assert_eq!(scout_statuses[0].2["identity_resolved"], true);
        let scout_summary_id = scout_statuses[0].2["stat_summary_id"].as_i64().unwrap();
        let scout_evidence: (serde_json::Value, String) = sqlx::query_as(
            "SELECT trigger_payload,input_components::text FROM public.stat_summaries WHERE id=$1",
        )
        .bind(scout_summary_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(scout_evidence.0["source_role"], "trigger_only");
        assert_eq!(scout_evidence.0["source_article_id"], ARTICLE);
        let scout_components: serde_json::Value = serde_json::from_str(&scout_evidence.1)?;
        assert_eq!(scout_components["harvester_trigger"]["article_id"], ARTICLE);
        assert_eq!(
            scout_components["harvester_trigger"]["source_role"],
            "trigger_only"
        );
        assert_eq!(scout_statuses[1].0, OTHER_TEAM);
        assert_eq!(scout_statuses[1].1, "relevant_but_unused");
        assert_eq!(scout_statuses[1].2["identity_resolved"], false);
        assert!(scout_statuses[1].2["stat_summary_id"].is_null());
        let availability_body = "Morgan Example was ruled out with a knee injury. Harvester Test Club expects another update next week.";
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,source,title,full_text) VALUES($1,$2,$3,'Example Wire','Harvester Test Club confirms injury',$4)")
            .bind(AVAILABILITY_ARTICLE).bind("harvester-smoke-availability-9690108")
            .bind("https://example.test/harvester-availability")
            .bind(availability_body).execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
            .bind(AVAILABILITY_ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await?;
        let availability_hash = hex::encode(Sha256::digest(availability_body.as_bytes()));
        let availability_classification: i64 = sqlx::query_scalar(
            "INSERT INTO public.harvester_classifications \
             (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
              body_sha256,headline,model_input_start,model_input_end,model_input_text, \
              context_start,context_end,context_text,distributions,model_provenance) \
             VALUES($1,'team',$2,$3,'harvest-context-v1','smoke-laya','relevant', \
                    $4,'Harvester Test Club confirms injury',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb) RETURNING id"
        ).bind(AVAILABILITY_ARTICLE).bind(TEAM).bind(SPORT).bind(&availability_hash)
            .bind(availability_body.len() as i32).bind(availability_body)
            .fetch_one(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_resolved_links \
             (article_id,entity_type,entity_id,sport,matched_norm,body_sha256,opening_sha256,resolution_method) \
             VALUES($1,'team',$2,$3,public.nrm('Harvester Test Club'),$4,$4,'unique_canonical_name_v1')")
            .bind(AVAILABILITY_ARTICLE).bind(TEAM).bind(SPORT).bind(&availability_hash)
            .execute(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_assignments(classification_id,plugin_id) VALUES($1,$2)",
        )
        .bind(availability_classification)
        .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
        .execute(&pool)
        .await?;
        let availability_id: i64 = sqlx::query_scalar(
            "INSERT INTO public.player_availability \
             (sport,player_id,team_id,kind,status,event_date,source_article_id,applied_at) \
             VALUES($1,$2,$3,'injury','applied',CURRENT_DATE,$4,now()) RETURNING id",
        )
        .bind(SPORT)
        .bind(PLAYER)
        .bind(TEAM)
        .bind(AVAILABILITY_ARTICLE)
        .fetch_one(&pool)
        .await?;
        work::enqueue(
            &pool,
            &Item {
                stage: crate::plugins::scout::manifest::TASK,
                entity_type: "team".into(),
                entity_id: i64::from(TEAM),
                sport: SPORT.into(),
                input_version: Some(format!("harvest-context-v1:c{availability_classification}")),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        let availability_claim = work::claim(&pool, crate::plugins::scout::manifest::TASK, 1)
            .await?
            .remove(0);
        assert_eq!(
            crate::plugins::scout::adapter::harvester::execute_with_backend(
                &pool,
                &SmokeScout,
                4096,
                &availability_claim
            )
            .await?,
            PluginOutcome::Committed
        );
        let availability_receipt: (String, serde_json::Value) = sqlx::query_as(
            "SELECT status,product_ref FROM public.harvester_assignments \
             WHERE classification_id=$1 AND plugin_id=$2",
        )
        .bind(availability_classification)
        .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
        .fetch_one(&pool)
        .await?;
        assert_eq!(availability_receipt.0, "used");
        assert_eq!(
            availability_receipt.1["structured_record_id"],
            availability_id
        );
        assert!(availability_receipt.1["stat_summary_id"].as_i64().is_some());
        let roster_body =
            "Morgan Example joined Harvester Test Club after leaving Another Test Club.";
        sqlx::query("INSERT INTO public.news_articles(id,url_hash,url,source,title,full_text) VALUES($1,$2,$3,'Example Wire','Morgan Example joined Harvester Test Club',$4)")
            .bind(ROSTER_ARTICLE).bind("harvester-smoke-roster-9690109")
            .bind("https://example.test/harvester-roster")
            .bind(roster_body).execute(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
            .bind(ROSTER_ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await?;
        let roster_hash = hex::encode(Sha256::digest(roster_body.as_bytes()));
        let roster_classification: i64 = sqlx::query_scalar(
            "INSERT INTO public.harvester_classifications \
             (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
              body_sha256,headline,model_input_start,model_input_end,model_input_text, \
              context_start,context_end,context_text,distributions,model_provenance) \
             VALUES($1,'team',$2,$3,'harvest-context-v1','smoke-laya','relevant', \
                    $4,'Morgan Example joined Harvester Test Club',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb) RETURNING id"
        ).bind(ROSTER_ARTICLE).bind(TEAM).bind(SPORT).bind(&roster_hash)
            .bind(roster_body.len() as i32).bind(roster_body)
            .fetch_one(&pool).await?;
        sqlx::query("INSERT INTO public.harvester_resolved_links \
             (article_id,entity_type,entity_id,sport,matched_norm,body_sha256,opening_sha256,resolution_method) \
             VALUES($1,'team',$2,$3,public.nrm('Harvester Test Club'),$4,$4,'unique_canonical_name_v1')")
            .bind(ROSTER_ARTICLE).bind(TEAM).bind(SPORT).bind(&roster_hash)
            .execute(&pool).await?;
        sqlx::query(
            "INSERT INTO public.harvester_assignments(classification_id,plugin_id) VALUES($1,$2)",
        )
        .bind(roster_classification)
        .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
        .execute(&pool)
        .await?;
        let roster_record_id: i64 = sqlx::query_scalar(
            "INSERT INTO public.transfer_identity_applications \
             (sport,player_id,old_team_id,new_team_id,deterministic_heat,deterministic_confidence, \
              decision,status,evidence,applied_at) \
             VALUES($1,$2,$3,$4,90,0.900,'apply','applied', \
                    jsonb_build_object('identity_evidence_article_ids',jsonb_build_array($5::bigint)),now()) RETURNING id"
        ).bind(SPORT).bind(PLAYER).bind(OTHER_TEAM).bind(TEAM).bind(ROSTER_ARTICLE)
            .fetch_one(&pool).await?;
        work::enqueue(
            &pool,
            &Item {
                stage: crate::plugins::scout::manifest::TASK,
                entity_type: "team".into(),
                entity_id: i64::from(TEAM),
                sport: SPORT.into(),
                input_version: Some(format!("harvest-context-v1:c{roster_classification}")),
                attempts: 0,
                claim_token: None,
            },
        )
        .await?;
        let roster_claim = work::claim(&pool, crate::plugins::scout::manifest::TASK, 1)
            .await?
            .remove(0);
        assert_eq!(
            crate::plugins::scout::adapter::harvester::execute_with_backend(
                &pool,
                &SmokeScout,
                4096,
                &roster_claim,
            )
            .await?,
            PluginOutcome::Committed
        );
        let roster_receipt: (String, serde_json::Value) = sqlx::query_as(
            "SELECT status,product_ref FROM public.harvester_assignments \
             WHERE classification_id=$1 AND plugin_id=$2",
        )
        .bind(roster_classification)
        .bind(crate::plugins::scout::manifest::MANIFEST.id.as_str())
        .fetch_one(&pool)
        .await?;
        assert_eq!(roster_receipt.0, "used");
        assert_eq!(roster_receipt.1["structured_record_id"], roster_record_id);
        assert!(roster_receipt.1["stat_summary_id"].as_i64().is_some());
        let current_reports =
            crate::evidence::personnel::load_harvester_scout_reports(&pool, "team", TEAM, SPORT)
                .await?;
        assert_eq!(current_reports.len(), 3);
        assert_eq!(current_reports[0].claim.article_id, ROSTER_ARTICLE);
        assert_eq!(current_reports[0].claim.story_type, "roster");
        assert_eq!(
            current_reports[0].claim.fact,
            "Morgan Example joined Harvester Test Club"
        );
        assert_eq!(current_reports[1].claim.article_id, AVAILABILITY_ARTICLE);
        assert_eq!(current_reports[1].claim.story_type, "injury");
        assert_eq!(
            current_reports[1].claim.fact,
            "Morgan Example was ruled out with a knee injury"
        );
        assert_eq!(current_reports[2].claim.article_id, ARTICLE);
        assert_eq!(current_reports[2].claim.story_type, "performance");
        assert!(!current_reports[2]
            .claim
            .fact
            .contains("Thin RSS description"));
        sqlx::query("UPDATE public.transfer_identity_applications SET status='reverted',reverted_at=now() WHERE id=$1")
            .bind(roster_record_id).execute(&pool).await?;
        assert!(crate::evidence::personnel::load_harvester_scout_reports(
            &pool, "team", TEAM, SPORT
        )
        .await
        .is_err());
        sqlx::query("UPDATE public.transfer_identity_applications SET status='applied',reverted_at=NULL WHERE id=$1")
            .bind(roster_record_id).execute(&pool).await?;
        sqlx::query("UPDATE public.player_availability SET reverted_at=now() WHERE id=$1")
            .bind(availability_id)
            .execute(&pool)
            .await?;
        assert!(crate::evidence::personnel::load_harvester_scout_reports(
            &pool, "team", TEAM, SPORT
        )
        .await
        .is_err());
        sqlx::query("UPDATE public.player_availability SET reverted_at=NULL WHERE id=$1")
            .bind(availability_id)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE public.news_articles SET full_text='altered source' WHERE id=$1")
            .bind(AVAILABILITY_ARTICLE)
            .execute(&pool)
            .await?;
        assert!(crate::evidence::personnel::load_harvester_scout_reports(
            &pool, "team", TEAM, SPORT,
        )
        .await
        .is_err());
        sqlx::query("UPDATE public.news_articles SET full_text=$2 WHERE id=$1")
            .bind(AVAILABILITY_ARTICLE)
            .bind(availability_body)
            .execute(&pool)
            .await?;
        let graph_work: i64 = sqlx::query_scalar("SELECT count(*) FROM public.pipeline_work WHERE stage='graph' AND entity_id=$1 AND sport=$2")
            .bind(ARTICLE as i32).bind(SPORT).fetch_one(&pool).await?;
        assert_eq!(graph_work, 1);
        sqlx::query("DELETE FROM public.pipeline_work WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.harvester_insider_wraps WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.insider_scores WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.stat_summaries WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query(
            "DELETE FROM public.player_availability WHERE sport=$1 AND source_article_id=$2",
        )
        .bind(SPORT)
        .bind(AVAILABILITY_ARTICLE)
        .execute(&pool)
        .await?;
        sqlx::query("DELETE FROM public.transfer_identity_applications WHERE id=$1")
            .bind(roster_record_id)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.team_stats WHERE sport=$1")
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.transfer_rumors WHERE team_id=$1 AND sport=$2")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(SECOND_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(AVAILABILITY_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ROSTER_ARTICLE)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id=$1 AND sport=$2")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.teams WHERE id=$1 AND sport=$2")
            .bind(OTHER_TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.players WHERE id=$1 AND sport=$2")
            .bind(PLAYER)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.players WHERE id=$1 AND sport=$2")
            .bind(PLAYER_TWO)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM public.entity_name_surfaces WHERE entity_type='team' AND entity_id=$1 AND sport=$2")
            .bind(TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query("DELETE FROM public.entity_name_surfaces WHERE entity_type='team' AND entity_id=$1 AND sport=$2")
            .bind(OTHER_TEAM).bind(SPORT).execute(&pool).await?;
        sqlx::query("DELETE FROM public.entity_name_surfaces WHERE entity_type='player' AND entity_id=$1 AND sport=$2")
            .bind(PLAYER).bind(SPORT).execute(&pool).await?;
        sqlx::query("DELETE FROM public.entity_name_surfaces WHERE entity_type='player' AND entity_id=$1 AND sport=$2")
            .bind(PLAYER_TWO).bind(SPORT).execute(&pool).await?;
        Ok::<(), anyhow::Error>(())
    }
}

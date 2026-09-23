//! Editor application: fetch and prepare, ask Studio, then publish under the exact queue claim.
use crate::application::models::Models;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::fetch::{
    content_hash, count_words, fetch_article, looks_paywalled, FetchedArticle, ARTICLE_MIN_WORDS,
};
use crate::plugins::editor::cognition::{
    derive, prompt, Assignment, EditorEntityRole, EditorRead, NameMention, EDITOR_CONTRACT_VERSION,
};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::plugin::{PluginManifest, PluginOutcome, ScheduledOperation, StudioPlugin};
use crate::studio::{Extracted, Generation, GenerationCall, Studio};
use crate::util::truncate;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgConnection, PgPool, Row};
use tracing::warn;
pub mod candidates;
mod maintenance;
pub mod nominate;
mod resolve;
pub mod storyline;
const EDITOR_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::editor::manifest::MANIFEST.id.as_str(),
    stage: "editor",
    lens: "editor",
    role: Role::Editor,
    product_table: "editor_reads",
    output_contract_version: EDITOR_CONTRACT_VERSION,
};
#[derive(Debug)]
pub struct EditorArticleRow {
    pub(crate) url: String,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) duplicate_of: Option<i64>,
}

/// Prepared work is either a no-write completion, a fetch/parser marker, or a Studio read.
/// Nothing here has been published yet.
enum Prepared {
    Unchanged,
    Terminal {
        status: &'static str,
        fetched: Option<FetchedArticle>,
        error: Option<String>,
    },
    Read {
        article: EditorArticleRow,
        fetched: FetchedArticle,
        body_hash: String,
        extracted: Box<Extracted<EditorRead>>,
    },
}

async fn prepare(pool: &sqlx::PgPool, models: &Models, item: &Item) -> Result<Prepared> {
    let Some(article) = load_article(pool, item.entity_id).await? else {
        return Ok(Prepared::Unchanged);
    };
    if article.duplicate_of.is_some() {
        return Ok(Prepared::Terminal {
            status: "duplicate",
            fetched: None,
            error: None,
        });
    }
    let fetched = match fetch_article(&article.url).await {
        Ok(f) => sanitize_fetched(f),
        Err(e) => {
            let error = format!("{e:#}");
            let status = if error.contains("HTTP 401") || error.contains("HTTP 403") {
                "blocked"
            } else {
                "fetch_failed"
            };
            return Ok(Prepared::Terminal {
                status,
                fetched: None,
                error: Some(error),
            });
        }
    };
    if count_words(&fetched.text) < ARTICLE_MIN_WORDS {
        let status = if looks_paywalled(&fetched.text) {
            "paywall"
        } else {
            "empty_body"
        };
        return Ok(Prepared::Terminal {
            status,
            fetched: Some(fetched),
            error: None,
        });
    }
    let body_hash = content_hash(&fetched.text);
    if read_is_current(pool, item.entity_id, &body_hash).await? {
        return Ok(Prepared::Unchanged);
    }
    let assignment = Assignment {
        source: article.source.clone(),
        title: article.title.clone(),
        description: article.description.clone(),
        text: fetched.text.clone(),
        hypothesis: load_hypothesis_entities(pool, item.entity_id, &item.sport).await?,
    };
    let model = models.router.for_role(Role::Editor);
    let extracted =
        crate::plugins::editor::cognition::read_article(&Studio::new(model.as_ref()), &assignment)
            .await?;
    Ok(Prepared::Read {
        article,
        fetched,
        body_hash,
        extracted: Box::new(extracted),
    })
}

/// All required effects are local Postgres writes, so a single transaction is simpler than
/// an outbox. Storyline state retains the packet compilation obligation; explicit queue
/// writes retain Investigator and Graph work before this claim is completed.
async fn commit_claimed(pool: &PgPool, item: &Item, prepared: &Prepared) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let tx = publication.transaction();
    match prepared {
        Prepared::Unchanged => {}
        Prepared::Terminal {
            status,
            fetched,
            error,
        } => {
            let url = fetched.as_ref().map(|f| f.final_url.as_str());
            let domain = fetched.as_ref().and_then(|f| f.final_domain.as_deref());
            let words = fetched.as_ref().map_or(0, |f| count_words(&f.text) as i32);
            persist_terminal(tx, item, status, url, domain, words, error.as_deref()).await?;
            insert_data_fetch_ledger(
                tx,
                item,
                status,
                url,
                url,
                domain,
                None,
                None,
                "no_call",
                error.as_deref(),
            )
            .await?;
        }
        Prepared::Read {
            article,
            fetched,
            body_hash,
            extracted,
        } => {
            let words = count_words(&fetched.text) as i32;
            let status = match &extracted.value {
                Some(read) => {
                    let resolved = if read.relevant {
                        resolve::resolve_names(tx, &item.sport, &read.names).await?
                    } else {
                        derive::Resolved::default()
                    };
                    let status = if read.relevant {
                        "success"
                    } else {
                        "irrelevant"
                    };
                    persist_read(
                        tx,
                        item.entity_id,
                        status,
                        fetched,
                        words,
                        body_hash,
                        read,
                        &resolved,
                        &extracted.model,
                    )
                    .await?;
                    nominate::nominate_fixture_from_result(tx, &item.sport, item.entity_id, read)
                        .await?;
                    candidates::sweep_candidates(
                        tx,
                        &item.sport,
                        item.entity_id,
                        &fetched.text,
                        &resolved,
                    )
                    .await?;
                    storyline::attach_read(
                        tx,
                        &item.sport,
                        item.entity_id,
                        &article.title,
                        read,
                        &resolved,
                        storyline::AttachMethod::Auto,
                        None,
                    )
                    .await?;
                    write_links(tx, item.entity_id, &item.sport, read.relevant, &resolved).await?;
                    if read.relevant {
                        harvest_team_links(tx, item.entity_id, &item.sport, read).await?;
                    }
                    enqueue_graph_for_article(tx, item.entity_id, &item.sport).await?;
                    status
                }
                None => {
                    persist_terminal(
                        tx,
                        item,
                        "parse_failed",
                        Some(&fetched.final_url),
                        fetched.final_domain.as_deref(),
                        words,
                        Some("editor read parser returned no committed blurb"),
                    )
                    .await?;
                    // A parser abstention still has a real model call and body provenance.
                    sqlx::query("UPDATE editor_reads SET model_version = $2, content_hash = $3, parser_outcome = 'fail_closed' WHERE article_id = $1")
                        .bind(item.entity_id).bind(&extracted.model).bind(body_hash).execute(&mut **tx).await?;
                    "parse_failed"
                }
            };
            insert_data_fetch_ledger(
                tx,
                item,
                status,
                Some(&article.url),
                Some(&fetched.final_url),
                fetched.final_domain.as_deref(),
                Some(body_hash),
                Some(&extracted.model),
                if extracted.value.is_some() {
                    "parsed"
                } else {
                    "fail_closed"
                },
                None,
            )
            .await?;
        }
    }
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}

pub struct EditorHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
    scheduled: Vec<std::sync::Arc<dyn ScheduledOperation>>,
}
impl EditorHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>, packet_compile: bool) -> Self {
        let scheduled = maintenance::operations(pool.clone(), packet_compile);
        Self {
            pool,
            models,
            scheduled,
        }
    }
}

#[async_trait]
impl StudioPlugin for EditorHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::editor::manifest::MANIFEST
    }

    fn scheduled_operations(&self) -> Vec<std::sync::Arc<dyn ScheduledOperation>> {
        self.scheduled.clone()
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        item.require_claim_token()?;
        let prepared = prepare(pool, models, item).await?;
        let outcome = commit_claimed(pool, item, &prepared).await?;
        if outcome == PluginOutcome::Committed {
            if let Prepared::Read {
                extracted,
                body_hash,
                ..
            } = &prepared
            {
                ledger_model_call(
                    pool,
                    item,
                    &extracted.model,
                    if extracted.value.is_some() {
                        "parsed"
                    } else {
                        "fail_closed"
                    },
                    extracted,
                    body_hash,
                )
                .await;
            }
        }
        Ok(outcome)
    }
}

/// Replaces an article's entity links in one transaction. Presence is the verdict;
/// an irrelevant read clears the set. `DISTINCT` collapses multiple matched surfaces.
async fn write_links(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
    relevant: bool,
    resolved: &derive::Resolved,
) -> Result<()> {
    let sport = sport.to_uppercase();

    sqlx::query("DELETE FROM public.news_article_entities WHERE article_id = $1 AND sport = $2")
        .bind(article_id)
        .bind(&sport)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("clear links for article {article_id}"))?;

    if relevant {
        // The resolver's links carry their own sport (the surface that matched decides it), so
        // filter rather than assume the article's.
        let types: Vec<String> = resolved
            .links
            .iter()
            .filter(|l| l.sport.eq_ignore_ascii_case(&sport))
            .map(|l| l.entity_type.clone())
            .collect();
        let ids: Vec<i32> = resolved
            .links
            .iter()
            .filter(|l| l.sport.eq_ignore_ascii_case(&sport))
            .map(|l| l.entity_id)
            .collect();

        if !types.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO public.news_article_entities (article_id, entity_type, entity_id, sport)
                SELECT DISTINCT $1, t.entity_type, t.entity_id, $2
                  FROM unnest($3::text[], $4::int[]) AS t(entity_type, entity_id)
                "#,
            )
            .bind(article_id)
            .bind(&sport)
            .bind(&types)
            .bind(&ids)
            .execute(&mut *conn)
            .await
            .with_context(|| format!("write editor links for article {article_id}"))?;
        }
    }

    Ok(())
}

/// Adds team links named word-bounded in key facts or person descriptors.
/// Names and aliases are database-backed; short aliases are excluded.
async fn harvest_team_links(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
    read: &EditorRead,
) -> Result<()> {
    let mut harvest = String::new();
    for f in &read.key_facts {
        harvest.push_str(f);
        harvest.push('\n');
    }
    for n in &read.names {
        harvest.push_str(&n.descriptor);
        harvest.push('\n');
    }
    if harvest.trim().is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO public.news_article_entities (article_id, entity_type, entity_id, sport)
        SELECT DISTINCT $1, 'team', t.id, $2
          FROM public.teams t
         WHERE t.sport = $2
           AND (
                 $3 ~* ('\m' || regexp_replace(t.name, '([^[:alnum:] ])', '\\\1', 'g') || '\M')
              OR EXISTS (
                    SELECT 1 FROM unnest(COALESCE(t.search_aliases, '{}')) AS a(alias)
                     WHERE length(a.alias) > 3
                       AND $3 ~* ('\m' || regexp_replace(a.alias, '([^[:alnum:] ])', '\\\1', 'g') || '\M')
                 )
           )
           AND NOT EXISTS (
                 SELECT 1 FROM public.news_article_entities nae
                  WHERE nae.article_id = $1 AND nae.entity_type = 'team'
                    AND nae.entity_id = t.id AND nae.sport = $2
           )
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .bind(&harvest)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("harvest team links for article {article_id}"))?;
    Ok(())
}

/// enqueue_graph_for_article is the Editor's graph hand-off, keyed on the EDITOR's read: the
/// input_version is `g:` || the editor read's content hash, so graph
/// re-runs when the article's TEXT changed and debounces when it did not — the same contract, off
/// the table that survives the cutover.
async fn enqueue_graph_for_article(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'graph', 'article', $1::integer, $2, 'pending',
               'g:' || er.content_hash, NOW(), NOW()
          FROM public.editor_reads er
         WHERE er.article_id = $1
           AND er.status = 'success'
           AND er.content_hash IS NOT NULL
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status        = 'pending',
            attempts      = 0,
            -- Pending rows keep their FIFO place (see work::enqueue).
            available_at  = CASE WHEN public.pipeline_work.status = 'pending'
                                 THEN public.pipeline_work.available_at
                                 ELSE NOW() END,
            updated_at    = NOW(),
            last_error    = NULL,
            input_version = EXCLUDED.input_version,
            running_input_version = NULL,
            claim_token = NULL
        WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
           OR public.pipeline_work.status = 'failed'
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("enqueue graph for article {article_id}"))?;
    Ok(())
}

/// Builds the exact production prompt for evaluation.
///
/// `Ok(None)` mirrors every path where the stage writes a terminal marker WITHOUT a model call
/// (article missing, duplicate, body too short or paywalled) — deterministic bookkeeping, not
/// judgments, so there is nothing for a model to be scored on. It fetches over the network,
/// exactly as the stage does.
pub async fn build_editor_prompt_for_eval(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Option<String>> {
    let Some(article) = load_article(pool, article_id).await? else {
        return Ok(None);
    };
    if article.duplicate_of.is_some() {
        return Ok(None);
    }
    let fetched = sanitize_fetched(fetch_article(&article.url).await?);
    if count_words(&fetched.text) < ARTICLE_MIN_WORDS {
        return Ok(None);
    }
    let hypothesis = load_hypothesis_entities(pool, article_id, sport).await?;
    Ok(Some(prompt::build_editor_prompt_parts(
        &article.source,
        &article.title,
        &article.description,
        &fetched.text,
        &hypothesis,
    )))
}

async fn load_article(pool: &sqlx::PgPool, article_id: i64) -> Result<Option<EditorArticleRow>> {
    let row = sqlx::query(
        r#"
        SELECT a.url, COALESCE(a.source, '') AS source, a.title,
               COALESCE(a.description, '') AS description, a.duplicate_of
        FROM public.news_articles a
        WHERE a.id = $1
        "#,
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await
    .context("load article for editor")?;

    Ok(row.map(|r| EditorArticleRow {
        url: r.get("url"),
        source: r.get("source"),
        title: r.get("title"),
        description: r.get("description"),
        duplicate_of: r.get("duplicate_of"),
    }))
}

/// Loads the query entity from article provenance. Previous resolved links are excluded so
/// a re-read cannot feed the model its own answer.
async fn load_hypothesis_entities(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Vec<String>> {
    let rows: Vec<(String, i32, String)> = sqlx::query_as(
        r#"
        SELECT 'team', t.id, t.name
          FROM public.news_articles a
          JOIN public.teams t
            ON t.id = (a.raw->>'query_team_id')::int AND t.sport = $2
         WHERE a.id = $1 AND a.raw ? 'query_team_id'
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .fetch_all(pool)
    .await
    .context("load editor hypothesis entities")?;

    let mut entities = Vec::new();
    for (entity_type, entity_id, name) in rows {
        if name.is_empty() {
            continue;
        }
        let context =
            crate::evidence::memories::load_identity_record(pool, &entity_type, entity_id, sport)
                .await?;
        entities.push(format!(
            "{name} ({entity_type} {entity_id})\n{}",
            context.unwrap_or_default()
        ));
    }
    Ok(entities)
}

/// read_is_current reports whether this article's read is already settled for this exact body
/// under the current contract — the T1 cache key.
async fn read_is_current(pool: &sqlx::PgPool, article_id: i64, body_hash: &str) -> Result<bool> {
    let hit: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status
        FROM public.editor_reads
        WHERE article_id = $1
          AND status IN ('success', 'irrelevant')
          AND content_hash = $2
          AND contract_version = $3
        "#,
    )
    .bind(article_id)
    .bind(body_hash)
    .bind(EDITOR_CONTRACT_VERSION)
    .fetch_optional(pool)
    .await
    .context("check existing editor read")?;
    Ok(hit.is_some())
}

async fn persist_terminal(
    conn: &mut PgConnection,
    item: &Item,
    status: &str,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    extracted_words: i32,
    last_error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.editor_reads (
            article_id, status, contract_version, model_version, parser_outcome,
            last_error, final_url, final_domain, content_hash, extracted_words,
            read, resolved, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, NULL, 'no_call',
            $4, $5, $6, NULL, $7,
            '{}'::jsonb, '{}'::jsonb, NOW(), NOW()
        )
        ON CONFLICT (article_id) DO UPDATE SET
            status = EXCLUDED.status,
            contract_version = EXCLUDED.contract_version,
            model_version = NULL,
            parser_outcome = EXCLUDED.parser_outcome,
            last_error = EXCLUDED.last_error,
            final_url = EXCLUDED.final_url,
            final_domain = EXCLUDED.final_domain,
            content_hash = NULL,
            extracted_words = EXCLUDED.extracted_words,
            read = '{}'::jsonb,
            resolved = '{}'::jsonb,
            fetched_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(item.entity_id)
    .bind(status)
    .bind(EDITOR_CONTRACT_VERSION)
    .bind(last_error.map(|e| truncate(e, 1000)))
    .bind(final_url)
    .bind(final_domain)
    .bind(extracted_words)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("persist editor terminal {}", item.entity_id))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn persist_read(
    conn: &mut PgConnection,
    article_id: i64,
    status: &str,
    fetched: &FetchedArticle,
    extracted_words: i32,
    body_hash: &str,
    read: &EditorRead,
    resolved: &derive::Resolved,
    model_version: &str,
) -> Result<()> {
    // Retain bodies for deterministic evidence slicing; avoid churning unchanged text.
    sqlx::query(
        r#"
        UPDATE public.news_articles
           SET full_text = $2
         WHERE id = $1
           AND full_text IS DISTINCT FROM $2
        "#,
    )
    .bind(article_id)
    .bind(&fetched.text)
    .execute(&mut *conn)
    .await
    .with_context(|| format!("persist article full_text {article_id}"))?;

    sqlx::query(
        r#"
        INSERT INTO public.editor_reads (
            article_id, status, contract_version, model_version, parser_outcome,
            last_error, final_url, final_domain, content_hash, extracted_words,
            read, resolved, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, 'parsed',
            NULL, $5, $6, $7, $8,
            $9::jsonb, $10::jsonb, NOW(), NOW()
        )
        ON CONFLICT (article_id) DO UPDATE SET
            status = EXCLUDED.status,
            contract_version = EXCLUDED.contract_version,
            model_version = EXCLUDED.model_version,
            parser_outcome = EXCLUDED.parser_outcome,
            last_error = NULL,
            final_url = EXCLUDED.final_url,
            final_domain = EXCLUDED.final_domain,
            content_hash = EXCLUDED.content_hash,
            extracted_words = EXCLUDED.extracted_words,
            read = EXCLUDED.read,
            resolved = EXCLUDED.resolved,
            fetched_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(article_id)
    .bind(status)
    .bind(EDITOR_CONTRACT_VERSION)
    .bind(model_version)
    .bind(&fetched.final_url)
    .bind(fetched.final_domain.as_deref())
    .bind(body_hash)
    .bind(extracted_words)
    .bind(read.envelope())
    .bind(resolved.to_json())
    .execute(&mut *conn)
    .await
    .with_context(|| format!("persist editor read {status} {article_id}"))?;

    Ok(())
}

/// Records one data-fetch ledger row per attempt.
#[allow(clippy::too_many_arguments)]
async fn insert_data_fetch_ledger(
    conn: &mut PgConnection,
    item: &Item,
    status: &str,
    source_url: Option<&str>,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    body_hash: Option<&str>,
    model_version: Option<&str>,
    parser_outcome: &str,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.data_fetch_ledger (
            target_type, target_id, sport, stage, status, source_url, final_url, final_domain,
            content_hash, model_version, prompt_version, output_contract_version,
            parser_outcome, error, generated_at
        ) VALUES (
            'article', $1, $2, 'editor', $3, $4, $5, $6,
            $7, $8, $9, $9, $10, $11, NOW()
        )
        "#,
    )
    .bind(item.entity_id)
    .bind(item.sport.to_uppercase())
    .bind(status)
    .bind(source_url)
    .bind(final_url)
    .bind(final_domain)
    .bind(body_hash)
    .bind(model_version)
    .bind(EDITOR_CONTRACT_VERSION)
    .bind(parser_outcome)
    .bind(error.map(|e| truncate(e, 1000)))
    .execute(&mut *conn)
    .await
    .context("record editor fetch provenance")?;
    Ok(())
}

/// Records one cognition-ledger row per model call.
async fn ledger_model_call(
    pool: &sqlx::PgPool,
    item: &Item,
    model: &str,
    parser_outcome: &str,
    extracted: &Extracted<EditorRead>,
    body_hash: &str,
) {
    let entity_id = match item.entity_id_i32() {
        Ok(id) => id,
        Err(e) => {
            warn!(article_id = item.entity_id, error = %e, "editor: ledger skipped");
            return;
        }
    };
    let generation = Generation::called(
        (),
        model.to_string(),
        EDITOR_CONTRACT_VERSION,
        vec![item.entity_id],
        Some(body_hash.to_string()),
        GenerationCall::from(extracted),
    );
    insert_generation_ledger_best_effort(
        pool,
        &generation,
        EDITOR_LEDGER,
        LedgerEvent {
            entity_type: "article",
            entity_id,
            sport: &item.sport.to_uppercase(),
            pair_entity: None,
            trigger_type: "queue",
            trigger_payload: json!({}),
            product_row_ids: vec![item.entity_id],
            included_evidence: json!({}),
            excluded_evidence: json!({}),
            context_budget: generation.context_budget(json!({})),
            parser_outcome,
        },
    )
    .await;
}

/// Drops NUL bytes before hashing, prompting, or persisting article text.
fn sanitize_fetched(mut fetched: FetchedArticle) -> FetchedArticle {
    // The common case allocates nothing: bodies with no NUL pass through untouched, byte-identical.
    if fetched.text.contains('\0') {
        fetched.text = fetched.text.replace('\0', "");
    }
    fetched
}

#[cfg(test)]
mod tests;

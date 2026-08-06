//! Editor stage (greenfield, PLAN-one-rail Phase 3) — the one seat that reads every arrival.
//!
//! Shadow mode: fetch the publisher page via `crate::fetch`, persist the body to
//! `news_articles.full_text`, read on the ep1 contract, derive in code (`derive.rs`), and write
//! `editor_reads`. It writes NOTHING the legacy rail reads — no `news_article_entities`, no
//! `bucket`, no `routing_tags`, no downstream enqueues. The legacy `article_read` stage keeps
//! running untouched beside it until cutover; forks (box scores, nominations, graph, the Desk)
//! arrive in Phases 4–6.

use crate::fetch::{
    content_hash, count_words, fetch_article, looks_paywalled, FetchedArticle, ARTICLE_MIN_WORDS,
};
use crate::harness::{Harness, Parser};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::{StageHandler, ARCHBOX_GEMMA_SLOTS};
use crate::util::truncate;
use crate::work::{Item, Stage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use tracing::warn;

pub mod candidates;
pub mod derive;
pub mod nominate;
pub mod packet;
pub mod render;
pub mod prompt;
pub mod storyline;
pub use prompt::{build_editor_prompt_parts, EDITOR_CONTRACT_VERSION, EDITOR_SYSTEM_PROMPT};

const EDITOR_NUM_PREDICT: i32 = 900;
/// The context size the pinned gemma3:4b runner is loaded with. Must agree with the legacy
/// seat's (`article_reader::ARTICLE_NUM_CTX`) and graph's for as long as those stages live —
/// ollama reloads the runner whenever a request asks for a different `num_ctx`, and all three
/// share one card. A unit test pins the agreement.
pub(crate) const EDITOR_NUM_CTX: i32 = 8192;

/// The model budget for one editor read — one definition for the stage and `bin/eval`, so a
/// fixture can never be scored under options production does not send. Temperature 0.2 live;
/// the eval overrides it per case.
///
/// Both schema forms travel together: `format_schema_raw` is what Ollama receives (order-true),
/// `format_schema` is the `Value` the ledger/eval capture path stores.
pub fn editor_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(EDITOR_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: EDITOR_NUM_PREDICT,
        num_ctx: EDITOR_NUM_CTX,
        json_mode: false,
        format_schema: Some(editor_format_schema()),
        format_schema_raw: Some(EDITOR_FORMAT_SCHEMA_RAW.to_string()),
    }
}

/// FIELD ORDER IS THE CONTRACT (§1a; the ar4 lesson): constrained decoding emits properties in
/// schema order, so extraction comes before anything a judgment could lean on. This is a RAW
/// JSON literal, not a `json!` value, because `serde_json`'s map is BTreeMap-backed — a schema
/// that travels as a `Value` reaches Ollama ALPHABETIZED, and the grammar then forces emission
/// in that accidental order (measured on the first ep1 gate run: `entity_roles` emitted second,
/// before a single name or fact was written — the ar3 shape). The raw string is POSTed
/// byte-for-byte via `format_schema_raw`.
///
/// `relevant` is absent — the model is never given the question (ar6, carried forward);
/// `co_mentions` is gone (the numbered-candidate loop dies with the legacy rail). The literal
/// template at the end of [`EDITOR_SYSTEM_PROMPT`] must match this order.
pub const EDITOR_FORMAT_SCHEMA_RAW: &str = r#"{
    "type": "object",
    "properties": {
        "source_language": { "type": "string" },
        "page_kind": { "type": "string", "enum": [
            "article", "score_table", "listing_or_schedule", "video_clip", "roundup", "other"
        ] },
        "names": {
            "type": "array",
            "maxItems": 12,
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "kind_hint": { "type": "string", "enum": [
                        "person", "club", "national_team", "other"
                    ] },
                    "descriptor": { "type": "string" }
                },
                "required": ["name", "kind_hint", "descriptor"]
            }
        },
        "entity_roles": {
            "type": "array",
            "maxItems": 12,
            "items": {
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "role": { "type": "string", "enum": [
                        "subject", "opponent", "passing_mention", "absent"
                    ] }
                },
                "required": ["entity", "role"]
            }
        },
        "story_type": { "type": "string", "enum": [
            "transfer", "injury", "performance", "fixture", "roster", "contract", "general"
        ] },
        "result_line": { "type": "string" },
        "register_phrase": { "type": "string" },
        "register": { "type": "string", "enum": [
            "celebration", "outrage", "resignation", "anticipation", "neutral"
        ] },
        "key_facts": { "type": "array", "items": { "type": "string" }, "maxItems": 8 },
        "caveats": { "type": "string" },
        "evidence_blurb": { "type": "string" }
    },
    "required": ["source_language", "page_kind", "names", "entity_roles", "story_type",
                 "result_line", "register_phrase", "register", "key_facts", "caveats",
                 "evidence_blurb"]
}"#;

/// The ep1 schema as a `Value` — the ledger/eval capture form. Parsed from the raw literal so
/// the two can never disagree on CONTENT; only the raw form carries the order.
pub fn editor_format_schema() -> serde_json::Value {
    serde_json::from_str(EDITOR_FORMAT_SCHEMA_RAW)
        .expect("EDITOR_FORMAT_SCHEMA_RAW is valid JSON (unit-tested)")
}

#[derive(Debug)]
pub struct EditorArticleRow {
    pub(crate) url: String,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) duplicate_of: Option<i64>,
}

/// One `names[]` entry — ep1's discovery channel.
#[derive(Clone, Debug, Deserialize)]
pub struct NameMention {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind_hint: String,
    #[serde(default)]
    pub descriptor: String,
}

/// One hypothesis entity's part in the article text.
#[derive(Clone, Debug, Deserialize)]
pub struct EditorEntityRole {
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub role: String,
}

/// The parsed ep1 envelope. `relevant` is DERIVED, never deserialized — the model is not asked
/// (`#[serde(skip)]`, the ar6 discipline carried forward).
#[derive(Clone, Debug, Deserialize)]
pub struct EditorRead {
    #[serde(skip)]
    pub relevant: bool,
    #[serde(default)]
    pub source_language: String,
    #[serde(default)]
    pub page_kind: String,
    #[serde(default)]
    pub names: Vec<NameMention>,
    #[serde(default)]
    pub entity_roles: Vec<EditorEntityRole>,
    #[serde(default)]
    pub story_type: String,
    #[serde(default)]
    pub result_line: String,
    #[serde(default)]
    pub register_phrase: String,
    #[serde(default)]
    pub register: String,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub caveats: String,
    pub evidence_blurb: String,
}

impl EditorRead {
    /// The full ep1 model envelope, persisted verbatim as `editor_reads.read` — model fields
    /// only, in schema order; the derived verdict lives in `status`, the resolver outcome in
    /// `resolved` (§1a).
    pub fn envelope(&self) -> serde_json::Value {
        json!({
            "source_language": self.source_language,
            "page_kind": self.page_kind,
            "names": self.names.iter().map(|n| json!({
                "name": n.name,
                "kind_hint": n.kind_hint,
                "descriptor": n.descriptor,
            })).collect::<Vec<_>>(),
            "entity_roles": self.entity_roles.iter().map(|r| json!({
                "entity": r.entity,
                "role": r.role,
            })).collect::<Vec<_>>(),
            "story_type": self.story_type,
            "result_line": self.result_line,
            "register_phrase": self.register_phrase,
            "register": self.register,
            "key_facts": self.key_facts,
            "caveats": self.caveats,
            "evidence_blurb": self.evidence_blurb,
        })
    }
}

/// The closed register vocabulary — same set as the legacy seat's; E1 routes on `!= neutral`.
pub const EDITOR_REGISTERS: &[&str] = &[
    "celebration",
    "outrage",
    "resignation",
    "anticipation",
    "neutral",
];

/// Carries the hypothesis entity names (decorated `Name (team 42)`), because relevance cannot be
/// derived without them — only OUR entities get a vote.
pub struct EditorReadParser<'a> {
    pub hypothesis: &'a [String],
}

impl Parser<EditorRead> for EditorReadParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<EditorRead>> {
        let Some(slice) = json_object_slice(raw) else {
            return Ok(None);
        };
        let mut read: EditorRead = serde_json::from_str(slice)
            .with_context(|| format!("parse editor read (raw={:?})", truncate(raw, 200)))?;
        read.evidence_blurb = normalize_space(&read.evidence_blurb);
        read.source_language = normalize_language_code(&read.source_language);
        read.page_kind = normalize_space(&read.page_kind);
        read.story_type = normalize_space(&read.story_type);
        read.caveats = normalize_space(&read.caveats);
        read.result_line = normalize_space(&read.result_line);
        read.key_facts = read
            .key_facts
            .into_iter()
            .map(|s| normalize_space(&s))
            .filter(|s| !s.is_empty())
            .take(8)
            .collect();
        read.names = read
            .names
            .into_iter()
            .map(|mut n| {
                n.name = normalize_space(&n.name);
                n.kind_hint = normalize_space(&n.kind_hint).to_lowercase();
                // The contract says ≤6 words FROM the text; enforce here so the derivation and
                // the archive both see the contract's descriptor, not the model's essay.
                n.descriptor = normalize_space(&n.descriptor)
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !matches!(
                    n.kind_hint.as_str(),
                    "person" | "club" | "national_team" | "other"
                ) {
                    n.kind_hint = "other".to_string();
                }
                n
            })
            .filter(|n| !n.name.is_empty())
            .take(12)
            .collect();
        read.entity_roles.retain(|r| !r.entity.trim().is_empty());
        // Register is DERIVED-normalized, not trusted (the C2 discipline): the prompt states
        // "an empty register_phrase means neutral", and enforcing it here makes the prompt and
        // the persisted value provably agree. Out-of-enum also falls back to neutral.
        read.register_phrase = normalize_space(&read.register_phrase);
        read.register = normalize_space(&read.register).to_lowercase();
        if read.register_phrase.is_empty() || !EDITOR_REGISTERS.contains(&read.register.as_str()) {
            read.register = "neutral".to_string();
        }
        // The verdict is COMPUTED here, not read (`relevant` is `#[serde(skip)]`).
        read.relevant = derive::derive_relevance(
            &read.page_kind,
            &read.entity_roles,
            self.hypothesis,
            &read.names,
        );
        if read.evidence_blurb.is_empty() {
            if !read.relevant {
                read.evidence_blurb =
                    "Full text is not materially about the hypothesis entities.".to_string();
                return Ok(Some(read));
            }
            return Ok(None);
        }
        Ok(Some(read))
    }
}

pub struct EditorHandler;

impl EditorHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EditorHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for EditorHandler {
    fn stage(&self) -> Stage {
        Stage::Editor
    }

    /// Eight, like the legacy seat: this IS a model stage, so a big batch would starve the
    /// rotation, but one item per rotation wastes more wall clock waiting than working.
    fn rotation_batch(&self) -> i64 {
        8
    }

    /// Up to the whole gemma3 card when its group-mates are idle; bounded by
    /// [`ARCHBOX_GEMMA_SLOTS`], so graph and the legacy reader reclaim their share the moment
    /// they have work.
    fn max_in_flight(&self) -> usize {
        ARCHBOX_GEMMA_SLOTS.1
    }

    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(ARCHBOX_GEMMA_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let article_id = item.entity_id;
        let Some(article) = load_article(&hx.pool, article_id).await? else {
            return Ok(());
        };

        // The mig-196 exact-title sweep already picked a canonical; the duplicate is free
        // coverage (~5–12% of arrivals). No fetch, no model call, no attach.
        if article.duplicate_of.is_some() {
            persist_terminal(hx, item, "duplicate", None, None, 0, None).await?;
            return Ok(());
        }

        let fetched = match fetch_article(&article.url).await {
            Ok(f) => sanitize_fetched(f),
            Err(e) => {
                let msg = format!("{e:#}");
                // fetch.rs surfaces auth walls as "article HTTP 401/403"; those are the
                // taxonomy's `blocked`, distinct from a transport-level `fetch_failed`.
                let status = if msg.contains("HTTP 401") || msg.contains("HTTP 403") {
                    "blocked"
                } else {
                    "fetch_failed"
                };
                persist_terminal(hx, item, status, None, None, 0, Some(&msg)).await?;
                return Ok(());
            }
        };

        let word_count = count_words(&fetched.text);
        if word_count < ARTICLE_MIN_WORDS {
            let status = if looks_paywalled(&fetched.text) {
                "paywall"
            } else {
                "empty_body"
            };
            persist_terminal(
                hx,
                item,
                status,
                Some(&fetched.final_url),
                fetched.final_domain.as_deref(),
                word_count as i32,
                None,
            )
            .await?;
            return Ok(());
        }

        let body_hash = content_hash(&fetched.text);
        // contract_version + content_hash are the cache key (T1): identical content under the
        // same contract is settled; a re-enqueue costs nothing.
        if read_is_current(&hx.pool, article_id, &body_hash).await? {
            return Ok(());
        }

        let hypothesis = load_hypothesis_entities(&hx.pool, article_id, &item.sport).await?;
        let prompt = prompt::build_editor_prompt_parts(
            &article.source,
            &article.title,
            &article.description,
            &fetched.text,
            &hypothesis,
        );
        let opts = editor_opts();
        let extracted = hx
            .extract(
                Role::Editor,
                &prompt,
                &opts,
                &EditorReadParser {
                    hypothesis: &hypothesis,
                },
            )
            .await?;

        let Some(read) = extracted.value.clone() else {
            persist_terminal(
                hx,
                item,
                "parse_failed",
                Some(&fetched.final_url),
                fetched.final_domain.as_deref(),
                word_count as i32,
                Some("editor read parser returned no committed blurb"),
            )
            .await?;
            ledger_model_call(hx, item, &extracted.model, "fail_closed", &extracted, &body_hash)
                .await;
            return Ok(());
        };

        // Resolve only a relevant read: an irrelevant page (score table, roundup, name
        // collision) mentions many names, and nominating from it would seed the Investigator's
        // queue with noise.
        let resolved = if read.relevant {
            derive::resolve_names(&hx.pool, &item.sport, &read.names).await?
        } else {
            derive::Resolved::default()
        };

        let status = if read.relevant { "success" } else { "irrelevant" };
        persist_read(
            hx,
            article_id,
            status,
            &fetched,
            word_count as i32,
            &body_hash,
            &read,
            &resolved,
            &extracted.model,
        )
        .await?;
        insert_data_fetch_ledger_best_effort(
            hx,
            item,
            status,
            Some(&article.url),
            Some(&fetched.final_url),
            fetched.final_domain.as_deref(),
            Some(&body_hash),
            Some(&extracted.model),
            "parsed",
            None,
        )
        .await;
        ledger_model_call(hx, item, &extracted.model, "parsed", &extracted, &body_hash).await;
        // The box-score fork (Phase 4.4) — the ONE live downstream of the Editor before
        // cutover: a parsed result_line whose teams both resolve upserts a completed
        // fixture; the existing fixture_boxscore_enqueue_on_final trigger owns the enqueue.
        // Best-effort: the read is persisted, and a nomination hiccup must not re-run the
        // model call. The other forks (nominations → Phase 5, packets/graph → Phases 6–7)
        // remain OFF in shadow.
        nominate::nominate_best_effort(&hx.pool, &item.sport, article_id, &read).await;
        // The nomination sweep (Phase 5.1/5.2): the resolver's leftovers become durable
        // candidates + evidence mentions; person-with-descriptor and refused ties enqueue
        // investigate_entity. Irrelevant reads carry an empty Resolved, so this no-ops for
        // them by construction. Best-effort for the same reason as the fixture fork.
        if let Err(e) =
            candidates::sweep_candidates(&hx.pool, &item.sport, article_id, &fetched.text, &resolved)
                .await
        {
            tracing::warn!(
                article_id,
                error = %format!("{e:#}"),
                "nomination sweep failed (read already persisted; continuing)"
            );
        }
        // The Desk (Phase 6.1): the read joins a story. Last in the handle, after the read is
        // persisted and after the nomination sweep, because the attachment scores the resolver's
        // links — which only exist once the read committed. Irrelevant reads carry an empty
        // `Resolved` and attach to nothing by construction.
        storyline::attach_best_effort(
            &hx.pool,
            &item.sport,
            article_id,
            &article.title,
            &read,
            &resolved,
        )
        .await;
        // The G1 seam (7.13): on the packet rail the Editor hands graph its article, because the
        // legacy `article_read` seat that enqueues it today (mig 193) is what Phase 8 turns off.
        // Gated, so until the flip graph keeps riding article_read and this is dead code with a
        // live test — the alternative, wiring it at flip time, is a seam nobody has ever run.
        //
        // AFTER the link writes on purpose: graph's candidate list is
        // `news_article_entities WHERE vetted`, so an enqueue that beat the links would extract
        // against an empty candidate set and fail closed for a reason that has nothing to do with
        // the article.
        if hx.rail.is_packet() {
            // 8.5, and it must come FIRST: the link writes are what `enqueue_graph_for_article`
            // below reads as its candidate set.
            if let Err(e) =
                write_links(&hx.pool, article_id, &item.sport, read.relevant, &resolved).await
            {
                tracing::warn!(
                    article_id,
                    error = %format!("{e:#}"),
                    "editor link write failed (read already persisted; continuing)"
                );
            }
            if let Err(e) = enqueue_graph_for_article(&hx.pool, article_id, &item.sport).await {
                tracing::warn!(
                    article_id,
                    error = %format!("{e:#}"),
                    "graph enqueue failed (read already persisted; continuing)"
                );
            }
        }
        Ok(())
    }
}


/// write_links records which entities an article is about. The Editor is the only writer.
///
/// **The whole function is: clear this article's links, write the ones the model resolved.** Two
/// statements, one transaction. That is the entire contract.
///
/// It used to be three reconciliation arms — confirm / deny / retract — juggling a tri-state
/// `vetted` column, a `match_confidence` sentinel, and `scrubbed_at`, because TWO writers shared
/// this table: Go wrote a 0.95 "query hypothesis" row at ingest and the Editor wrote a verdict
/// later. Every one of those arms existed to keep two writers coherent in one table, and that
/// shape is what left room for a bug that silently dropped an article's entire link set
/// (8.10). Go no longer writes links at all, so there is nothing to reconcile with:
///
///   * **A row exists ⟺ the Editor read the article and resolved that entity in it.** No column
///     encodes the verdict any more; presence IS the verdict, absence IS the denial. Nothing
///     downstream filters on `vetted` because there is nothing to filter — every row is a link a
///     model established by reading the body.
///   * **An irrelevant read is the DELETE alone.** No second statement, no retraction pass.
///   * **A re-read replaces the set.** Delete-then-insert is idempotent by construction, which is
///     why no `ON CONFLICT` clause appears here at all.
///
/// `SELECT DISTINCT` is still required and is not ceremony: the model names whoever the article
/// names, so one entity can arrive twice under two surfaces ("Spurs" and "Tottenham Hotspur" share
/// a `nrm()` norm), and two identical rows in one INSERT violate the primary key whether or not the
/// table was just cleared. The projection here is exactly the key columns, so a plain DISTINCT is a
/// distinct-on-the-key. Collapsing costs nothing: `editor_reads.resolved` keeps every mention with
/// its `via_surface`, so the model's full account survives at mention grain. The model describes,
/// code derives one link per entity (T2).
///
/// One consequence worth knowing: `created_at` is the moment the link was established, so a re-read
/// restamps it. Re-reads are rare (a re-enqueue), and the alternative — preserving a timestamp for a
/// link the current read may not even agree with — is worse.
async fn write_links(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
    relevant: bool,
    resolved: &derive::Resolved,
) -> Result<()> {
    let sport = sport.to_uppercase();
    let mut tx = pool.begin().await.context("begin editor link write")?;

    sqlx::query("DELETE FROM public.news_article_entities WHERE article_id = $1 AND sport = $2")
        .bind(article_id)
        .bind(&sport)
        .execute(&mut *tx)
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
            .execute(&mut *tx)
            .await
            .with_context(|| format!("write editor links for article {article_id}"))?;
        }
    }

    tx.commit().await.context("commit editor link write")?;
    Ok(())
}

/// enqueue_graph_for_article is the Editor's copy of the legacy seat's graph hand-off
/// (`article_reader::enqueue_graph_for_article`), keyed on the EDITOR's read instead of
/// `news_article_readings`: the input_version is `g:` || the editor read's content hash, so graph
/// re-runs when the article's TEXT changed and debounces when it did not — the same contract, off
/// the table that survives the cutover.
async fn enqueue_graph_for_article(pool: &sqlx::PgPool, article_id: i64, sport: &str) -> Result<()> {
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
            available_at  = NOW(),
            updated_at    = NOW(),
            last_error    = NULL,
            input_version = EXCLUDED.input_version
        WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
           OR public.pipeline_work.status = 'failed'
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .execute(pool)
    .await
    .with_context(|| format!("enqueue graph for article {article_id}"))?;
    Ok(())
}

/// build_editor_prompt_for_eval assembles the EXACT production user prompt for one article —
/// the same DB load, the same fetch, the same builder the stage calls — so `bin/eval` scores
/// the contract that actually runs rather than a reconstruction of it (the legacy seat's
/// `build_article_read_prompt_for_eval` pattern).
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

/// The hypothesis list (§1a): who we ASKED Google about — decorated `Name (team 42)`, the shape
/// `entity_matches` strips.
///
/// Sourced from the article's own query provenance (`news_articles.raw`, written on INSERT by the
/// ingest sweep) rather than from a links table. That is what the hypothesis always literally was:
/// Go used to record it as a 0.95 row in `news_article_entities` and this function read it back
/// out, so the link table was being used as a message queue between two processes that already
/// share the article row. Go writes no links at all now (8.11), and the question "which entity's
/// sweep surfaced this article?" is answered by the sweep that surfaced it.
///
/// Deliberately only the query entity. It does NOT read back the Editor's own resolved links: on a
/// re-read that would feed the model its previous answer as a premise, which is how a wrong link
/// becomes permanent.
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

    Ok(rows
        .into_iter()
        .filter(|(_, _, name)| !name.is_empty())
        .map(|(entity_type, entity_id, name)| format!("{name} ({entity_type} {entity_id})"))
        .collect())
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
    hx: &Harness,
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
    .execute(&hx.pool)
    .await
    .with_context(|| format!("persist editor terminal {}", item.entity_id))?;
    insert_data_fetch_ledger_best_effort(
        hx,
        item,
        status,
        final_url,
        final_url,
        final_domain,
        None,
        None,
        "no_call",
        last_error,
    )
    .await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn persist_read(
    hx: &Harness,
    article_id: i64,
    status: &str,
    fetched: &FetchedArticle,
    extracted_words: i32,
    body_hash: &str,
    read: &EditorRead,
    resolved: &derive::Resolved,
    model_version: &str,
) -> Result<()> {
    let mut tx = hx
        .pool
        .begin()
        .await
        .with_context(|| format!("begin editor persist {article_id}"))?;

    // The ONLY legacy-table column the Editor touches, named by the plan: bodies are retained
    // so evidence can be sliced deterministically (§1a). Write-if-different keeps a re-read of
    // unchanged content from churning the row.
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
    .execute(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .with_context(|| format!("persist editor read {status} {article_id}"))?;

    tx.commit()
        .await
        .with_context(|| format!("commit editor persist {article_id}"))?;
    Ok(())
}

/// One `data_fetch_ledger` row per fetch attempt, stage `editor` — the same append-only
/// provenance the legacy seat writes, under the greenfield stage name.
#[allow(clippy::too_many_arguments)]
async fn insert_data_fetch_ledger_best_effort(
    hx: &Harness,
    item: &Item,
    status: &str,
    source_url: Option<&str>,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    body_hash: Option<&str>,
    model_version: Option<&str>,
    parser_outcome: &str,
    error: Option<&str>,
) {
    if let Err(e) = sqlx::query(
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
    .execute(&hx.pool)
    .await
    {
        warn!(
            article_id = item.entity_id,
            status,
            error = %e,
            "editor: data_fetch_ledger insert failed (continuing)"
        );
    }
}

/// One `cognition_ledger` row per MODEL CALL (closing the ledger gap the legacy seat carried:
/// it wrote data_fetch_ledger only). entity_type 'article', per the plan.
async fn ledger_model_call(
    hx: &Harness,
    item: &Item,
    model: &str,
    parser_outcome: &str,
    extracted: &crate::harness::Extracted<EditorRead>,
    body_hash: &str,
) {
    let entity_id = match item.entity_id_i32() {
        Ok(id) => id,
        Err(e) => {
            warn!(article_id = item.entity_id, error = %e, "editor: ledger skipped");
            return;
        }
    };
    insert_cognition_ledger_best_effort(
        &hx.pool,
        CognitionLedgerEntry {
            stage: "editor".to_string(),
            lens: "editor".to_string(),
            role: Role::Editor.as_str().to_string(),
            entity_type: "article".to_string(),
            entity_id,
            sport: item.sport.to_uppercase(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: "queue".to_string(),
            trigger_payload: json!({}),
            product_table: "editor_reads".to_string(),
            product_row_ids: vec![item.entity_id],
            model_version: model.to_string(),
            prompt_version: EDITOR_CONTRACT_VERSION.to_string(),
            output_contract_version: EDITOR_CONTRACT_VERSION.to_string(),
            input_ids: vec![item.entity_id],
            input_hash: Some(body_hash.to_string()),
            request_body: Some(extracted.request_body.clone()),
            built_prompt: Some(extracted.built_prompt.clone()),
            included_evidence: json!({}),
            excluded_evidence: json!({}),
            context_budget: json!({
                "eval_count": extracted.eval_count,
                "wall_ms": extracted.wall_ms,
            }),
            parser_outcome: parser_outcome.to_string(),
        },
    )
    .await;
}

/// A scraped body occasionally carries a NUL byte (a stray control char surviving `clean_html`),
/// and Postgres cannot store one in a `text` column at ALL — the write dies with
/// `invalid byte sequence for encoding "UTF8": 0x00`. Before this, ~1 article/day dead-lettered
/// the Editor on the `news_articles.full_text` write, which §2 clause 4 (dead-letters = 0 over
/// the 7-day window) can never tolerate.
///
/// This is sanitisation, not policy: NUL carries no meaning in article prose, so it is dropped
/// the moment the body enters the Editor — before it is hashed, prompted, sliced for candidate
/// evidence, or persisted. Every one of those is a text column or a model input, and every one
/// of them wants the same body. Nothing else about the body is touched.
fn sanitize_fetched(mut fetched: FetchedArticle) -> FetchedArticle {
    // The common case allocates nothing: bodies with no NUL pass through untouched, byte-identical.
    if fetched.text.contains('\0') {
        fetched.text = fetched.text.replace('\0', "");
    }
    fetched
}

fn normalize_language_code(raw: &str) -> String {
    let s = raw.trim().to_lowercase();
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_lowercase()) {
        return s;
    }
    "unknown".to_string()
}

fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_object_slice(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if *b == b'\\' {
                esc = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        match *b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start.map(|s| &raw[s..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;

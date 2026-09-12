//! Typed narrative extraction from one Editor-read article and its linked entities.
//! It reads
//! (`news_article_entities`, the Editor sole author) and extracts:
//!
//!   * typed relations among listed entities, with sentiment and confidence;
//!   * person discoveries — coaches/agents/executives named in the article but absent
//!     from the seeded entity world (`narrative_persons` candidates).
//!
//! CLOSED CANDIDATE LIST: the model never resolves free-text entity names. It picks
//! subjects/objects by NUMBER from the linked list (the resolve.rs trick), so the
//! names — the Editor already did the resolving, and the only novel names the model may emit
//! are person discoveries, which land as narrative_persons CANDIDATES (evidence-gated
//! promotion, never direct entityhood).
//!
//! Fail-closed (the §1.2 invariant): an unparseable reply is `None` — the caller writes
//! nothing. Individually invalid relations/persons are dropped, not repaired; a partial
//! salvage of valid entries from a valid JSON body is allowed (mirrors the scrub
//! parser's out-of-range index handling).

use crate::harness::{Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::{StageHandler, ARCHBOX_SLOTS};
use crate::util::hash_components;
use crate::work::{Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use tracing::debug;

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{build_graph_prompt, GRAPH_PROMPT_VERSION, GRAPH_SYSTEM_PROMPT};

const GRAPH_LEDGER: LedgerSpec = LedgerSpec {
    stage: "graph",
    lens: "graph",
    role: Role::EmotionalNews,
    product_table: "narrative_events",
    output_contract_version: "graph-extraction-v1",
};

/// The six-predicate vocabulary — MUST mirror the `narrative_events_predicate_check`
/// constraint. Grow both together with schema and evaluation evidence.
pub const PREDICATES: &[&str] = &[
    "trade_rumor",
    "trade_confirmed",
    "injury",
    "contract_dispute",
    "praise",
    "criticism",
];

/// Person kinds mirror the database constraint. An out-of-vocabulary
/// role guess maps to "other" rather than dropping the discovery (the promotion gate,
/// not the extractor, decides who becomes an entity).
pub const PERSON_KINDS: &[&str] = &["coach", "agent", "executive", "family", "other"];

/// The model budget for one extraction call. Temperature 0.2 (tight but a judgment
/// call, matching scrub adjudication); JSON mode tightens contract adherence.
///
/// Graph shares the Editor's local context size to avoid runner reloads.
pub fn graph_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(GRAPH_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 768,
        num_ctx: crate::route::LOCAL_STAGE_NUM_CTX,
        json_mode: true,
        format_schema: None,
        format_schema_raw: None,
    }
}

/// One numbered candidate shown to the model. `descriptor` is the identity card line
/// ("player, currently at X" / "(team)") — same disambiguation surface as resolve.rs.
#[derive(Clone, Debug)]
pub struct GraphCandidate {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub descriptor: String,
}

/// A validated typed relation, subject/object resolved back to (entity_type, entity_id)
/// — ready for a `narrative_events` row.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphRelation {
    pub subject_type: String,
    pub subject_id: i32,
    pub predicate: String,
    pub object_type: Option<String>,
    pub object_id: Option<i32>,
    pub sentiment: Option<f64>,
    pub confidence: String,
}

/// A person discovery — a `narrative_persons` candidate (or an evidence increment for
/// an existing one).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPerson {
    pub name: String,
    pub kind: String,
    pub team_context_type: Option<String>,
    pub team_context_id: Option<i32>,
}

#[derive(Debug, Default)]
pub struct GraphExtraction {
    pub relations: Vec<GraphRelation>,
    pub persons: Vec<GraphPerson>,
}

/// One article's metadata for the extraction prompt.
#[derive(Clone, Debug)]
pub struct GraphArticle {
    pub source: String,
    pub published: String,
    pub title: String,
    pub description: String,
}

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
               COALESCE(p.name, t.name, '?') AS name,
               COALESCE(ct.name, '') AS current_club
        FROM news_article_entities e
        LEFT JOIN players p ON e.entity_type='player' AND p.id=e.entity_id AND p.sport=e.sport
        LEFT JOIN teams t ON e.entity_type='team' AND t.id=e.entity_id AND t.sport=e.sport
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
    let candidates = cand_rows
        .into_iter()
        .map(|r| {
            let entity_type: String = r.get(0);
            let entity_id: i32 = r.get(1);
            let name: String = r.get(2);
            let club: String = r.get(3);
            let descriptor = if entity_type == "team" {
                format!("{name} (team)")
            } else if club.is_empty() {
                format!("{name} (player, current club unknown)")
            } else {
                format!("{name} (player, currently at {club})")
            };
            GraphCandidate {
                entity_type,
                entity_id,
                descriptor,
            }
        })
        .collect();
    Ok(Some((article, candidates)))
}

/// GraphParser validates the model reply against the candidate list and vocabularies.
/// Fail-closed: no JSON object / unparseable ⇒ `Ok(None)`. Within a parsed body:
/// out-of-range entity numbers, unknown predicates, unknown confidences, and self-loops
/// drop THAT entry; sentiment clamps to [-1, 1]; person role guesses outside the
/// vocabulary map to "other"; empty/duplicate person names drop.
pub struct GraphParser<'a> {
    pub candidates: &'a [GraphCandidate],
}

impl GraphParser<'_> {
    fn resolve(&self, idx: i64) -> Option<&GraphCandidate> {
        if idx >= 1 && (idx as usize) <= self.candidates.len() {
            Some(&self.candidates[idx as usize - 1])
        } else {
            None
        }
    }
}

impl Parser<GraphExtraction> for GraphParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<GraphExtraction>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None),
        };
        #[derive(Deserialize)]
        struct RelReply {
            subject: Option<i64>,
            #[serde(default)]
            predicate: String,
            object: Option<i64>,
            sentiment: Option<f64>,
            #[serde(default)]
            confidence: String,
        }
        #[derive(Deserialize)]
        struct PersonReply {
            #[serde(default)]
            name: String,
            #[serde(default)]
            kind: String,
            team_context: Option<i64>,
        }
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            relations: Vec<RelReply>,
            #[serde(default)]
            persons: Vec<PersonReply>,
        }
        let reply: Reply = match serde_json::from_str(&raw[start..=end]) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        let mut out = GraphExtraction::default();
        for r in reply.relations {
            let Some(subj_idx) = r.subject else { continue };
            let Some(subj) = self.resolve(subj_idx) else {
                continue;
            };
            let predicate = r.predicate.trim().to_lowercase();
            if !PREDICATES.contains(&predicate.as_str()) {
                continue;
            }
            let confidence = r.confidence.trim().to_lowercase();
            if !["speculative", "reported", "confirmed"].contains(&confidence.as_str()) {
                continue;
            }
            let (object_type, object_id) = match r.object {
                None => (None, None),
                Some(oi) => match self.resolve(oi) {
                    Some(obj) => {
                        if obj.entity_type == subj.entity_type && obj.entity_id == subj.entity_id {
                            continue; // self-loop
                        }
                        (Some(obj.entity_type.clone()), Some(obj.entity_id))
                    }
                    None => continue, // dangling object number: drop the relation
                },
            };
            out.relations.push(GraphRelation {
                subject_type: subj.entity_type.clone(),
                subject_id: subj.entity_id,
                predicate,
                object_type,
                object_id,
                sentiment: r.sentiment.map(|s| s.clamp(-1.0, 1.0)),
                confidence,
            });
        }

        let mut seen = std::collections::HashSet::new();
        for p in reply.persons {
            let name = p.name.trim().to_string();
            if name.is_empty() || !seen.insert(name.to_lowercase()) {
                continue;
            }
            // A person "discovery" that names a listed candidate is a model slip —
            // those entities are already known; drop it.
            if self
                .candidates
                .iter()
                .any(|c| c.descriptor.to_lowercase().contains(&name.to_lowercase()))
            {
                continue;
            }
            let kind_raw = p.kind.trim().to_lowercase();
            let kind = if PERSON_KINDS.contains(&kind_raw.as_str()) {
                kind_raw
            } else {
                "other".to_string()
            };
            let (tc_type, tc_id) = match p.team_context.and_then(|i| self.resolve(i)) {
                Some(c) if c.entity_type == "team" => {
                    (Some(c.entity_type.clone()), Some(c.entity_id))
                }
                _ => (None, None), // non-team or dangling context: keep person, drop tie
            };
            out.persons.push(GraphPerson {
                name,
                kind,
                team_context_type: tc_type,
                team_context_id: tc_id,
            });
        }
        Ok(Some(out))
    }
}

// ---------------------------------------------------------------------------
// Article-keyed stage handler, enqueued after the Editor writes links.
// ---------------------------------------------------------------------------

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
pub struct GraphHandler;

impl GraphHandler {
    pub fn new() -> Self {
        GraphHandler
    }
}

impl Default for GraphHandler {
    fn default() -> Self {
        Self::new()
    }
}

async fn upsert_event(
    pool: &PgPool,
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
    .fetch_one(pool)
    .await
    .context("upsert narrative_event")?;
    Ok(row.get("id"))
}

/// accumulate_person resolves-or-creates the candidate person and, ONLY when this
/// article is a NEW mention (the mention PK), bumps the evidence counters. Same-name
/// active rows win the resolve; provider-dupe merges stay a later data-layer pass.
async fn accumulate_person(
    pool: &PgPool,
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
    .fetch_one(pool)
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
    .fetch_optional(pool)
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
        .execute(pool)
        .await
        .context("bump person evidence")?;
    }
    Ok(person_id)
}

#[async_trait]
impl StageHandler for GraphHandler {
    fn stage(&self) -> Stage {
        Stage::Graph
    }

    /// Matches the Editor's local-model batch size.
    fn rotation_batch(&self) -> i64 {
        8
    }

    /// graph shares the archbox card's slots on demand rather than taking a fixed split.
    /// graph registers BEFORE the Editor, so the drain offers it slots first on every top-up
    /// pass: a burst of graph work reclaims the card within one pass instead of waiting on the
    /// Editor's backlog.
    fn max_in_flight(&self) -> usize {
        ARCHBOX_SLOTS.1
    }

    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(ARCHBOX_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let article_id = item.entity_id;
        let sport = item.sport.to_uppercase();
        let Some((article, candidates)) =
            load_graph_article_context(&hx.pool, article_id, &sport).await?
        else {
            debug!(article_id, sport = %sport, "graph: no article or no vetted candidates");
            return Ok(());
        };

        let input_components = build_graph_input_components(&article, &candidates);
        let input_hash = hash_components(&input_components);
        // Debounce on the bookkeeping row: same material AND same prompt contract ⇒ the
        // extraction already ran (including a fail-closed one — never re-hammer the GPU
        // on identical bytes). A prompt bump re-extracts everything once, like rating.
        let prior: Option<(String, String)> = sqlx::query_as(
            "SELECT input_hash, prompt_version FROM graph_extractions WHERE article_id = $1",
        )
        .bind(article_id)
        .fetch_optional(&hx.pool)
        .await
        .context("graph debounce read")?;
        if prior
            .as_ref()
            .is_some_and(|(h, v)| h == &input_hash && v == GRAPH_PROMPT_VERSION)
        {
            debug!(
                article_id,
                "graph: debounce-skip, material + contract unchanged"
            );
            return Ok(());
        }

        let prompt = build_graph_prompt(
            &article.source,
            &article.published,
            &article.title,
            &article.description,
            &candidates,
        );
        let parser = GraphParser {
            candidates: &candidates,
        };
        let extracted = hx
            .extract(Role::EmotionalNews, &prompt, &graph_opts(), &parser)
            .await?;
        let model = extracted.model.clone();

        let (outcome, relations, persons) = match &extracted.value {
            None => ("failed_closed", &[][..], &[][..]),
            Some(g) => ("extracted", g.relations.as_slice(), g.persons.as_slice()),
        };

        let mut event_ids = Vec::with_capacity(relations.len());
        for r in relations {
            event_ids.push(upsert_event(&hx.pool, article_id, &sport, &model, r).await?);
        }
        for p in persons {
            accumulate_person(&hx.pool, article_id, &sport, &model, p).await?;
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
        .bind(&input_hash)
        .bind(GRAPH_PROMPT_VERSION)
        .bind(&model)
        .bind(outcome)
        .bind(relations.len() as i32)
        .bind(persons.len() as i32)
        .execute(&hx.pool)
        .await
        .context("upsert graph_extractions")?;

        let entity_id_i32 = i32::try_from(article_id)
            .map_err(|_| anyhow!("graph: article_id {article_id} outside i32 range"))?;
        let generation = Generation::called(
            (),
            model,
            GRAPH_PROMPT_VERSION,
            vec![article_id],
            Some(input_hash),
            GenerationCall::from(&extracted),
        );
        insert_generation_ledger_best_effort(
            &hx.pool,
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

        Ok(())
    }
}

#[cfg(test)]
mod tests;

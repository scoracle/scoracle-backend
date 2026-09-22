//! Insider application adapter: evidence, partial progress, publication, identity, and work.
//!
//! Per team/subject pair this composes model extraction, validation, subject matching, and
//! persistence. Postgres owns `compute_transfer_heat` and the team relationship; Studio derives
//! direction from that prepared relationship (the model never computes the number or direction).
//! The model ONLY vets: is this a live rumor about THIS exact player, what stage, and a grounded
//! one-line summary. The subject same-person test is realised as the verdict's `subject` field plus
//! the identity-card framing in the system prompt; both fields come back in one JSON object.
//!
//! FAIL CLOSED (the §1.2 invariant): `is_rumor: Option<bool>` — a model timeout, unparseable output,
//! or a verdict that never committed to is_rumor persists an UNKNOWN row (is_rumor NULL), which is
//! NEVER served (every read requires `is_rumor IS TRUE`) and is counted so the team's stage item is
//! re-enqueued for a retry. Only a successful POSITIVE verdict ever becomes a served rumor.
//!
//! Before each model call, the handler fingerprints MATERIAL pair inputs — sorted corpus IDs, the
//! corpus-stable source diversity, and the deterministic relationship; no timestamps, no prose,
//! no recency decay. A resolved row with the same hash skips generation and persistence. UNKNOWN
//! markers never satisfy the gate, so a
//! model-failure retry re-vets ONLY the failed pair: the completed pairs skip on fingerprint
//! instead of repeating every call in a team batch.

use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};

use crate::application::models::Models;
use crate::application::products::EntityKey;
use crate::application::queue::work::Item;
use crate::evidence::corpus::load_transfer_heat;
use crate::evidence::trajectory::{classify_delta, DEFAULT_TRAJECTORY};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::util::hash_components;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

mod identity;
use crate::studio::insider::{
    build_insider_score_input_components, build_insider_score_prompt,
    build_transfer_identity_adjudication_prompt, build_transfer_input_components,
    build_transfer_prompt, transfer_system_prompt, NewsItem, Outcome, PairAssignment,
    TransferCandidate, TransferEvidence, TransferIdentityAdjudicationParser, TransferPairOutput,
    TransferRow, INSIDER_SCORE_NUM_PREDICT, INSIDER_SCORE_OUTPUT_CONTRACT_VERSION,
    TRANSFER_DEFAULT_MIN_ARTICLES, TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION,
    TRANSFER_NUM_PREDICT, TRANSFER_OUTPUT_CONTRACT_VERSION, TRANSFER_PROMPT_VERSION,
    TRANSFER_TEMPERATURE,
};
#[cfg(test)]
pub(crate) use identity::identity_apply_deterministic_score;
use identity::{
    bank_transfer_junction_event, load_transfer_identity_threshold, maybe_apply_transfer_identity,
    refresh_sport_autofill_concurrently,
};

const TRANSFER_LEDGER: LedgerSpec = LedgerSpec {
    stage: "transfers",
    lens: "transfer",
    role: Role::TransferLogic,
    product_table: "transfer_rumors",
    output_contract_version: TRANSFER_OUTPUT_CONTRACT_VERSION,
};

/// Corpus + candidate governors.
const TRANSFER_MAX_CORPUS_NEWS: i64 = 12;
const TRANSFER_MAX_CANDIDATES: i32 = 40;

// Self-pacing against the worker's per-item ceiling. Reserve time for the wire wrap after the
// variable-length pair loop; defer remaining pairs rather than cancelling them.

/// Fraction of the run's budget the pair loop may spend before it stops and defers the remainder.
pub(crate) const TRANSFER_PAIR_BUDGET_FRAC: f64 = 0.50;
/// Fraction at which the wire wrap stops too. The gap below 1.0 is headroom for the autofill
/// refresh and the bookkeeping that follow — this handler must land inside the ceiling, not race
/// it to the line.
pub(crate) const TRANSFER_WRAP_BUDGET_FRAC: f64 = 0.85;
/// How long a deferred team waits before it is claimable again. Claims order by `available_at`, so
/// this puts a big team behind the other pending teams rather than letting it immediately re-take
/// the stage's single in-flight slot and monopolise it round after round.
const TRANSFER_DEFER_DELAY: Duration = Duration::from_secs(60);

/// budget_deadline is the instant at which `frac` of the run's budget is spent, or `None` when the
/// budget is unbounded (`Duration::ZERO` — eval and the one-shot binaries). `None` is what makes an
/// inspection run drive a team to completion however long it takes.
pub(crate) fn budget_deadline(start: Instant, budget: Duration, frac: f64) -> Option<Instant> {
    if budget.is_zero() {
        return None;
    }
    Some(start + budget.mul_f64(frac))
}

/// past reports whether a deadline exists and has arrived. An absent deadline is never past.
pub(crate) fn past(deadline: Option<Instant>) -> bool {
    matches!(deadline, Some(d) if Instant::now() >= d)
}
// ---------------------------------------------------------------------------
// Loaders.
// ---------------------------------------------------------------------------

/// Return a team's corroborated co-mention candidates with identity cards.
pub async fn load_candidates(
    pool: &PgPool,
    team_id: i32,
    sport: &str,
    min_articles: i32,
) -> Result<Vec<TransferCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM (
            SELECT pe.entity_id, p.name,
                   COALESCE(p.nationality, '')                    AS nationality,
                   COALESCE(ct.name, '')                          AS current_club,
                   COALESCE(NULLIF(pci.position, 'Unknown'), '')  AS position,
                   'player'::text                                 AS subject_type,
                   NULL::text                                     AS relationship_override,
                   max(a.topic_heat)                              AS topic_heat,
                   count(DISTINCT te.article_id)                  AS article_n
            FROM news_article_entities te
            JOIN news_article_entities pe
              ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
            JOIN news_articles a ON a.id = te.article_id
            JOIN players p ON p.id = pe.entity_id AND p.sport = pe.sport
            LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
            LEFT JOIN teams ct ON ct.id = pci.team_id AND ct.sport = p.sport
            WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
              AND a.bucket IS DISTINCT FROM 'non_transfer'
              AND te.created_at > NOW() - INTERVAL '14 days'
            GROUP BY pe.entity_id, p.name, p.nationality, ct.name, pci.position

            UNION ALL

            -- Coach candidates use the same co-mention and corroboration gate.
            -- Executives and agents are not transfer subjects.
            SELECT pe.entity_id, pp.full_name AS name,
                   ''::text                                       AS nationality,
                   COALESCE(ct.name, '')                          AS current_club,
                   pp.kind                                        AS position,
                   'person'::text                                 AS subject_type,
                   CASE WHEN pp.team_id = $1 THEN 'current' ELSE 'none' END
                                                                  AS relationship_override,
                   max(a.topic_heat)                              AS topic_heat,
                   count(DISTINCT te.article_id)                  AS article_n
            FROM news_article_entities te
            JOIN news_article_entities pe
              ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'person'
            JOIN news_articles a ON a.id = te.article_id
            JOIN public.persons pp ON pp.id = pe.entity_id AND pp.sport = pe.sport AND pp.kind = 'coach'
            LEFT JOIN teams ct ON ct.id = pp.team_id AND ct.sport = pp.sport
            WHERE te.entity_type = 'team' AND te.entity_id = $1 AND te.sport = $2
              AND a.bucket IS DISTINCT FROM 'non_transfer'
              AND te.created_at > NOW() - INTERVAL '14 days'
            GROUP BY pe.entity_id, pp.full_name, ct.name, pp.team_id, pp.kind
            HAVING count(DISTINCT te.article_id) >= $3
        ) u
        WHERE u.article_n >= $3
        ORDER BY u.topic_heat DESC NULLS LAST, u.article_n DESC
        LIMIT $4
        "#,
    )
    .bind(team_id)
    .bind(sport)
    .bind(min_articles)
    .bind(TRANSFER_MAX_CANDIDATES)
    .fetch_all(pool)
    .await
    .context("load candidates")?;

    Ok(rows
        .iter()
        .map(|r| TransferCandidate {
            player_id: r.get("entity_id"),
            player_name: r.get("name"),
            nationality: r.get("nationality"),
            current_club: r.get("current_club"),
            position: r.get("position"),
            subject_type: r.get("subject_type"),
            relationship_override: r.get("relationship_override"),
        })
        .collect())
}

/// compute_pair_heat calls the deterministic `compute_transfer_heat` SQL function (migration 032 —
/// the number stays in Postgres, NEVER the model's). Returns (heat, components-jsonb-text, news_ids);
/// `heat` is `None` when there is no pair corpus (the Skipped short-circuit). Mirrors the
/// `analyzePair` opening query. `components::text` + COALESCE keep the scan null-safe.
pub async fn compute_pair_heat(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    subject_type: &str,
) -> Result<(Option<i16>, String, Vec<i64>)> {
    let row = sqlx::query(
        "SELECT heat, COALESCE(components::text, '{}') AS components, news_ids \
         FROM compute_transfer_heat($1, $2, $3, $4)",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(subject_type)
    .fetch_one(pool)
    .await
    .context("compute transfer heat")?;
    let heat: Option<i16> = row.get("heat");
    let components: String = row.get("components");
    let news_ids: Option<Vec<i64>> = row.get("news_ids");
    Ok((heat, components, news_ids.unwrap_or_default()))
}

/// load_pair_news returns the pair's corpus headlines, newest first, capped — the model's grounding.
/// `published_at` stays in the ordering without entering the model-facing row.
pub async fn load_pair_news(pool: &PgPool, ids: &[i64]) -> Result<Vec<NewsItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT id, title, COALESCE(description, '') AS description, COALESCE(source, '') AS source
        FROM news_articles WHERE id = ANY($1)
        ORDER BY published_at DESC NULLS LAST LIMIT $2
        "#,
    )
    .bind(ids)
    .bind(TRANSFER_MAX_CORPUS_NEWS)
    .fetch_all(pool)
    .await
    .context("load pair news")?;
    Ok(rows
        .iter()
        .map(|r| NewsItem {
            id: r.get("id"),
            title: r.get("title"),
            description: r.get("description"),
            source: r.get("source"),
        })
        .collect())
}

/// How many of the team's live packets may contribute material to one pair's prompt. A pair's
/// articles rarely span more than one storyline; the cap is a ceiling on a pathological team, not
/// a tuning knob.
const PAIR_PACKET_LIMIT: i64 = 5;

/// The packet material for one pair (7.5), keyed by article id: this article's transfer-typed
/// claims, in the packet's order (newest first), contested ones already marked.
type PairPacketFacts = HashMap<i64, Vec<String>>;

/// Load the Editor's transfer claims for the pair's existing corpus. Postgres still decides pair
/// identity, heat, and corpus IDs; the packet replaces only the article text shown to the model.
///
/// The Insider's slice is the transfer-typed claims — exactly the subset
/// `slice_fingerprints ->> 'transfers'` hashes (E2), so a re-fan and a re-read agree about what
/// moved. Articles the Desk has not assembled (or whose claims are another type) simply carry no
/// overlay and travel on their headline, which is what a headline is for.
///
/// Returns the per-article facts plus the storyline framing of the packets that actually
/// contributed — a packet whose claims all missed this pair frames nothing.
async fn load_pair_packet_material(
    pool: &PgPool,
    team_id: i32,
    team_name: &str,
    sport: &str,
    news_ids: &[i64],
) -> Result<(PairPacketFacts, String)> {
    use crate::evidence::news::render;

    let mut facts: PairPacketFacts = HashMap::new();
    let mut framing = String::new();
    if news_ids.is_empty() {
        return Ok((facts, framing));
    }
    let wanted: std::collections::HashSet<i64> = news_ids.iter().copied().collect();

    let loaded = crate::evidence::news::packet::load_packets_for_entity(
        pool,
        "team",
        team_id,
        sport,
        crate::application::journalist::PACKET_LOOKBACK_HOURS,
        PAIR_PACKET_LIMIT,
    )
    .await?;

    for (view, mut part) in loaded {
        part.name = team_name.to_string();
        let slice = render::slice_claims(&view.claims, render::Voice::Insider);
        let marked = render::mark_contested(&slice);
        let mut contributed = false;
        for m in marked {
            if !wanted.contains(&m.claim.article_id) {
                continue;
            }
            contributed = true;
            let fact = if m.marked {
                // The contradiction survives into the wire's own prompt (T3/D6). The Insider is
                // the one voice whose whole job is staging a contested claim, so the marker
                // matters most here: "agreement in principle" beside "deal not agreed" is a
                // reason to hold the stage down, not noise to resolve away.
                format!("⇄ {}", m.claim.fact)
            } else {
                m.claim.fact.clone()
            };
            facts.entry(m.claim.article_id).or_default().push(fact);
        }
        if contributed {
            if !framing.is_empty() {
                framing.push('\n');
            }
            framing.push_str(&render::framing(&view, Some(&part), render::Voice::Insider));
        }
    }
    Ok((facts, framing))
}

async fn load_stale_pair_news_ids(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<Vec<i64>> {
    let ids = sqlx::query_scalar(
        r#"
        SELECT DISTINCT a.id
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type = 'team'
             AND te.entity_id = $1 AND te.sport = $3
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type = 'player'
             AND pe.entity_id = $2 AND pe.sport = $3
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at <= NOW() - INTERVAL '14 days'
        ORDER BY a.id
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .context("load stale pair news ids")?;
    Ok(ids)
}

/// Batch [`team_relationship`] over a team's candidate set. Batch and single-pair reads must
/// agree because relationship is part of the material fingerprint.
pub async fn team_relationships(
    pool: &PgPool,
    team_id: i32,
    player_ids: &[i32],
    sport: &str,
) -> Result<HashMap<i32, String>> {
    let rows = sqlx::query(
        r#"
        SELECT p.pid,
               COALESCE((SELECT pci.team_id = $3
                         FROM public.player_current_identity pci
                         WHERE pci.player_id = p.pid AND pci.sport = $2), false) AS is_current,
               COALESCE((SELECT bool_or(ps.team_id = $3)
                         FROM player_stats ps
                         WHERE ps.player_id = p.pid AND ps.sport = $2), false) AS is_ever
        FROM unnest($1::int4[]) AS p(pid)
        "#,
    )
    .bind(player_ids)
    .bind(sport)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .context("team relationships (batch)")?;
    Ok(rows
        .iter()
        .map(|r| {
            let pid: i32 = r.get("pid");
            let is_current: bool = r.get("is_current");
            let is_ever: bool = r.get("is_ever");
            let rel = if is_current {
                "current"
            } else if is_ever {
                "former"
            } else {
                "none"
            };
            (pid, rel.to_string())
        })
        .collect())
}

/// team_relationship classifies the player's deterministic relationship to the team:
/// "current" comes from canonical current identity, while "former" comes from
/// historical player_stats. Drives `direction` and the former-player noise filter — NOT the model's
/// guess. `$1=player, $2=sport, $3=team`.
pub async fn team_relationship(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
) -> Result<String> {
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE((SELECT pci.team_id = $3
                      FROM public.player_current_identity pci
                      WHERE pci.player_id = $1 AND pci.sport = $2), false) AS is_current,
            COALESCE((SELECT bool_or(ps.team_id = $3)
                      FROM player_stats ps
                      WHERE ps.player_id = $1 AND ps.sport = $2), false) AS is_ever
        "#,
    )
    .bind(player_id)
    .bind(sport)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .context("team relationship")?;
    let is_current: bool = row.get("is_current");
    let is_ever: bool = row.get("is_ever");
    Ok(if is_current {
        "current".to_string()
    } else if is_ever {
        "former".to_string()
    } else {
        "none".to_string()
    })
}

/// primary_source returns the first attributed source in the prompt corpus.
fn primary_source(news: &[NewsItem]) -> String {
    for n in news {
        if !n.source.is_empty() {
            return n.source.clone();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// The per-pair core + the production handler.
// ---------------------------------------------------------------------------

/// Returns true when the latest resolved pair row carries this input hash.
/// UNKNOWN and unstamped rows never satisfy the gate.
pub async fn pair_unchanged(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    subject_type: &str,
    input_hash: &str,
) -> Result<bool> {
    let latest: Option<(Option<String>, Option<bool>)> = sqlx::query_as(
        "SELECT input_hash, is_rumor FROM transfer_rumors \
         WHERE team_id = $1 AND player_id = $2 AND sport = $3 AND subject_type = $4 \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(subject_type)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("transfer debounce check {team_id}/{player_id}"))?;
    Ok(matches!(latest, Some((Some(h), Some(_))) if h == input_hash))
}

/// PairBuild is the deterministic prefix of `analyze_pair`: everything computed before the model
/// call (heat -> corpus -> relationship -> prompt -> request body). Shared by production and eval
/// paths so inspection artifacts match what production sends. `Skipped` means no pair corpus
/// (heat NULL), so no model call and no row.
pub enum PairBuild {
    Skipped {
        components: String,
        news_ids: Vec<i64>,
    },
    Ready(Box<PairAssignment>),
}

/// Loads the model-facing source track-record card for the pair's live corpus.
pub async fn load_source_reliability(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT source_reliability_for_pair($1, $2, $3)")
        .bind(sport)
        .bind(player_id)
        .bind(team_id)
        .fetch_one(pool)
        .await
        .context("source_reliability_for_pair")?;
    Ok(row.0)
}

/// Builds the deterministic pair inputs and exact request body without calling the model.
#[allow(clippy::too_many_arguments)]
pub async fn build_pair_request(
    pool: &sqlx::PgPool,
    models: &Models,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    relationship: String,
    temperature: f64,
) -> Result<PairBuild> {
    let (heat, components, news_ids) =
        compute_pair_heat(pool, team_id, c.player_id, sport, &c.subject_type).await?;
    let Some(heat) = heat else {
        return Ok(PairBuild::Skipped {
            components,
            news_ids,
        });
    };

    let mut news = load_pair_news(pool, &news_ids).await?;
    // The packet rail may replace article prose, never corpus membership.
    let packet_framing = {
        let (facts, framing) =
            load_pair_packet_material(pool, team_id, team_name, sport, &news_ids).await?;
        for n in news.iter_mut() {
            let Some(article_facts) = facts.get(&n.id) else {
                continue; // not assembled, or nothing transfer-typed in it — it keeps its headline
            };
            let mut it = article_facts.iter();
            if let Some(first) = it.next() {
                n.title = first.clone();
                n.description = it.cloned().collect::<Vec<_>>().join(" · ");
            }
        }
        Some(framing).filter(|f| !f.trim().is_empty())
    };
    let prompted_news_ids = news.iter().map(|n| n.id).collect();
    let attribution = primary_source(&news);

    // Fingerprint material inputs before the handler decides whether to call the model.
    let input_components = build_transfer_input_components(&news_ids, &components, &relationship);
    let mut request = MemoryRequest::new(Mission::Insider, &c.subject_type, c.player_id, sport);
    request.pair_team_id = Some(team_id);
    request.current_article_ids = &news_ids;
    let memories = memories::load(pool, request).await?;
    let team_identity =
        crate::evidence::memories::load_identity_record(pool, "team", team_id, sport).await?;
    let input_components = memories.with_input_components(&input_components)?;
    let mut input_value: serde_json::Value = serde_json::from_str(&input_components)?;
    input_value["team_identity"] = serde_json::json!(team_identity);
    let input_components = input_value.to_string();
    let input_hash = hash_components(&input_components);

    let evidence = TransferEvidence::from_news(&news, news_ids.len(), &attribution);
    // Reliability remains player-keyed; persons must never collide with player IDs.
    let source_reliability = if c.subject_type == "person" {
        None
    } else {
        load_source_reliability(pool, sport, c.player_id, team_id).await?
    };
    let memory = memories.render_for_model()?;
    let mut built_prompt = build_transfer_prompt(
        team_name,
        c,
        sport,
        &relationship,
        &news,
        &evidence,
        source_reliability.as_deref(),
        Some(&memory),
        packet_framing.as_deref(),
    );
    if let Some(card) = team_identity {
        built_prompt.push_str(&format!(
            "\nProposed destination, not current affiliation: {card}\n"
        ));
    }
    // Person subjects use the same contract with a separately versioned noun substitution.
    let system = if c.subject_type == "person" {
        transfer_system_prompt(sport).replace("player", "person")
    } else {
        transfer_system_prompt(sport)
    };
    let options = crate::studio::model::GenerateOptions {
        system: Some(system),
        temperature: Some(temperature),
        num_predict: TRANSFER_NUM_PREDICT,
        num_ctx: models.voice_num_ctx,
        json_mode: true,
        format_schema: None,
        format_schema_raw: None,
    };
    let backend = models.router.for_role(Role::TransferLogic);
    let request_body = backend.request_body(&built_prompt, &options);
    let model_configured = backend.model().to_string();
    let stale_news_ids = load_stale_pair_news_ids(pool, team_id, c.player_id, sport).await?;

    Ok(PairBuild::Ready(Box::new(PairAssignment {
        player_id: c.player_id,
        heat,
        subject_type: c.subject_type.clone(),
        components,
        news_ids,
        prompted_news_ids,
        news,
        relationship,
        attribution,
        stale_news_ids,
        options,
        prompt: built_prompt,
        failed_request_body: request_body,
        model_configured,
        input_hash,
    })))
}

/// skipped_pair_output is the no-corpus result: heat NULL ⇒ no model call, no row
/// (Go: `res.Skipped++, return nil`). No fingerprint either — there is no row to stamp.
fn skipped_pair_output(
    models: &Models,
    player_id: i32,
    subject_type: &str,
    components: String,
    news_ids: Vec<i64>,
) -> TransferPairOutput {
    let model = models
        .router
        .for_role(Role::TransferLogic)
        .model()
        .to_string();
    crate::studio::insider::skipped_pair(player_id, subject_type, components, news_ids, model)
}

/// Vets one pair without persisting it. Model transport failures become UNKNOWN outputs;
/// preparation/database errors are returned.
pub async fn analyze_pair(
    pool: &sqlx::PgPool,
    models: &Models,
    team_id: i32,
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    temperature: f64,
) -> Result<TransferPairOutput> {
    // Offline composition loads the relationship per pair; the production handler batches it.
    // Person subjects carry their own (persons.team_id-derived) — the player relationship
    // read must never see a person id.
    let relationship = match &c.relationship_override {
        Some(r) => r.clone(),
        None => team_relationship(pool, team_id, c.player_id, sport).await?,
    };
    match build_pair_request(
        pool,
        models,
        team_id,
        team_name,
        c,
        sport,
        relationship,
        temperature,
    )
    .await?
    {
        PairBuild::Skipped {
            components,
            news_ids,
        } => Ok(skipped_pair_output(
            models,
            c.player_id,
            &c.subject_type,
            components,
            news_ids,
        )),
        PairBuild::Ready(assignment) => {
            let backend = models.router.for_role(Role::TransferLogic);
            Ok(crate::studio::insider::create_pair(
                &crate::studio::Studio::new(backend.as_ref()),
                *assignment,
            )
            .await)
        }
    }
}

fn transfer_components_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({ "raw": s }))
}

fn transfer_trigger_payload_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
}

/// persist_transfer_row writes ONE row to the LIVE transfer_rumors table — the scored rumor, the
/// cleared row, and the UNKNOWN marker, which differ only in the bound values. Mirrors the
/// deleted `transfer.go::persist` (generated_at defaults NOW()).
/// `confidence` is bound float8 then cast to the numeric(3,2) column (sqlx has no numeric encode
/// without the decimal feature — the dual of the scrub `::float8` read landmine).
#[allow(clippy::too_many_arguments)]
pub async fn persist_transfer_row(
    pool: &PgPool,
    item: &Item,
    team_id: i32,
    player_id: i32,
    sport: &str,
    trigger_type: &str,
    out: &TransferPairOutput,
    row: &TransferRow,
) -> Result<Option<i64>> {
    let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
        load_transfer_source_metadata(pool, &out.news_ids).await?;
    let (trajectory, trajectory_components) =
        classify_transfer_trajectory(pool, team_id, player_id, sport, out, row).await?;
    let trajectory_json = trajectory_components.to_string();

    let mut tx = pool.begin().await.context("begin transfer publication")?;
    if !crate::application::queue::work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close superseded transfer publication")?;
        return Ok(None);
    }

    let served_rumor = row.is_rumor == Some(true);
    let inserted = sqlx::query(
        r#"
        INSERT INTO transfer_rumors (
            team_id, player_id, sport, trigger_type, heat, heat_components,
            is_rumor, direction, stage, model_summary, source_attribution, confidence,
            input_news_ids,
            rumor_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, trigger_payload, input_hash, subject_type
        ) VALUES (
            $1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10,$11,$12::float8::numeric,$13,
            COALESCE(to_timestamp($14::double precision), NOW()), $15, $16,
            to_timestamp($17::double precision), to_timestamp($18::double precision),
            $19, $20::jsonb,
            $21,$22,$23::jsonb,$24,$25
        )
        RETURNING id
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(trigger_type)
    .bind(out.heat)
    .bind(&out.components)
    .bind(row.is_rumor)
    .bind(row.direction.as_deref())
    .bind(row.stage.as_deref())
    .bind(row.summary.as_deref())
    .bind(row.attribution.as_deref())
    .bind(row.confidence)
    .bind(out.news_ids.as_slice())
    .bind(source_latest_epoch)
    .bind(source_count)
    .bind(&source_names)
    .bind(source_latest_epoch)
    .bind(source_oldest_epoch)
    .bind(trajectory)
    .bind(&trajectory_json)
    .bind(row.model.as_deref())
    .bind(out.provenance.prompt_version)
    .bind(&row.trigger_payload)
    .bind(out.provenance.input_hash.as_deref())
    .bind(&out.subject_type)
    .fetch_one(&mut *tx)
    .await
    .context("persist transfer row")?;
    let row_id: i64 = inserted.get("id");
    if served_rumor {
        crate::application::queue::outbox::record_transfer_published(
            &mut tx,
            item,
            "player",
            player_id,
            Some(&row_id.to_string()),
        )
        .await?;
    }
    tx.commit().await.context("commit transfer publication")?;
    Ok(Some(row_id))
}

async fn load_transfer_source_metadata(
    pool: &PgPool,
    news_ids: &[i64],
) -> Result<(i32, Vec<String>, Option<i64>, Option<i64>)> {
    if news_ids.is_empty() {
        return Ok((0, Vec::new(), None, None));
    }

    let row: (i32, Vec<String>, Option<i64>, Option<i64>) = sqlx::query_as(
        r#"
        SELECT count(id)::int,
               COALESCE(ARRAY(
                   SELECT DISTINCT NULLIF(a2.source, '')
                   FROM news_articles a2
                   WHERE a2.id = ANY($1)
                     AND NULLIF(a2.source, '') IS NOT NULL
                   ORDER BY 1
               ), '{}'::text[]),
               EXTRACT(EPOCH FROM max(COALESCE(published_at, fetched_at)))::bigint,
               EXTRACT(EPOCH FROM min(COALESCE(published_at, fetched_at)))::bigint
        FROM news_articles
        WHERE id = ANY($1)
        "#,
    )
    .bind(news_ids)
    .fetch_one(pool)
    .await
    .context("load transfer source metadata")?;

    Ok(row)
}

async fn classify_transfer_trajectory(
    pool: &PgPool,
    team_id: i32,
    player_id: i32,
    sport: &str,
    out: &TransferPairOutput,
    row: &TransferRow,
) -> Result<(&'static str, serde_json::Value)> {
    let previous: Option<i32> = sqlx::query_scalar(
        r#"
        SELECT heat::int
        FROM transfer_rumors
        WHERE team_id = $1
          AND player_id = $2
          AND sport = $3
          AND subject_type = $4
          AND heat IS NOT NULL
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(team_id)
    .bind(player_id)
    .bind(sport)
    .bind(&out.subject_type)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("classify transfer trajectory {team_id}/{player_id}"))?;

    let current = out.heat.map(i32::from);
    let (trajectory, reason, delta) = if row.is_rumor == Some(false) {
        (
            "cooling_off",
            "cleared",
            previous.zip(current).map(|(p, c)| c - p),
        )
    } else if row.is_rumor != Some(true) {
        (
            DEFAULT_TRAJECTORY,
            "unresolved",
            previous.zip(current).map(|(p, c)| c - p),
        )
    } else {
        let (trajectory, delta_reason, delta) = classify_delta(previous, current);
        let reason = match delta_reason {
            "up" => "heat_up",
            "down" => "heat_down",
            "stable" => "heat_stable",
            other => other,
        };
        (trajectory, reason, delta)
    };

    Ok((
        trajectory,
        serde_json::json!({
            "previous_heat": previous,
            "current_heat": current,
            "heat_delta": delta,
            "reason": reason,
        }),
    ))
}

// ---------------------------------------------------------------------------
// The Insider's card score: one wrap per touched entity after pair verdicts are filed.
// It versions independently and never enters a pair's debounce fingerprint.
// ---------------------------------------------------------------------------

const INSIDER_SCORE_LEDGER: LedgerSpec = LedgerSpec {
    stage: "transfers",
    lens: "insider_score",
    role: Role::TransferLogic,
    product_table: "insider_scores",
    output_contract_version: INSIDER_SCORE_OUTPUT_CONTRACT_VERSION,
};

/// Returns players on the team's recent served rumors, a superset of the active board.
async fn load_wire_touched_players(
    pool: &PgPool,
    team_id: i32,
    sport: &str,
) -> Result<Vec<(i32, String)>> {
    sqlx::query_as(
        r#"
        SELECT DISTINCT tr.player_id, p.name
        FROM transfer_rumors tr
        JOIN players p ON p.id = tr.player_id AND p.sport = tr.sport
        WHERE tr.team_id = $1 AND tr.sport = $2
          AND tr.is_rumor IS TRUE AND tr.heat > 0
          AND tr.generated_at > NOW() - INTERVAL '7 days'
        "#,
    )
    .bind(team_id)
    .bind(sport)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load wire-touched players team {team_id}"))
}

/// score_insider_entity runs the wire wrap for ONE entity: load the active board, skip when the
/// wire is dead (no call, no row — the Veil comes from the empty rumor board, `insider_scores`
/// deliberately has no marker rows) or unchanged (the board-hash debounce), else one
/// `Role::TransferLogic` call → one `insider_scores` row + one cognition-ledger entry.
async fn score_insider_entity(
    pool: &sqlx::PgPool,
    models: &Models,
    item: &Item,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport: &str,
) -> Result<bool> {
    let heat = load_transfer_heat(pool, entity_type, entity_id, sport).await?;
    if heat.is_empty() {
        // A never-scored empty wire skips. A previously scored wire files one quiet close;
        // the empty-board hash then debounces further calls until the board revives.
        let has_row = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM insider_scores WHERE entity_type = $1 AND entity_id = $2 AND sport = $3)",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await?;
        if !has_row {
            return Ok(true);
        }
    }
    let memories = memories::load(
        pool,
        MemoryRequest::new(Mission::Insider, entity_type, entity_id, sport),
    )
    .await?;
    let components =
        memories.with_input_components(&build_insider_score_input_components(&heat))?;
    let input_hash = hash_components(&components);
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.to_string(),
        season: None,
    };
    if crate::application::products::debounce_unchanged(pool, "insider_scores", &key, &input_hash)
        .await?
    {
        debug!(
            entity_type,
            entity_id, "transfers: insider score debounce-skip, board unchanged"
        );
        return Ok(true);
    }
    let identity = Some(memories.render_for_model()?);
    let prompt =
        build_insider_score_prompt(entity_name, sport, entity_type, &heat, identity.as_deref());
    let options = crate::studio::insider::score_options(models.voice_num_ctx);
    let backend = models.router.for_role(Role::TransferLogic);
    let generation = crate::studio::insider::create_score(
        &crate::studio::Studio::new(backend.as_ref()),
        &prompt,
        &options,
        input_hash,
    )
    .await?;
    let reply = &generation.product;
    let previous_score = memories.previous_score;

    let mut tx = pool
        .begin()
        .await
        .context("begin insider score publication")?;
    if !crate::application::queue::work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close superseded insider score publication")?;
        return Ok(false);
    }
    let row = sqlx::query(
        r#"
        INSERT INTO insider_scores (
            sport, entity_type, entity_id, score, previous_score, read, headline,
            model_version, prompt_version, input_hash, generated_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW())
        RETURNING id
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(reply.score)
    .bind(previous_score)
    .bind(reply.read.as_str())
    .bind(reply.headline.as_deref())
    .bind(generation.provenance.model_version.as_str())
    .bind(generation.provenance.prompt_version)
    .bind(generation.provenance.input_hash.as_deref())
    .fetch_one(&mut *tx)
    .await
    .context("persist insider score")?;
    let row_id: i64 = row.get("id");
    crate::application::queue::outbox::record_transfer_published(
        &mut tx,
        item,
        entity_type,
        entity_id,
        Some(&row_id.to_string()),
    )
    .await?;
    tx.commit()
        .await
        .context("commit insider score publication")?;

    insert_generation_ledger_best_effort(
        pool,
        &generation,
        INSIDER_SCORE_LEDGER,
        LedgerEvent {
            entity_type,
            entity_id,
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::Value::Null,
            product_row_ids: vec![row_id],
            included_evidence: serde_json::json!({
                "active_rumors": heat.len(),
                "board": heat
                    .iter()
                    .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
                    .collect::<Vec<_>>(),
                "score": reply.score,
                "previous_score": previous_score,
            }),
            excluded_evidence: serde_json::json!([]),
            context_budget: generation.context_budget(serde_json::json!({
                "num_predict": INSIDER_SCORE_NUM_PREDICT,
            })),
            parser_outcome: "parsed",
        },
    )
    .await;
    Ok(true)
}

/// Drains team-keyed transfers: vet pairs, persist verdicts, and wrap each touched entity.
/// UNKNOWN or infrastructure failures retry the item; resolved unchanged pairs debounce-skip.
pub struct TransferHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

async fn complete_claimed(pool: &PgPool, item: &Item) -> Result<PluginOutcome> {
    let mut tx = pool.begin().await.context("begin transfer completion")?;
    if !crate::application::queue::work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close non-current transfer completion")?;
        return Ok(PluginOutcome::Superseded);
    }
    crate::application::queue::outbox::record_transfer_published(
        &mut tx,
        item,
        &item.entity_type,
        item.entity_id_i32()?,
        item.input_version.as_deref(),
    )
    .await?;
    if !crate::application::queue::work::complete_in_transaction(&mut tx, item).await? {
        bail!("transfer claim changed while its completion transaction held the row lock");
    }
    tx.commit().await.context("commit transfer completion")?;
    Ok(PluginOutcome::Committed)
}

impl TransferHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for TransferHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::studio::fleet::INSIDER
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        if item.entity_type != "team" {
            bail!(
                "transfers: non-team entity {}/{}",
                item.entity_type,
                item.entity_id
            );
        }
        let team_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let team_name = crate::evidence::corpus::lookup_entity_name(
            pool,
            &item.entity_type,
            team_id,
            &item.sport,
        )
        .await?;
        let candidates =
            load_candidates(pool, team_id, &sport, TRANSFER_DEFAULT_MIN_ARTICLES).await?;
        // Load relationships and the identity threshold once per team.
        // Player subjects only — person ids collide with player ids, and person
        // relationships arrive on the candidate itself (relationship_override).
        let player_ids: Vec<i32> = candidates
            .iter()
            .filter(|c| c.subject_type == "player")
            .map(|c| c.player_id)
            .collect();
        let (relationships, identity_threshold) = tokio::try_join!(
            team_relationships(pool, team_id, &player_ids, &sport),
            load_transfer_identity_threshold(pool, &sport),
        )?;

        let start = Instant::now();
        let pair_deadline =
            budget_deadline(start, models.handler_budget, TRANSFER_PAIR_BUDGET_FRAC);
        let wrap_deadline =
            budget_deadline(start, models.handler_budget, TRANSFER_WRAP_BUDGET_FRAC);

        let mut unknown = 0usize;
        let mut errored = 0usize;
        let mut vetted = 0usize;
        let mut pairs_deferred = 0usize;
        let mut autofill_refresh_wanted = false;
        for (idx, c) in candidates.iter().enumerate() {
            // Stop on the budget, not on the axe. Everything behind this point is already
            // persisted and will debounce-skip next round, so the pairs left here are the only
            // ones that cost anything to carry forward.
            if past(pair_deadline) {
                pairs_deferred = candidates.len() - idx;
                break;
            }
            // Model-failure UNKNOWN is not an infrastructure error: it is a successful fail-closed
            // row that the `unknown` tally turns into a team retry. DB/build/persist errors are
            // infrastructure failures; keep scanning pairs for visibility, then fail the team item.
            let pair = async {
                // A missing batch row is semantically "none" (no identity row + no stats row),
                // matching the per-pair COALESCE(false) defaults. Person subjects carry
                // their own relationship (persons.team_id-derived).
                let relationship = c.relationship_override.clone().unwrap_or_else(|| {
                    relationships
                        .get(&c.player_id)
                        .cloned()
                        .unwrap_or_else(|| "none".to_string())
                });
                let out = match build_pair_request(
                    pool,
                    models,
                    team_id,
                    &team_name,
                    c,
                    &sport,
                    relationship,
                    TRANSFER_TEMPERATURE,
                )
                .await?
                {
                    PairBuild::Skipped {
                        components,
                        news_ids,
                    } => skipped_pair_output(
                        models,
                        c.player_id,
                        &c.subject_type,
                        components,
                        news_ids,
                    ),
                    PairBuild::Ready(ready) => {
                        // An unchanged resolved pair keeps serving without another model call.
                        if pair_unchanged(
                            pool,
                            team_id,
                            c.player_id,
                            &sport,
                            &c.subject_type,
                            &ready.input_hash,
                        )
                        .await?
                        {
                            debug!(
                                team = team_id,
                                player = c.player_id,
                                "transfers: pair debounce-skip, material inputs unchanged"
                            );
                            return Ok((Outcome::Skipped, true));
                        }
                        let backend = models.router.for_role(Role::TransferLogic);
                        crate::studio::insider::create_pair(
                            &crate::studio::Studio::new(backend.as_ref()),
                            *ready,
                        )
                        .await
                    }
                };
                if let Some(row) = &out.row {
                    let persisted_rumor_id = persist_transfer_row(
                        pool,
                        item,
                        team_id,
                        c.player_id,
                        &sport,
                        "periodic",
                        &out,
                        row,
                    )
                    .await?;
                    let Some(persisted_rumor_id) = persisted_rumor_id else {
                        return Ok((Outcome::Skipped, false));
                    };
                    let included_evidence = serde_json::json!({
                        "input_news_ids": &out.news_ids,
                        "prompted_news_ids": &out.prompted_news_ids,
                        "heat": out.heat,
                        "heat_components": transfer_components_json(&out.components),
                        "identity_apply_news_count": out.identity_apply_news.len(),
                        "is_rumor": row.is_rumor,
                        "direction": &row.direction,
                        "stage": &row.stage,
                        "confidence": row.confidence,
                        "source_attribution": &row.attribution,
                    });
                    let mut excluded = Vec::new();
                    let excluded_reason = match out.outcome {
                        Outcome::Cleared => Some("model_cleared_pair"),
                        Outcome::Unknown => Some("model_unknown_or_generate_failure"),
                        _ => None,
                    };
                    if let Some(reason) = excluded_reason {
                        excluded.push(serde_json::json!({
                            "reason": reason,
                            "trigger_payload": transfer_trigger_payload_json(&row.trigger_payload),
                        }));
                    }
                    if out.news_ids.len() > out.prompted_news_ids.len() {
                        let prompted: std::collections::HashSet<_> =
                            out.prompted_news_ids.iter().copied().collect();
                        let dropped_news_ids: Vec<_> = out
                            .news_ids
                            .iter()
                            .copied()
                            .filter(|id| !prompted.contains(id))
                            .collect();
                        if !dropped_news_ids.is_empty() {
                            excluded.push(serde_json::json!({
                                "reason": "budget_truncated",
                                "dropped_count": dropped_news_ids.len(),
                                "dropped_news_ids": dropped_news_ids,
                                "limit": TRANSFER_MAX_CORPUS_NEWS,
                            }));
                        }
                    }
                    if !out.stale_news_ids.is_empty() {
                        excluded.push(serde_json::json!({
                            "reason": "stale_news",
                            "dropped_count": out.stale_news_ids.len(),
                            "dropped_news_ids": &out.stale_news_ids,
                            "lookback_days": 14,
                        }));
                    }
                    insert_generation_ledger_best_effort(
                        pool,
                        &out,
                        TRANSFER_LEDGER,
                        LedgerEvent {
                            entity_type: "team",
                            entity_id: team_id,
                            sport: &sport,
                            pair_entity: Some(("player", c.player_id)),
                            trigger_type: "periodic",
                            trigger_payload: transfer_trigger_payload_json(&row.trigger_payload),
                            product_row_ids: vec![persisted_rumor_id],
                            included_evidence,
                            excluded_evidence: serde_json::json!(excluded),
                            context_budget: out.context_budget(serde_json::json!({
                                "num_predict": TRANSFER_NUM_PREDICT,
                            })),
                            parser_outcome: match out.outcome {
                                Outcome::Rumor => "rumor",
                                Outcome::Cleared => "cleared",
                                Outcome::Unknown => "unknown",
                                Outcome::Skipped => "skipped",
                            },
                        },
                    )
                    .await;
                    // Bank served rumors for memory and audit, outside the numeric loop.
                    if row.is_rumor == Some(true) {
                        if let Some(stage) = row.stage.as_deref() {
                            if let Err(e) = bank_transfer_junction_event(
                                pool,
                                &sport,
                                c.player_id,
                                team_id,
                                stage,
                                &out.provenance.model_version,
                                &out.news_ids,
                            )
                            .await
                            {
                                warn!(
                                    team = team_id,
                                    player = c.player_id,
                                    error = %e,
                                    "transfers: junction-event banking failed (best-effort)"
                                );
                            }
                        }
                    }
                    if let Some(heat) = out.heat {
                        if maybe_apply_transfer_identity(
                            pool,
                            models,
                            item,
                            team_id,
                            &team_name,
                            c,
                            &sport,
                            heat,
                            &out.identity_apply_news,
                            persisted_rumor_id,
                            row,
                            out.outcome,
                            identity_threshold.as_ref(),
                        )
                        .await?
                        {
                            autofill_refresh_wanted = true;
                        }
                    }
                }
                Ok::<(Outcome, bool), anyhow::Error>((out.outcome, true))
            }
            .await;
            match pair {
                Ok((_, false)) => return Ok(PluginOutcome::Superseded),
                Ok((Outcome::Unknown, true)) => unknown += 1,
                // Rumor/Cleared is a pair that reached a verdict on THIS run — the durable
                // progress the deferral protocol requires. Skipped is a debounce hit or an empty
                // corpus: correct, but it did not move the pair forward, so it does not count.
                Ok((Outcome::Rumor, true)) | Ok((Outcome::Cleared, true)) => vetted += 1,
                Ok((Outcome::Skipped, true)) => {}
                Err(e) => {
                    errored += 1;
                    warn!(
                        team = team_id,
                        player = c.player_id,
                        error = %e,
                        "transfers: pair infrastructure/persist error"
                    );
                }
            }
        }

        // Refresh autofill once per team drain after all applied pairs.
        if autofill_refresh_wanted {
            refresh_sport_autofill_concurrently(pool, &sport, "applied_transfer_identity").await?;
        }

        // Wrap the team, all candidates, and every player on the served board. This runs last
        // under its own budget; dead or unchanged boards skip inside the scorer.
        let mut wrap_targets: Vec<(&str, i32, String)> = vec![("team", team_id, team_name.clone())];
        let mut seen_players: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for c in &candidates {
            if seen_players.insert(c.player_id) {
                wrap_targets.push(("player", c.player_id, c.player_name.clone()));
            }
        }
        match load_wire_touched_players(pool, team_id, &sport).await {
            Ok(rumored) => {
                for (player_id, name) in rumored {
                    if seen_players.insert(player_id) {
                        wrap_targets.push(("player", player_id, name));
                    }
                }
            }
            Err(e) => {
                errored += 1;
                warn!(
                    team = team_id,
                    error = %e,
                    "transfers: wire-touched player load failed"
                );
            }
        }
        let mut wraps_deferred = 0usize;
        for (idx, (entity_type, entity_id, entity_name)) in wrap_targets.iter().enumerate() {
            if past(wrap_deadline) {
                wraps_deferred = wrap_targets.len() - idx;
                break;
            }
            match score_insider_entity(
                pool,
                models,
                item,
                entity_type,
                *entity_id,
                entity_name,
                &sport,
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => return Ok(PluginOutcome::Superseded),
                Err(e) => {
                    errored += 1;
                    warn!(
                        entity_type,
                        entity_id,
                        error = %e,
                        "transfers: insider score failed"
                    );
                }
            }
        }

        let deferred = pairs_deferred + wraps_deferred;
        info!(
            team = team_id,
            candidates = candidates.len(),
            pairs_vetted = vetted,
            pairs_unknown = unknown,
            pairs_deferred,
            wrap_targets = wrap_targets.len(),
            wraps_deferred,
            errored,
            elapsed_s = start.elapsed().as_secs(),
            "transfers: team drain finished"
        );

        // A real infrastructure failure still walks the retry ladder — that is what `attempts` is
        // for, and a broken pool or a failing persist is not something another turn fixes.
        if errored > 0 {
            bail!(
                "transfers: {errored} pair infrastructure/persist error(s) — retrying team {}",
                item.entity_id
            );
        }

        // Out of budget with work left. `vetted > 0` is the progress guarantee `work::defer`
        // demands: a round that resolved nothing must not get a free turn, or a team whose very
        // first pair outruns the whole budget would defer forever. Such a round falls through to
        // the bails below and burns an attempt like any other stuck item.
        //
        // Defer a progressing partial drain before charging unresolved pairs to the retry ladder.
        if deferred > 0 && vetted > 0 {
            let note = format!(
                "deferred: {pairs_deferred} pair(s) + {wraps_deferred} wrap(s) left after {}s",
                start.elapsed().as_secs()
            );
            // The plugin reports the partial drain; the worker performs the defer and
            // owns the superseded-claim check.
            debug!(team = team_id, %note, "transfers: team deferred to another turn");
            return Ok(PluginOutcome::deferred(note, TRANSFER_DEFER_DELAY));
        }

        if unknown > 0 {
            bail!(
                "transfers: {unknown} unresolved pair(s) (model failure) — retrying team {}",
                item.entity_id
            );
        }
        if deferred > 0 {
            bail!(
                "transfers: out of budget with {deferred} unit(s) left and no pair resolved — \
                 retrying team {}",
                item.entity_id
            );
        }
        complete_claimed(pool, item).await
    }
}

#[cfg(test)]
mod tests;

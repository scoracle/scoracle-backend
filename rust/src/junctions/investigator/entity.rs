//! The `investigate_entity` stage handler (PLAN-one-rail 5.3, re-scoped by Scott's NBA
//! vetting ruling — Phase 4 Log, 2026-08-03).
//!
//! Two work classes on one stage:
//!
//! * `entity_type='candidate'` — a mystery name the Editor's sweep nominated
//!   (`entity_candidates.id`). Discovery → gate → on ACCEPT, resolve to an existing
//!   entity (alias write, no new row) or create a `persons` row; every write cites a
//!   `source_documents` row; `acquisition_runs` records the attempt whatever the verdict.
//! * `entity_type='player'` — metadata enrichment of an EXISTING player (the NBA vetting
//!   project: date_of_birth, weight, height, photo_url are missing/untrusted). Identity is
//!   re-proven (career-team discriminator), facts land in `entity_facts` with provenance,
//!   and the convenience columns on `players` are updated — never inserted (D-2: the
//!   players table stays box-score-owned).
//!
//! Interpretation is CODE over Wikidata's structured claims — no model call. The gemma
//! prose-triage path (5.4's fallback for names Wikimedia doesn't know) is deferred to the
//! tuning ledger; the seat (`Role::Investigator`) exists and idles until then.

use super::discover::{
    wikidata_item, wikidata_search, WikidataHit, WikidataItem,
};
use super::gate::{
    decide, display_height, display_weight, nba_headshot_url, wire_date, RoleClass, Verdict,
};
use crate::fetch::{BudgetedFetcher, FetchPolicy};
use crate::harness::Harness;
use crate::stage::{StageHandler, ARCHBOX_GEMMA_SLOTS};
use crate::work::{Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

pub const INVESTIGATE_PARSER_VERSION: &str = "investigate-entity-wikidata-v1";
/// How many search hits get a full item fetch. Three covers namesakes without burning the
/// budget on long-tail hits.
const MAX_ITEMS: usize = 3;

/// Wikimedia gets a polite, long-cache policy: labels and claims move slowly, and repeat
/// investigations (30 NBA teams, recurring coaches) should be cache hits, not fetches.
fn wikimedia_policy() -> FetchPolicy {
    FetchPolicy::new(Duration::from_secs(2), Duration::from_secs(7 * 24 * 3600))
}

pub struct InvestigateEntityHandler {
    fetcher: BudgetedFetcher,
}

impl InvestigateEntityHandler {
    pub fn new() -> Result<Self> {
        Ok(Self {
            fetcher: BudgetedFetcher::new()?,
        })
    }
}

#[async_trait]
impl StageHandler for InvestigateEntityHandler {
    fn stage(&self) -> Stage {
        Stage::InvestigateEntity
    }

    fn max_in_flight(&self) -> usize {
        1 // the Editor outranks the Investigator on the shared card (5.3)
    }

    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(ARCHBOX_GEMMA_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        match item.entity_type.as_str() {
            "candidate" => investigate_candidate(hx, &self.fetcher, item).await,
            "player" => enrich_player(hx, &self.fetcher, item).await,
            other => Err(anyhow!(
                "investigate_entity got entity_type='{other}' (candidate|player)"
            )),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Shared discovery: name → screened items + team discriminators.
// ---------------------------------------------------------------------------------------

struct Discovery {
    hits: Vec<WikidataHit>,
    items: Vec<WikidataItem>,
    /// For items[i]: did some name form nrm-match the sought name (SQL nrm — mig 198).
    name_agreed: Vec<bool>,
    /// For items[i]: our team ids its P54/P6087 links resolved onto.
    our_teams: Vec<Vec<i32>>,
}

async fn discover(
    hx: &Harness,
    fetcher: &BudgetedFetcher,
    sport: &str,
    name: &str,
) -> Result<Discovery> {
    let policy = wikimedia_policy();
    let hits = wikidata_search(fetcher, &hx.pool, &policy, name, 5).await?;
    let mut items = Vec::new();
    for hit in hits.iter().take(MAX_ITEMS) {
        match wikidata_item(fetcher, &hx.pool, &policy, &hit.qid).await {
            Ok(it) => items.push(it),
            Err(e) => warn!(qid = %hit.qid, error = %format!("{e:#}"), "wikidata item fetch failed"),
        }
    }
    let mut name_agreed = Vec::with_capacity(items.len());
    let mut our_teams = Vec::with_capacity(items.len());
    for it in &items {
        name_agreed.push(name_forms_agree(&hx.pool, name, it).await?);
        let mut qids: Vec<String> = it.member_of_teams.clone();
        qids.extend(it.coach_of_teams.iter().cloned());
        qids.truncate(24);
        our_teams.push(resolve_team_qids(hx, fetcher, sport, &qids).await?);
    }
    Ok(Discovery {
        hits,
        items,
        name_agreed,
        our_teams,
    })
}

/// name_forms_agree runs the name SCREEN through `public.nrm()` — the database's one
/// normalizer (mig 198), never a Rust re-implementation. Screen only; identity is the
/// discriminator clause.
async fn name_forms_agree(pool: &PgPool, sought: &str, it: &WikidataItem) -> Result<bool> {
    let mut forms = vec![it.label.clone()];
    forms.extend(it.aliases.iter().cloned());
    let agreed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM unnest($2::text[]) f
            WHERE public.nrm(f) = public.nrm($1) AND public.nrm(f) <> ''
        )
        "#,
    )
    .bind(sought)
    .bind(&forms)
    .fetch_one(pool)
    .await
    .context("nrm name screen")?;
    Ok(agreed)
}

/// resolve_team_qids maps Wikidata team QIDs onto OUR team ids: known mappings from
/// `entity_external_ids` first, then one batched label fetch for the rest, matching labels
/// through `entity_name_surfaces` (sport-scoped, exact nrm — T9). Newly proven mappings are
/// written back with provenance, so the mapping bootstraps itself over the first few runs.
async fn resolve_team_qids(
    hx: &Harness,
    fetcher: &BudgetedFetcher,
    sport: &str,
    qids: &[String],
) -> Result<Vec<i32>> {
    if qids.is_empty() {
        return Ok(Vec::new());
    }
    let mut map: HashMap<String, i32> = HashMap::new();
    let rows = sqlx::query(
        r#"
        SELECT external_id, entity_id FROM public.entity_external_ids
        WHERE namespace = 'wikidata' AND entity_type = 'team' AND sport = $1
          AND external_id = ANY($2)
        "#,
    )
    .bind(sport)
    .bind(qids)
    .fetch_all(&hx.pool)
    .await
    .context("load known team qid mappings")?;
    for r in rows {
        map.insert(r.get::<String, _>("external_id"), r.get::<i32, _>("entity_id"));
    }

    let unknown: Vec<&String> = qids.iter().filter(|q| !map.contains_key(*q)).collect();
    if !unknown.is_empty() {
        let ids = unknown
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("|");
        let url = format!(
            "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={}&props=labels&languages=en&format=json",
            super::discover::urlencode(&ids)
        );
        match fetcher.fetch(&hx.pool, &url, &wikimedia_policy()).await {
            Ok(fetched) => {
                let v: serde_json::Value =
                    serde_json::from_str(&fetched.body).context("parse team labels batch")?;
                for q in unknown {
                    let Some(label) = v
                        .pointer(&format!("/entities/{q}/labels/en/value"))
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    // Exact nrm surface match, and ONLY a unique one — two teams sharing a
                    // normalized label refuse (never a coin flip).
                    let matches: Vec<i32> = sqlx::query_scalar(
                        r#"
                        SELECT DISTINCT entity_id FROM public.entity_name_surfaces
                        WHERE sport = $1 AND entity_type = 'team' AND norm = public.nrm($2)
                        "#,
                    )
                    .bind(sport)
                    .bind(label)
                    .fetch_all(&hx.pool)
                    .await
                    .context("match team label to surfaces")?;
                    if let [team_id] = matches.as_slice() {
                        map.insert(q.clone(), *team_id);
                        sqlx::query(
                            r#"
                            INSERT INTO public.entity_external_ids
                                (entity_type, entity_id, sport, namespace, external_id,
                                 source_document_id)
                            SELECT 'team', $1, $2, 'wikidata', $3, $4
                            WHERE NOT EXISTS (
                                SELECT 1 FROM public.entity_external_ids
                                WHERE entity_type = 'team' AND entity_id = $1 AND sport = $2
                                  AND namespace = 'wikidata' AND external_id = $3
                            )
                            "#,
                        )
                        .bind(team_id)
                        .bind(sport)
                        .bind(q)
                        .bind(fetched.document_id)
                        .execute(&hx.pool)
                        .await
                        .context("record team qid mapping")?;
                    }
                }
            }
            Err(e) => warn!(error = %e, "team label batch fetch failed; discriminators degrade to known mappings"),
        }
    }
    Ok(qids.iter().filter_map(|q| map.get(q).copied()).collect())
}

/// provenance_holds is gate clause (a), asserted against the STORED provenance row right
/// before any write. Two ways to satisfy it: the retained excerpt contains the name form
/// we are about to trust, OR the document is the item's own `wbgetentities` fetch — in
/// which case the label was PARSED FROM that document, so containment holds by
/// construction even when the excerpt bound truncated the labels section (measured on the
/// 2026-08-03 smoke: Şengün's claims JSON exceeds the 100k excerpt and his accept was
/// wrongly refused). The excerpt arm stays load-bearing for future prose-page sources.
async fn provenance_holds(pool: &PgPool, it: &WikidataItem) -> Result<bool> {
    let found: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT position(lower($2) in lower(retained_excerpt)) > 0
            OR (url LIKE '%wbgetentities%' AND url LIKE '%' || $3 || '%')
        FROM public.source_documents WHERE id = $1
        "#,
    )
    .bind(it.source_document_id)
    .bind(&it.label)
    .bind(&it.qid)
    .fetch_optional(pool)
    .await
    .context("check provenance containment")?;
    Ok(found.unwrap_or(false))
}

// ---------------------------------------------------------------------------------------
// Mode 1: the mystery candidate (entity_type='candidate').
// ---------------------------------------------------------------------------------------

struct CandidateRow {
    id: i64,
    /// nrm()-normalized; Wikidata search is case/diacritic-tolerant, so this searches fine.
    norm_name: String,
    state: String,
    sport: Option<String>,
    descriptor: Option<String>,
}

async fn load_candidate(pool: &PgPool, id: i64) -> Result<Option<CandidateRow>> {
    let row = sqlx::query(
        r#"
        SELECT c.id, c.norm_name, c.sport, c.state, m.editor_descriptor AS descriptor
        FROM public.entity_candidates c
        LEFT JOIN LATERAL (
            SELECT editor_descriptor FROM public.candidate_mentions
            WHERE candidate_id = c.id AND editor_descriptor IS NOT NULL
            ORDER BY observed_at DESC LIMIT 1
        ) m ON true
        WHERE c.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("load entity_candidate")?;
    Ok(row.map(|r| CandidateRow {
        id: r.get("id"),
        norm_name: r.get("norm_name"),
        sport: r.get("sport"),
        state: r.get("state"),
        descriptor: r.get("descriptor"),
    }))
}

async fn investigate_candidate(hx: &Harness, fetcher: &BudgetedFetcher, item: &Item) -> Result<()> {
    let Some(cand) = load_candidate(&hx.pool, item.entity_id).await? else {
        return Ok(()); // candidate deleted; nothing to do
    };
    if cand.state != "pending" {
        return Ok(()); // decided while queued (idempotent claim)
    }
    let sport = cand
        .sport
        .clone()
        .or_else(|| Some(item.sport.clone()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("candidate {} has no sport", cand.id))?;
    let search_name = cand.norm_name.clone();

    let d = discover(hx, fetcher, &sport, &search_name).await?;
    let verdict = decide(&sport, &d.items, &d.name_agreed, &bools_of(&d.our_teams));

    let run_plan = json!({
        "search": search_name,
        "sport": sport,
        "hits": d.hits.iter().map(|h| json!({"qid": h.qid, "label": h.label, "description": h.description})).collect::<Vec<_>>(),
        "items": d.items.iter().map(|i| json!({"qid": i.qid, "doc": i.source_document_id})).collect::<Vec<_>>(),
        "descriptor": cand.descriptor,
    });

    match verdict {
        Verdict::Accept { item_idx, role } => {
            let it = &d.items[item_idx];
            // Clause (a): the provenance row must contain the label we are trusting.
            if !provenance_holds(&hx.pool, it).await? {
                finish_candidate(hx, &cand, "rejected_insufficient_evidence", None, &run_plan,
                    "accept item excerpt missing name form").await?;
                return Ok(());
            }
            let Some(kind) = role.person_kind() else {
                finish_candidate(hx, &cand, "rejected_not_sport", None, &run_plan,
                    "accepted item has no writable kind").await?;
                return Ok(());
            };
            let team_id = d.our_teams[item_idx].first().copied();
            accept_candidate(hx, &cand, &sport, it, kind, role, team_id, &run_plan).await?;
        }
        Verdict::Ambiguous { survivor_idxs } => {
            let reason = format!("{} sport-relevant survivors, no unique discriminator", survivor_idxs.len());
            finish_candidate(hx, &cand, "ambiguous", None, &run_plan, &reason).await?;
        }
        Verdict::RejectedNotSport => {
            finish_candidate(hx, &cand, "rejected_not_sport", None, &run_plan, "no sport-relevant item").await?;
        }
        Verdict::RejectedInsufficientEvidence => {
            finish_candidate(hx, &cand, "rejected_insufficient_evidence", None, &run_plan,
                "no name-agreeing item").await?;
        }
    }
    Ok(())
}

fn bools_of(our_teams: &[Vec<i32>]) -> Vec<bool> {
    our_teams.iter().map(|t| !t.is_empty()).collect()
}

/// accept_candidate is the ONE write path for a new/linked person — a single transaction:
/// resolve-to-existing first (alias, no new row), else a `persons` row; aliases (append-only)
/// + direct surface mirror; a role fact; `coach_of` when the role is coach and a team
/// resolved; the acquisition run; the candidate's state. Every row cites the source doc.
#[allow(clippy::too_many_arguments)]
async fn accept_candidate(
    hx: &Harness,
    cand: &CandidateRow,
    sport: &str,
    it: &WikidataItem,
    kind: &str,
    role: RoleClass,
    team_id: Option<i32>,
    run_plan: &serde_json::Value,
) -> Result<()> {
    let mut tx = hx.pool.begin().await.context("begin accept")?;

    // Resolve-to-existing FIRST (5.5): an exact-surface person/player whose sport matches.
    // Discriminator for the merge: same team when both sides know one; a player-kind match
    // with no team agreement refuses the merge (new person is NOT created either — that
    // would duplicate; the verdict downgrades to ambiguous).
    let existing = sqlx::query(
        r#"
        SELECT DISTINCT s.entity_type, s.entity_id
        FROM public.entity_name_surfaces s
        WHERE s.sport = $1 AND s.entity_type IN ('player', 'person')
          AND s.norm = public.nrm($2)
        "#,
    )
    .bind(sport)
    .bind(&it.label)
    .fetch_all(&mut *tx)
    .await
    .context("existing entity lookup")?;

    let (resolved_type, resolved_id): (String, i32) = match existing.len() {
        0 => {
            let person_id: i32 = sqlx::query_scalar(
                r#"
                INSERT INTO public.persons (sport, full_name, kind, team_id, search_aliases, meta)
                VALUES ($1, $2, $3, $4, $5, jsonb_build_object('wikidata', $6::text))
                RETURNING id
                "#,
            )
            .bind(sport)
            .bind(&it.label)
            .bind(kind)
            .bind(team_id)
            .bind(&it.aliases)
            .bind(&it.qid)
            .fetch_one(&mut *tx)
            .await
            .context("insert persons row")?;
            ("person".to_string(), person_id)
        }
        1 => {
            let r = &existing[0];
            let etype: String = r.get("entity_type");
            let eid: i32 = r.get("entity_id");
            // Merging onto an existing entity needs the discriminator to AGREE, not merely
            // exist: for players, the item's career teams must include the player's team.
            if etype == "player" {
                let player_team: Option<i32> =
                    sqlx::query_scalar("SELECT team_id FROM public.players WHERE id = $1")
                        .bind(eid)
                        .fetch_optional(&mut *tx)
                        .await
                        .context("load player team for merge check")?
                        .flatten();
                let agree = match (player_team, team_id) {
                    (Some(pt), _) => {
                        // team_id here is the FIRST resolved team; check all resolved.
                        // Conservative: require the player's team among the item's teams.
                        let all: Vec<i32> = sqlx::query_scalar(
                            r#"
                            SELECT entity_id FROM public.entity_external_ids
                            WHERE namespace = 'wikidata' AND entity_type = 'team'
                              AND sport = $1 AND external_id = ANY($2)
                            "#,
                        )
                        .bind(sport)
                        .bind(&it.member_of_teams)
                        .fetch_all(&mut *tx)
                        .await
                        .context("merge team check")?;
                        all.contains(&pt)
                    }
                    (None, _) => false,
                };
                if !agree {
                    tx.rollback().await.ok();
                    finish_candidate(hx, cand, "ambiguous", None, run_plan,
                        "surface matches existing player but team discriminator disagrees").await?;
                    return Ok(());
                }
            }
            (etype, eid)
        }
        _ => {
            tx.rollback().await.ok();
            finish_candidate(hx, cand, "ambiguous", None, run_plan,
                "multiple existing entities share the surface").await?;
            return Ok(());
        }
    };

    // Aliases (append-only ledger) + direct surface mirror, per name form.
    let mut forms: Vec<String> = vec![it.label.clone()];
    forms.extend(it.aliases.iter().cloned());
    for form in &forms {
        sqlx::query(
            r#"
            INSERT INTO public.entity_aliases
                (entity_type, entity_id, sport, alias, norm_alias, source_document_id, state)
            VALUES ($1, $2, $3, $4, public.nrm($4), $5, 'active')
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(form)
        .bind(it.source_document_id)
        .execute(&mut *tx)
        .await
        .context("insert entity_alias")?;
        sqlx::query(
            r#"
            INSERT INTO public.entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind)
            SELECT $1, $2, $3, public.nrm($4), 'alias'
            WHERE public.nrm($4) <> ''
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(form)
        .execute(&mut *tx)
        .await
        .context("mirror surface")?;
    }

    // External id + role fact + the coach_of relationship when it applies.
    sqlx::query(
        r#"
        INSERT INTO public.entity_external_ids
            (entity_type, entity_id, sport, namespace, external_id, source_document_id)
        SELECT $1, $2, $3, 'wikidata', $4, $5
        WHERE NOT EXISTS (
            SELECT 1 FROM public.entity_external_ids
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
              AND namespace = 'wikidata' AND external_id = $4
        )
        "#,
    )
    .bind(&resolved_type)
    .bind(resolved_id)
    .bind(sport)
    .bind(&it.qid)
    .bind(it.source_document_id)
    .execute(&mut *tx)
    .await
    .context("insert person external id")?;

    sqlx::query(
        r#"
        INSERT INTO public.entity_facts
            (entity_type, entity_id, sport, fact_type, value_text, source_document_id, state)
        VALUES ($1, $2, $3, 'role', $4, $5, 'active')
        "#,
    )
    .bind(&resolved_type)
    .bind(resolved_id)
    .bind(sport)
    .bind(kind)
    .bind(it.source_document_id)
    .execute(&mut *tx)
    .await
    .context("insert role fact")?;

    if role == RoleClass::Coach {
        if let Some(team) = team_id {
            sqlx::query(
                r#"
                INSERT INTO public.entity_relationships
                    (subject_entity_type, subject_entity_id, subject_sport,
                     predicate, object_entity_type, object_entity_id, object_sport,
                     source_document_id, state)
                VALUES ($1, $2, $3, 'coach_of', 'team', $4, $3, $5, 'active')
                "#,
            )
            .bind(&resolved_type)
            .bind(resolved_id)
            .bind(sport)
            .bind(team)
            .bind(it.source_document_id)
            .execute(&mut *tx)
            .await
            .context("insert coach_of relationship")?;
        }
    }

    record_run(&mut tx, cand.id, "accepted", run_plan, None).await?;
    sqlx::query(
        r#"
        UPDATE public.entity_candidates
        SET state = 'accepted', resolved_entity_type = $2, resolved_entity_id = $3,
            decided_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(cand.id)
    .bind(&resolved_type)
    .bind(resolved_id)
    .execute(&mut *tx)
    .await
    .context("mark candidate accepted")?;

    tx.commit().await.context("commit accept")?;
    info!(
        candidate_id = cand.id,
        entity_type = %resolved_type,
        entity_id = resolved_id,
        qid = %it.qid,
        kind,
        "investigator accepted candidate"
    );
    Ok(())
}

/// finish_candidate records a non-accept verdict + its acquisition run in one transaction.
async fn finish_candidate(
    hx: &Harness,
    cand: &CandidateRow,
    state: &str,
    resolved: Option<(String, i32)>,
    run_plan: &serde_json::Value,
    reason: &str,
) -> Result<()> {
    let mut tx = hx.pool.begin().await.context("begin finish")?;
    record_run(&mut tx, cand.id, state, run_plan, Some(reason)).await?;
    sqlx::query(
        r#"
        UPDATE public.entity_candidates
        SET state = $2,
            resolved_entity_type = $3, resolved_entity_id = $4,
            decided_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(cand.id)
    .bind(state)
    .bind(resolved.as_ref().map(|(t, _)| t.clone()))
    .bind(resolved.as_ref().map(|(_, i)| *i))
    .execute(&mut *tx)
    .await
    .context("mark candidate decided")?;
    tx.commit().await.context("commit finish")?;
    info!(candidate_id = cand.id, state, reason, "investigator decided candidate");
    Ok(())
}

async fn record_run(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    candidate_id: i64,
    outcome: &str,
    query_plan: &serde_json::Value,
    rejection_reason: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.acquisition_runs
            (candidate_id, status, query_plan, outcome, rejection_reason,
             model_version, parser_version, started_at, finished_at)
        VALUES ($1, 'completed', $2::jsonb, $3, $4, NULL, $5, NOW(), NOW())
        "#,
    )
    .bind(candidate_id)
    .bind(query_plan)
    .bind(outcome)
    .bind(rejection_reason)
    .bind(INVESTIGATE_PARSER_VERSION)
    .execute(&mut **tx)
    .await
    .context("record acquisition run")?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Mode 2: player enrichment (the NBA vetting project).
// ---------------------------------------------------------------------------------------

async fn enrich_player(hx: &Harness, fetcher: &BudgetedFetcher, item: &Item) -> Result<()> {
    let player_id = item.entity_id_i32()?;
    let Some(row) = sqlx::query(
        "SELECT name, sport, team_id FROM public.players WHERE id = $1",
    )
    .bind(player_id)
    .fetch_optional(&hx.pool)
    .await
    .context("load player for enrichment")?
    else {
        return Ok(());
    };
    let name: String = row.get("name");
    let sport: String = row.get("sport");
    let team_id: Option<i32> = row.get("team_id");

    let d = discover(hx, fetcher, &sport, &name).await?;
    // The enrichment discriminator is STRICTER than membership-of-any-our-team: the item's
    // career must include THIS player's current team. Without a team on our side, refuse —
    // enrichment of team-less players waits for a reconciled team, not a guess.
    let matched: Vec<bool> = d
        .our_teams
        .iter()
        .map(|teams| team_id.is_some_and(|t| teams.contains(&t)))
        .collect();
    let verdict = decide(&sport, &d.items, &d.name_agreed, &matched);

    let Verdict::Accept { item_idx, .. } = verdict else {
        info!(player_id, %name, ?verdict, "enrichment refused (no unique discriminated item)");
        return Ok(());
    };
    let it = &d.items[item_idx];
    if !provenance_holds(&hx.pool, it).await? {
        warn!(player_id, qid = %it.qid, "enrichment accept item fails provenance containment; refusing");
        return Ok(());
    }

    let dob = it.date_of_birth.as_deref().and_then(wire_date);
    let weight = it.weight_kg.map(|kg| display_weight(&sport, kg));
    let height = it.height_cm.map(|cm| display_height(&sport, cm));
    let photo = it.nba_id.as_deref().and_then(nba_headshot_url).filter(|_| sport == "NBA");

    let mut tx = hx.pool.begin().await.context("begin enrichment")?;

    // Facts with provenance; a correction is a revision — prior active facts of the same
    // type are superseded, never overwritten (§4).
    let weight_fact = it.weight_kg.map(|k| format!("{k}"));
    let height_fact = it.height_cm.map(|c| format!("{c}"));
    for (ft, val) in [
        ("date_of_birth", dob.as_deref()),
        ("weight_kg", weight_fact.as_deref()),
        ("height_cm", height_fact.as_deref()),
        ("photo_url", photo.as_deref()),
    ] {
        let Some(val) = val else { continue };
        sqlx::query(
            r#"
            UPDATE public.entity_facts SET state = 'superseded'
            WHERE entity_type = 'player' AND entity_id = $1 AND sport = $2
              AND fact_type = $3 AND state = 'active' AND value_text IS DISTINCT FROM $4
            "#,
        )
        .bind(player_id)
        .bind(&sport)
        .bind(ft)
        .bind(val)
        .execute(&mut *tx)
        .await
        .context("supersede prior fact")?;
        sqlx::query(
            r#"
            INSERT INTO public.entity_facts
                (entity_type, entity_id, sport, fact_type, value_text, source_document_id, state)
            SELECT 'player', $1, $2, $3, $4, $5, 'active'
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_facts
                WHERE entity_type = 'player' AND entity_id = $1 AND sport = $2
                  AND fact_type = $3 AND state = 'active' AND value_text = $4
            )
            "#,
        )
        .bind(player_id)
        .bind(&sport)
        .bind(ft)
        .bind(val)
        .bind(it.source_document_id)
        .execute(&mut *tx)
        .await
        .context("insert fact")?;
    }

    // External ids (wikidata + the sport's own id when present).
    for (ns, ext) in [
        ("wikidata", Some(it.qid.clone())),
        ("nba", it.nba_id.clone()),
    ] {
        let Some(ext) = ext else { continue };
        sqlx::query(
            r#"
            INSERT INTO public.entity_external_ids
                (entity_type, entity_id, sport, namespace, external_id, source_document_id)
            SELECT 'player', $1, $2, $3, $4, $5
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_external_ids
                WHERE entity_type = 'player' AND entity_id = $1 AND sport = $2
                  AND namespace = $3 AND external_id = $4
            )
            "#,
        )
        .bind(player_id)
        .bind(&sport)
        .bind(ns)
        .bind(&ext)
        .bind(it.source_document_id)
        .execute(&mut *tx)
        .await
        .context("insert player external id")?;
    }

    // Convenience copies on players — UPDATE only (the table stays box-score-owned, D-2).
    sqlx::query(
        r#"
        UPDATE public.players
        SET date_of_birth = COALESCE($2::date, date_of_birth),
            weight = COALESCE($3, weight),
            height = COALESCE($4, height),
            photo_url = COALESCE($5, photo_url),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(player_id)
    .bind(dob.as_deref())
    .bind(weight.as_deref())
    .bind(height.as_deref())
    .bind(photo.as_deref())
    .execute(&mut *tx)
    .await
    .context("update player convenience columns")?;

    tx.commit().await.context("commit enrichment")?;
    info!(
        player_id,
        %name,
        qid = %it.qid,
        dob = dob.as_deref().unwrap_or("-"),
        weight = weight.as_deref().unwrap_or("-"),
        height = height.as_deref().unwrap_or("-"),
        photo = photo.is_some(),
        "investigator enriched player"
    );
    Ok(())
}

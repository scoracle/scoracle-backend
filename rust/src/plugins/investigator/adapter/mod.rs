//! The `investigate_entity` stage handler.
//!
//! Two work classes on one stage:
//!
//! * `entity_type='candidate'` — a mystery name nominated by source discovery (including historical Editor nominations)
//!   (`entity_candidates.id`). Discovery → gate → on ACCEPT, resolve to an existing
//!   entity (alias write, no new row) or create a `persons` row; every write cites a
//!   `source_documents` row; `acquisition_runs` records the attempt whatever the verdict.
//! * `entity_type='player'` — metadata enrichment of an EXISTING player (the NBA vetting
//!   project: date_of_birth, weight, height, photo_url are missing/untrusted). Identity is
//!   re-proven (career-team discriminator), facts land in `entity_facts` with provenance,
//!   and the convenience columns on `players` are updated, never inserted; the
//!   players table stays box-score-owned).
//!
//! Wikidata interpretation is deterministic. Unsupported prose evidence cannot authorize writes.

use self::discover::{wikidata_item, wikidata_search, WikidataHit};
use crate::application::queue::work::Item;
use crate::application::tools::{ScopedWeb, ToolLedger, WebBroker};
use crate::evidence::fetch::FetchPolicy;
use crate::plugins::investigator::cognition::gate::{
    commons_image_url, decide, display_height, display_weight, nba_headshot_url, wire_date,
    RoleClass, Verdict,
};
use crate::plugins::investigator::cognition::WikidataItem;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

pub const INVESTIGATE_PARSER_VERSION: &str = "investigate-entity-wikidata-v1";
/// Search hits that receive a full item fetch.
const MAX_ITEMS: usize = 5;

/// Wikimedia gets a polite, long-cache policy: labels and claims move slowly, and repeat
/// investigations (30 NBA teams, recurring coaches) should be cache hits, not fetches.
fn wikimedia_policy() -> FetchPolicy {
    FetchPolicy::new(Duration::from_secs(2), Duration::from_secs(7 * 24 * 3600))
}

pub struct InvestigateEntityHandler {
    pool: sqlx::PgPool,
    /// The room's web workspace. The plugin's manifest declares the Wikimedia domain
    /// class; this broker enforces that grant on every call.
    web: std::sync::Arc<WebBroker>,
}

impl InvestigateEntityHandler {
    pub fn new(pool: sqlx::PgPool, web: std::sync::Arc<WebBroker>) -> Self {
        Self { pool, web }
    }
}

#[async_trait]
impl StudioPlugin for InvestigateEntityHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::investigator::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let mut mappings = Vec::new();
        let ledger = ToolLedger::new();
        let web = self.web.scope(pool, self.manifest(), &ledger);
        let decision = match item.entity_type.as_str() {
            "candidate" => investigate_candidate(pool, &web, item, &mut mappings).await?,
            "player" => enrich_player(pool, &web, item, &mut mappings).await?,
            "team" => enrich_team(pool, &web, item).await?,
            other => {
                return Err(anyhow!(
                    "investigate_entity got entity_type='{other}' (candidate|player|team)"
                ))
            }
        };
        commit_claimed(pool, item, &mappings, &decision).await
    }
}

// ---------------------------------------------------------------------------------------
// Shared discovery: name → screened items + team discriminators.
// ---------------------------------------------------------------------------------------

struct Discovery {
    hits: Vec<WikidataHit>,
    items: Vec<WikidataItem>,
    /// For each item, whether a normalized name form matches the sought name.
    name_agreed: Vec<bool>,
    /// For items[i]: our team ids its P54/P6087 links resolved onto.
    our_teams: Vec<Vec<i32>>,
}

async fn discover(
    pool: &sqlx::PgPool,
    web: &ScopedWeb<'_>,
    sport: &str,
    name: &str,
    mappings: &mut Vec<TeamMapping>,
) -> Result<Discovery> {
    let policy = wikimedia_policy();
    let hits = wikidata_search(web, &policy, name, 5).await?;
    let mut items = Vec::new();
    for hit in hits.iter().take(MAX_ITEMS) {
        match wikidata_item(web, &policy, &hit.qid).await {
            Ok(it) => items.push(it),
            Err(e) => {
                warn!(qid = %hit.qid, error = %format!("{e:#}"), "wikidata item fetch failed")
            }
        }
    }
    let mut name_agreed = Vec::with_capacity(items.len());
    let mut our_teams = Vec::with_capacity(items.len());
    for it in &items {
        name_agreed.push(name_forms_agree(pool, name, it).await?);
        let mut qids: Vec<String> = it.member_of_teams.clone();
        qids.extend(it.coach_of_teams.iter().cloned());
        qids.extend(it.owner_of_teams.iter().cloned());
        qids.truncate(24);
        our_teams.push(resolve_team_qids(pool, web, sport, &qids, mappings).await?);
    }
    Ok(Discovery {
        hits,
        items,
        name_agreed,
        our_teams,
    })
}

/// Runs the name screen through the database's one `public.nrm()` normalizer.
/// Screen only; identity is the
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
    pool: &sqlx::PgPool,
    web: &ScopedWeb<'_>,
    sport: &str,
    qids: &[String],
    mappings: &mut Vec<TeamMapping>,
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
    .fetch_all(pool)
    .await
    .context("load known team qid mappings")?;
    for r in rows {
        map.insert(
            r.get::<String, _>("external_id"),
            r.get::<i32, _>("entity_id"),
        );
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
            crate::plugins::investigator::cognition::urlencode(&ids)
        );
        match web.fetch_wikimedia(&url, &wikimedia_policy()).await {
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
                    .fetch_all(pool)
                    .await
                    .context("match team label to surfaces")?;
                    if let [team_id] = matches.as_slice() {
                        map.insert(q.clone(), *team_id);
                        mappings.push(TeamMapping {
                            team_id: *team_id,
                            sport: sport.to_string(),
                            qid: q.clone(),
                            document_id: fetched.document_id,
                        });
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "team label batch fetch failed; discriminators degrade to known mappings")
            }
        }
    }
    Ok(qids.iter().filter_map(|q| map.get(q).copied()).collect())
}

/// Checks that stored provenance contains the trusted name or is the item's own entity fetch.
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

#[derive(Clone)]
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

async fn investigate_candidate(
    pool: &sqlx::PgPool,
    web: &ScopedWeb<'_>,
    item: &Item,
    mappings: &mut Vec<TeamMapping>,
) -> Result<Decision> {
    let Some(cand) = load_candidate(pool, item.entity_id).await? else {
        return Ok(Decision::Unchanged); // candidate deleted; nothing to do
    };
    if cand.state != "pending" {
        return Ok(Decision::Unchanged); // decided while queued (idempotent claim)
    }
    let sport = cand
        .sport
        .clone()
        .or_else(|| Some(item.sport.clone()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("candidate {} has no sport", cand.id))?;
    let search_name = cand.norm_name.clone();

    let d = discover(pool, web, &sport, &search_name, mappings).await?;
    let verdict = decide(
        &sport,
        &d.items,
        &d.name_agreed,
        &d.our_teams
            .iter()
            .map(|teams| !teams.is_empty())
            .collect::<Vec<_>>(),
    );

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
            if !provenance_holds(pool, it).await? {
                return finish_candidate(
                    &cand,
                    "rejected_insufficient_evidence",
                    None,
                    &run_plan,
                    "accept item excerpt missing name form",
                );
            }
            let Some(kind) = role.person_kind() else {
                return finish_candidate(
                    &cand,
                    "rejected_not_sport",
                    None,
                    &run_plan,
                    "accepted item has no writable kind",
                );
            };
            accept_candidate(
                &cand,
                &sport,
                it,
                kind,
                role,
                &d.our_teams[item_idx],
                &run_plan,
            )
        }
        Verdict::Ambiguous { survivor_idxs } => {
            let reason = format!(
                "{} sport-relevant survivors, no unique discriminator",
                survivor_idxs.len()
            );
            finish_candidate(&cand, "ambiguous", None, &run_plan, &reason)
        }
        Verdict::RejectedNotSport => finish_candidate(
            &cand,
            "rejected_not_sport",
            None,
            &run_plan,
            "no sport-relevant item",
        ),
        Verdict::RejectedInsufficientEvidence => finish_candidate(
            &cand,
            "rejected_insufficient_evidence",
            None,
            &run_plan,
            "no supported structured identity evidence; prose extraction is unavailable",
        ),
    }
}

// ---------------------------------------------------------------------------------------
// Mode 2: player enrichment (the NBA vetting project).
// ---------------------------------------------------------------------------------------

async fn enrich_player(
    pool: &sqlx::PgPool,
    web: &ScopedWeb<'_>,
    item: &Item,
    mappings: &mut Vec<TeamMapping>,
) -> Result<Decision> {
    let player_id = item.entity_id_i32()?;
    let Some(row) = sqlx::query(
        "SELECT p.name, p.sport, i.team_id FROM public.players p
        LEFT JOIN public.player_current_identity i ON i.player_id=p.id AND i.sport=p.sport
        WHERE p.id=$1 AND p.sport=$2",
    )
    .bind(player_id)
    .bind(item.sport.to_uppercase())
    .fetch_optional(pool)
    .await
    .context("load player for enrichment")?
    else {
        return Ok(Decision::Unchanged);
    };
    let name: String = row.get("name");
    let sport: String = row.get("sport");
    let team_id: Option<i32> = row.get("team_id");

    let d = discover(pool, web, &sport, &name, mappings).await?;
    // The enrichment discriminator is STRICTER than membership-of-any-our-team: the item's
    // career must include THIS player's current team. Without a team on our side, refuse —
    // enrichment of team-less players waits for a reconciled team, not a guess.
    let matched: Vec<bool> = d
        .our_teams
        .iter()
        .map(|teams| team_id.is_some_and(|t| teams.contains(&t)))
        .collect();
    let verdict = decide(&sport, &d.items, &d.name_agreed, &matched);

    let item_idx = match verdict {
        Verdict::Accept { item_idx, .. } => item_idx,
        _ => {
            info!(player_id, %name, ?verdict, "enrichment refused (no unique structured discriminator; prose corroboration unavailable)");
            return Ok(Decision::Unchanged);
        }
    };
    let it = &d.items[item_idx];
    if !provenance_holds(pool, it).await? {
        warn!(player_id, qid = %it.qid, "enrichment accept item fails provenance containment; refusing");
        return Ok(Decision::Unchanged);
    }

    Ok(Decision::Player {
        player_id,
        sport,
        it: Box::new(it.clone()),
    })
}

/// Team enrichment. No search or disambiguation: the team's Wikidata QID is
/// already bound in entity_external_ids (the team resolver bootstraps them), so this
/// fetches a KNOWN item, screens the name, and revises exactly what the policy allows —
/// venue_name (P115, current tenure) and logo_url (P154, P18 fallback). City, founding
/// year, sport, name: absent from the policy, frozen, untouchable from here.
async fn enrich_team(pool: &sqlx::PgPool, web: &ScopedWeb<'_>, item: &Item) -> Result<Decision> {
    let team_id = item.entity_id_i32()?;
    let Some(row) =
        sqlx::query("SELECT name, sport FROM public.teams WHERE id = $1 AND sport = $2")
            .bind(team_id)
            .bind(item.sport.to_uppercase())
            .fetch_optional(pool)
            .await
            .context("load team for enrichment")?
    else {
        return Ok(Decision::Unchanged);
    };
    let name: String = row.get("name");
    let sport: String = row.get("sport");

    let Some(qid) = sqlx::query_scalar::<_, String>(
        r#"
        SELECT external_id FROM public.entity_external_ids
        WHERE entity_type = 'team' AND entity_id = $1 AND namespace = 'wikidata'
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(team_id)
    .fetch_optional(pool)
    .await
    .context("load team wikidata id")?
    else {
        info!(team_id, %name, "team enrichment skipped: no wikidata handle");
        return Ok(Decision::Unchanged);
    };

    let policy = load_fact_policy(pool).await?;
    let fetch_policy = wikimedia_policy();
    let it = wikidata_item(web, &fetch_policy, &qid).await?;

    // The same two code gates every acceptance passes: the item must carry our name
    // form, and the stored excerpt must literally contain it.
    if !name_forms_agree(pool, &name, &it).await? {
        info!(team_id, %name, qid = %it.qid, "team enrichment refused: name disagreement");
        return Ok(Decision::Unchanged);
    }
    if !provenance_holds(pool, &it).await? {
        warn!(team_id, qid = %it.qid, "team enrichment fails provenance containment; refusing");
        return Ok(Decision::Unchanged);
    }

    // Venue label needs the venue item's own fetch (its wbgetentities doc IS its
    // provenance, per the standing rule).
    let venue_name = match it.venue_qid.as_deref() {
        Some(vq) if policy_allows(&policy, "team", "venue_name") => {
            match wikidata_item(web, &fetch_policy, vq).await {
                Ok(v) if !v.label.is_empty() => Some((v.label, v.source_document_id)),
                Ok(_) => None,
                Err(e) => {
                    warn!(team_id, venue_qid = vq, error = %format!("{e:#}"),
                        "venue item fetch failed; skipping venue this round");
                    None
                }
            }
        }
        _ => None,
    };
    let logo = it
        .logo_file
        .as_deref()
        .or(it.image_file.as_deref())
        .and_then(commons_image_url)
        .filter(|_| policy_allows(&policy, "team", "logo_url"));

    if venue_name.is_none() && logo.is_none() {
        info!(team_id, %name, qid = %it.qid, "team enrichment: nothing writable this round");
        return Ok(Decision::Unchanged);
    }

    Ok(Decision::Team {
        team_id,
        sport,
        venue: venue_name,
        logo: logo.map(|value| (value, it.source_document_id)),
    })
}

struct TeamMapping {
    team_id: i32,
    sport: String,
    qid: String,
    document_id: i64,
}

enum Decision {
    Unchanged,
    Accept {
        candidate: CandidateRow,
        sport: String,
        item: Box<WikidataItem>,
        kind: String,
        role: RoleClass,
        teams: Vec<i32>,
        plan: serde_json::Value,
    },
    Refuse {
        candidate: CandidateRow,
        state: String,
        resolved: Option<(String, i32)>,
        plan: serde_json::Value,
        reason: String,
    },
    Player {
        player_id: i32,
        sport: String,
        it: Box<WikidataItem>,
    },
    Team {
        team_id: i32,
        sport: String,
        venue: Option<(String, i64)>,
        logo: Option<(String, i64)>,
    },
}

#[allow(clippy::too_many_arguments)]
fn accept_candidate(
    cand: &CandidateRow,
    sport: &str,
    it: &WikidataItem,
    kind: &str,
    role: RoleClass,
    career_team_ids: &[i32],
    run_plan: &serde_json::Value,
) -> Result<Decision> {
    Ok(Decision::Accept {
        candidate: cand.clone(),
        sport: sport.to_string(),
        item: Box::new(it.clone()),
        kind: kind.to_string(),
        role,
        teams: career_team_ids.to_vec(),
        plan: run_plan.clone(),
    })
}

fn finish_candidate(
    cand: &CandidateRow,
    state: &str,
    resolved: Option<(String, i32)>,
    run_plan: &serde_json::Value,
    reason: &str,
) -> Result<Decision> {
    Ok(Decision::Refuse {
        candidate: cand.clone(),
        state: state.to_string(),
        resolved,
        plan: run_plan.clone(),
        reason: reason.to_string(),
    })
}

#[cfg(test)]
mod tests;

mod discover;
mod factsweep;
pub use factsweep::{run_factsweep, FactsweepRequest, FactsweepRunContext};
mod publish;
use publish::{commit_claimed, load_fact_policy, policy_allows};

//! The Editor's derivations — every judgment the ep1 contract does NOT ask the model (T2).
//!
//! The model DESCRIBES (page shape, names with descriptors, roles, a verbatim result line);
//! this module DERIVES: relevance, entity links, nomination eligibility, routing tags, and the
//! parsed result. Phase 3 persists the resolver outcome into `editor_reads.resolved`; later
//! phases consume the rest (routing tags → packets in Phase 6, result parse → box-score fork in
//! Phase 4, nomination → `entity_candidates` in Phase 5).

use super::{EditorEntityRole, NameMention};
use anyhow::{Context, Result};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use tracing::debug;
use tracing::debug;

/// `page_kind` values whose BODY is not reporting, whatever the headline promised. A page of this
/// shape cannot be materially about an entity because it is not materially about anything.
/// (Ported from the legacy seat, where three measured prompt revisions earned it.)
pub const NON_REPORTING_PAGE_KINDS: &[&str] = &["score_table", "listing_or_schedule", "roundup"];

/// derive_relevance computes the verdict from the model's DESCRIPTION of the page — the
/// second layer of false-positive defence, behind Google's ranked query, which is only ever a
/// hypothesis.
///
/// Two independent grounds for rejection:
///   * the page is not reporting at all (`page_kind`), or
///   * every one of our hypothesis entities is `absent` from it (`entity_roles`) — the model's
///     own word for "a different club that merely shares the name, or not in the text at all".
///
/// The bar is `absent`, not `subject`: an opponent-only or passing-mention
/// story is still in this entity's world. Only OUR entities vote — asked to label every listed
/// entity, the model also volunteers people it found in the body, and an unfiltered scan lets
/// those outvote the truth. Empty `entity_roles` is UNKNOWN, not rejection. When the model
/// placed none of ours but still listed one among the names it found, the omission is
/// sloppiness rather than a verdict (measured at 86% of the rejections it drove), so `names[]`
/// is consulted as a last resort before rejecting.
///
/// A third ground, the descriptor arm (§1a, measured 2026-08-01): a hypothesis entity whose
/// own `names[]` entry DESCRIBES a place ("capital city") does not count as present.
/// ⚠ **Measured on `gemma3:4b`, the runner at the time — the seat is `ministral-3:3b` now and the
/// behaviour has NOT been re-measured, but the arm is kept because it is cheap and fail-safe.**
/// That model reliably wrote the truth in the descriptor while still mislabeling the kind and the
/// role (`Paris` on a Tour de France page: kind `club`, role `subject`, descriptor "capital city"
/// — stable across seven prompt iterations). The description is the model's; the judgment is
/// ours (T2).
pub fn derive_relevance(
    page_kind: &str,
    entity_roles: &[EditorEntityRole],
    hypothesis: &[String],
    names: &[NameMention],
) -> bool {
    if NON_REPORTING_PAGE_KINDS
        .iter()
        .any(|k| page_kind.eq_ignore_ascii_case(k))
    {
        return false;
    }
    if entity_roles.is_empty() || hypothesis.is_empty() {
        return true;
    }
    // The model's own names[] entry for a role's entity, when it describes a place, retracts
    // that role's vote — the descriptor outranks the role label (it is copied from the text;
    // the label is a guess).
    let described_as_place = |entity: &str| {
        let hit = names
            .iter()
            .any(|n| n.name.eq_ignore_ascii_case(entity.trim()) && descriptor_names_place(&n.descriptor));
        // Friction 1 observability: log when the descriptor arm fires so we can measure whether
        // it's still needed on ministral-3:3b (originally measured on gemma3:4b, 2026-08-01).
        if hit {
            let descriptor = names
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(entity.trim()))
                .map(|n| n.descriptor.as_str())
                .unwrap_or("");
            debug!(
                entity = %entity,
                descriptor = %descriptor,
                "descriptor_names_place arm fired (relevance gate)"
            );
        }
        hit
    };
    // A `passing_mention` vote with no matching names[] entry is a label with no referent
    // (measured 2026-08-01: hypothesis "Fortuna" gets a passing_mention on a page whose only
    // Fortuna is "Fortuna Mining Corp" — the model string-associates the role, but its own
    // names list holds no such entity). Subject/opponent votes stand on their own: the model
    // reliably lists a story's principals, so an unlisted principal is names under-fill, not
    // evidence of absence.
    let supported = |r: &EditorEntityRole| {
        !r.role.eq_ignore_ascii_case("passing_mention")
            || names
                .iter()
                .any(|n| n.name.eq_ignore_ascii_case(r.entity.trim()))
    };
    let mut placed = entity_roles
        .iter()
        .filter(|r| entity_matches(hypothesis, &r.entity))
        .peekable();
    if placed.peek().is_some() {
        let result = placed.any(|r| {
            let not_absent = !r.role.eq_ignore_ascii_case("absent");
            let place = described_as_place(&r.entity);
            let sup = supported(r);
            
            // Observability: log when descriptor arm fires (unmeasured on ministral-3:3b)
            if place && not_absent {
                let descriptor = names
                    .iter()
                    .find(|n| n.name.eq_ignore_ascii_case(r.entity.trim()))
                    .map(|n| n.descriptor.as_str())
                    .unwrap_or("");
                debug!(
                    entity = %r.entity,
                    role = %r.role,
                    descriptor,
                    "descriptor_names_place arm fired (relevance rejected)"
                );
            }
            
            not_absent && !place && sup
        });
        return result;
    }
    
    // Fallback path: check names[] when entity_roles is empty
    let result = names
        .iter()
        .any(|n| {
            let matches = entity_matches(hypothesis, &n.name);
            let place = descriptor_names_place(&n.descriptor);
            
            // Observability: log when descriptor arm fires in fallback path
            if matches && place {
                debug!(
                    entity = %n.name,
                    descriptor = %n.descriptor,
                    "descriptor_names_place arm fired in fallback (relevance rejected)"
                );
            }
            
            matches && !place
        });
    result
}

/// descriptor_names_place is the descriptor arm of the resolver's gate (§1a: "the descriptor is
/// what lets code refuse `Paris`-the-city → Paris-the-club"). True when the ≤6-word descriptor
/// names a place rather than a club. Two lists, club words vetoing place words, because football
/// descriptors use place words in club senses ("city rivals", "town's biggest derby"): a
/// descriptor with any club-sense word is never place-flagged.
pub fn descriptor_names_place(descriptor: &str) -> bool {
    const PLACE_WORDS: &[&str] = &["city", "capital", "town", "village", "municipality"];
    const CLUB_WORDS: &[&str] = &[
        "club", "team", "side", "fc", "squad", "rivals", "rival", "derby", "manager", "coach",
        "player", "striker", "keeper", "goalkeeper", "midfielder", "defender", "forward",
        "winger", "captain",
    ];
    let mut saw_place = false;
    for w in descriptor.split(|c: char| !c.is_alphanumeric()) {
        if w.is_empty() {
            continue;
        }
        let w = w.to_lowercase();
        if CLUB_WORDS.contains(&w.as_str()) {
            return false;
        }
        if PLACE_WORDS.contains(&w.as_str()) {
            saw_place = true;
        }
    }
    saw_place
}

/// entity_matches tests one model-emitted name against our hypothesis list. The list carries the
/// `Name (team 42)` decoration the prompt renders, so a bare `Name` from the model has to match
/// the undecorated head — an exact comparison against the decorated string never fires.
pub(crate) fn entity_matches(ours: &[String], candidate: &str) -> bool {
    let c = candidate.trim();
    if c.is_empty() {
        return false;
    }
    ours.iter().any(|v| {
        let head = v.split(" (").next().unwrap_or(v).trim();
        head.eq_ignore_ascii_case(c) || v.trim().eq_ignore_ascii_case(c)
    })
}

/// Routing tags for one read: the story type's tag plus `charged` when the register is
/// non-neutral (E1). Phase 6's packet compiler consumes this; nothing persists it in shadow.
pub fn routing_tags(story_type: &str, register: &str) -> Vec<String> {
    let mut tags: Vec<String> = crate::bucket::routing_tags_from_story_type(story_type)
        .into_iter()
        .map(str::to_string)
        .collect();
    if !register.trim().is_empty() && !register.trim().eq_ignore_ascii_case("neutral") {
        tags.push("charged".to_string());
    }
    tags
}

/// A parsed final result — derived from ep1's verbatim `result_line`, never asked of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedResult {
    pub home: String,
    pub home_score: u32,
    pub away_score: u32,
    pub away: String,
}

/// parse_result_line parses the verbatim-or-empty `result_line` ("Real Madrid 2-1 Arsenal").
/// Strict on purpose: one score token (`D-D`, en/em dash or colon accepted) with a non-empty
/// name on each side. A line that does not parse nominates nothing — the honest failure mode
/// for a field the model copies rather than computes. Phase 4's box-score fork consumes this.
pub fn parse_result_line(line: &str) -> Option<ParsedResult> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut found: Option<(usize, u32, u32)> = None;
    for (i, tok) in tokens.iter().enumerate() {
        if let Some((h, a)) = parse_score_token(tok) {
            if found.is_some() {
                return None; // two score tokens = not a single final-result line
            }
            found = Some((i, h, a));
        }
    }
    let (i, home_score, away_score) = found?;
    if i == 0 || i == tokens.len() - 1 {
        return None;
    }
    Some(ParsedResult {
        home: tokens[..i].join(" "),
        home_score,
        away_score,
        away: tokens[i + 1..].join(" "),
    })
}

fn parse_score_token(tok: &str) -> Option<(u32, u32)> {
    let sep = tok.find(['-', '–', '—', ':'])?;
    let (h, a) = (&tok[..sep], &tok[sep..]);
    let a = a.trim_start_matches(['-', '–', '—', ':']);
    if h.is_empty() || a.is_empty() {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let a: u32 = a.parse().ok()?;
    // Sanity band: sports scores, not years ("2024-25 season" must not parse).
    if h > 150 || a > 150 {
        return None;
    }
    Some((h, a))
}

/// nominates_immediately is the 5.2 first-sight rule (Scott 2026-08-01): a person-kind mention
/// WITH a descriptor enqueues on first sight; descriptor-less bare names wait for the 2-mention
/// floor; refused ties always nominate (the caller applies that arm — a refusal is not an
/// unresolved name). Phase 5 consumes this; Phase 3 only classifies.
pub fn nominates_immediately(kind_hint: &str, descriptor: &str) -> bool {
    kind_hint.eq_ignore_ascii_case("person") && !descriptor.trim().is_empty()
}

/// The resolver outcome for one read — persisted verbatim as `editor_reads.resolved` (§1a).
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub links: Vec<ResolvedLink>,
    pub unresolved: Vec<NameMention>,
    pub refused_ambiguous: Vec<RefusedName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedLink {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    /// The nrm() surface the exact match fired on — the link's provenance.
    pub via_surface: String,
}

#[derive(Clone, Debug)]
pub struct RefusedName {
    pub name: String,
    pub kind_hint: String,
    pub descriptor: String,
    /// The tied candidates, recorded for review — refusal is a decision with evidence, not a drop.
    pub candidates: Vec<(String, i32)>,
}

impl Resolved {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "links": self.links.iter().map(|l| json!({
                "entity_type": l.entity_type,
                "entity_id": l.entity_id,
                "sport": l.sport,
                "via_surface": l.via_surface,
            })).collect::<Vec<_>>(),
            "unresolved": self.unresolved.iter().map(|n| json!({
                "name": n.name,
                "kind_hint": n.kind_hint,
                "descriptor": n.descriptor,
            })).collect::<Vec<_>>(),
            "refused_ambiguous": self.refused_ambiguous.iter().map(|r| json!({
                "name": r.name,
                "kind_hint": r.kind_hint,
                "descriptor": r.descriptor,
                "candidates": r.candidates.iter().map(|(t, id)| json!({
                    "entity_type": t, "entity_id": id,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

/// One exact-surface hit from the database, keyed back to the emitted name it matched.
#[derive(Clone, Debug)]
pub struct SurfaceHit {
    pub name: String,
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    pub norm: String,
}

/// resolve_names is the automatic link path: EXACT match on `nrm()` surfaces, sport-scoped,
/// nothing else (T9 — trigram ranks for review, never writes). `public.nrm()` is called IN SQL
/// on purpose: the database owns the one normalizer, and a Rust copy that drifts from it is the
/// failure mode mig 198 exists to avoid.
pub async fn resolve_names(
    pool: &PgPool,
    sport: &str,
    names: &[NameMention],
) -> Result<Resolved> {
    if names.is_empty() {
        return Ok(Resolved::default());
    }
    let raw: Vec<String> = names.iter().map(|n| n.name.clone()).collect();
    let rows = sqlx::query(
        r#"
        SELECT x.name, s.entity_type, s.entity_id, s.sport, s.norm
        FROM unnest($1::text[]) AS x(name)
        JOIN public.entity_name_surfaces s
          ON s.norm = public.nrm(x.name) AND s.sport = $2
        "#,
    )
    .bind(&raw)
    .bind(sport.to_uppercase())
    .fetch_all(pool)
    .await
    .context("resolve editor names against entity_name_surfaces")?;

    let hits: Vec<SurfaceHit> = rows
        .into_iter()
        .map(|r| SurfaceHit {
            name: r.get("name"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            sport: r.get("sport"),
            norm: r.get("norm"),
        })
        .collect();
    Ok(group_hits(names, &hits))
}

/// group_hits turns raw surface hits into the resolver verdict, one name at a time. Pure, so the
/// gating rules are unit-testable without a database:
///
/// * kind gate first: a `club`/`national_team` mention may only link to a `team` surface; a
///   `person` mention only to `player`/`person`. A `kind_hint` of `other` NEVER auto-links —
///   that is the descriptor doing its job ("Paris", kind `other`, "city hosting the finale"
///   must not become the club link, T9). Kind-incompatible hits are discarded before counting.
/// * exactly one surviving entity → a link, carrying `via_surface`.
/// * two or more → `refused_ambiguous`, candidates recorded, never a coin flip (the Vinicius
///   namesake tie). Refused ties always nominate (5.2) — the caller reads this bucket.
/// * zero → `unresolved`, the Investigator's discovery channel.
pub fn group_hits(names: &[NameMention], hits: &[SurfaceHit]) -> Resolved {
    let mut out = Resolved::default();
    for mention in names {
        // The descriptor arm (§1a): a mention whose descriptor names a place never takes a
        // `team` link, whatever the kind_hint claims — the measured failure was a model labelling
        // "Paris" kind `club` while describing the truth, "capital city". Person links untouched.
        let place = descriptor_names_place(&mention.descriptor);
        let compatible: Vec<&SurfaceHit> = hits
            .iter()
            .filter(|h| {
                h.name == mention.name
                    && kind_compatible(&mention.kind_hint, &h.entity_type)
                    && !(place && h.entity_type == "team")
            })
            .collect();
        // One entity can match through several surfaces (name + alias on the same norm);
        // count DISTINCT entities, not rows. BTreeMap for deterministic candidate order.
        let mut entities: BTreeMap<(String, i32), &SurfaceHit> = BTreeMap::new();
        for h in &compatible {
            entities
                .entry((h.entity_type.clone(), h.entity_id))
                .or_insert(h);
        }
        match entities.len() {
            0 => out.unresolved.push(mention.clone()),
            1 => {
                let ((entity_type, entity_id), hit) = entities.into_iter().next().unwrap();
                out.links.push(ResolvedLink {
                    entity_type,
                    entity_id,
                    sport: hit.sport.clone(),
                    via_surface: hit.norm.clone(),
                });
            }
            _ => out.refused_ambiguous.push(RefusedName {
                name: mention.name.clone(),
                kind_hint: mention.kind_hint.clone(),
                descriptor: mention.descriptor.clone(),
                candidates: entities.into_keys().collect(),
            }),
        }
    }
    out
}

/// kind_compatible is the resolver's kind gate. Deliberately conservative: an auto-link needs
/// BOTH an exact surface match AND an agreeing kind; everything else flows to unresolved, where
/// nomination (Phase 5) — not a guess — decides.
fn kind_compatible(kind_hint: &str, entity_type: &str) -> bool {
    match kind_hint.to_lowercase().as_str() {
        "club" | "national_team" => entity_type == "team",
        "person" => entity_type == "player" || entity_type == "person",
        _ => false,
    }
}

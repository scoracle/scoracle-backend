//! The write gate (PLAN-one-rail 5.5) — deterministic, and the ONLY road to a write.
//!
//! A model mention is never a database write; neither is a Wikidata hit. ACCEPT requires,
//! in code: (a) a `source_documents` row whose retained excerpt contains a name form,
//! (b) sport-relevance from the described occupation/description, and (c) a discriminator
//! agreement — for this rail, the item's team links resolving onto OUR teams (name
//! similarity alone never merges; T9's cousin). Anything less is `ambiguous` (first-class)
//! or a `rejected_*` with its reason. One false merge is a stop-the-line event (5.8), so
//! every arm here prefers refusal over inference.
//!
//! This module is PURE — classification over already-fetched facts. The handler
//! ([`super::entity`]) owns retrieval and the transactional writes.

use super::discover::WikidataItem;

/// Occupation QIDs per sport — the deterministic sport-relevance table. Extended as sports
/// onboard; an occupation absent here falls back to the description keyword screen.
fn sport_occupations(sport: &str) -> (&'static [&'static str], &'static [&'static str], &'static str) {
    // (player occupation QIDs, coach occupation QIDs, description keyword)
    match sport {
        "NBA" => (
            &["Q3665646"],  // basketball player
            &["Q5137571"], // basketball coach
            "basketball",
        ),
        "FOOTBALL" => (
            &["Q937857"], // association football player
            &["Q628099"], // association football manager
            "football",
        ),
        "NFL" => (
            &["Q19204627"], // American football player
            &["Q41583"],    // head coach (generic; description screen tightens)
            "football",
        ),
        _ => (&[], &[], ""),
    }
}

/// The role class the page describes — mapped to D-2 person kinds by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleClass {
    Player,
    Coach,
    Executive,
    Owner,
    Agent,
    Official,
    Unknown,
}

impl RoleClass {
    /// The D-2 persons.kind string. `family` is deliberately unreachable — nothing
    /// auto-writes it (Appendix C).
    pub fn person_kind(self) -> Option<&'static str> {
        match self {
            RoleClass::Player => Some("player"),
            RoleClass::Coach => Some("coach"),
            RoleClass::Executive => Some("executive"),
            RoleClass::Owner => Some("owner"),
            RoleClass::Agent => Some("agent"),
            RoleClass::Official => Some("official"),
            RoleClass::Unknown => None,
        }
    }
}

/// classify_role reads the item's claims + description into a role class, sport-gated.
/// P6087 (coach of a sports team) outranks everything: it is a CURRENT structural claim,
/// while P106 occupations accumulate history — nearly every NBA head coach carries
/// "basketball player" from a playing career (measured on the first live smoke,
/// 2026-08-03: Spoelstra and Kerr both classified player and lost their coach_of edges).
/// Occupation tables next, description keywords last, Unknown otherwise.
pub fn classify_role(sport: &str, item: &WikidataItem) -> RoleClass {
    let (players, coaches, kw) = sport_occupations(sport);
    if !item.coach_of_teams.is_empty() {
        return RoleClass::Coach;
    }
    // Coach occupation before player occupation: a dual player+coach P106 record is a
    // retired player who coaches NOW (actives never carry the coach occupation), while the
    // reverse order misfiled Spoelstra as player on the 2026-08-03 smoke (his item has no
    // P6087 — the coaching lives only in P106).
    if item.occupations.iter().any(|q| coaches.contains(&q.as_str())) {
        return RoleClass::Coach;
    }
    if item.occupations.iter().any(|q| players.contains(&q.as_str())) {
        return RoleClass::Player;
    }
    let d = item.description.to_lowercase();
    if kw.is_empty() || !d.contains(kw) {
        // The description screen also admits the executive/agent/official classes ONLY
        // when the sport keyword is present ("basketball executive").
        return RoleClass::Unknown;
    }
    if d.contains("coach") || d.contains("manager") {
        RoleClass::Coach
    } else if d.contains("player") {
        RoleClass::Player
    } else if d.contains("executive") || d.contains("general manager") || d.contains("president") {
        RoleClass::Executive
    } else if d.contains("owner") {
        RoleClass::Owner
    } else if d.contains("agent") {
        RoleClass::Agent
    } else if d.contains("referee") || d.contains("official") {
        RoleClass::Official
    } else {
        RoleClass::Unknown
    }
}

/// sport_relevant is gate clause (b): the item is about OUR sport at all.
pub fn sport_relevant(sport: &str, item: &WikidataItem) -> bool {
    classify_role(sport, item) != RoleClass::Unknown
}

/// The gate verdict for one candidate/enrichment target, over the surviving items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Exactly one item survived name screen + sport relevance + team discriminator.
    Accept { item_idx: usize, role: RoleClass },
    /// Two or more items survived — recorded with the survivors, never a coin flip.
    Ambiguous { survivor_idxs: Vec<usize> },
    /// Items existed but none was our sport.
    RejectedNotSport,
    /// Nothing usable came back (no hits, no name agreement, or no discriminator).
    RejectedInsufficientEvidence,
}

/// decide applies the three clauses over pre-fetched items — pure over pre-computed
/// screens, because BOTH screens belong to other authorities: `name_agreed[i]` comes from
/// `public.nrm()` in SQL (mig 198: the database owns the ONE normalizer — a Rust fold that
/// drifts from it is the failure mode that migration exists to avoid), and
/// `team_matched[i]` is clause (c) — the item's team links resolved onto OUR teams via
/// `entity_name_surfaces`. Clause (a) — excerpt containment — is asserted by the handler
/// at write time against the stored `source_documents` row.
pub fn decide(
    sport: &str,
    items: &[WikidataItem],
    name_agreed: &[bool],
    team_matched: &[bool],
) -> Verdict {
    debug_assert_eq!(items.len(), team_matched.len());
    debug_assert_eq!(items.len(), name_agreed.len());
    let named: Vec<usize> = (0..items.len()).filter(|&i| name_agreed[i]).collect();
    if named.is_empty() {
        return Verdict::RejectedInsufficientEvidence;
    }
    let relevant: Vec<usize> = named
        .iter()
        .copied()
        .filter(|&i| sport_relevant(sport, &items[i]))
        .collect();
    if relevant.is_empty() {
        return Verdict::RejectedNotSport;
    }
    let discriminated: Vec<usize> = relevant
        .iter()
        .copied()
        .filter(|&i| team_matched[i])
        .collect();
    match discriminated.as_slice() {
        [] => {
            // Sport-relevant but no team agreement: with ONE relevant item this is thin
            // evidence (ambiguous — surfaces for review), with several it is a genuine tie.
            Verdict::Ambiguous {
                survivor_idxs: relevant,
            }
        }
        [one] => Verdict::Accept {
            item_idx: *one,
            role: classify_role(sport, &items[*one]),
        },
        many => Verdict::Ambiguous {
            survivor_idxs: many.to_vec(),
        },
    }
}

/// nba_headshot_url derives the display URL from the NBA.com player id (P3647). Stored,
/// never fetched by us — clients load it; cdn.nba.com serves browsers fine (it blocks
/// bots, which is why the Investigator does not fetch it — 4.3 review).
pub fn nba_headshot_url(nba_id: &str) -> Option<String> {
    if nba_id.is_empty() || !nba_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!(
        "https://cdn.nba.com/headshots/nba/latest/1040x760/{nba_id}.png"
    ))
}

/// wire_date trims Wikidata's "+1970-11-01T00:00:00Z" time shape to an ISO date, refusing
/// reduced-precision values (year-only dates carry month/day = 00 — worse than NULL).
pub fn wire_date(raw: &str) -> Option<String> {
    let d = raw.trim_start_matches('+').split('T').next()?;
    let parts: Vec<&str> = d.split('-').collect();
    if parts.len() != 3 || parts[1] == "00" || parts[2] == "00" {
        return None;
    }
    Some(d.to_string())
}

/// display_weight renders kilograms in the sport's display convention (players.weight is a
/// legacy text column; NBA displays pounds).
pub fn display_weight(sport: &str, kg: f64) -> String {
    match sport {
        "NBA" | "NFL" => format!("{} lbs", (kg * 2.20462).round() as i64),
        _ => format!("{} kg", kg.round() as i64),
    }
}

/// display_height renders centimeters in the sport's display convention (feet-inches for
/// the US leagues).
pub fn display_height(sport: &str, cm: f64) -> String {
    match sport {
        "NBA" | "NFL" => {
            let total_in = (cm / 2.54).round() as i64;
            format!("{}'{}\"", total_in / 12, total_in % 12)
        }
        _ => format!("{} cm", cm.round() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, desc: &str, occupations: &[&str]) -> WikidataItem {
        WikidataItem {
            qid: "Q1".to_string(),
            label: label.to_string(),
            description: desc.to_string(),
            occupations: occupations.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn role_classification_is_occupation_first_description_second() {
        let coach = item("Erik Spoelstra", "American basketball coach", &["Q5137571"]);
        assert_eq!(classify_role("NBA", &coach), RoleClass::Coach);
        // Description-only path (no occupation claim).
        let gm = item("Pat Riley", "American basketball executive", &[]);
        assert_eq!(classify_role("NBA", &gm), RoleClass::Executive);
        // Wrong sport keyword → Unknown, whatever the words say.
        let author = item("Lee Child", "British author", &[]);
        assert_eq!(classify_role("NBA", &author), RoleClass::Unknown);
    }

    #[test]
    fn the_pep_guardiola_rule_name_alone_never_accepts() {
        // A football manager sharing a surname with a player: sport-relevant on FOOTBALL,
        // but with no team discriminator the verdict must be Ambiguous, never Accept.
        let pep = item("Pep Guardiola", "Spanish football manager", &["Q628099"]);
        let verdict = decide("FOOTBALL", &[pep], &[true], &[false]);
        assert_eq!(
            verdict,
            Verdict::Ambiguous {
                survivor_idxs: vec![0]
            }
        );
    }

    #[test]
    fn accept_requires_exactly_one_discriminated_survivor() {
        let a = item("Vinícius", "Brazilian footballer", &["Q937857"]);
        let b = item("Vinícius", "Brazilian footballer", &["Q937857"]);
        // Both team-matched → tie → ambiguous (the namesake rule).
        let v = decide("FOOTBALL", &[a.clone(), b.clone()], &[true, true], &[true, true]);
        assert!(matches!(v, Verdict::Ambiguous { .. }));
        // One matched → accept, carrying the role.
        let v = decide("FOOTBALL", &[a, b], &[true, true], &[true, false]);
        assert_eq!(
            v,
            Verdict::Accept {
                item_idx: 0,
                role: RoleClass::Player
            }
        );
    }

    #[test]
    fn not_sport_and_no_name_reject_with_their_reasons() {
        // Name agrees, sport does not → not-sport.
        let author = item("Andy Burnham", "British politician", &[]);
        assert_eq!(
            decide("NBA", &[author], &[true], &[false]),
            Verdict::RejectedNotSport
        );
        // Sport agrees, name does not (SQL nrm screen said no) → insufficient evidence.
        let stranger = item("Someone Else", "American basketball coach", &["Q5137571"]);
        assert_eq!(
            decide("NBA", &[stranger], &[false], &[true]),
            Verdict::RejectedInsufficientEvidence
        );
    }

    #[test]
    fn derived_display_fields_are_deterministic() {
        assert_eq!(
            nba_headshot_url("1629029").as_deref(),
            Some("https://cdn.nba.com/headshots/nba/latest/1040x760/1629029.png")
        );
        assert_eq!(nba_headshot_url("not-an-id"), None);
        assert_eq!(wire_date("+1970-11-01T00:00:00Z").as_deref(), Some("1970-11-01"));
        assert_eq!(wire_date("+1970-00-00T00:00:00Z"), None);
        assert_eq!(display_weight("NBA", 104.3), "230 lbs");
        assert_eq!(display_height("NBA", 206.0), "6'9\"");
        assert_eq!(display_weight("FOOTBALL", 83.0), "83 kg");
    }
}

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
fn sport_occupations(
    sport: &str,
) -> (
    &'static [&'static str],
    &'static [&'static str],
    &'static str,
) {
    // (player occupation QIDs, coach occupation QIDs, description keyword)
    match sport {
        "NBA" => (
            &["Q3665646"], // basketball player
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
    // Ownership (P1830, current) outranks occupation history for the same reason coaching
    // does: P106 accumulates a whole career. The measured case is Jerry Jones — occupation
    // "American football player" (college, 1960s), P54 Arkansas, P1830 Dallas Cowboys NOW.
    if !item.owner_of_teams.is_empty() {
        return RoleClass::Owner;
    }
    // Coach occupation before player occupation: a dual player+coach P106 record is a
    // retired player who coaches NOW (actives never carry the coach occupation), while the
    // reverse order misfiled Spoelstra as player on the 2026-08-03 smoke (his item has no
    // P6087 — the coaching lives only in P106).
    if item
        .occupations
        .iter()
        .any(|q| coaches.contains(&q.as_str()))
    {
        return RoleClass::Coach;
    }
    if item
        .occupations
        .iter()
        .any(|q| players.contains(&q.as_str()))
    {
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

// ---------------------------------------------------------------------------------------
// The prose arm (5.4, built 2026-08-09) — the same three clauses over a model's VERBATIM
// quotes instead of Wikidata's claims. Purity holds: these functions classify pre-fetched,
// pre-screened facts; the handler owns retrieval, the model call and every write.
// ---------------------------------------------------------------------------------------

/// contains_normalized is the anti-hallucination check the verbatim contract makes
/// possible: every model field must be a substring of the page text it was quoted from.
/// Normalization is deliberately minimal — collapse whitespace, straighten curly quotes,
/// case-fold — enough to survive markup residue, never enough to let a paraphrase pass.
pub fn contains_normalized(haystack: &str, needle: &str) -> bool {
    let n = norm_for_containment(needle);
    if n.is_empty() {
        return false;
    }
    norm_for_containment(haystack).contains(&n)
}

fn norm_for_containment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// mentions_all_tokens is the DISCOVERY prescreen: every word of the sought name appears
/// somewhere in the page surface. Deliberately weaker than [`contains_normalized`], and it
/// has to be — measured on the live probe, the page the D-T8 arm exists for writes
/// `Airious "Ace" Bailey`, so the sought "Airious Bailey" is never a contiguous substring:
/// the nickname interrupts it. Token presence decides only WHICH pages earn a model read;
/// the model's quoted evidence still passes the strict contiguous check.
pub fn mentions_all_tokens(haystack: &str, name: &str) -> bool {
    // Both sides get the SAME punctuation strip, or "A.J." (→ "aj") in a sought name can
    // never match the page's own "A.J." (which would keep its dots).
    let depunct = |s: &str| {
        let mut n = norm_for_containment(s);
        n.retain(|c| c.is_alphanumeric() || c.is_whitespace());
        n
    };
    let h = depunct(haystack);
    let tokens = depunct(name);
    let mut any = false;
    for t in tokens.split_whitespace() {
        any = true;
        if !h.contains(t) {
            return false;
        }
    }
    any
}

/// strip_paren_title derives the person's display name from a Wikipedia title — the model
/// is deliberately NOT asked for a name field code can compute ("Ace Bailey (basketball,
/// born 2006)" → "Ace Bailey").
pub fn strip_paren_title(title: &str) -> String {
    match title.split_once(" (") {
        Some((head, _)) => head.trim().to_string(),
        None => title.trim().to_string(),
    }
}

/// prose_role_class maps a verbatim occupation phrase onto the role classes, sport-gated:
/// the phrase must carry the sport's keyword (or an unambiguous sport-implying role word
/// like "footballer") before any role word counts — "American author" stays Unknown on
/// every sport, exactly like the description screen on the Wikidata arm.
///
/// `team_matched` relaxes ONLY the keyword gate, never the role words: when the page's own
/// team names already resolved onto OUR sport's teams, the sport is proven by the
/// resolution (which is sport-scoped), and requiring the word "football" too is what
/// blocks the owner/executive class — Jerry Jones's lede says "owner … of the Dallas
/// Cowboys", never "football owner". A phrase with no role word stays Unknown regardless.
pub fn prose_role_class(sport: &str, occupation_phrase: &str, team_matched: bool) -> RoleClass {
    let p = occupation_phrase.to_lowercase();
    if p.is_empty() {
        return RoleClass::Unknown;
    }
    let (_, _, kw) = sport_occupations(sport);
    let sport_implied = match sport {
        "FOOTBALL" => p.contains("footballer") || p.contains("soccer"),
        _ => false,
    };
    if !team_matched && (kw.is_empty() || (!p.contains(kw) && !sport_implied)) {
        return RoleClass::Unknown;
    }
    // Owner and executive words OUTRANK the bare "manager": Jerry Jones's lede is "owner,
    // president, and general manager of the Dallas Cowboys" — a coach-first chain reads
    // "general manager" as Coach. FOOTBALL's "manager" (= head coach) still classifies
    // right because those phrases carry no owner/executive word.
    if p.contains("owner") {
        RoleClass::Owner
    } else if p.contains("general manager")
        || p.contains("executive")
        || p.contains("president")
        || p.contains("chairman")
    {
        RoleClass::Executive
    } else if p.contains("coach") || p.contains("manager") {
        RoleClass::Coach
    } else if p.contains("player") || p.contains("footballer") {
        RoleClass::Player
    } else if p.contains("agent") {
        RoleClass::Agent
    } else if p.contains("referee") || p.contains("official") {
        RoleClass::Official
    } else {
        RoleClass::Unknown
    }
}

/// descriptor_role_class reads the EDITOR's descriptor ("Rangers defender", "Villa head
/// coach") into the same classes — the second, independent observation the count-threshold
/// rule wants: one from the news text, one from the encyclopedia. Position words imply
/// Player without any sport keyword (the news text rarely says "basketball").
pub fn descriptor_role_class(descriptor: &str) -> RoleClass {
    let d = descriptor.to_lowercase();
    const PLAYER_WORDS: &[&str] = &[
        "player",
        "footballer",
        "striker",
        "midfielder",
        "defender",
        "goalkeeper",
        "keeper",
        "winger",
        "forward",
        "guard",
        "center",
        "centre-back",
        "full-back",
        "quarterback",
        "rookie",
        "prospect",
        "signing",
        "loanee",
        "freshman",
    ];
    // Same precedence as prose_role_class: owner, then executive words (which include
    // "general manager"), THEN the bare coach/manager/boss check.
    if d.contains("owner") {
        return RoleClass::Owner;
    }
    if d.contains("executive")
        || d.contains("director")
        || d.contains("president")
        || d.contains("chairman")
        || d.contains("general manager")
    {
        return RoleClass::Executive;
    }
    if d.contains("coach") || d.contains("manager") || d.contains("boss") {
        return RoleClass::Coach;
    }
    if PLAYER_WORDS.iter().any(|w| d.contains(w)) {
        return RoleClass::Player;
    }
    if d.contains("agent") {
        return RoleClass::Agent;
    }
    if d.contains("referee") || d.contains("official") {
        return RoleClass::Official;
    }
    RoleClass::Unknown
}

/// One page's pre-computed screens, assembled by the handler for [`decide_prose`]. Every
/// boolean is a CODE check over verbatim text — nothing here trusts the model's judgment.
#[derive(Clone, Debug)]
pub struct ProseScreen {
    /// The model said the page is about a person AND its `sought_name_evidence` survived
    /// containment — the page itself connects the sought name to its subject.
    pub evidence_ok: bool,
    /// `prose_role_class` over the containment-surviving occupation phrase.
    pub role: RoleClass,
    /// ≥1 of the page's team names resolved onto OUR teams (sport-scoped exact nrm).
    pub team_matched: bool,
    /// The Editor's descriptor classifies to a DIFFERENT role class (both non-Unknown).
    /// A conflict between the two independent observations refuses; agreement or silence
    /// does not.
    pub descriptor_conflict: bool,
}

/// decide_prose applies the same shape as [`decide`]: evidence screen, sport relevance,
/// discriminator, exactly-one-survivor. The bar is deliberately the FULL three clauses —
/// this arm exists to recover honest refusals, not to lower the bar that made them honest.
pub fn decide_prose(screens: &[ProseScreen]) -> Verdict {
    let evidenced: Vec<usize> = (0..screens.len())
        .filter(|&i| screens[i].evidence_ok)
        .collect();
    if evidenced.is_empty() {
        return Verdict::RejectedInsufficientEvidence;
    }
    let relevant: Vec<usize> = evidenced
        .iter()
        .copied()
        .filter(|&i| screens[i].role != RoleClass::Unknown && !screens[i].descriptor_conflict)
        .collect();
    if relevant.is_empty() {
        return Verdict::RejectedNotSport;
    }
    let discriminated: Vec<usize> = relevant
        .iter()
        .copied()
        .filter(|&i| screens[i].team_matched)
        .collect();
    match discriminated.as_slice() {
        [] => Verdict::Ambiguous {
            survivor_idxs: relevant,
        },
        [one] => Verdict::Accept {
            item_idx: *one,
            role: screens[*one].role,
        },
        many => Verdict::Ambiguous {
            survivor_idxs: many.to_vec(),
        },
    }
}

/// commons_image_url renders a P18 Commons filename as a stable thumbnail URL — the
/// sport-agnostic portrait source (Special:FilePath redirects to the current file, width
/// keeps it headshot-sized). Stored, never fetched by us, like the NBA URL.
pub fn commons_image_url(file: &str) -> Option<String> {
    let f = file.trim();
    if f.is_empty() {
        return None;
    }
    Some(format!(
        "https://commons.wikimedia.org/wiki/Special:FilePath/{}?width=600",
        super::discover::urlencode(&f.replace(' ', "_"))
    ))
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
        let v = decide(
            "FOOTBALL",
            &[a.clone(), b.clone()],
            &[true, true],
            &[true, true],
        );
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
    fn containment_survives_markup_residue_but_never_a_paraphrase() {
        let page = "Airious \u{201C}Ace\u{201D} Bailey Jr. (born August 28, 2006) is an American college basketball player";
        assert!(contains_normalized(page, "Airious \"Ace\" Bailey Jr."));
        assert!(contains_normalized(
            page,
            "american college basketball player"
        ));
        assert!(
            !contains_normalized(page, "plays basketball in college"),
            "paraphrase must fail"
        );
        assert!(
            !contains_normalized(page, ""),
            "empty evidence is no evidence"
        );
    }

    #[test]
    fn prose_role_class_is_sport_gated_like_the_description_screen() {
        assert_eq!(
            prose_role_class("NBA", "American college basketball player", false),
            RoleClass::Player
        );
        assert_eq!(
            prose_role_class("NBA", "American basketball coach", false),
            RoleClass::Coach
        );
        assert_eq!(
            prose_role_class("NBA", "American author", false),
            RoleClass::Unknown
        );
        // "footballer" implies the sport without the keyword.
        assert_eq!(
            prose_role_class("FOOTBALL", "Scottish footballer", false),
            RoleClass::Player
        );
        assert_eq!(
            prose_role_class("FOOTBALL", "Spanish football manager", false),
            RoleClass::Coach
        );
        assert_eq!(prose_role_class("NBA", "", false), RoleClass::Unknown);
    }

    #[test]
    fn team_anchor_unlocks_the_owner_class_but_never_invents_a_role() {
        // The Jerry Jones lede: no sport keyword, "general manager" inside — must be Owner,
        // and ONLY with the team anchor.
        let jones = "owner, president, and general manager of the Dallas Cowboys";
        assert_eq!(prose_role_class("NFL", jones, true), RoleClass::Owner);
        assert_eq!(prose_role_class("NFL", jones, false), RoleClass::Unknown);
        // A team anchor with no role word stays Unknown — the anchor relaxes the sport
        // gate, never the role vocabulary.
        assert_eq!(
            prose_role_class("NFL", "American businessman", true),
            RoleClass::Unknown
        );
        // GM-without-owner is Executive, not Coach, under either gate.
        assert_eq!(
            prose_role_class("NBA", "general manager of the Los Angeles Lakers", true),
            RoleClass::Executive
        );
        // FOOTBALL's "manager" still reads Coach (no owner/executive word present).
        assert_eq!(
            prose_role_class("FOOTBALL", "Spanish football manager", true),
            RoleClass::Coach
        );
    }

    #[test]
    fn ownership_outranks_occupation_history_on_the_wikidata_arm() {
        // The Jerry Jones item shape: P106 carries "American football player" (college,
        // 1960s), P1830 carries the Cowboys NOW. Player-first classification misfiles him.
        let jones = WikidataItem {
            qid: "Q1280022".to_string(),
            label: "Jerry Jones".to_string(),
            description: "American businessman and owner of the Dallas Cowboys".to_string(),
            occupations: vec!["Q19204627".to_string()],
            owner_of_teams: vec!["Q204862".to_string()],
            ..Default::default()
        };
        assert_eq!(classify_role("NFL", &jones), RoleClass::Owner);
        assert_eq!(RoleClass::Owner.person_kind(), Some("owner"));
    }

    #[test]
    fn commons_image_url_renders_p18_files() {
        assert_eq!(
            commons_image_url("Jerry Jones at the 2018 draft.jpg").as_deref(),
            Some("https://commons.wikimedia.org/wiki/Special:FilePath/Jerry_Jones_at_the_2018_draft.jpg?width=600")
        );
        assert_eq!(commons_image_url("  "), None);
    }

    #[test]
    fn descriptor_and_page_are_two_observations_and_a_conflict_refuses() {
        assert_eq!(descriptor_role_class("Rangers defender"), RoleClass::Player);
        assert_eq!(descriptor_role_class("Villa head coach"), RoleClass::Coach);
        assert_eq!(
            descriptor_role_class("supporters' trust chair"),
            RoleClass::Unknown
        );
        let ok = ProseScreen {
            evidence_ok: true,
            role: RoleClass::Player,
            team_matched: true,
            descriptor_conflict: false,
        };
        assert_eq!(
            decide_prose(&[ok.clone()]),
            Verdict::Accept {
                item_idx: 0,
                role: RoleClass::Player
            }
        );
        // The same page with a conflicting descriptor (news said coach, page says player)
        // must not accept — two independent sources disagree on WHAT this person is.
        let conflicted = ProseScreen {
            descriptor_conflict: true,
            ..ok.clone()
        };
        assert_eq!(decide_prose(&[conflicted]), Verdict::RejectedNotSport);
        // No team discriminator → ambiguous, never accept (the Pep rule holds on prose).
        let undiscriminated = ProseScreen {
            team_matched: false,
            ..ok.clone()
        };
        assert!(matches!(
            decide_prose(&[undiscriminated]),
            Verdict::Ambiguous { .. }
        ));
        // Two discriminated survivors → tie.
        assert!(matches!(
            decide_prose(&[ok.clone(), ok]),
            Verdict::Ambiguous { .. }
        ));
        // No evidence at all → insufficient.
        let no_evidence = ProseScreen {
            evidence_ok: false,
            role: RoleClass::Player,
            team_matched: true,
            descriptor_conflict: false,
        };
        assert_eq!(
            decide_prose(&[no_evidence]),
            Verdict::RejectedInsufficientEvidence
        );
    }

    #[test]
    fn discovery_prescreen_is_token_presence_not_contiguity() {
        // The D-T8 page shape: the nickname interrupts the sought phrase.
        let page = "Ace Bailey (basketball)\nAmerican basketball player\nAirious \"Ace\" Bailey (born August 13, 2006) is an American professional basketball player";
        assert!(
            !contains_normalized(page, "Airious Bailey"),
            "the strict check must fail here"
        );
        assert!(
            mentions_all_tokens(page, "Airious Bailey"),
            "the prescreen must pass here"
        );
        assert!(
            !mentions_all_tokens(page, "Matthew Bailey"),
            "a missing token drops the page"
        );
        assert!(
            !mentions_all_tokens(page, ""),
            "an empty name matches nothing"
        );
        // Punctuation in the sought form must not block its tokens ("A.J. Green").
        assert!(mentions_all_tokens(
            "A.J. Green is a receiver",
            "A.J. Green"
        ));
    }

    #[test]
    fn paren_titles_reduce_to_display_names() {
        assert_eq!(
            strip_paren_title("Ace Bailey (basketball, born 2006)"),
            "Ace Bailey"
        );
        assert_eq!(strip_paren_title("Erik Spoelstra"), "Erik Spoelstra");
    }

    #[test]
    fn derived_display_fields_are_deterministic() {
        assert_eq!(
            nba_headshot_url("1629029").as_deref(),
            Some("https://cdn.nba.com/headshots/nba/latest/1040x760/1629029.png")
        );
        assert_eq!(nba_headshot_url("not-an-id"), None);
        assert_eq!(
            wire_date("+1970-11-01T00:00:00Z").as_deref(),
            Some("1970-11-01")
        );
        assert_eq!(wire_date("+1970-00-00T00:00:00Z"), None);
        assert_eq!(display_weight("NBA", 104.3), "230 lbs");
        assert_eq!(display_height("NBA", 206.0), "6'9\"");
        assert_eq!(display_weight("FOOTBALL", 83.0), "83 kg");
    }
}

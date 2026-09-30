//! Legacy packet claim slices and framing retained for Scout and Insider.
//! Selection preserves source attribution, order and contested claims.

use super::packet::PacketView;
use std::collections::HashSet;

/// Surviving consumers of legacy packet claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Insider,
    Scout,
}

impl Voice {
    fn slice(self) -> &'static [&'static str] {
        match self {
            Voice::Insider => &["transfer"],
            Voice::Scout => &["performance", "roster", "injury", "suspension"],
        }
    }
}

/// One claim, as the renderer reads it out of `packets.claims`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RenderClaim {
    pub article_id: i64,
    pub source: String,
    pub fact: String,
    pub published_at: Option<i64>,
    pub story_type: String,
}

/// The entity's part in this storyline — its role and the span it has been in the story
/// (`storyline_entities`: `role`, `joined_at`, `last_seen_at`). D5: the part has its own lifespan,
/// so the render states it rather than implying the entity was there from the first word.
#[derive(Clone, Debug)]
pub struct Participation {
    pub name: String,
    pub role: Option<String>,
    /// Formatted dates (`YYYY-MM-DD`); the caller owns the clock, this module never reads one.
    pub joined_on: Option<String>,
    pub last_seen_on: Option<String>,
}

/// Source-bound storyline context used by the Insider's per-article overlay.
pub fn framing(packet: &PacketView, part: Option<&Participation>) -> String {
    let mut header = String::new();
    if let Some(h) = &packet.headline {
        header.push_str("STORY: ");
        header.push_str(h.trim());
        header.push('\n');
    }
    if let Some(p) = part {
        header.push_str(&role_line(p));
        header.push('\n');
    }
    if !packet.story_types.is_empty() {
        header.push_str("TYPE: ");
        header.push_str(&packet.story_types.join(", "));
        header.push('\n');
    }
    if let Some(line) = &packet.result_line {
        // Verbatim from the text, parsed by code, never invented by a model (§1a).
        header.push_str("RESULT: ");
        header.push_str(line.trim());
        header.push('\n');
    }
    // The thin, structured facts (§1c): who else is in this story, and how much of it there is.
    // Names only — the entity list is data the code assembled, not prose a model wrote.
    let others = other_participants(&packet.facts, part);
    if !others.is_empty() {
        header.push_str("ALSO IN THIS STORY: ");
        header.push_str(&others.join(", "));
        header.push('\n');
    }
    if let Some(prior) = &packet.prior_headline {
        // ONE continuity line from the prior packet: enough for a voice to know this story has a
        // yesterday, far short of re-reading it.
        header.push_str("PREVIOUSLY: ");
        header.push_str(prior.trim());
        header.push('\n');
    }

    header
}

/// The other named participants from `facts.entities`, minus the entity this render is FOR.
/// Capped: a listicle-seeded storyline can carry a dozen, and this line is context, not a cast
/// list — the ones past the cap are behind the `+N more` the caller can see is bounded.
fn other_participants(facts: &serde_json::Value, part: Option<&Participation>) -> Vec<String> {
    const MAX_OTHERS: usize = 8;
    let me = part.map(|p| p.name.as_str()).unwrap_or("");
    let mut names: Vec<String> = facts
        .get("entities")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let n = e
                        .get("name")
                        .and_then(|n| n.as_str())
                        .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(me))?;
                    // Person casts carry their kind ("coach, Real Madrid") — a bare
                    // person name is trivia, a described one is context. Absent on
                    // pre-mig-234 packets and on players/teams.
                    match e.get("descriptor").and_then(|d| d.as_str()) {
                        Some(d) if !d.is_empty() => Some(format!("{n} ({d})")),
                        _ => Some(n.to_string()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    names.dedup();
    if names.len() > MAX_OTHERS {
        let extra = names.len() - MAX_OTHERS;
        names.truncate(MAX_OTHERS);
        names.push(format!("+{extra} more"));
    }
    names
}

fn role_line(p: &Participation) -> String {
    let mut s = format!("ENTITY: {}", p.name);
    if let Some(r) = p.role.as_deref().filter(|r| !r.is_empty()) {
        s.push_str(&format!(" ({r})"));
    }
    match (&p.joined_on, &p.last_seen_on) {
        (Some(j), Some(l)) if j == l => s.push_str(&format!(" — in this story {j}")),
        (Some(j), Some(l)) => s.push_str(&format!(" — in this story {j} → {l}")),
        (Some(j), None) => s.push_str(&format!(" — in this story since {j}")),
        _ => {}
    }
    s
}

/// A claim plus whether it contradicts another claim in the same render.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MarkedClaim {
    pub claim: RenderClaim,
    /// True when another claim in the same set says the opposite. A POINTER, never a filter:
    /// both members of the pair are always carried (T3/D6).
    pub marked: bool,
}

/// Select source claims in stored order, without changing their attribution.
pub fn slice_claims(claims: &[RenderClaim], voice: Voice) -> Vec<RenderClaim> {
    claims
        .iter()
        .filter(|c| {
            voice
                .slice()
                .iter()
                .any(|w| c.story_type.eq_ignore_ascii_case(w))
        })
        .cloned()
        .collect()
}

/// mark_contested finds pairs that say opposite things about the same subject and flags BOTH.
///
/// The rule is deliberately mechanical (T2 — code renders the judgment, never a model): two claims
/// are a contested pair when they talk about the same thing (stem-overlap of their content words
/// at or above [`OVERLAP_FLOOR`], by the overlap coefficient, so a six-word claim can contest a
/// thirty-word one) and their NEGATION POLARITY differs. "Arsenal have reached an agreement in
/// principle" against "The Athletic: deal not agreed" — shared stem `agree`, opposite polarity.
///
/// It marks; it never merges, drops, or reorders. A false mark costs a `⇄` on a line that stands
/// anyway; a missed one costs nothing but the pointer. Both failure modes are survivable, which is
/// why a heuristic is allowed to hold this pen at all.
pub fn mark_contested(claims: &[RenderClaim]) -> Vec<MarkedClaim> {
    let prepared: Vec<(HashSet<String>, bool)> = claims
        .iter()
        .map(|c| (content_stems(&c.fact), is_negated(&c.fact)))
        .collect();

    let mut marked = vec![false; claims.len()];
    for i in 0..claims.len() {
        for j in (i + 1)..claims.len() {
            if prepared[i].1 == prepared[j].1 {
                continue; // agreeing polarity: not a contradiction, however similar
            }
            if overlap_coefficient(&prepared[i].0, &prepared[j].0) >= OVERLAP_FLOOR {
                marked[i] = true;
                marked[j] = true;
            }
        }
    }

    claims
        .iter()
        .cloned()
        .zip(marked)
        .map(|(claim, marked)| MarkedClaim { claim, marked })
        .collect()
}

/// |A∩B| / min(|A|,|B|) at or above this, with opposite polarity, is a contradiction. Half the
/// shorter claim's content words being shared is a strong signal the two are about one thing.
const OVERLAP_FLOOR: f32 = 0.5;

fn overlap_coefficient(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    let smaller = a.len().min(b.len());
    if smaller == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f32 / smaller as f32
}

/// Negation markers. Kept tight and literal: every entry is a word that flips a claim about a
/// deal, a move, or an injury, and none is a word that merely hedges ("could", "reportedly").
/// Hedging is not contradiction — a report that a deal "could collapse" does not contest a report
/// that it is agreed; it is the same story, told softer.
const NEGATIONS: &[&str] = &[
    "not",
    "no",
    "never",
    "nor",
    "denied",
    "denies",
    "deny",
    "rejected",
    "rejects",
    "reject",
    "refused",
    "refuses",
    "without",
    "wont",
    "isnt",
    "arent",
    "hasnt",
    "havent",
    "doesnt",
    "dont",
    "didnt",
    "cannot",
    "cant",
    "false",
    "unfounded",
    "dismissed",
    "off",
    "collapsed",
    "failed",
    "fails",
    "ruled",
];

/// "yet to reach", "yet to agree" — a negation spelled as two words, and the single most common
/// way this corpus states a deal is NOT done.
const NEGATION_PHRASES: &[&str] = &["yet to", "far from", "no agreement", "not agreed"];

fn is_negated(fact: &str) -> bool {
    let lower = fact.to_lowercase();
    if NEGATION_PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // Apostrophes are stripped before comparison so "won't"/"wont"/"won’t" are one token.
    tokens(&lower).any(|t| NEGATIONS.contains(&t.as_str()))
}

/// Words too common to mean "these two claims are about the same thing".
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "from",
    "by", "as", "is", "are", "was", "were", "be", "been", "being", "has", "have", "had", "will",
    "would", "could", "should", "may", "might", "that", "this", "it", "its", "his", "her", "their",
    "he", "she", "they", "we", "you", "i", "said", "says", "say", "after", "before", "over",
    "into", "about", "who", "which", "than", "then", "there", "here", "up", "out", "if", "so",
];

/// Content stems: alphanumeric tokens, stopwords and negation markers removed, truncated to five
/// characters. The truncation IS the stemmer — crude, but it is what makes "agreed", "agreement"
/// and "agreeing" one stem, which is the whole job here. No stemming library, no model.
fn content_stems(fact: &str) -> HashSet<String> {
    let lower = fact.to_lowercase();
    tokens(&lower)
        .filter(|t| !STOPWORDS.contains(&t.as_str()) && !NEGATIONS.contains(&t.as_str()))
        .map(|t| t.chars().take(5).collect::<String>())
        .filter(|t| t.len() > 1)
        .collect()
}

fn tokens(lower: &str) -> impl Iterator<Item = String> {
    lower
        .replace(['\'', '\u{2019}'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(article_id: i64, source: &str, fact: &str, story_type: &str) -> RenderClaim {
        RenderClaim {
            article_id,
            source: source.to_string(),
            fact: fact.to_string(),
            published_at: Some(1_754_000_000 + article_id),
            story_type: story_type.to_string(),
        }
    }

    fn packet(claims: Vec<RenderClaim>) -> PacketView {
        PacketView {
            packet_id: 1,
            storyline_id: 7474,
            sport: "FOOTBALL".into(),
            headline: Some("Vinicius Junior and Arsenal: where the deal stands".into()),
            story_types: vec!["transfer".into()],
            register: Some("anticipation".into()),
            register_phrase: Some("the whole of north London is holding its breath".into()),
            result_line: None,
            prior_headline: None,
            claims,
            facts: serde_json::json!({}),
        }
    }

    fn part() -> Participation {
        Participation {
            name: "Vinicius Junior".into(),
            role: Some("subject".into()),
            joined_on: Some("2026-08-02".into()),
            last_seen_on: Some("2026-08-05".into()),
        }
    }

    #[test]
    fn contested_pair_preserves_order_attribution_and_both_claims() {
        let claims = vec![
            claim(3, "Football365", "Arsenal have reached an agreement in principle on personal terms with Vinicius Junior", "transfer"),
            claim(2, "The Athletic", "deal not agreed", "transfer"),
            claim(1, "ESPN", "Vinicius Junior is set to stay at Real Madrid despite Arsenal interest", "transfer"),
        ];
        let marked = mark_contested(&claims);
        assert_eq!(marked.len(), claims.len());
        for (item, source) in marked.iter().zip(&claims) {
            assert_eq!(item.claim.article_id, source.article_id);
            assert_eq!(item.claim.source, source.source);
            assert_eq!(item.claim.fact, source.fact);
        }
        assert!(marked[0].marked && marked[1].marked);
        assert!(!marked[2].marked);
    }

    #[test]
    fn agreeing_claims_are_not_marked() {
        let claims = vec![
            claim(
                2,
                "Goal",
                "Vinicius Junior is determined to stay at Real Madrid",
                "transfer",
            ),
            claim(
                1,
                "ESPN",
                "Vinicius Junior is set to stay at Real Madrid",
                "transfer",
            ),
        ];
        assert!(mark_contested(&claims).iter().all(|c| !c.marked));
    }

    #[test]
    fn hedging_is_not_contradiction() {
        let claims = vec![
            claim(
                2,
                "Marca",
                "The move could still collapse before deadline day",
                "transfer",
            ),
            claim(
                1,
                "Football365",
                "The move is agreed and will be completed",
                "transfer",
            ),
        ];
        assert!(mark_contested(&claims).iter().all(|c| !c.marked));
    }

    #[test]
    fn surviving_voices_keep_their_source_slices() {
        let claims = vec![
            claim(3, "ESPN", "Arsenal agreed personal terms", "transfer"),
            claim(
                2,
                "BBC",
                "He trained fully on Monday after a knock",
                "injury",
            ),
        ];
        let insider = slice_claims(&claims, Voice::Insider);
        assert_eq!(insider.len(), 1);
        assert_eq!(insider[0].article_id, 3);
        let scout = slice_claims(&claims, Voice::Scout);
        assert_eq!(scout.len(), 1);
        assert_eq!(scout[0].article_id, 2);
        assert!(slice_claims(&claims[1..], Voice::Insider).is_empty());
        assert!(slice_claims(&[], Voice::Insider).is_empty());
    }

    #[test]
    fn retired_register_is_excluded_from_framing() {
        let header = framing(&packet(vec![]), Some(&part()));
        assert!(!header.contains("MOOD:"));
        assert!(!header.contains("holding its breath"));
    }

    #[test]
    fn role_line_states_the_entitys_span() {
        let header = framing(&packet(vec![]), Some(&part()));
        assert!(header
            .contains("ENTITY: Vinicius Junior (subject) — in this story 2026-08-02 → 2026-08-05"));
    }

    #[test]
    fn prior_packet_contributes_exactly_one_line() {
        let mut p = packet(vec![]);
        p.prior_headline = Some("Arsenal open talks for Vinicius".into());
        let header = framing(&p, Some(&part()));
        assert_eq!(
            header
                .lines()
                .filter(|l| l.starts_with("PREVIOUSLY:"))
                .count(),
            1
        );
    }
}

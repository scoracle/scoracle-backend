//! Source attribution and contested-report marking for publisher evidence.
use std::collections::HashSet;

/// One claim, as the renderer reads it out of `packets.claims`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RenderClaim {
    pub article_id: i64,
    pub source: String,
    pub fact: String,
    pub published_at: Option<i64>,
    pub story_type: String,
}

/// A claim plus whether it contradicts another claim in the same render.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MarkedClaim {
    pub claim: RenderClaim,
    /// True when another claim in the same set says the opposite. A POINTER, never a filter:
    /// both members of the pair are always carried (T3/D6).
    pub marked: bool,
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
}

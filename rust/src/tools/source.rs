//! Source identities and instruction guards shared by headline character plugins.
/// Retained publisher context consumed by character plugins, independent of intake ownership.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceContext {
    pub classification_id: i64,
    pub article_id: i64,
    pub headline: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
    /// Native, source-bound relationships; absent on historical intake receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
}

// shortcut: ten articles per call while debugging the backlog; revisit after output review.
pub const CHARACTER_ARTICLE_LIMIT: usize = 10;

/// Rank by the character's decision criterion. Stable ties retain incoming order;
/// missing/invalid scores rank below every supplied score, including zero.
pub fn top_articles<T: Clone>(
    articles: &[T],
    criterion: &str,
    world: impl Fn(&T) -> Option<&serde_json::Value>,
) -> Vec<T> {
    let score = |article: &T| {
        world(article)
            .and_then(|w| w["spectrum"][criterion].as_f64())
            .filter(|s| (0.0..=100.0).contains(s))
    };
    let mut ranked = articles.to_vec();
    ranked.sort_by(|a, b| match (score(a), score(b)) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    ranked.truncate(CHARACTER_ARTICLE_LIMIT);
    ranked
}

/// Mechanical detection of explicit attempts to override the writing contract.
/// Plugins decide whether and where to apply this admission guard.
pub fn contains_instruction_override(source: &str) -> bool {
    let decoded = crate::tools::fetch::decode_entities(source);
    let normalized = decoded
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        "ignore previous instructions",
        "ignore all previous instructions",
        "ignore any previous instructions",
        "ignore the previous instructions",
        "disregard previous instructions",
        "disregard the previous instructions",
        "override previous instructions",
        "override the previous instructions",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

/// Reject decorated stream solicitations without rejecting ordinary broadcast reporting.
pub fn is_headline_spam(headline: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    let decoded = crate::tools::fetch::decode_entities(headline);
    let normalized = decoded.nfkc().collect::<String>().to_lowercase();
    let compact: String = normalized.chars().filter(|c| c.is_alphanumeric()).collect();
    let streaming = [
        "livestream",
        "freestream",
        "livefree",
        "watchfree",
        "watchlive",
        "liveonline",
        "livereddit",
        "livebroadcasttodaygame",
    ]
    .iter()
    .any(|marker| compact.contains(marker))
        || (compact.contains("endirect") && compact.contains("gratuit"));
    // shortcut: observed stream solicitations only; broader source quality still needs review.
    let decorated = decoded.chars().any(|c| {
        matches!(c as u32,
        0x1D400..=0x1D7FF | 0xFF21..=0xFF3A | 0xFF41..=0xFF5A)
    }) || [
        "[+",
        "++>",
        "++[",
        "@!",
        "live@",
        "play@",
        "!$+",
        "~$!",
        "[[[",
        "[here's way",
        "[heres way",
        "xd835_",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    let decorated = decorated || (normalized.contains('✪') && normalized.contains('◇'));
    let words: Vec<_> = normalized
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut phrases = std::collections::HashMap::new();
    let stuffed = words.windows(7).any(|phrase| {
        let count = phrases.entry(phrase).or_insert(0);
        *count += 1;
        *count >= 3
    });
    (streaming && decorated) || (compact.contains("footballlive") && stuffed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn top_ten_use_the_requested_score_with_stable_ties_and_zero_before_unknown() {
        let articles = serde_json::json!([
            {"id":0,"spectrum":{"subject":10,"emotion":100}},
            {"id":1,"spectrum":{"subject":70}},
            {"id":2,"spectrum":{"subject":50}},
            {"id":3,"spectrum":{"subject":100}},
            {"id":4,"spectrum":{"subject":0}},
            {"id":5,"spectrum":{"subject":null}},
            {"id":6,"spectrum":{"subject":70}},
            {"id":7,"spectrum":{"subject":101}},
            {"id":8},{"id":9},{"id":10},{"id":11}
        ]);
        let articles = articles.as_array().unwrap();
        let ids = |items: Vec<serde_json::Value>| {
            items
                .iter()
                .map(|a| a["id"].as_i64().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(top_articles(articles, "subject", |v| Some(v))),
            vec![3, 1, 6, 2, 0, 4, 5, 7, 8, 9]
        );
        assert_eq!(
            ids(top_articles(articles, "emotion", |v| Some(v))),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
        assert_eq!(
            ids(top_articles(&articles[4..], "subject", |v| Some(v))),
            vec![6, 4, 5, 7, 8, 9, 10, 11]
        );
        assert!(top_articles::<serde_json::Value>(&[], "subject", |v| Some(v)).is_empty());
    }

    #[test]
    fn livestream_spam_preserves_broadcast_reporting_and_availability() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/classifier/headline-spam-2026-10-22.json"
        ))
        .unwrap();
        for (key, blocked) in [("blocked", true), ("allowed", false)] {
            for title in cases[key].as_array().unwrap() {
                let title = title.as_str().unwrap();
                assert_eq!(is_headline_spam(title), blocked, "{title}");
            }
        }
    }

    #[test]
    fn instruction_guard_folds_case_punctuation_and_line_wrapping() {
        assert!(contains_instruction_override(
            "IGNORE—all\nprevious instructions."
        ));
        assert!(contains_instruction_override(
            "Disregard the previous instructions."
        ));
        assert!(!contains_instruction_override(
            "The coach gave instructions before training."
        ));
    }
}

/// Default narrative trajectory written by Journalist and Oracle.
pub const DEFAULT_TRAJECTORY: &str = "developing_story";

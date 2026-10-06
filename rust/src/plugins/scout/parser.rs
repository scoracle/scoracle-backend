//! Scout response shape and guards against unsupported factual claims.
use super::performance::RelativeDirection;
use crate::harness::Parser;
use anyhow::Result;
use std::collections::BTreeMap;

/// Scout supplies the title and enforces a 1,200-character body ceiling.
/// There is no per-paragraph ceiling.
pub const SCOUT_PARAGRAPH_MAX_CHARS: Option<usize> = None;

pub fn prose() -> crate::tools::form::Prose {
    crate::tools::form::Prose::new(
        &["body"],
        crate::tools::form::Dimensions::new(
            crate::tools::form::BODY_MAX_CHARS,
            SCOUT_PARAGRAPH_MAX_CHARS,
        ),
    )
}

/// Output contract captured separately in the diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v8-parts";

/// Backstops against internal notation and unsupported claims from stored coverage.
pub const RATING_BODY_BANS: &[&str] = &[
    " · ",
    "exact labels and bands",
    "mid-season",
    "printed measurements",
    "limited playing time",
    "playing time is likely limited",
    "reduced playing time",
    "reduced minutes",
    "fewer minutes",
    "lower total minutes",
    "constrained role",
    "shift in role",
    "tactical adjustments",
    "positional or tactical changes",
    "substituted early",
    "typical team averages",
    "only verified fixture",
];

/// The model supplies only the body.
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub body: String,
}

/// An explicit JSON null is a completed pass. Empty or invalid cards remain errors.
pub struct RatingParser;

/// Production parser with facts from the exact built request. Shape and global
/// prose guards remain in [`RatingParser`]; this layer catches contradictions
/// that can only be judged against this entity's selected comparison evidence.
pub struct RatingRequestParser<'a> {
    prompt: &'a str,
    directions: &'a BTreeMap<String, RelativeDirection>,
    bands: &'a BTreeMap<String, String>,
}

impl<'a> RatingRequestParser<'a> {
    pub fn new(
        prompt: &'a str,
        directions: &'a BTreeMap<String, RelativeDirection>,
        bands: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            prompt,
            directions,
            bands,
        }
    }
}

impl Parser<RatingReply> for RatingParser {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        // The declared surface is the shared keyed prose map, so decoding routes
        // through the shared validator over this plugin's own keys. That is what
        // makes the `form` part a contract rather than decoration: the grammar
        // offered to the model and the surface actually accepted come from one
        // declaration, and an undeclared field or a dropped slot is refused.
        let prose = prose();
        let map = crate::tools::form::decode_prose_map(raw, &prose.keys)?;
        // A declined body is this plugin's abstention. It is distinct from a
        // dropped slot, which `decode_prose_map` has already refused.
        let Some(body) = map.get("body").map(clean_commentary) else {
            return Ok(None);
        };
        let mut prose_only = crate::tools::form::ProseMap::new();
        prose_only.push("body", Some(body.clone()));
        prose_only
            .validate(prose.dims)
            .map_err(|e| crate::tools::form::SurfaceError(e.to_string()))?;
        if let Some(p) = crate::tools::guards::first_banned_phrase(&body, RATING_BODY_BANS) {
            tracing::warn!(
                guard = "rating_body_ban",
                phrase = p,
                "rating body rejected"
            );
            return Err(crate::tools::form::SurfaceError(format!(
                "Body makes the unsupported inference {p:?}; remove that claim and use only retained evidence."
            ))
            .into());
        }
        if let Some(p) = crate::tools::guards::first_product_name(&body) {
            tracing::warn!(guard = "product_name", name = p, "rating body rejected");
            anyhow::bail!("rating: body names product {p:?}");
        }
        if crate::tools::guards::has_foreign_script(&body) {
            tracing::warn!(guard = "foreign_script", "rating body rejected");
            anyhow::bail!("rating: body carries a foreign-script run");
        }
        Ok(Some(RatingReply { body }))
    }
}

impl Parser<RatingReply> for RatingRequestParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        let Some(reply) = RatingParser.parse(raw)? else {
            return Ok(None);
        };
        if let Some((label, stated, expected)) =
            first_direction_contradiction(&reply.body, self.directions)
        {
            return Err(crate::tools::form::SurfaceError(format!(
                "Body says {label} {stated}, but the compatible percentile evidence says it {expected}. Keep the supplied arithmetic direction."
            ))
            .into());
        }
        if has_internal_form_contradiction(&reply.body) {
            return Err(crate::tools::form::SurfaceError(
                "Body describes recent form as both strong/rising and declining/falling. Keep one interpretation supported by the supplied recent-form evidence.".into(),
            )
            .into());
        }
        if let Some(error) = first_measure_association_error(&reply.body) {
            return Err(crate::tools::form::SurfaceError(error.into()).into());
        }
        if let Some(error) = first_source_shape_error(&reply.body, self.prompt, self.bands) {
            return Err(crate::tools::form::SurfaceError(error.into()).into());
        }
        if let Some((label, stated, expected)) = first_band_contradiction(&reply.body, self.bands) {
            return Err(crate::tools::form::SurfaceError(format!(
                "Body calls {label} {stated}, but its supplied percentile band is {expected}. Use the supplied band."
            ))
            .into());
        }
        if let Some(height) = first_unsupported_height(&reply.body, self.prompt) {
            return Err(crate::tools::form::SurfaceError(format!(
                "Body invents height {height:?}, which is absent from the retained evidence. Remove it."
            ))
            .into());
        }
        if let Some(number) = first_unsupported_number(&reply.body, self.prompt) {
            return Err(crate::tools::form::SurfaceError(format!(
                "Body uses numeric value {number}, which is absent from the retained evidence. Remove it or use the exact supplied measurement."
            ))
            .into());
        }
        Ok(Some(reply))
    }
}

/// Numeric claims are evidence, not decoration. Every literal emitted on the card must occur in
/// the exact prepared assignment. Comparing parsed values admits harmless formatting differences
/// such as `95` versus `95.0`, while rejecting invented ratings, deltas and percentiles.
fn first_unsupported_number(body: &str, prompt: &str) -> Option<String> {
    let supplied = numeric_literals(prompt);
    numeric_literals(body)
        .into_iter()
        .find(|(_, value)| {
            !supplied
                .iter()
                .any(|(_, known)| (known - value).abs() < 0.000_001)
        })
        .map(|(literal, _)| literal)
}

fn numeric_literals(text: &str) -> Vec<(String, f64)> {
    let chars = text.char_indices().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < chars.len() {
        let (start, ch) = chars[cursor];
        let unary_sign = matches!(ch, '+' | '-')
            && chars
                .get(cursor + 1)
                .is_some_and(|(_, next)| next.is_ascii_digit())
            && cursor
                .checked_sub(1)
                .and_then(|index| chars.get(index))
                .is_none_or(|(_, previous)| {
                    previous.is_whitespace() || matches!(previous, '(' | ':' | '=')
                });
        if !ch.is_ascii_digit() && !unary_sign {
            cursor += 1;
            continue;
        }

        let mut end_cursor = cursor + usize::from(unary_sign);
        let mut decimal_seen = false;
        while let Some((_, next)) = chars.get(end_cursor) {
            if next.is_ascii_digit() {
                end_cursor += 1;
            } else if *next == '.'
                && !decimal_seen
                && chars
                    .get(end_cursor + 1)
                    .is_some_and(|(_, after)| after.is_ascii_digit())
            {
                decimal_seen = true;
                end_cursor += 1;
            } else {
                break;
            }
        }
        let end = chars
            .get(end_cursor)
            .map(|(index, _)| *index)
            .unwrap_or(text.len());
        let literal = &text[start..end];
        if let Ok(value) = literal.parse::<f64>() {
            out.push((literal.to_string(), value));
        }
        cursor = end_cursor.max(cursor + 1);
    }
    out
}

fn first_direction_contradiction(
    body: &str,
    directions: &BTreeMap<String, RelativeDirection>,
) -> Option<(String, &'static str, &'static str)> {
    const POSITIVE: &[&str] = &[
        "improv", "rose", "risen", "rising", "increas", "gained", "stronger", "higher",
    ];
    const NEGATIVE: &[&str] = &[
        "declin", "fell", "fall", "decreas", "dropped", "dropping", "slipped", "regress",
        "worsened",
    ];
    let folded = body.to_lowercase();
    let has_rise = directions
        .values()
        .any(|direction| *direction == RelativeDirection::Rose);
    let has_fall = directions
        .values()
        .any(|direction| *direction == RelativeDirection::Fell);
    if has_rise
        && has_fall
        && [
            "consistent improvement across",
            "improvement across all",
            "improved across all",
            "all metrics improved",
        ]
        .iter()
        .any(|phrase| folded.contains(phrase))
    {
        return Some((
            "the comparison".into(),
            "only rose",
            "contains mixed directions",
        ));
    }

    for sentence in folded.split(['.', '!', '?', '\n']) {
        let has_positive = POSITIVE.iter().any(|stem| sentence.contains(stem));
        let has_negative = NEGATIVE.iter().any(|stem| sentence.contains(stem));
        if has_positive == has_negative {
            continue;
        }
        for (label, expected) in directions {
            if !mentions_direction_label(sentence, label) {
                continue;
            }
            match expected {
                RelativeDirection::Rose if has_negative => {
                    return Some((label.clone(), "fell", "rose"));
                }
                RelativeDirection::Fell if has_positive => {
                    return Some((label.clone(), "rose", "fell"));
                }
                RelativeDirection::Held => {
                    return Some((label.clone(), "changed", "held"));
                }
                _ => {}
            }
        }
    }

    let clauses = folded
        .split(['.', '!', '?', ';', ',', '\n'])
        .flat_map(|sentence| sentence.split(" while "))
        .flat_map(|clause| clause.split(" whereas "))
        .flat_map(|clause| clause.split(" but "))
        .flat_map(|clause| clause.split(" and "))
        .collect::<Vec<_>>();
    for (label, expected) in directions {
        for clause in clauses
            .iter()
            .filter(|clause| mentions_direction_label(clause, label))
        {
            let has_positive = POSITIVE.iter().any(|stem| clause.contains(stem));
            let has_negative = NEGATIVE.iter().any(|stem| clause.contains(stem));
            let has_stable = ["consistent", "held", "stable", "stayed", "unchanged"]
                .iter()
                .any(|stem| clause.contains(stem));
            match expected {
                RelativeDirection::Rose if has_negative => {
                    return Some((label.clone(), "fell", "rose"));
                }
                RelativeDirection::Fell if has_positive => {
                    return Some((label.clone(), "rose", "fell"));
                }
                RelativeDirection::Rose if has_stable => {
                    return Some((label.clone(), "held", "rose"));
                }
                RelativeDirection::Fell if has_stable => {
                    return Some((label.clone(), "held", "fell"));
                }
                RelativeDirection::Held if has_positive || has_negative => {
                    return Some((label.clone(), "changed", "held"));
                }
                _ => {}
            }
        }
    }
    None
}

fn mentions_direction_label(clause: &str, label: &str) -> bool {
    let folded = label.to_lowercase();
    if clause.contains(&folded) {
        return true;
    }
    folded.split_whitespace().any(|word| {
        let word = word.strip_suffix('s').unwrap_or(word);
        let stem = word.strip_suffix("ing").unwrap_or(word);
        stem.len() >= 5 && clause.contains(stem)
    })
}

fn has_internal_form_contradiction(body: &str) -> bool {
    let folded = body.to_lowercase();
    let positive = ["strong form", "good form", "upward trend", "rising form"]
        .iter()
        .any(|phrase| folded.contains(phrase));
    let negative = [
        "downward trend",
        "declining form",
        "falling form",
        "recent decline",
    ]
    .iter()
    .any(|phrase| folded.contains(phrase));
    positive && negative
}

fn first_measure_association_error(body: &str) -> Option<&'static str> {
    for claim in body.to_lowercase().split(['.', '!', '?', ';', '\n']) {
        let has_word = |word| {
            claim
                .split(|c: char| !c.is_alphanumeric())
                .any(|token| token == word)
        };
        let has_xg = claim.contains("expected goals") || has_word("xg");
        let has_xa = claim.contains("expected assists") || has_word("xa");
        let creation = ["creation", "creative", "playmaking", "assist"]
            .iter()
            .any(|term| claim.contains(term));
        let scoring = ["scoring", "goalscoring", "finishing"]
            .iter()
            .any(|term| claim.contains(term));
        if has_xg && has_xa && claim.contains("percentile") {
            return Some(
                "State xG and xA percentile evidence in separate claims so one measure cannot inherit the other's ranks.",
            );
        }
        if has_xg && creation && !has_xa {
            return Some(
                "Expected goals (xG) is shooting/scoring evidence, not creation or playmaking evidence. Remove that association or use supplied xA evidence.",
            );
        }
        if has_xa && scoring && !has_xg {
            return Some(
                "Expected assists (xA) is creation evidence, not scoring/finishing evidence. Remove that association or use supplied xG evidence.",
            );
        }
    }
    None
}

fn first_source_shape_error(
    body: &str,
    prompt: &str,
    bands: &BTreeMap<String, String>,
) -> Option<&'static str> {
    let body_folded = body.to_lowercase();
    let prompt_folded = prompt.to_lowercase();
    let world: serde_json::Value = serde_json::from_str(prompt).unwrap_or_default();
    let profile = &world["fresh"];
    let has_profile = profile.is_object();
    let unsupported_comparison = profile["supports_cross_season"] == false;
    if has_profile
        && [
            "development",
            "developed",
            "growth",
            "became better",
            "became worse",
        ]
        .iter()
        .any(|phrase| body_folded.contains(phrase))
    {
        return Some(
            "Relative percentile movement does not establish player development, growth or changed ability. Describe only the supplied movement in contribution or standing.",
        );
    }
    if prompt_folded.contains("yellow cards + 3 x red cards") && body_folded.contains("red card") {
        return Some(
            "The supplied discipline value is a weighted formula, not separate yellow/red-card counts. Do not invent its components; describe only the supplied discipline value or percentile.",
        );
    }
    if unsupported_comparison {
        if ["frustrat", "confidence", "morale", "motivation"]
            .iter()
            .any(|stem| body_folded.contains(stem) && !prompt_folded.contains(stem))
            || ["may influence", "could influence", "might influence"]
                .iter()
                .any(|phrase| body_folded.contains(phrase))
        {
            return Some(
                "The attributed report does not support an emotional, motivational or psychological inference. Keep the reported action or quote without inventing its effect.",
            );
        }
        // Stability is a cross-time claim. On a thin sample the only supplied timeline is
        // the snapshot itself, so stability language is a violation when it reaches across
        // time (seasons, prior form, "no change") — a within-snapshot description of
        // standing stays free prose.
        let temporal_refers_back = ["season", "last year", "prior", "previous", "across"]
            .iter()
            .any(|word| body_folded.contains(word));
        let stability_word = body_folded
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|word| {
                matches!(
                    word,
                    "unchanged"
                        | "stable"
                        | "stability"
                        | "consistently"
                        | "reliable"
                        | "reliability"
                )
            });
        if (stability_word && temporal_refers_back)
            || ["relative standing", "no change"]
                .iter()
                .any(|phrase| body_folded.contains(phrase))
        {
            return Some(
                "The thin sample has no computed cross-season direction or stability evidence. Remove claims of no change, stable standing, consistency or reliability.",
            );
        }
        // A per-measure direction claim needs per-measure trend evidence, which no thin
        // sample carries. The supplied recent-form line is about overall scores; it does
        // not license direction verbs on a named measurement.
        let direction_verbs = [
            "improve",
            "improved",
            "improving",
            "decline",
            "declined",
            "declining",
            "weaken",
            "weakened",
            "weakening",
            "strengthen",
            "strengthened",
            "strengthening",
            "regress",
            "regressed",
            "regressing",
            "deteriorate",
            "deteriorated",
            "deteriorating",
        ];
        for claim in body_folded.split(['.', '!', '?', ';', '\n']) {
            let names_measure = bands
                .keys()
                .any(|label| claim.contains(&label.to_lowercase()));
            if names_measure && direction_verbs.iter().any(|verb| claim.contains(verb)) {
                return Some(
                    "The thin sample carries no per-measure trend evidence. Describe the named measure's current standing; do not claim it improved, declined, weakened or strengthened.",
                );
            }
        }
        for claim in body_folded.split(['.', '!', '?', ';', '\n']) {
            let actualized = claim
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|words| {
                    words[0]
                        .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                        .parse::<f64>()
                        .is_ok()
                        && (words[1].starts_with("game")
                            || words[1].starts_with("appearance")
                            || words[1].starts_with("minute"))
                });
            let qualified = ["recorded", "stored", "snapshot", "source sample"]
                .iter()
                .any(|term| claim.contains(term));
            if actualized && !qualified {
                return Some(
                    "The thin stored sample is source coverage, not proof of complete participation. Say recorded/stored/snapshot appearances rather than claiming the player played that many games.",
                );
            }
        }
    }
    None
}

fn first_band_contradiction(
    body: &str,
    bands: &BTreeMap<String, String>,
) -> Option<(String, String, String)> {
    const BAND_TERMS: &[&str] = &[
        "above average",
        "below average",
        "elite",
        "strong",
        "average",
        "poor",
    ];
    let folded = body.to_lowercase();
    let clauses = folded
        .split(['.', '!', '?', ';', ',', '\n'])
        .flat_map(|sentence| sentence.split(" while "))
        .flat_map(|clause| clause.split(" whereas "))
        .flat_map(|clause| clause.split(" but "))
        .flat_map(|clause| clause.split(" and "));
    for clause in clauses {
        let matched = bands
            .iter()
            .filter(|(label, _)| clause.contains(&label.to_lowercase()))
            .max_by_key(|(label, _)| label.len());
        if let Some((label, expected)) = matched {
            let words = clause
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>();
            if let Some(stated) = BAND_TERMS.iter().find(|term| {
                let term_words = term.split_whitespace().collect::<Vec<_>>();
                words
                    .windows(term_words.len())
                    .any(|window| window == term_words)
            }) {
                if *stated != expected {
                    return Some((label.clone(), (*stated).into(), expected.clone()));
                }
            }
        }
    }
    None
}

fn first_unsupported_height<'a>(body: &'a str, prompt: &str) -> Option<&'a str> {
    body.split_whitespace()
        .map(|token| token.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '(' | ')')))
        .find(|token| {
            let Some((feet, mark)) = token
                .char_indices()
                .find(|(_, character)| matches!(character, '\'' | '’'))
            else {
                return false;
            };
            let before = &token[..feet];
            let after = &token[feet + mark.len_utf8()..];
            !before.is_empty()
                && before.chars().all(|c| c.is_ascii_digit())
                && after.chars().next().is_some_and(|c| c.is_ascii_digit())
                && (after.contains('"') || after.contains('”'))
                && !prompt.contains(token)
        })
}

/// Normalize the served prose and remove an accidental wrapping code fence.
pub(super) fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    crate::tools::guards::clean_served_prose(s)
}

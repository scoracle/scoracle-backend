//! guards — the served-prose guide rails, shared by the production parsers AND the eval gate.
//!
//! Doctrine (`planning_docs/DOCTRINE-directing.md`): show the model the path so it can express
//! itself within the guide rails, instead of having to find its own way. A rule that is
//! string-checkable is a GUARD (tier 3), not an eval-only expectation (tier 4): the parser that
//! owns the prose scans it and fails closed, the work item re-rolls through the queue's
//! `retry_backoff`, and a violation can never serve. `util::has_foreign_script` is the founding
//! precedent (the 3b delegation leak, 2026-08-15).
//!
//! ONE LIST, ONE HOME. These constants began life inside `eval_tasks.rs`, where the
//! `MOMENTUM_BANNED_PHRASES` doc already ruled that "the ban is global, so it belongs in one
//! place" — this module completes that ruling: the eval checks and the production guards now
//! read the SAME vocabulary, so the gate measures exactly what production enforces. Only GLOBAL
//! invariants live here; a fixture-contextual expectation (e.g. "this steady spread must not say
//! `ascendant`") stays in the fixture's `expect` block, because it is about that spread, not
//! about the contract.
//!
//! Guard rejections should be logged by the rejecting parser (`tracing::warn!` with the guard
//! name) — the per-model violation RATE is the telemetry that prices future model swaps.

/// Product / internal-system names banned from SERVED prose (Scott's brief, 2026-08-10: *"I
/// don't want anything referencing PEAK or Vibe, or other of our products. Just use those as
/// context without naming them"*, extended the same evening to the Analyst — *"it should
/// reference Vibe output as something like 'the emotion around the club' versus 'Vibe'. Same
/// with the PEAK"* — and the Oracle — *"if it references another Character, it should be their
/// name and not PEAK or Vibe"*).
///
/// CASE-SENSITIVE deliberately, unlike the `contains_ci` checks: lowercase "peak" is legitimate
/// English ("at the peak of his powers") and banning it would fail honest prose. The product
/// names as the prompts' own vocabulary sets them — "PEAK", "DECISION CARD" — are what an
/// echoing model copies, caps and all.
pub const PRODUCT_NAME_BANS: &[&str] = &[
    "PEAK",
    "Vibe",
    "Scoracle",
    "Rating Engine",
    "SCOUTING DECISION",
    "DECISION CARD",
];

/// The first product name found in served prose, or `None` when it is clean. Case-sensitive —
/// see [`PRODUCT_NAME_BANS`].
pub fn first_product_name(prose: &str) -> Option<&'static str> {
    PRODUCT_NAME_BANS.iter().find(|p| prose.contains(*p)).copied()
}

/// Phrases the momentum contract bans outright, checked on EVERY momentum READ as a single
/// invariant rather than as a per-fixture expectation.
///
/// Deliberately specific. Bare "the engine" is banned in the prompt but NOT here: a football READ
/// can legitimately say "the engine room of midfield", and a check that fails on correct prose
/// trains everyone to ignore it.
pub const MOMENTUM_BANNED_PHRASES: &[&str] = &[
    "isn't a surge",
    "isn't a collapse",
    "the tape calls this",
    "the engine sees this as",
    "the momentum engine",
    "the numbers say",
    "steady band",
];

/// Vocabulary the Oracle's reading may never carry, whatever the spread: internal metric names,
/// mechanism words, and the verdict formula ("the omen is" — the omen is DECLARED in its field,
/// never narrated). The omen words themselves (`ascendant`/`waning`/`crossroads`) are NOT here:
/// the right one is legitimate in its own spread — that expectation is fixture-contextual.
pub const ORACLE_READING_BANS: &[&str] = &[
    "notability",
    "convergence",
    "sentiment",
    "z-score",
    "percentile",
    "composite",
    "momentum score",
    "the omen is",
    "(",
    "**",
];

/// The Scout's brief is prose, never a bullet list and never Markdown — the legacy 7B's ` · `
/// habit is the measured offender (08-19 gate: 8 of its 9 rating reds were this).
pub const RATING_BODY_BANS: &[&str] = &[" · ", "**"];

/// Served vibe prose carries no Markdown decoration (the labeled-line `**SCORE:**` case is
/// stripped by `util::strip_markdown_emphasis` BEFORE parsing; this bans emphasis inside the
/// body itself).
pub const VIBE_BODY_BANS: &[&str] = &["**"];

/// The first phrase from `list` found (case-insensitive, quote/diacritic-folded) in `prose`.
pub fn first_banned_phrase(prose: &str, list: &[&'static str]) -> Option<&'static str> {
    list.iter().find(|p| contains_ci(prose, p)).copied()
}

/// How many DISTINCT peers a reading names. The Oracle may name at most one, and only when that
/// card carries the turn — a roll call makes the reading a summary of the table rather than the
/// Oracle's own verdict.
///
/// Matches "the Analyst"-style references only. A bare sport word ("the scout said") would be a
/// false positive, so the definite article is required, which is how the prompt's own examples are
/// written ("the Insider's wire stirs", "the Analyst's call holds").
pub fn count_named_peers(reading: &str) -> usize {
    const PEERS: [&str; 5] = ["analyst", "insider", "scout", "influencer", "journalist"];
    let lower = reading.to_lowercase();
    PEERS
        .iter()
        .filter(|p| lower.contains(&format!("the {p}")))
        .count()
}

/// The Influencer's HOOK contract, v13+: a card title — 12 words or fewer, no colon, no
/// question mark. Returns the violated rule's name, or `None` when the hook is clean.
pub fn hook_violation(hook: &str) -> Option<&'static str> {
    if hook.split_whitespace().count() > 12 {
        return Some("hook_max_words");
    }
    if hook.contains(':') {
        return Some("hook_colon");
    }
    if hook.contains('?') {
        return Some("hook_question_mark");
    }
    None
}

/// Whether prose carries any ASCII digit — the momentum READ (s14+) speaks its numbers in
/// words; a digit in the READ is internals pasted into the card.
pub fn has_ascii_digit(s: &str) -> bool {
    s.bytes().any(|b| b.is_ascii_digit())
}

/// Case-insensitive, fold-aware contains — the matcher behind every `*_excludes`/`*_includes`
/// eval axis and the case-insensitive guards.
pub fn contains_ci(haystack: &str, needle: &str) -> bool {
    fold_for_match(haystack).contains(&fold_for_match(needle))
}

/// Lowercase, fold the typographic quote characters to their ASCII equivalents, and fold Latin
/// letter diacritics to their base letters (ø→o, é→e, ß→ss …). The table is curated for the
/// scripts sports names actually arrive in (Latin-1/Latin-2 European), not a full Unicode
/// normalization — NFD decomposition isn't in std, and a dependency for this would be the tail
/// wagging the dog. Lowercasing runs FIRST so the table only needs lowercase entries.
pub fn fold_for_match(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .flat_map(|c| {
            let one = |c: char| std::iter::once(c).chain(None);
            let two = |a: char, b: char| std::iter::once(a).chain(Some(b));
            match c {
                '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{02BC}' => one('\''),
                '\u{201C}' | '\u{201D}' | '\u{201F}' => one('"'),
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => one('a'),
                'ç' | 'ć' | 'č' => one('c'),
                'ď' => one('d'),
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => one('e'),
                'ğ' | 'ģ' => one('g'),
                'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => one('i'),
                'ķ' => one('k'),
                'ĺ' | 'ļ' | 'ľ' | 'ł' => one('l'),
                'ñ' | 'ń' | 'ņ' | 'ň' => one('n'),
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => one('o'),
                'ŕ' | 'ř' => one('r'),
                'ś' | 'ş' | 'š' | 'ș' => one('s'),
                'ţ' | 'ť' | 'ț' => one('t'),
                'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => one('u'),
                'ý' | 'ÿ' => one('y'),
                'ź' | 'ż' | 'ž' => one('z'),
                'æ' => two('a', 'e'),
                'œ' => two('o', 'e'),
                'ß' => two('s', 's'),
                'þ' => two('t', 'h'),
                'ð' => one('d'),
                other => one(other),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_names_are_case_sensitive() {
        assert_eq!(first_product_name("at the peak of his powers"), None);
        assert_eq!(first_product_name("the PEAK confirms it"), Some("PEAK"));
        assert_eq!(first_product_name("a good vibe in the room"), None);
        assert_eq!(first_product_name("the Vibe shows warmth"), Some("Vibe"));
    }

    #[test]
    fn banned_phrases_fold_case_and_quotes() {
        assert_eq!(
            first_banned_phrase("this Isn\u{2019}t a Surge by any measure", MOMENTUM_BANNED_PHRASES),
            Some("isn't a surge")
        );
        assert_eq!(first_banned_phrase("a steady phase", MOMENTUM_BANNED_PHRASES), None);
        assert_eq!(
            first_banned_phrase("holding the steady band", MOMENTUM_BANNED_PHRASES),
            Some("steady band")
        );
    }

    #[test]
    fn oracle_bans_catch_the_verdict_formula_and_internals() {
        assert_eq!(
            first_banned_phrase("The Omen Is unequivocally waning", ORACLE_READING_BANS),
            Some("the omen is")
        );
        assert_eq!(
            first_banned_phrase("a mood of 80/100 (rising)", ORACLE_READING_BANS),
            Some("(")
        );
        assert_eq!(first_banned_phrase("form and feeling move together", ORACLE_READING_BANS), None);
    }

    #[test]
    fn peer_roll_call_counts_distinct_peers() {
        assert_eq!(count_named_peers("the Analyst's call holds"), 1);
        assert_eq!(
            count_named_peers("the Influencer's card and the Scout's brief and the Insider's wire"),
            3
        );
        assert_eq!(count_named_peers("a scout would respect it"), 0);
    }

    #[test]
    fn hook_rules() {
        assert_eq!(hook_violation("Vale sits while the room questions his future"), None);
        assert_eq!(
            hook_violation("one two three four five six seven eight nine ten eleven twelve thirteen"),
            Some("hook_max_words")
        );
        assert_eq!(hook_violation("Breaking: a move"), Some("hook_colon"));
        assert_eq!(hook_violation("Is he done?"), Some("hook_question_mark"));
    }

    #[test]
    fn fold_for_match_leaves_dashes_and_ordinary_text_alone() {
        assert_eq!(fold_for_match("A\u{2014}B"), "a\u{2014}b");
        assert_eq!(fold_for_match("\u{201C}quoted\u{201D}"), "\"quoted\"");
        assert_eq!(fold_for_match("It\u{2019}s"), "it's");
    }

    #[test]
    fn fold_for_match_folds_diacritics_both_directions() {
        // The D-T55 artifact: fixture says Sørensen, honest model output says Sorensen.
        assert!(contains_ci("a bid for Sorensen is live", "Sørensen"));
        assert!(contains_ci("a bid for Sørensen is live", "Sorensen"));
        assert!(contains_ci("Müller and Sánchez", "muller"));
        assert_eq!(fold_for_match("Nikšić ØRSTED ß"), "niksic orsted ss");
    }

    #[test]
    fn digit_scan() {
        assert!(has_ascii_digit("trending down 1.1 over five samples"));
        assert!(!has_ascii_digit("trending down by one point one over five samples"));
    }
}

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
/// never narrated). The "(" ban pins the no-parentheses rule mechanically — the measured failure
/// mode was bookkeeping citations like "(Mood: 30/100)" folded into otherwise-passing readings.
/// The omen words themselves (`ascendant`/`waning`/`crossroads`) are NOT here:
/// the right one is legitimate in its own spread — that expectation is fixture-contextual.
/// Also deliberately absent: "impact", "heat", "slope" — the prompt bans the field-name sense,
/// but each is ordinary sporting English ("the impact of the injury", "the heat of a title
/// race"), and a check that fails on correct prose gets ignored — an ignored check is worse
/// than none (the same reason the momentum list asserts "the engine sees this as", never bare
/// "the engine").
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

/// Trim a two-beat hook to its first beat — the deterministic salvage behind the one-clause
/// rule (v21/s18, the fail-rate session). The 3b's residual overruns share one shape: a clean
/// take plus a hung twist ("Trent's old fire is fading into the quiet, but the crowd still
/// remembers" — 13 words). The first beat IS the title the contract wants; the twist belongs
/// to the body. Cutting at the earliest beat separator is code enforcing the written rule,
/// never rewriting the model's prose.
///
/// Returns `Some(first beat)` only when the hook VIOLATES the contract, a separator exists,
/// and the trimmed beat both passes `hook_violation` and keeps at least four words (a title,
/// not a fragment). A clean hook returns `None` — callers salvage only on violation.
pub fn salvage_hook(hook: &str) -> Option<String> {
    hook_violation(hook)?;
    const SEPS: [&str; 5] = ["\u{2014}", "\u{2013}", ", but ", "; ", ": "];
    let cut = SEPS.iter().filter_map(|s| hook.find(s)).min()?;
    let head = hook[..cut]
        .trim()
        .trim_end_matches([',', ';', ':', '?', '.', ' '])
        .to_string();
    (head.split_whitespace().count() >= 4 && hook_violation(&head).is_none()).then_some(head)
}

/// Whether prose carries any ASCII digit — the momentum READ (s14+) speaks its numbers in
/// words; a digit in the READ is internals pasted into the card.
pub fn has_ascii_digit(s: &str) -> bool {
    s.bytes().any(|b| b.is_ascii_digit())
}

/// has_foreign_script reports whether card-facing English prose carries a run of a
/// non-Latin writing system — the ministral-3:3b multilingual leak ("his playmaking
/// has زمنed in Milwaukee", measured at ~2% of Analyst READs on 2026-08-15, 0% on the
/// 9B). Parsers that own free prose call this and fail closed so the retry re-rolls;
/// at a 2% leak rate a second attempt lands clean essentially always. The founding
/// guard — moved here from `util` when guards got their own home (08-19).
///
/// Latin diacritics (Militão, Éder, Müller) and typographic punctuation pass — only
/// Arabic, CJK, Hangul, Cyrillic, Devanagari, Thai, and Hebrew code points trip it.
pub fn has_foreign_script(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c as u32,
            0x0400..=0x04FF   // Cyrillic
            | 0x0590..=0x05FF // Hebrew
            | 0x0600..=0x06FF // Arabic
            | 0x0900..=0x097F // Devanagari
            | 0x0E00..=0x0E7F // Thai
            | 0x3040..=0x30FF // Hiragana + Katakana
            | 0x4E00..=0x9FFF // CJK unified
            | 0xAC00..=0xD7AF // Hangul
        )
    })
}

/// count_sentences approximates a prose field's sentence count for the contract budgets: a
/// sentence ends at a run of `.` / `!` / `?` followed by whitespace or end-of-text. A decimal
/// point ("a 2.5 assist bump") is followed by a digit, so it never counts. THE sentence
/// counter — the eval's cruder `sentence_runs` (which miscounted decimals) folded into this
/// one 08-19 so every prose lens measures length the same way.
pub fn count_sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut n = 0;
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '.' | '!' | '?') {
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j], '.' | '!' | '?') {
                j += 1;
            }
            if j >= chars.len() || chars[j].is_whitespace() {
                n += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    n
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
    fn salvage_trims_a_two_beat_hook_to_its_first_beat() {
        // The v21 gate's real residual: 13 words, ", but " twist.
        assert_eq!(
            salvage_hook("Trent\u{2019}s old fire is fading into the quiet, but the crowd still remembers"),
            Some("Trent\u{2019}s old fire is fading into the quiet".to_string())
        );
        // Production shape: em-dash twist ending in a question — trim clears both rules.
        assert_eq!(
            salvage_hook("The 76ers\u{2019} superteam hums with ego and chaos\u{2014}who\u{2019}s the only name that could finally silence it?"),
            Some("The 76ers\u{2019} superteam hums with ego and chaos".to_string())
        );
        // A 13-word single clause has no beat to cut — not salvageable, retries as before.
        assert_eq!(
            salvage_hook("Harbor City\u{2019}s quiet win feels like the first step in a slow climb"),
            None
        );
        // Clean hooks are never touched.
        assert_eq!(salvage_hook("Vale sits while the room questions his future"), None);
        // A colon title whose head is too short stays a violation.
        assert_eq!(salvage_hook("Breaking: a move"), None);
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

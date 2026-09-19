//! Served-prose integrity guards shared by production parsers and the eval gate.
//!
//! Production guards cover mechanical invariants: wire shape, product leaks, foreign script,
//! and internal bookkeeping. Style and fixture-specific wording belong in eval expectations.

/// Product and internal-system names banned from served prose. Matching is case-sensitive so
/// ordinary words such as "peak" remain legal.
pub const PRODUCT_NAME_BANS: &[&str] = &[
    "PEAK",
    "Vibe",
    "Scoracle",
    "Rating Engine",
    "SCOUTING DECISION",
    "DECISION CARD",
    "The Scout",
    "The Analyst",
    "The Influencer",
    "The Journalist",
    "The Insider",
    "The Oracle",
];

/// The first product name found in served prose, or `None` when it is clean. Case-sensitive —
/// see [`PRODUCT_NAME_BANS`].
pub fn first_product_name(prose: &str) -> Option<&'static str> {
    PRODUCT_NAME_BANS
        .iter()
        .find(|p| prose.contains(*p))
        .copied()
}

/// Mechanical fourth-wall and bookkeeping leaks banned from the momentum read. Bare "the
/// engine" remains legal because it is ordinary football language.
pub const MOMENTUM_BANNED_PHRASES: &[&str] = &[
    "the engine sees this as",
    "the momentum engine",
    "steady band",
];

/// Detect internal numeric field citations, while allowing sporting parentheticals.
pub fn has_bookkeeping_citation(prose: &str) -> bool {
    const FIELDS: &[&str] = &[
        "mood",
        "form",
        "omen",
        "momentum",
        "sentiment",
        "convergence",
        "notability",
    ];
    prose.split('(').skip(1).any(|tail| {
        let Some((span, _)) = tail.split_once(')') else {
            return false;
        };
        let Some((label, value)) = span.split_once(':').or_else(|| span.split_once('=')) else {
            return false;
        };
        FIELDS.contains(&label.trim().to_ascii_lowercase().as_str())
            && value.chars().any(|c| c.is_ascii_digit())
    })
}

/// Card notation that may not appear in the Scout's prose report.
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

/// The first phrase from `list` found (case-insensitive, quote/diacritic-folded) in `prose`.
pub fn first_banned_phrase(prose: &str, list: &[&'static str]) -> Option<&'static str> {
    list.iter().find(|p| contains_ci(prose, p)).copied()
}

/// Maximum card-title length in characters.
const HOOK_MAX_CHARS: usize = crate::studio::form::HOOK_MAX_CHARS;

/// Return the stable telemetry key when a card title exceeds [`HOOK_MAX_CHARS`]. Colons and
/// question marks are voice, not violations.
pub fn hook_violation(hook: &str) -> Option<&'static str> {
    // chars(), not len(): a byte count would penalise the accented club names the five European
    // leagues are full of — "Atlético", "Beşiktaş" — for being spelled correctly.
    (hook.chars().count() > HOOK_MAX_CHARS).then_some("hook_max_words")
}

/// Remove matched `<...>` template spans copied from an output contract. A stray `<` without a
/// closing `>` is left untouched.
pub fn strip_template_spans(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut removed = false;
    while let Some(open) = rest.find('<') {
        match rest[open..].find('>') {
            Some(close) => {
                out.push_str(&rest[..open]);
                rest = &rest[open + close + 1..];
                removed = true;
            }
            None => break,
        }
    }
    if !removed {
        // Span-free prose passes through byte-identical — this fn must be invisible
        // to text it has no business touching.
        return s.to_string();
    }
    out.push_str(rest);
    // Collapse the doubled spaces a mid-sentence excision leaves, per line so the
    // paragraph breaks the form asks for survive.
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Whether a card title names its entity. The deliberately loose match accepts any folded name
/// word of four or more characters; shorter names fall back to the complete folded name.
pub fn title_names_entity(title: &str, entity_name: &str) -> bool {
    let t = fold_for_match(title);
    let name = fold_for_match(entity_name);
    let mut had_long = false;
    for w in name.split_whitespace() {
        if w.chars().count() >= 4 {
            had_long = true;
            if t.contains(w) {
                return true;
            }
        }
    }
    !had_long && t.contains(name.trim())
}

/// Whether prose carries any ASCII digit. Input-side check only: the Analyst's tests assert no
/// figure reaches her prompt. Not a production guard.
pub fn has_ascii_digit(s: &str) -> bool {
    s.bytes().any(|b| b.is_ascii_digit())
}

/// Whether card-facing English prose contains a non-Latin writing system. Latin diacritics and
/// typographic punctuation pass.
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

/// Approximate a prose field's sentence count for contract budgets: a
/// sentence ends at a run of `.` / `!` / `?` followed by whitespace or end-of-text. A decimal
/// point followed by a digit does not count.
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
        assert_eq!(
            first_product_name("The Analyst calls the direction rising"),
            Some("The Analyst")
        );
        assert_eq!(first_product_name("an analyst sees a rise"), None);
    }

    #[test]
    fn banned_phrases_fold_case_and_quotes() {
        // The fold still matches curly quotes and case on the phrases that REMAIN.
        assert_eq!(
            first_banned_phrase("The Momentum Engine ticks over", MOMENTUM_BANNED_PHRASES),
            Some("the momentum engine")
        );
        assert_eq!(
            first_banned_phrase("a steady phase", MOMENTUM_BANNED_PHRASES),
            None
        );
        assert_eq!(
            first_banned_phrase("holding the steady band", MOMENTUM_BANNED_PHRASES),
            Some("steady band")
        );
        // The eval-scar sweep (2026-08-23): hedge closers are gate taste, not production
        // mechanics — a READ carrying one no longer burns the generation.
        assert_eq!(
            first_banned_phrase(
                "this isn\u{2019}t a surge by any measure",
                MOMENTUM_BANNED_PHRASES
            ),
            None
        );
    }

    #[test]
    fn scout_guard_rejects_coverage_as_playing_time_inference() {
        assert_eq!(
            first_banned_phrase("His playing time is likely limited", RATING_BODY_BANS),
            Some("playing time is likely limited")
        );
        assert_eq!(
            first_banned_phrase("The stored sample is limited", RATING_BODY_BANS),
            None
        );
        assert_eq!(
            first_banned_phrase(
                "The higher rates point to a more constrained role and fewer minutes",
                RATING_BODY_BANS
            ),
            Some("fewer minutes")
        );
    }

    #[test]
    fn bookkeeping_citations_are_precise_about_the_defect() {
        // Internal numeric fields leak the input contract.
        assert!(has_bookkeeping_citation(
            "his rim protection holds (Mood: 30/100) even now"
        ));
        assert!(!has_bookkeeping_citation(
            "elite at the line (4th percentile)"
        ));
        // Honest parenthetical asides pass — the blanket "(" ban was rejecting these
        // at ~1 per 2 crowns (2026-08-23).
        assert!(!has_bookkeeping_citation(
            "the crowd turned (and rightly so) before the form did"
        ));
        // Digits in OPEN prose stay legal for this seat.
        assert!(!has_bookkeeping_citation(
            "a 96th percentile mark carries the profile"
        ));
        assert!(!has_bookkeeping_citation(
            "They won (score: 2–1) after extra time."
        ));
        assert!(!has_bookkeeping_citation(
            "A signing (fee: 40 million) is confirmed."
        ));
        assert!(has_bookkeeping_citation("The room cools (sentiment=30)."));
        // An unclosed paren is broken prose, not a citation.
        assert!(!has_bookkeeping_citation("the wire stirs (fee near 40"));
    }

    /// Punctuation is VOICE, and voice is not a production guard's business (2026-08-24).
    /// Both of these used to burn a finished generation; both now ship.
    #[test]
    fn hook_rules() {
        assert_eq!(
            hook_violation("Vale sits while the room questions his future"),
            None
        );
        assert_eq!(hook_violation("Breaking: a move"), None);
        assert_eq!(hook_violation("Is he done?"), None);
        // Length is the ONLY rule left, so it is the only thing that can be returned.
        assert_eq!(hook_violation(&"x".repeat(200)), Some("hook_max_words"));
    }

    /// The 140-character ceiling (2026-08-24). The thirteen-word hook that used to be the
    /// canonical violation now SHIPS — it was 91% of the fleet's hook rejections, and the
    /// leaderboard row it has to fit is measured in characters, not words.
    #[test]
    fn the_hook_ceiling_is_140_characters_not_twelve_words() {
        // The old canonical failure: thirteen words, 71 chars. Clean now.
        let thirteen = "one two three four five six seven eight nine ten eleven twelve thirteen";
        assert!(thirteen.split_whitespace().count() > 12);
        assert_eq!(hook_violation(thirteen), None);

        // A real production shape that used to burn a generation.
        assert_eq!(
            hook_violation(
                "Trent\u{2019}s old fire is fading into the quiet, but the crowd still remembers"
            ),
            None
        );

        // Exactly 140 ships; 141 does not.
        let at_limit = "x".repeat(140);
        assert_eq!(at_limit.chars().count(), 140);
        assert_eq!(hook_violation(&at_limit), None);
        assert_eq!(hook_violation(&"x".repeat(141)), Some("hook_max_words"));

        // Counted in CHARS, not bytes: an accented club name must not be charged double for
        // being spelled correctly. 140 multi-byte chars is >140 bytes and must still pass.
        let accented = "é".repeat(140);
        assert!(accented.len() > 140, "precondition: multi-byte");
        assert_eq!(hook_violation(&accented), None);
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
        assert!(!has_ascii_digit(
            "trending down by one point one over five samples"
        ));
    }
}

// ---------------------------------------------------------------------------
// The served-prose pipeline shared by every voice.
// ---------------------------------------------------------------------------

/// Normalize typography only. Content and sentence boundaries belong to the model.
pub fn clean_served_prose(s: &str) -> String {
    crate::studio::form::normalize_body(&crate::runtime::util::strip_markdown_emphasis(s))
}

/// settle_title applies the card-title contract and returns what should SHIP.
///
/// `Some(title)` when it fits, and `None`
/// when it does not — never an error. **A junk title costs the title, never the card**, which is
/// the rule the Analyst reached at s18 ("a junk TITLE never kills it") and the Scout and
/// Influencer each reached later and separately.
///
/// Logs on the seat's behalf so the per-model violation RATE stays visible — that telemetry is
/// what prices a future model swap, and it was the only reason these bugs were findable at all.
pub fn settle_title(seat: &str, raw: Option<&str>) -> Option<String> {
    // Strip emphasis before validation and salvage.
    let t = crate::runtime::util::strip_markdown_emphasis(raw?);
    // A title that is (or contains) the contract's own `<the HOOK — …>` placeholder is
    // notation, not a title; strip the span and let the emptiness check decide.
    let t = strip_template_spans(&t);
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    if has_foreign_script(t) {
        tracing::warn!(seat, guard = "foreign_script", title = t, "title dropped");
        return None;
    }
    match hook_violation(t) {
        None => Some(t.to_string()),
        Some(rule) => {
            tracing::warn!(
                seat,
                guard = rule,
                title = t,
                "title dropped (card ships without one)"
            );
            None
        }
    }
}

#[cfg(test)]
mod served_prose_tests {
    use super::{clean_served_prose, settle_title, strip_template_spans, title_names_entity};

    #[test]
    fn template_spans_are_stripped_and_clean_prose_is_untouched() {
        // The measured leak shapes (2026-08-26): a mid-prose fill and a whole placeholder line.
        assert_eq!(
            strip_template_spans("The form is flat. <two to four sentences> The mood climbs."),
            "The form is flat. The mood climbs."
        );
        assert_eq!(
            strip_template_spans("<the HOOK — write it as a tweet. 140 characters>"),
            ""
        );
        // Span-free prose passes through byte-identical, trailing spaces and all.
        let honest = "Three straight losses, and the room  knows it.";
        assert_eq!(strip_template_spans(honest), honest);
        // An unmatched '<' is broken prose, not notation — untouched.
        let broken = "the score < what the tape suggested";
        assert_eq!(strip_template_spans(broken), broken);
        // Paragraph breaks survive an excision.
        assert_eq!(
            strip_template_spans("The claim holds.<x>\n\nThe close lands."),
            "The claim holds.\n\nThe close lands."
        );
    }

    #[test]
    fn normalization_preserves_every_sentence_and_paragraph() {
        let prose = "The headline is a distraction.\n\nClaim: the result still matters.";
        assert_eq!(clean_served_prose(prose), prose);
        let double = "The room waits for a sign. The room waits for a sign.";
        assert_eq!(clean_served_prose(double), double);
    }

    #[test]
    fn placeholder_title_settles_to_none() {
        assert_eq!(
            settle_title("analyst", Some("<the HOOK — write it as a tweet.>")),
            None
        );
    }

    #[test]
    fn title_naming_floor() {
        // The measured fabrications fail…
        assert!(!title_names_entity(
            "Harborview defends like champions and creates like a relegation side.",
            "AC Milan"
        ));
        assert!(!title_names_entity(
            "Rovers' back line holds and the attack goes missing.",
            "Chelsea"
        ));
        assert!(!title_names_entity(
            "Nadia Kerr steady as the form holds its line",
            "AFC Bournemouth"
        ));
        // …and honest naming passes, including partial and diacritic-folded forms.
        assert!(title_names_entity(
            "Milan's back line holds and the attack goes missing.",
            "AC Milan"
        ));
        assert!(title_names_entity(
            "Atletico hold the line again",
            "Atlético de Madrid"
        ));
        assert!(title_names_entity(
            "The Lakers are running out of Augusts",
            "Los Angeles Lakers"
        ));
        // A name with no word of four letters falls back to the whole name.
        assert!(title_names_entity("Rio Ave dig in", "Rio Ave"));
        assert!(!title_names_entity("The wire stays quiet", "Rio Ave"));
    }

    #[test]
    fn emphasis_is_stripped_across_lines_and_prose_survives() {
        let body = "**Brandt** is the story now.\nThe mood is __loud__ in a way it has not been.";
        let got = clean_served_prose(body);
        assert!(
            !got.contains("**") && !got.contains("__"),
            "stripped: {got}"
        );
        assert!(
            got.contains("Brandt is the story now"),
            "prose intact: {got}"
        );
        assert!(got.contains("loud in a way"), "prose intact: {got}");
    }

    #[test]
    fn a_title_never_costs_the_card() {
        // Clean titles pass through untouched.
        assert_eq!(
            settle_title("t", Some("Arsenal hold firm as the window shuts")).as_deref(),
            Some("Arsenal hold firm as the window shuts")
        );
        // Two-beat titles under 140 chars now SHIP WHOLE (2026-08-24): the twist is the
        // model's voice, and only length is the guard's business.
        assert_eq!(
            settle_title(
                "t",
                Some("The room has turned on him — and the window is closing fast")
            )
            .as_deref(),
            Some("The room has turned on him — and the window is closing fast")
        );
        // Punctuation is voice too — a colon or a question mark never costs the card now.
        assert_eq!(
            settle_title("t", Some("Breaking: the room has turned")).as_deref(),
            Some("Breaking: the room has turned")
        );
        assert_eq!(
            settle_title("t", Some("Is the window already shut?")).as_deref(),
            Some("Is the window already shut?")
        );
        // What used to be "unsalvageable" is a perfectly good title at 74 chars.
        assert_eq!(
            settle_title(
                "t",
                Some("Brandt walks into the market like a man who is already out of every option")
            )
            .as_deref(),
            Some("Brandt walks into the market like a man who is already out of every option")
        );
        // Genuinely overlong with no beat to cut still drops to None rather than erroring.
        assert_eq!(settle_title("t", Some(&"x".repeat(200))), None);
        // Absent and empty are simply absent.
        assert_eq!(settle_title("t", None), None);
        assert_eq!(settle_title("t", Some("   ")), None);
        // Emphasis is stripped before the contract runs (the review-pass gap, 2026-08-23):
        // a bolded but otherwise clean title ships clean, never with its asterisks.
        assert_eq!(
            settle_title("t", Some("**Arsenal hold firm as the window shuts**")).as_deref(),
            Some("Arsenal hold firm as the window shuts")
        );
        // And emphasis alone is an empty title, not a shipped decoration.
        assert_eq!(settle_title("t", Some("****")), None);
    }

    /// Whatever ships always satisfies the contract — that is the invariant callers rely on.
    #[test]
    fn a_shipped_title_always_satisfies_the_contract() {
        for t in [
            "Arsenal hold firm",
            "The room has turned on him — and the window is closing fast",
            "one two three four five six seven eight nine ten eleven twelve thirteen",
            "Hornets: Elite shooter, poor containment inside the arc",
        ] {
            if let Some(shipped) = settle_title("t", Some(t)) {
                assert!(
                    super::hook_violation(&shipped).is_none(),
                    "shipped a violating title {shipped:?} from {t:?}"
                );
            }
        }
    }
}

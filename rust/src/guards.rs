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
pub const RATING_BODY_BANS: &[&str] = &[" · "];

/// The first phrase from `list` found (case-insensitive, quote/diacritic-folded) in `prose`.
pub fn first_banned_phrase(prose: &str, list: &[&'static str]) -> Option<&'static str> {
    list.iter().find(|p| contains_ci(prose, p)).copied()
}

/// Maximum card-title length in characters.
const HOOK_MAX_CHARS: usize = 140;

/// Return the stable telemetry key when a card title exceeds [`HOOK_MAX_CHARS`]. Colons and
/// question marks are voice, not violations.
pub fn hook_violation(hook: &str) -> Option<&'static str> {
    // chars(), not len(): a byte count would penalise the accented club names the five European
    // leagues are full of — "Atlético", "Beşiktaş" — for being spelled correctly.
    (hook.chars().count() > HOOK_MAX_CHARS).then_some("hook_max_words")
}

/// Trim an overlong two-beat title to its first complete beat. Returns a title only when the
/// original violates the contract and the result has at least four words.
pub fn salvage_hook(hook: &str) -> Option<String> {
    hook_violation(hook)?;
    // Salvage runs only for an already-invalid title, so integral conjunctions in valid titles
    // are never touched.
    const SEPS: [&str; 6] = ["\u{2014}", "\u{2013}", ", but ", ", and ", "; ", ": "];
    let cut = SEPS.iter().filter_map(|s| hook.find(s)).min()?;
    let head = hook[..cut]
        .trim()
        .trim_end_matches([',', ';', ':', '?', '.', ' '])
        .to_string();
    (head.split_whitespace().count() >= 4 && hook_violation(&head).is_none()).then_some(head)
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
    fn self_review_truncates_at_the_measured_markers() {
        // The three shapes measured live on 2026-08-25 (ctx_ab probes), verbatim heads.
        let note = "The crowd holds its breath, not from fear. (Note: This stays within 6 \
                    sentences, present tense, names real players.)";
        assert_eq!(
            truncate_self_review(note),
            "The crowd holds its breath, not from fear."
        );
        let but_wait = "No drama, just the quiet tension of a story. But wait—this doesn’t \
                        quite fit the required format or tone. Let me tighten it.";
        assert_eq!(
            truncate_self_review(but_wait),
            "No drama, just the quiet tension of a story."
        );
        let restate = "He’s still the anchor. But the card must stay tight: SCORE: 35 VIBE: \
                       The room is holding its breath.";
        assert_eq!(truncate_self_review(restate), "He’s still the anchor.");
        // Earliest marker wins when several appear.
        let both = "Solid start. Check format: within limits.";
        assert_eq!(truncate_self_review(both), "Solid start.");
        let prose_note = "The room stays warm.\n\nNote: The body prose is written as natural, plain language without labels.";
        assert_eq!(truncate_self_review(prose_note), "The room stays warm.");
    }

    #[test]
    fn self_review_leaves_honest_prose_alone() {
        for s in [
            "But wait, this is where the comeback begins.",
            "(Note: the match ended 2–1.) A narrow win still counts.",
            "Let me revise that prediction: the reported injury changes the picture.",
            "But wait — the third act of this transfer saga is still unwritten.",
            "The count matters: three wins from three, and the away end knows it.",
            "A revised deal reached the table on Friday, per the Athletic.",
            "He checks his runs, notes the keeper's line, and finishes low.",
        ] {
            assert_eq!(truncate_self_review(s), s);
        }
    }

    #[test]
    fn self_review_opening_marker_empties_the_body_for_the_retry_path() {
        assert_eq!(truncate_self_review("Check format: entirely meta."), "");
        assert_eq!(clean_served_prose("**Check format: bolded meta.**"), "");
    }

    #[test]
    fn clean_served_prose_strips_then_truncates() {
        // Marker arrives BOLDED: emphasis strip must run first or the marker hides.
        let s = "The bench holds space.\n**But wait—this** doesn't fit the rules.";
        assert_eq!(clean_served_prose(s), "The bench holds space.");
    }

    #[test]
    fn an_exact_double_collapses_and_near_doubles_do_not() {
        // The team:14 momentum shape, measured 2026-08-25: the whole READ repeated inline.
        let double = "The form is flat, and the mood is drifting up. The tape shows steady \
                      progress. The form is flat, and the mood is drifting up. The tape shows \
                      steady progress.";
        assert_eq!(
            clean_served_prose(double),
            "The form is flat, and the mood is drifting up. The tape shows steady progress."
        );
        // One changed word = not a double = untouched.
        let near = "The room waits for a sign. The room waits for the sign.";
        assert_eq!(clean_served_prose(near), near);
        // A short deliberate refrain stays (under the 8-word floor).
        assert_eq!(
            clean_served_prose("So it holds. So it holds."),
            "So it holds. So it holds."
        );
    }

    #[test]
    fn form_scaffolding_labels_are_stripped_and_meta_parens_truncate() {
        // Measured on the 2026-08-25 deck probes, the day THE STORY FORM shipped.
        let s = "Claim: tension, carried by the back line.\nEvidence: three defeats and a silent bench.\nClose: the room braces for the opener.";
        assert_eq!(
            clean_served_prose(s),
            "tension, carried by the back line.\nthree defeats and a silent bench.\nthe room braces for the opener."
        );
        let inline = "Claim: The room warms. Evidence: The away end sings. Support: The noise holds. Summary: Belief is rising.";
        assert_eq!(
            clean_served_prose(inline),
            "The room warms. The away end sings. The noise holds. Belief is rising."
        );
        let paragraph = "The room warms as the away end sings, and belief keeps rising.";
        assert_eq!(
            clean_served_prose(&format!("{paragraph}\n\n{paragraph}")),
            paragraph
        );
        assert_eq!(
            clean_served_prose("Ipswich Town holds steady. The hook is steady."),
            "Ipswich Town holds steady."
        );
        let meta = "The room leans forward, steady and alert.\n\n(One paragraph — claim, evidence, close — as required.)";
        assert_eq!(
            clean_served_prose(meta),
            "The room leans forward, steady and alert."
        );
        // Mid-sentence form words are prose, not scaffolding — untouched.
        let honest = "Their claim to the title rests on the evidence of April.";
        assert_eq!(clean_served_prose(honest), honest);
    }

    #[test]
    fn prompt_echo_truncates_at_the_measured_markers() {
        // The Chelsea team:18 shape, measured 2026-09-05: felt read, then the
        // briefing transcribed — narratives block, transfer temperature,
        // relational memory — all verbatim prompt scaffolding.
        let s = "The trophy promise remains unbroken, a quiet conviction in every pass.\n\n\
                 Narratives forming around them (ordered by relevance/topic heat; impact in brackets): - [55, Heating up] …";
        assert_eq!(
            truncate_prompt_echo(s),
            "The trophy promise remains unbroken, a quiet conviction in every pass."
        );
        // The highest-frequency marker (710 bodies over 14 days).
        let s = "No noise. Only consistency.\n\nThe stories running around them right now (assembled from the reads …): …";
        assert_eq!(truncate_prompt_echo(s), "No noise. Only consistency.");
        // A body that OPENS with echo empties — the caller re-rolls it.
        let s = "Transfer/trade chatter — the TEMPERATURE only; the wire itself is another desk's card: - warm";
        assert_eq!(truncate_prompt_echo(s), "");
        // Honest prose about the wire survives: the markers are the prompt's
        // exact scaffolding phrases, not topic words.
        let honest = "The chatter around the club is warm, and the room leans in.";
        assert_eq!(truncate_prompt_echo(honest), honest);
    }

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

    /// Salvage after the 140-char rule (2026-08-24). Its whole job narrowed with the guard: it
    /// used to rescue thirteen-word overruns, em-dash twists ending in a question, and colon
    /// labels — **all of which are now legal titles that ship untouched.** What is left is the
    /// genuinely overlong hook, trimmed at its first beat.
    #[test]
    fn salvage_trims_only_a_genuinely_overlong_hook() {
        // Every specimen the old test salvaged is CLEAN now and must be returned untouched.
        for legal in [
            "Trent\u{2019}s old fire is fading into the quiet, but the crowd still remembers",
            "The 76ers\u{2019} superteam hums with ego and chaos\u{2014}who\u{2019}s the only name that could finally silence it?",
            "Breaking: a move",
            "Is he done?",
            "Vale sits while the room questions his future",
        ] {
            assert_eq!(salvage_hook(legal), None, "clean hook was touched: {legal}");
        }

        // Over 140 chars WITH a beat separator: trimmed to the first beat.
        let long_two_beat = format!(
            "{}, but the crowd still remembers every last one of them and will not soon forget",
            "Trent\u{2019}s old fire is fading into the quiet of a season nobody enjoyed watching"
        );
        assert!(long_two_beat.chars().count() > 140);
        assert_eq!(
            salvage_hook(&long_two_beat),
            Some(
                "Trent\u{2019}s old fire is fading into the quiet of a season nobody enjoyed watching"
                    .to_string()
            )
        );

        // Over 140 chars with NO beat to cut: not salvageable, retries as before.
        let long_single_beat = "x".repeat(200);
        assert_eq!(salvage_hook(&long_single_beat), None);

        // A trim that would leave a fragment is refused rather than shipped.
        assert_eq!(salvage_hook(&format!("Yes, but {}", "x".repeat(200))), None);
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

/// clean_served_prose is the scrub every served prose field passes through.
///
/// Strips cosmetic markup and scaffold labels instead of rejecting an otherwise usable card.
pub fn clean_served_prose(s: &str) -> String {
    // Template spans go first: a `<two to four sentences>` fill is notation, and the label
    // strip below reasons line-by-line while a span may cross a line.
    let s = strip_template_spans(s);
    let stripped = s
        .lines()
        .map(|l| {
            let l = crate::util::strip_markdown_emphasis(l);
            // Form labels are structure, not served prose. Small models sometimes put every
            // label on its own line and sometimes run them together after normalization, so
            // remove the exact scaffold tokens wherever they occur. Ordinary lower-case prose
            // about a claim, evidence or summary remains untouched.
            let original_len = l.len();
            let mut clean = l;
            for label in ["Claim:", "Evidence:", "Support:", "Summary:", "Close:"] {
                clean = clean.replace(label, "");
            }
            if clean.len() == original_len {
                clean
            } else {
                clean.split_whitespace().collect::<Vec<_>>().join(" ")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    collapse_exact_double(truncate_self_review(&stripped).trim())
}

/// A body that is exactly two identical halves collapses to one. Word-exact halves only:
/// no honest prose is a perfect double of itself,
/// so the check cannot fire on a deliberate refrain, and anything short of exact stays
/// untouched. Runs after the self-review truncation, whose markers introduce most restatements.
fn collapse_exact_double(prose: &str) -> String {
    let words: Vec<&str> = prose.split_whitespace().collect();
    let n = words.len();
    if n >= 8 && n.is_multiple_of(2) && words[..n / 2] == words[n / 2..] {
        // Rebuild from the ORIGINAL text so intra-half newlines survive: cut at the byte
        // offset where the second half's first word begins.
        let mut seen = 0usize;
        let mut cut = prose.len();
        let mut in_word = false;
        for (i, c) in prose.char_indices() {
            if c.is_whitespace() {
                in_word = false;
            } else if !in_word {
                in_word = true;
                if seen == n / 2 {
                    cut = i;
                    break;
                }
                seen += 1;
            }
        }
        return prose[..cut].trim_end().to_string();
    }
    prose.to_string()
}

/// Remove explicit output-format self-review, preserving ordinary narrative asides.
pub fn truncate_self_review(prose: &str) -> &str {
    const SELF_REVIEW_MARKERS: &[&str] = &[
        "(Note: This stays within",
        "(One paragraph — claim, evidence, close",
        "(Blank line before next paragraph",
        "But wait—this doesn’t quite fit the required format",
        "But wait—this doesn't fit the rules",
        "But the card must stay tight:",
        "Now check constraints",
        "Now check the constraints",
        "Check format:",
        "Check character counts",
        "Count characters:",
        "Count words:",
        "Note: The body prose",
        "The hook is",
        "The headline is",
        "Revised VIBE:",
        "Revised READ:",
        "Revised HOOK:",
        "Revised final output:",
    ];
    let cut = SELF_REVIEW_MARKERS
        .iter()
        .filter_map(|m| prose.find(m))
        .min();
    match cut {
        Some(i) => prose[..i].trim_end(),
        None => prose,
    }
}

/// Where a vibe body stops speaking and starts ECHOING its own prompt, cut it there.
///
/// Same treatment and admission rule as [`truncate_self_review`]: markers are exact phrases
/// from prompt scaffolding that no honest felt read would
/// ever say; the list grows only from observed output. Everything before the first marker is
/// the card the seat intended. A body that OPENS with a marker truncates to empty and the
/// caller fails it into a retry.
///
/// Kept OUT of `clean_served_prose` deliberately: these phrases are the INFLUENCER's prompt
/// vocabulary. On her card they are unambiguous echo; on another seat's card a phrase like
/// "transfer/trade chatter" could be honest prose, so the vibe parser applies this itself.
pub fn truncate_prompt_echo(prose: &str) -> &str {
    const PROMPT_ECHO_MARKERS: &[&str] = &[
        "The stories running around them",
        "MOOD is the charge",
        "Narratives forming around them",
        "ordered by relevance/topic heat",
        "Transfer/trade chatter",
        "the TEMPERATURE only",
        "Relational memory (",
        "use for arc and continuity",
        "PREVIOUS VIBE",
        "Respond now (SCORE",
    ];
    let cut = PROMPT_ECHO_MARKERS
        .iter()
        .filter_map(|m| prose.find(m))
        .min();
    match cut {
        Some(i) => prose[..i].trim_end(),
        None => prose,
    }
}

/// settle_title applies the card-title contract and returns what should SHIP.
///
/// `Some(title)` when it is clean, `Some(first beat)` when a two-beat title salvages, and `None`
/// when it cannot — never an error. **A junk title costs the title, never the card**, which is
/// the rule the Analyst reached at s18 ("a junk TITLE never kills it") and the Scout and
/// Influencer each reached later and separately.
///
/// Logs on the seat's behalf so the per-model violation RATE stays visible — that telemetry is
/// what prices a future model swap, and it was the only reason these bugs were findable at all.
pub fn settle_title(seat: &str, raw: Option<&str>) -> Option<String> {
    // Strip emphasis before validation and salvage.
    let t = crate::util::strip_markdown_emphasis(raw?);
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
        Some(rule) => match salvage_hook(t) {
            Some(beat) => {
                tracing::info!(seat, guard = rule, title = t, salvaged = %beat,
                    "title salvaged to first beat");
                Some(beat)
            }
            None => {
                tracing::warn!(
                    seat,
                    guard = rule,
                    title = t,
                    "title dropped (card ships without one)"
                );
                None
            }
        },
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
    fn clean_served_prose_strips_template_spans() {
        let got = clean_served_prose("The mood wobbles day to day. <two to four sentences>");
        assert_eq!(got, "The mood wobbles day to day.");
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

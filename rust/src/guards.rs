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
//!
//! THE MECHANICAL-FLOOR RULE (2026-08-23, the eval-scar sweep — Scott: "the guards allow the
//! model freedom, which is our goal"). The first guard lists were copy-pasted from eval-era
//! language limitations built against a retired model, and they were more restrictive than the
//! product needs — the Oracle's vocabulary list was rejecting 13 of every 14 crowns Scott
//! judged fine. A PRODUCTION guard earns its place only as a mechanical floor: contract shape
//! (the hook rules), integrity (foreign script, digits where numbers are junction-owned),
//! product leaks (product names, fourth-wall mechanism reveals, bookkeeping citations). Style
//! and vocabulary taste belong to the GATE's fixture expectations, where a red is information —
//! in production the same check burns a finished generation.

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

/// Phrases the momentum READ may never carry — trimmed 2026-08-23 (the eval-scar sweep,
/// Scott: "we built the original evals and the language limitations on an eval model... a lot
/// of those eval params were copy+pasted over to the guards and they're more restrictive than
/// they need to be. The guards allow the model freedom").
///
/// What remains is the MECHANICAL floor: fourth-wall breaks that reveal the machine to the
/// seeker ("the momentum engine", "the engine sees this as") and internal bookkeeping
/// vocabulary ("steady band") — the same family as [`PRODUCT_NAME_BANS`]. What left is the
/// eval-era style policing: the hedge closers ("isn't a surge"/"isn't a collapse") and the
/// authority formulas ("the tape calls this"/"the numbers say") were measured defects of
/// RETIRED models under retired prompts, and in production they were burning ~30 finished
/// READs a day over taste the current prompt already carries. Style lives in the gate's
/// fixture expectations, where a red is information instead of a lost generation.
///
/// Deliberately specific. Bare "the engine" is banned in the prompt but NOT here: a football READ
/// can legitimately say "the engine room of midfield", and a check that fails on correct prose
/// trains everyone to ignore it.
pub const MOMENTUM_BANNED_PHRASES: &[&str] = &[
    "the engine sees this as",
    "the momentum engine",
    "steady band",
];

/// What the Oracle's reading may never carry — trimmed to the one MECHANICAL defect on Scott's
/// ruling (2026-08-23: "Those Oracle crowns seem fine. Let's clean up the overbearing guards").
///
/// The vocabulary list this replaced (notability/convergence/sentiment/z-score/percentile/
/// composite/"momentum score"/"the omen is") was rejecting 13 of every 14 crowns the day sigil
/// finally started claiming, and the crowns it rejected read fine — the overbearing-check
/// failure mode the momentum seat has now recorded THREE withdrawals over (a rule cannot beat
/// a phrase in the model's input; an ignored or false-positive check is worse than none). The
/// worst offender words are also leaving at the SOURCE (scout s23 renamed z-score → rating on
/// the card the Oracle reads), which is the fix that actually takes.
///
/// EMPTY since the same day it was trimmed to `"("`: the paren ban was itself measured
/// over-broad within the hour — rejecting ~1 crown per 2 shipped on honest parenthetical
/// asides, on the seat with the deepest queue. The mechanical defect it guarded — a
/// bookkeeping citation like "(Mood: 30/100)" pasted into prose — is what
/// [`has_bookkeeping_citation`] now catches precisely: a parenthetical CARRYING A DIGIT.
/// Digits in open prose stay legal for this seat (percentiles and values are ordinary
/// sporting evidence — see `descrub_z`); digits in parens are the analyst's desk. `"**"` left
/// with the shared `clean_served_prose` pipeline. Kept as an empty seam, the
/// [`VIBE_BODY_BANS`] precedent.
pub const ORACLE_READING_BANS: &[&str] = &[];

/// A parenthetical carrying an ASCII digit — the bookkeeping-citation shape ("(Mood: 30/100)",
/// "(4th percentile)") that turns a reading into the analyst's desk notes. The paren pair must
/// close; an unclosed "(" is ordinary broken prose, not a citation, and the retry costs more
/// than the stray character.
pub fn has_bookkeeping_citation(prose: &str) -> bool {
    let mut digit_in_span = false;
    let mut in_span = false;
    for c in prose.chars() {
        match c {
            '(' => {
                in_span = true;
                digit_in_span = false;
            }
            ')' if in_span => {
                if digit_in_span {
                    return true;
                }
                in_span = false;
            }
            _ if in_span && c.is_ascii_digit() => digit_in_span = true,
            _ => {}
        }
    }
    false
}

/// The Scout's report is prose, never a bullet list and never the card's notation — the legacy
/// 7B's ` · ` habit is the measured offender (08-19 gate: 8 of its 9 rating reds were this).
///
/// `"**"` left this list at s21. It is no longer reachable: `clean_commentary` strips emphasis
/// before the body is graded, the Insider-is4 treatment. Keeping a ban that can never fire reads
/// as protection and provides none — copying the card's ` · ` notation is a content defect and
/// stays a hard fail; bolding is typography and is now simply removed.
pub const RATING_BODY_BANS: &[&str] = &[" · "];

/// Served vibe prose carries no Markdown decoration. EMPTY since 2026-08-23: the body is now
/// stripped by `util::strip_markdown_emphasis` in the parser, the same treatment the Scout's
/// `clean_commentary` and the Insider's `parse_insider_score_reply` already take, so a `"**"`
/// entry here could never fire. It was firing 89 times as a hard bail before that — discarding
/// a finished felt read, and the SCORE momentum reads, over typography.
///
/// Kept as an empty list rather than deleted: it is the seam where a real vibe-body content ban
/// belongs if one is ever measured, and `first_banned_phrase` handles an empty list.
pub const VIBE_BODY_BANS: &[&str] = &[];

/// The first phrase from `list` found (case-insensitive, quote/diacritic-folded) in `prose`.
pub fn first_banned_phrase(prose: &str, list: &[&'static str]) -> Option<&'static str> {
    list.iter().find(|p| contains_ci(prose, p)).copied()
}

/// How many DISTINCT peers a reading names. The Oracle may name at most one, and only when that
/// card carries the turn — a roll call makes the reading a summary of the table rather than the
/// Oracle's own verdict. GATE-ONLY since the 08-23 eval-scar sweep: production no longer bails
/// on a roll call (voice taste, not mechanics); the eval's `reading_max_peers` still measures it.
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

/// **THE TWITTER RULE** — the whole of the card-title contract: 140 characters.
///
/// Scott, 2026-08-24: *"Mark this 140 character limit 'the Twitter rule' and make it the guard
/// and framing for headlines. A tweet states an opinion and grabs attention. 140 characters is
/// well thought out."*
///
/// It is a FRAMING before it is a limit, and that is why it replaces three rules with one. A
/// tweet is a complete thought that earns a tap: it may use a colon, ask a question, land a
/// twist. What it may not do is run past the space it has. So the guard enforces the space, and
/// the seats' prompts carry the frame — direction in the prompt, a floor in the guard, which is
/// the split this module exists to keep.
///
/// It replaces a TWELVE-WORD cap plus bans on colons and question marks, and the measurement is
/// the argument. Over the three days to 2026-08-24 the journal carried **11,272 `hook_max_words`
/// drops, 931 `hook_colon` and 247 `hook_question_mark`** — 12,450 finished generations burned
/// and re-rolled through `retry_backoff`, 86% of every guard rejection on the rail.
///
/// Same shape as the Oracle vocabulary trim that prompted THE MECHANICAL-FLOOR RULE above: limits
/// inherited from the eval era, tighter than the product needs, rejecting work Scott judged fine.
/// A ceiling measured in CHARACTERS also matches what the constraint actually is — the
/// leaderboard row the title has to fit — where a word count only ever approximated it.
const HOOK_MAX_CHARS: usize = 140;

/// THE card-title contract — born as the Influencer's HOOK (v13) and cross-character since
/// Scott's hook doctrine (2026-08-23). **One rule: [`HOOK_MAX_CHARS`] or fewer.** Returns the
/// violated rule's name, or `None` when the hook is clean.
///
/// # The colon and question-mark bans are RETIRED (2026-08-24)
///
/// Scott: *"I think we could have question marks and colons in there. That's part of the model
/// expressing its voice! 140 character limit should resolve all issues."*
///
/// This is THE MECHANICAL-FLOOR RULE at the top of this module applied to its own author. That
/// rule admits a production guard only for contract shape, integrity, or product leaks, and sends
/// *"style and vocabulary taste"* to the GATE's fixture expectations — *"where a red is
/// information; in production the same check burns a finished generation."* A colon and a
/// question mark are punctuation, which is voice. Length is the only one of the three that is
/// contract shape: the title has to fit a leaderboard row.
///
/// Both bans were also cheap by their own telemetry — 931 and 247 drops against the word cap's
/// 11,272 — so this is not where the rejections were. It is where the model's range was.
///
/// What replaced them is not nothing: the seats' prompts still carry the hook doctrine (one
/// sentence, the entity's name inside the report's sharpest claim, no "Label: description"
/// taxonomy). Direction in the prompt, a floor in the guard — the split this module exists to
/// keep.
///
/// The rule NAME stays `hook_max_words` although it now counts characters: it is the telemetry
/// key that three days of journal history and the eval fixtures already join on, and renaming it
/// would silently orphan that series exactly when the change most needs measuring.
pub fn hook_violation(hook: &str) -> Option<&'static str> {
    // chars(), not len(): a byte count would penalise the accented club names the five European
    // leagues are full of — "Atlético", "Beşiktaş" — for being spelled correctly.
    (hook.chars().count() > HOOK_MAX_CHARS).then_some("hook_max_words")
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
    // ", and " joined the beat separators in the 08-23 review pass: the or12 gate's first
    // specimen was "…is a defensive revolution, and the court is watching" — the same hung
    // twist as ", but ", conjunction swapped. Safe because salvage only ever runs on a
    // VIOLATING title; an integral "and" inside a clean hook is never touched.
    const SEPS: [&str; 6] = ["\u{2014}", "\u{2013}", ", but ", ", and ", "; ", ": "];
    let cut = SEPS.iter().filter_map(|s| hook.find(s)).min()?;
    let head = hook[..cut]
        .trim()
        .trim_end_matches([',', ';', ':', '?', '.', ' '])
        .to_string();
    (head.split_whitespace().count() >= 4 && hook_violation(&head).is_none()).then_some(head)
}

/// Whether prose carries any ASCII digit.
///
/// **No longer a production guard.** It gated the momentum READ from s14 until 2026-08-24, on the
/// reasoning that the seat "speaks its numbers in words, so a digit is internals pasted into the
/// card". Measured, that cost 1,221 rejections in three days and permanently dead-lettered
/// momentum player 367 — and the Oracle had already reached the opposite ruling for its own seat
/// (digits in open prose are "ordinary sporting evidence"; see [`ORACLE_READING_BANS`]). The
/// defect actually worth catching is a bookkeeping citation, which
/// [`has_bookkeeping_citation`] catches precisely. `analyst/mod.rs` now calls that instead.
///
/// Kept because it is still the right check on the INPUT side: the Analyst's tests assert no
/// figure reaches her prompt, which stands on its own footing — a narrative seat should not be
/// handed raw numbers it does not own, whatever it is now permitted to write.
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
        let both = "Solid start. Let me tighten this. (Note: within limits.)";
        assert_eq!(truncate_self_review(both), "Solid start.");
    }

    #[test]
    fn self_review_leaves_honest_prose_alone() {
        for s in [
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
        assert_eq!(truncate_self_review("(Note: entirely meta.)"), "");
        assert_eq!(clean_served_prose("**(Note: bolded meta.)**"), "");
    }

    #[test]
    fn clean_served_prose_strips_then_truncates() {
        // Marker arrives BOLDED: emphasis strip must run first or the marker hides.
        let s = "The bench holds space.\n**But wait—this** doesn't fit the rules.";
        assert_eq!(clean_served_prose(s), "The bench holds space.");
    }

    #[test]
    fn form_scaffolding_labels_are_stripped_and_meta_parens_truncate() {
        // Measured on the 2026-08-25 deck probes, the day THE STORY FORM shipped.
        let s = "Claim: tension, carried by the back line.\nEvidence: three defeats and a silent bench.\nClose: the room braces for the opener.";
        assert_eq!(
            clean_served_prose(s),
            "tension, carried by the back line.\nthree defeats and a silent bench.\nthe room braces for the opener."
        );
        let meta = "The room leans forward, steady and alert.\n\n(One paragraph — claim, evidence, close — as required.)";
        assert_eq!(clean_served_prose(meta), "The room leans forward, steady and alert.");
        // Mid-sentence form words are prose, not scaffolding — untouched.
        let honest = "Their claim to the title rests on the evidence of April.";
        assert_eq!(clean_served_prose(honest), honest);
    }

    #[test]
    fn product_names_are_case_sensitive() {
        assert_eq!(first_product_name("at the peak of his powers"), None);
        assert_eq!(first_product_name("the PEAK confirms it"), Some("PEAK"));
        assert_eq!(first_product_name("a good vibe in the room"), None);
        assert_eq!(first_product_name("the Vibe shows warmth"), Some("Vibe"));
    }

    #[test]
    fn banned_phrases_fold_case_and_quotes() {
        // The fold still matches curly quotes and case on the phrases that REMAIN.
        assert_eq!(
            first_banned_phrase("The Momentum Engine ticks over", MOMENTUM_BANNED_PHRASES),
            Some("the momentum engine")
        );
        assert_eq!(first_banned_phrase("a steady phase", MOMENTUM_BANNED_PHRASES), None);
        assert_eq!(
            first_banned_phrase("holding the steady band", MOMENTUM_BANNED_PHRASES),
            Some("steady band")
        );
        // The eval-scar sweep (2026-08-23): hedge closers are gate taste, not production
        // mechanics — a READ carrying one no longer burns the generation.
        assert_eq!(
            first_banned_phrase("this isn\u{2019}t a surge by any measure", MOMENTUM_BANNED_PHRASES),
            None
        );
    }

    #[test]
    fn bookkeeping_citations_are_precise_about_the_defect() {
        // The measured defect: a digit-bearing parenthetical.
        assert!(has_bookkeeping_citation("his rim protection holds (Mood: 30/100) even now"));
        assert!(has_bookkeeping_citation("elite at the line (4th percentile)"));
        // Honest parenthetical asides pass — the blanket "(" ban was rejecting these
        // at ~1 per 2 crowns (2026-08-23).
        assert!(!has_bookkeeping_citation("the crowd turned (and rightly so) before the form did"));
        // Digits in OPEN prose stay legal for this seat.
        assert!(!has_bookkeeping_citation("a 96th percentile mark carries the profile"));
        // An unclosed paren is broken prose, not a citation.
        assert!(!has_bookkeeping_citation("the wire stirs (fee near 40"));
        // The vocabulary list is an empty seam; nothing in prose can trip it.
        assert_eq!(
            first_banned_phrase("the omen is waning and the percentile tells the story", ORACLE_READING_BANS),
            None
        );
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

    /// Punctuation is VOICE, and voice is not a production guard's business (2026-08-24).
    /// Both of these used to burn a finished generation; both now ship.
    #[test]
    fn hook_rules() {
        assert_eq!(hook_violation("Vale sits while the room questions his future"), None);
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
        assert!(!has_ascii_digit("trending down by one point one over five samples"));
    }
}

// ---------------------------------------------------------------------------
// The served-prose pipeline. Two rules that apply to EVERY voice, in one place.
// ---------------------------------------------------------------------------
//
// Added 2026-08-23 after the same two bugs were found and fixed per-seat, in
// production, four and three times respectively:
//
//   markdown reaching a card    Scout (clean_commentary), Insider (is4),
//                               Influencer — while the Analyst, Journalist and
//                               Oracle had no protection at all
//   a junk title killing a card Analyst (s18), Scout, Influencer — each one
//                               rediscovered by watching dead letters
//
// Every instance cost real cards: 89 vibe items died on `**` in a body, 39 on an
// over-long hook, and a complete graded Scout profile was discarded over a colon.
// A rule that every voice needs is a rule that belongs where every voice can
// reach it, which is here — not re-derived in six parsers.

/// clean_served_prose is the scrub every served prose field passes through.
///
/// Line by line because `util::strip_markdown_emphasis` is written for ONE line of a labelled
/// reply, and a card body is many. Stripping rather than banning is deliberate and measured: a
/// `"**"` ban fails the whole generation, and the model then reproduces the same decoration on
/// retry at temp=0, so the ban converts a cosmetic flaw into a permanent stall. The stripped
/// prose is exactly the prose the seat intended.
pub fn clean_served_prose(s: &str) -> String {
    let stripped = s
        .lines()
        .map(|l| {
            let l = crate::util::strip_markdown_emphasis(l);
            // THE STORY FORM's scaffolding vocabulary, measured leaking as literal labels the
            // day the form shipped (2026-08-25 deck probes: "Claim: tension, carried by…").
            // The structure is invisible on the card; the label is decoration, so it takes
            // the same strip-not-reject treatment as `**`.
            ["Claim:", "Evidence:", "Close:"]
                .iter()
                .find_map(|p| l.strip_prefix(p))
                .map(|rest| rest.trim_start().to_string())
                .unwrap_or(l)
        })
        .collect::<Vec<_>>()
        .join("\n");
    truncate_self_review(&stripped).trim().to_string()
}

/// Where a served body stops narrating sport and starts grading its own answer, cut it there.
///
/// Measured on granite4.2:3b the day it became the resident (2026-08-25, live corpus, busy
/// entities): the model's instruction-following instinct turns inward and it REVIEWS the card
/// inside the card — "But wait—this doesn't quite fit the required format… Revised VIBE:",
/// "(Note: This stays within 6 sentences…)", and one card that restated itself in full after
/// "But the card must stay tight:". The fixtures never provoked it; Lakers-sale-grade live
/// material does. Thinking mode is NOT the fix for the prose seats — measured the same day:
/// granite's deliberation scales with the CONTRACT mass, not the material (~2,500 tokens of
/// rule rehearsal for a 331-char card), against Scott's compression direction — report what's
/// there, compress abundance, never expand into available space.
///
/// So the treatment is the strip-not-reject family, same as `**`: everything before the first
/// marker is exactly the card the seat intended; everything after is the model grading its
/// homework. Markers are exact, case-sensitive phrases measured in production output — the
/// admission rule for the list is "is this phrase about THE ANSWER rather than the sport?",
/// and it grows only from observed output, never speculatively (a speculative marker is a ban
/// list by another name). A body that OPENS with a marker truncates to empty, and the seat's
/// own empty-reply guard then fails it honestly into a retry.
pub fn truncate_self_review(prose: &str) -> &str {
    // Measured 2026-08-25: the ctx_ab live probes and the fixture gate's A-sides.
    const SELF_REVIEW_MARKERS: &[&str] = &[
        "(Note:",
        // The form-meta parentheticals: the model narrating THE STORY FORM's own mechanics
        // back at the card (measured on the 2026-08-25 deck probes: "(One paragraph — claim,
        // evidence, close — as required.)", "(Blank line before next paragraph if needed…").
        "(One claim",
        "(One paragraph",
        "(Two claims",
        "(Three claims",
        "(Blank line",
        "(The claim:",
        "But wait—this",
        "But wait, this",
        "But the card must stay tight",
        "Now check constraints",
        "Now check the constraints",
        "Check format:",
        "Check character counts",
        "Count characters:",
        "Count words:",
        "Let me tighten",
        "Let me rewrite",
        "Let me revise",
        "Let me refine",
        "Revised VIBE",
        "Revised READ",
        "Revised HOOK",
        "Revised final output",
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
    // Emphasis is stripped BEFORE the contract runs — the same stripping-not-banning rule as
    // clean_served_prose, closed 2026-08-23 after the review pass found the gap: a bolded
    // title with no other violation shipped its asterisks to the card ("**Las Vegas
    // Raiders…**" reached this fn bold in production), and a bolded two-beat title salvaged
    // WITH its leading `**` glued to the first word.
    let t = crate::util::strip_markdown_emphasis(raw?);
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
                tracing::warn!(seat, guard = rule, title = t,
                    "title dropped (card ships without one)");
                None
            }
        },
    }
}

#[cfg(test)]
mod served_prose_tests {
    use super::{clean_served_prose, settle_title};

    #[test]
    fn emphasis_is_stripped_across_lines_and_prose_survives() {
        let body = "**Brandt** is the story now.\nThe mood is __loud__ in a way it has not been.";
        let got = clean_served_prose(body);
        assert!(!got.contains("**") && !got.contains("__"), "stripped: {got}");
        assert!(got.contains("Brandt is the story now"), "prose intact: {got}");
        assert!(got.contains("loud in a way"), "prose intact: {got}");
    }

    #[test]
    fn a_title_never_costs_the_card() {
        // Clean titles pass through untouched.
        assert_eq!(settle_title("t", Some("Arsenal hold firm as the window shuts")).as_deref(),
                   Some("Arsenal hold firm as the window shuts"));
        // Two-beat titles under 140 chars now SHIP WHOLE (2026-08-24): the twist is the
        // model's voice, and only length is the guard's business.
        assert_eq!(settle_title("t", Some("The room has turned on him — and the window is closing fast")).as_deref(),
                   Some("The room has turned on him — and the window is closing fast"));
        // Punctuation is voice too — a colon or a question mark never costs the card now.
        assert_eq!(settle_title("t", Some("Breaking: the room has turned")).as_deref(),
                   Some("Breaking: the room has turned"));
        assert_eq!(settle_title("t", Some("Is the window already shut?")).as_deref(),
                   Some("Is the window already shut?"));
        // What used to be "unsalvageable" is a perfectly good title at 74 chars.
        assert_eq!(settle_title("t", Some("Brandt walks into the market like a man who is already out of every option")).as_deref(),
                   Some("Brandt walks into the market like a man who is already out of every option"));
        // Genuinely overlong with no beat to cut still drops to None rather than erroring.
        assert_eq!(settle_title("t", Some(&"x".repeat(200))), None);
        // Absent and empty are simply absent.
        assert_eq!(settle_title("t", None), None);
        assert_eq!(settle_title("t", Some("   ")), None);
        // Emphasis is stripped before the contract runs (the review-pass gap, 2026-08-23):
        // a bolded but otherwise clean title ships clean, never with its asterisks.
        assert_eq!(settle_title("t", Some("**Arsenal hold firm as the window shuts**")).as_deref(),
                   Some("Arsenal hold firm as the window shuts"));
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
                assert!(super::hook_violation(&shipped).is_none(),
                    "shipped a violating title {shipped:?} from {t:?}");
            }
        }
    }
}

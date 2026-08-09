//! # THE EDITOR — the greenfield rail's reader (contract `ep1`)
//!
//! The first junction of PLAN-one-rail: it fetches the publisher page, persists the body, and
//! DESCRIBES it on the ep1 contract; every judgment — relevance, entity links, nominations,
//! routing tags, the box-score fork — is derived in code (T2). It shadows the legacy
//! `article_read` seat (module `junctions/article_reader/`) until cutover.
//!
//! | | |
//! |---|---|
//! | **Seat** | `gemma3:4b` — pinned, settled by hardware (§4; OLMo is out) |
//! | **Contract** | `ep1` — describe the page, name who is in it; nothing derived is asked |
//! | **Reads** | the fetched publisher page |
//! | **Writes** | `editor_reads` + `news_articles.full_text` ONLY (shadow purity) |
//! | **Budget** | every arrival, best-first by Google's `feed_rank` |
//!
//! ## What ep1 changes from ar7 (the legacy contract it descends from)
//!
//! * `names[]` replaces `relevant_entities[]`: every person and club the TEXT involves, each with
//!   a `kind_hint` and a ≤6-word `descriptor` FROM the text. The descriptor is what lets code
//!   refuse `Paris`-the-city → Paris-the-club and route an unknown coach to person-discovery
//!   instead of a fuzzy player match (T9).
//! * `result_line` is new: the VERBATIM final-score line if the text states a completed result,
//!   else empty. Verbatim-or-empty is the describe-then-derive shape — code parses it; the model
//!   never says "a game happened".
//! * `co_mentions[]` is GONE: the numbered-candidate voting loop dies with the legacy rail.
//!
//! **Property order IS the contract** (the ar4 lesson): constrained decoding emits required
//! properties in schema order, so extraction comes before anything a judgment could lean on.
//! That order lives in ONE place — [`super::EDITOR_FORMAT_SCHEMA_RAW`]. `ep2` deleted the literal
//! JSON template that used to restate it at the end of [`EDITOR_SYSTEM_PROMPT`]: the grammar
//! already pins all 11 keys, their order, and every enum, so the template was belt-and-braces over
//! a structural guarantee — 235 measured tokens on every one of ~27k weekly calls, and an
//! UNTESTED coupling (no test ever compared the template to the schema; they agreed by luck).
//! See D-T40.

use crate::util::truncate;

/// The Editor's contract version — a CACHE KEY, not a label (T1): `editor_reads` rows carry it
/// as `contract_version`, and bumping it is what reopens work.
/// `ep2` (2026-08-09): the terminal JSON template was deleted from [`EDITOR_SYSTEM_PROMPT`] —
/// −235 tok/call, worst-case overflow 68.1% → 55.4% (D-T40). A bump here is RETROACTIVELY FREE:
/// only Go's ingest enqueues editor work, so nothing re-reads on a contract change.
pub const EDITOR_CONTRACT_VERSION: &str = "ep2";

pub const EDITOR_SYSTEM_PROMPT: &str = r#"Task: read one fetched sports article and describe it for the newsroom: what shape the page is, who is in it, what happened, and how it feels.

This article reached you because a ranked Google News query for a team matched it. That match is a HYPOTHESIS and nothing has checked it against the full text. You are NOT asked whether the article is relevant, and you must not answer that question anywhere. Describe the page accurately and the system decides everything else from your description.

FIELD 1 — page_kind: what SHAPE is this page, judged by its text, not its headline?
- article — prose reporting: someone wrote sentences about what happened and what it means.
- score_table — a result/boxscore/live-score page whose body is a score plus lineups, stats, possession, cards, attendance, next fixtures. A table with a headline is still a table.
- listing_or_schedule — a "how to watch"/TV-times/streaming/kickoff-times page, or a schedule roundup of many fixtures. A page for a single broadcast — watch X vs Y, the channel, the start time, where to stream, check local listings — is a listing too, even when it names only one game. Note: a page TITLED "How to watch" that then contains a real 1,000-word preview is an `article`; judge the body.
- video_clip — a page whose body is a video/highlights wrapper with little prose.
- roundup — a link list, navigation, tags, or "related stories" aggregation.
- other — anything else.

FIELD 2 — names: WHO IS IN THIS ARTICLE? List EVERY person and EVERY club the text actually involves, one entry each: the clubs playing or negotiating, every player, every coach or manager — including anyone the text merely quotes — and every executive the text names. Work through the WHOLE text and do not stop after the first name: a typical article yields three to eight entries, and a list that names a club but not the people in the story is wrong.

Example shape (a different, imaginary story): [{"name":"Feyenoord","kind_hint":"club","descriptor":"selling club"},{"name":"Santiago Gimenez","kind_hint":"person","descriptor":"Feyenoord striker"},{"name":"AC Milan","kind_hint":"club","descriptor":"club agreeing the fee"},{"name":"Zlatan Ibrahimovic","kind_hint":"person","descriptor":"Milan senior advisor"}]

For each entry:
- name: as the article writes it, in full: "Bukayo Saka", not "Saka". Name the club, never its city: "Paris Saint-Germain", not "Paris". A city is never a club — if the text is about the city itself (an event, a race, a venue), the city does not belong in names at all, and never with kind_hint club.
After listing, re-scan the text once for people you missed: anyone the text quotes ("said", "told"), any scorer, any manager or coach mentioned even by surname alone — each of them belongs in names. People only in this re-scan — never stadiums, stands, streets, or competitions.
- kind_hint: person, club, national_team, or other.
- descriptor: up to 6 words FROM THE TEXT naming the role, club, or context — "Real Madrid manager", "PSG sporting director". Copy what the text says; never write your own knowledge. If the text gives no role or context, use an empty string.
People and clubs only. Not competitions or trophies, not stadiums or their stands, ends, or streets, not broadcasters, newspapers or websites, not companies or sponsors. Only names the text actually contains — never expand a name into a club it resembles.

FIELD 3 — entity_roles: for EVERY hypothesis entity listed above — and ONLY those — say what part it plays in THIS text. One entry per hypothesis entity, using the entity's name exactly as listed, even when the entity is absent from the text. Never add anyone else here — people and clubs you found in the text belong in `names`:
- subject — the article is reporting ABOUT this entity; it is who the story concerns.
- opponent — this entity appears only as the opposition in a story about someone else.
- passing_mention — named in passing, in a list, or as background; the story is not about it.
- absent — a different club, age level, or competition that merely shares the name (youth, academy, reserves, women's, flag football, a same-named club elsewhere — even in the same sport), a city or place that merely shares the club's name in a story that is not about the club, or not present in the text at all. This is NOT the hypothesis entity.

Be strict about `subject`. If the story is about another club's player and this entity is who they face, that is `opponent`, not `subject`. If a youth or flag-football team shares the name, that is `absent`. If the hypothesis entity is a club but the text never discusses the club itself — its matches, its players, its business — the entity is `absent`, even when its name appears as a place.

Two worked examples of `absent` (imaginary, for shape only). Hypothesis "Santos", but the text covers the port city of Santos welcoming a cruise terminal — the name is everywhere, yet nothing is about the football club → {"entity":"Santos","role":"absent"}. Hypothesis "Minnesota Vikings", but the text is a youth flag football bracket where a team called the Vikings plays → {"entity":"Minnesota Vikings","role":"absent"} — a youth team sharing the name is not the NFL club.

Then story_type — what the story is ABOUT (transfer, injury, performance, fixture, roster, contract, general). A hiring or firing is `roster`.

FIELD 5 — result_line: if the text states a COMPLETED final result, copy that line from the text verbatim, in the shape "Real Madrid 2-1 Arsenal". Copy the names and the score exactly as the text gives them; do not compute, complete, or infer a result the text does not state. If no completed result is stated, use an empty string.

FIELD 6 — register_phrase, then register: how does this text FEEL, and what in it shows that?
- register_phrase: quote the SHORT run of words from the article that carries the most feeling — a fan reaction, a manager's complaint, a celebration, a warning. If the text quotes anger, protest, celebration, or complaint anywhere, this field must carry that quote. Copy it from the text; do not write your own. Only when the whole text is flat reporting with no such phrase, use an empty string.
- register: which one word best describes the phrase you just quoted? celebration, outrage, resignation, anticipation, or neutral. An empty register_phrase means neutral.

Then the facts and the evidence card. key_facts: one claim per fact, each attributable to the text or its named source ("The Athletic: deal not agreed"). If nothing here is about a hypothesis entity, do not summarize the unrelated story; give a short evidence_blurb explaining the mismatch.

Other rules:
- Use only the article text and the hypothesis entities.
- Detect the source article language. Translate meaning into English before writing the evidence card.
- evidence_blurb, key_facts, story_type, and caveats must be English.
- Preserve names, teams, dates, injuries, transactions, quotes-as-claims, scores, and reported uncertainty.
- Do not invent context, implications, or sourcing.
- Keep the evidence_blurb dense and neutral: what happened, who is involved, where it stands, and why it matters.
- If the article is mostly boilerplate, say so in caveats.
- Keep proper names in their canonical/source spelling unless an English name is clearly canonical.

Return strict JSON only. The key order and the allowed values are enforced for you by the response schema — write the content, not the shape."#;

/// The character budget for the article body in the prompt — same value as the legacy seat's
/// (`article_reader::ARTICLE_MAX_MODEL_CHARS`): both feed the same 8192-ctx runner.
pub(crate) const EDITOR_MAX_MODEL_CHARS: usize = 9_000;

pub fn build_editor_prompt_parts(
    source: &str,
    title: &str,
    description: &str,
    text: &str,
    hypothesis_names: &[String],
) -> String {
    let mut p = String::new();
    p.push_str(&format!("Source: {source}\n"));
    p.push_str(&format!("Title: {title}\n"));
    if !description.trim().is_empty() {
        p.push_str(&format!("RSS description: {description}\n"));
    }
    if !hypothesis_names.is_empty() {
        p.push_str("\nHypothesis entities (from the query that found this article):\n");
        for e in hypothesis_names {
            p.push_str("- ");
            p.push_str(e);
            p.push('\n');
        }
    }
    p.push_str("\nArticle text:\n");
    p.push_str(&truncate(&normalize_space(text), EDITOR_MAX_MODEL_CHARS));
    p.push_str("\n\nReturn the JSON object now.");
    p
}

/// normalize_space collapses the fetched body's whitespace so the prompt spends its character
/// budget on words rather than the publisher's markup residue.
fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

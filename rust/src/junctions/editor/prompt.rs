//! # THE EDITOR — the rail's reader (contract `ep5`)
//!
//! The first junction of PLAN-one-rail. It fetches the publisher page, persists the body, and
//! DESCRIBES it. **Every judgment is derived in code from that description (T2)** — relevance
//! (`derive::derive_relevance`), entity links (`derive::classify`), nominations (`nominate`),
//! routing tags (`derive::routing_tags`), the box-score fork. The model is never asked a verdict.
//!
//! **What the Editor is for, in Scott's words (2026-08-09):** read the article, summarise it with
//! special attention to **emotional text, names, injuries/suspensions and transfers**, and give
//! code what it needs to compile packets and tag the downstream characters — more than one
//! character can be tagged. It is also **the second layer of false-positive defence**: Google's
//! ranked query does the first pass and is only a HYPOTHESIS, and `entity_roles.absent` is where
//! this junction rejects it. And **names it does not recognise are the point, not a problem** —
//! unresolved names are the Investigator's discovery channel.
//!
//! | | |
//! |---|---|
//! | **Seat** | `ministral-3:3b` on archbox, `num_ctx` 4096 (`COGNITION_ROUTE_EDITOR`) |
//! | **Reads** | the fetched publisher page, furniture stripped (`fetch::extract_article_text`) |
//! | **Writes** | `editor_reads` + `news_articles.full_text` |
//! | **Budget** | every arrival, best-first by Google's `feed_rank`, capped per entity-day (D-T21) |
//!
//! **Property order IS the contract** (the ar4 lesson): constrained decoding emits required
//! properties in schema order, so extraction lands before anything a judgment could lean on. That
//! order lives in exactly ONE place — [`super::EDITOR_FORMAT_SCHEMA_RAW`].
//!
//! ⚠ **THE RULE THAT GOVERNS EVERY EDIT TO THE PROMPT BELOW (D-T43, learned the hard way):**
//! **a grammar constrains SHAPE, never MEANING.** The schema pins keys, order, types and enums for
//! free — it never enters the context window. It says NOTHING about the CONTENT of a free-text
//! field. So: **put it in the schema when you can (free), in the prose only when you must (costs
//! tokens on every call).** Deleting prose because "the grammar enforces it" is how ep2 shipped a
//! 100% regression. The contract history lives in PLAN-character-tuning §D-T40/§D-T43, not here.

use crate::util::truncate;

/// The Editor's contract version — a CACHE KEY, not a label (T1). `editor_reads` rows carry it as
/// `contract_version`, and `read_is_current` is the only query that reads it, so bumping it is
/// what reopens work. **Retroactively free:** only Go's ingest enqueues editor work, so a bump
/// changes how NEW arrivals are read and re-reads nothing.
///
/// `ep5` (2026-08-09) — the rewrite. Four contracts of accreted prose were cut back to the job:
/// the phantom `FIELD 4` (there was never one), the ar7/`co_mentions` history, the `gemma3:4b`
/// seat and the 8192-ctx sizing, the 250-char `names` example blob and two long worked `absent`
/// examples whose content is now one clause each. `suspension` joins `story_type`. See §D-T44.
pub const EDITOR_CONTRACT_VERSION: &str = "ep5";

pub const EDITOR_SYSTEM_PROMPT: &str = r#"Read one fetched sports article and describe it for the newsroom: what the page is, who is in it, what happened, and how it feels. Describe only — code turns your description into every decision, so never state a verdict.

A ranked news query for one team found this page. That match is a HYPOTHESIS and nothing has checked it against the body. You are that check. Do not answer whether the article is relevant; describe the page accurately and the system decides.

page_kind — what the page IS, judged by its body and not its headline:
- article: prose reporting; someone wrote sentences about what happened and what it means.
- score_table: a result, boxscore or live-score page whose body is a score plus lineups, stats, cards, attendance, next fixtures. A table with a headline is still a table.
- listing_or_schedule: how-to-watch, TV times, streaming or kickoff-time pages, and schedule roundups. A page for a single broadcast is still a listing. But one titled "How to watch" carrying a real 1,000-word preview is an article — judge the body.
- video_clip: a video or highlights wrapper with little prose.
- roundup: a link list, tag page or "related stories" aggregation.
- other: anything else.

names — WHO IS IN THIS TEXT. Every person and every club it actually involves: the clubs playing or negotiating, every player, every coach or manager, anyone it quotes, every executive it names. Work through the whole text and do not stop at the first name — a typical report yields three to eight, and a list naming a club but none of its people is wrong. Names we do not already know are how new people enter the system, so list them even when they look unfamiliar.
- name: as the text writes it, in full — "Bukayo Saka", not "Saka". Name the club, never its city: "Paris Saint-Germain", not "Paris". A city is never a club; if the text is about the city itself, it does not belong here at all.
- kind_hint: what the text treats this name as.
- descriptor: up to 6 words COPIED FROM THE TEXT giving the role or context — "Real Madrid manager", "PSG sporting director". Never your own knowledge, and never a role word from entity_roles. Empty string if the text gives none.
People and clubs only — not competitions or trophies, not stadiums or their stands, not broadcasters, publications or sponsors. Only names the text actually contains; never expand a name into a club it resembles.

entity_roles — one entry for each HYPOTHESIS ENTITY listed in the message below, and nothing else. Never put a name here that is not a hypothesis entity; those belong in names. Use the hypothesis entity's name exactly as given, even when it turns out to be absent:
- subject: the story is about it.
- opponent: it appears only as the opposition in a story about someone else.
- passing_mention: named in passing, in a list, or as background.
- absent: not in this text, or only a DIFFERENT thing sharing the name — a youth, academy, reserve, women's or flag-football team, a same-named club elsewhere even in the same sport, or a place that merely shares the club's name in a story that is not about the club.
Be strict about subject. If the story is about another club's player and this entity is who they face, that is opponent. If the text never discusses the club itself — its matches, its players, its business — it is absent even where the name appears.

story_type — what the story is ABOUT. A hiring or firing is roster. A ban, red-card ban or doping ban is suspension.

result_line — if the text states a COMPLETED final result, copy that line verbatim, in the shape "Real Madrid 2-1 Arsenal". Copy names and score exactly as given; never compute, complete or infer a result the text does not state. Empty string if no completed result is stated.

register_phrase, then register — how this text FEELS and what in it shows that. Quote the SHORT run of words carrying the most feeling: a fan reaction, a manager's complaint, a celebration, a warning. If the text carries anger, protest, celebration or complaint anywhere, this field must hold that quote, copied from the text and not written by you. Empty only when the whole piece is flat reporting — and an empty phrase means register neutral.

key_facts — one claim each, attributable to the text or a source it names ("The Athletic: deal not agreed"). Never let an injury, a suspension or a transfer development go unrecorded: who, which club, and where it stands. If nothing in the text concerns a hypothesis entity, do not summarise the unrelated story — say why it does not match, in evidence_blurb.

evidence_blurb — 2-4 compact sentences, dense and neutral: what happened, who is involved, where it stands, why it matters.

caveats — short; say so when the page is mostly boilerplate. Empty string if none.

source_language — the language the ARTICLE ITSELF is written in. Use "unknown" only when it genuinely cannot be told, never as a default.

Use only the article text and the hypothesis entities, and invent no context, implications or sourcing. Preserve dates, scores, injuries, transactions, quotes-as-claims and any stated uncertainty. Write key_facts, caveats and evidence_blurb in English, translating the meaning where the article is in another language, but keep proper names in their source spelling. Plain prose in every field — no markdown, no bold, no headings. Return strict JSON only; the keys, their order and the allowed values are enforced for you."#;

/// The character budget for the article body — **re-derived at `ep5` from the window it actually
/// has to fit in** (D-T40 item 2, which had flagged that the old 9,000 contradicted the budget).
///
/// The arithmetic, all four terms measured: `EDITOR_NUM_CTX` 4096 − 554 chat-template floor −
/// ~1,010 for this system prompt − `EDITOR_NUM_PREDICT` 900 = **~1,630 tokens** for the user
/// message, and at the measured 4.68 chars/token that is ~7,600 chars, of which the Source/Title/
/// hypothesis preamble takes ~150.
///
/// **7,500 is not the squeeze it looks like**, because `fetch::extract_article_text` now strips
/// site furniture before this cap ever applies: the old 9,000 was mostly spent on navigation and
/// "Related Stories", so the cap used to truncate real prose to make room for menus. Measured
/// across 12 publishers the extractor returns a median body well inside this budget.
pub(crate) const EDITOR_MAX_MODEL_CHARS: usize = 7_500;

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

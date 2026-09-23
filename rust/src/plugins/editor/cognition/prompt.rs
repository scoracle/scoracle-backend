//! The Editor's article-description contract.
//!
//! Studio describes the prepared publisher text. Deterministic code derives relevance,
//! entity matches, routing tags and parsed results. Application adapters own fetching,
//! database writes, nominations and scheduling; model routing is selected outside Studio.
//!
//! Property order is part of the contract: constrained decoding emits required
//! properties in schema order. That
//! order lives in exactly ONE place — [`super::EDITOR_FORMAT_SCHEMA_RAW`].
//!
//! A grammar constrains shape, not the meaning of free-text fields. Put shape rules in the
//! schema and semantic rules in the prompt.

use crate::util::truncate;

/// The Editor's contract version — a CACHE KEY, not a label (T1). `editor_reads` rows carry it as
/// `contract_version`, and `read_is_current` is the only query that reads it, so bumping it is
/// what reopens work. **Retroactively free:** only Go's ingest enqueues editor work, so a bump
/// changes how NEW arrivals are read and re-reads nothing.
///
pub const EDITOR_CONTRACT_VERSION: &str = "ep8";

pub const EDITOR_SYSTEM_PROMPT: &str = r#"Read one fetched sports article and describe it for the newsroom: what the page is, who is in it, what happened, and how it feels. Describe only — code turns your description into every decision, so never state a verdict.

A ranked news query found this page for one team. That match is a HYPOTHESIS you are the check on: describe the page accurately and the system decides relevance.

page_kind — what the page IS, judged by its body and not its headline:
- article: prose reporting; someone wrote sentences about what happened and what it means.
- score_table: a result, boxscore or live-score page whose body is a score plus lineups, stats, cards, attendance, next fixtures. A table with a headline is still a table.
- listing_or_schedule: how-to-watch, TV times, streaming or kickoff-time pages, and schedule roundups. A page for a single broadcast is still a listing. But one titled "How to watch" carrying a real 1,000-word preview is an article — judge the body.
- video_clip: a video or highlights wrapper with little prose.
- roundup: a link list, tag page or "related stories" aggregation.
- other: anything else.

names — WHO IS IN THIS TEXT. Every person and every club it actually involves: the clubs playing or negotiating, every player, every coach or manager, anyone it quotes, every executive it names. Work through the whole text and do not stop at the first name — a typical report yields three to twelve, and a long feature can yield two dozen. A list naming a club but none of its people is wrong. Names we do not already know are how new people enter the system, so list them even when they look unfamiliar. Example shape: [{"name":"Santiago Gimenez","kind_hint":"person","descriptor":"Feyenoord striker"}]
- name: as the text writes it, in full — "Bukayo Saka", not "Saka". Name the club, never its city: "Paris Saint-Germain", not "Paris". A city is never a club; if the text is about the city itself — an event, a race, a venue — it does not belong here at all, and never as club.
- kind_hint: what the name ITSELF is, never its affiliation — a "Rangers defender" is a person, and his club is its own entry in this list. Anything that is not a person or a club — a city, a competition — is other, never club.
- descriptor: up to 6 words COPIED FROM THE TEXT — "Real Madrid manager". Never your own knowledge, never a role word from entity_roles. Empty string if the text gives none.
People and clubs only — never competitions, trophies, stadiums, broadcasters, publications or sponsors. Only names the text contains; never expand a name into a club it resembles.

entity_roles — one entry for each HYPOTHESIS ENTITY listed in the message below, and nothing else. Never put a name here that is not a hypothesis entity; those belong in names. Use the hypothesis entity's name exactly as given, even when it turns out to be absent:
- subject: the story is about it.
- opponent: it appears only as the opposition in a story about someone else.
- passing_mention: named in passing, in a list, or as background.
- absent: not in this text, or only a DIFFERENT thing sharing the name — a youth, academy, reserve, women's or flag-football team, a same-named club elsewhere even in the same sport, or a place that merely shares the club's name in a story that is not about the club.
Be strict about subject. If the story is about another club's player and this entity is who they face, that is opponent. If the text never discusses the club itself — its matches, its players, its business — it is absent even where the name appears.

story_type — what the story is ABOUT, one value: transfer for a move between clubs or talk of one; injury for fitness — a player hurt or ruled out hurt; suspension for a ban imposed as punishment; performance for a match played and how it went; fixture for a match still to come; roster for a hiring, firing or squad change; contract for a renewal or extension at the same club; general for anything else.

result_line — if the text states a COMPLETED final result, copy that line verbatim, in the shape "Real Madrid 2-1 Arsenal". Copy names and score exactly as given; never compute, complete or infer a result the text does not state. Empty string if no completed result is stated.

register_phrase, then register — how this text FEELS and what in it shows that. Quote the SHORT run of words carrying the most feeling: a fan reaction, a manager's complaint, a celebration, a warning. If the text carries anger, protest, celebration or complaint anywhere, this field must hold that quote, copied from the text and not written by you. Empty only when the whole piece is flat reporting — and an empty phrase means register neutral. register labels the phrase you quoted, nothing else: fury or protest is outrage, joy is celebration, accepting defeat is resignation — and a phrase that merely reports, however important the news, is still neutral.

key_facts — one claim each, attributable to the text or a source it names ("The Athletic: deal not agreed"). Never let an injury, a suspension or a transfer development go unrecorded: who, which club, and where it stands. If nothing in the text concerns a hypothesis entity, do not summarise the unrelated story — say why it does not match, in evidence_blurb.

evidence_blurb — 2-4 compact sentences, dense and neutral: what happened, who, where it stands, why it matters.

caveats — short; say so when the page is mostly boilerplate. Empty string if none.

source_language — the language the ARTICLE ITSELF is written in; an article in English is "en". Use "unknown" only when it genuinely cannot be told, never as a default.

Use only the article text and the hypothesis entities, and invent no context, implications or sourcing. Preserve dates, scores, injuries, transactions, quotes-as-claims and any stated uncertainty. Write key_facts, caveats and evidence_blurb in English, translating the meaning where the article is in another language, but keep proper names in their source spelling. Plain prose in every field."#;

/// Keep enough space for chat-template variance in addition to the explicit output reservation.
/// The estimator deliberately over-counts punctuation-heavy and non-ASCII text; actual provider
/// counts remain the diagnostic source of truth.
const EDITOR_CONTEXT_SAFETY_TOKENS: usize = 128;
const EDITOR_CHAT_TEMPLATE_TOKENS: usize = 32;

/// Conservative tokenizer-independent estimate for BPE-style model inputs. Alphanumeric runs
/// are charged at least one token per three UTF-8 bytes and punctuation one token per scalar.
/// This is intentionally stricter than ordinary English tokenization and reacts to the tables,
/// scores, URLs and non-English text that defeated the old article-character cap.
pub(crate) fn estimate_editor_input_tokens(system: &str, user: &str) -> usize {
    fn text_tokens(text: &str) -> usize {
        let mut total = 0usize;
        let mut run_bytes = 0usize;
        for ch in text.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                run_bytes += ch.len_utf8();
            } else {
                if run_bytes > 0 {
                    total += run_bytes.div_ceil(3).max(1);
                    run_bytes = 0;
                }
                if !ch.is_whitespace() {
                    total += 1;
                }
            }
        }
        if run_bytes > 0 {
            total += run_bytes.div_ceil(3).max(1);
        }
        total
    }

    EDITOR_CHAT_TEMPLATE_TOKENS + text_tokens(system) + text_tokens(user)
}

fn render_editor_user_prompt(
    source: &str,
    title: &str,
    description: &str,
    text: &str,
    hypothesis_names: &[String],
    compact_metadata: bool,
) -> String {
    let mut p = String::new();
    // Metadata is part of the same request budget. Bound pathological feed fields while
    // preserving enough of each to identify the article and hypothesis.
    let (source_cap, title_cap, description_cap, hypothesis_cap, hypothesis_count) =
        if compact_metadata {
            (40, 120, 0, 60, 4)
        } else {
            (160, 320, 480, 120, 16)
        };
    p.push_str(&format!("Source: {}\n", truncate(source, source_cap)));
    p.push_str(&format!("Title: {}\n", truncate(title, title_cap)));
    if description_cap > 0 && !description.trim().is_empty() {
        p.push_str(&format!(
            "RSS description: {}\n",
            truncate(&normalize_space(description), description_cap)
        ));
    }
    if !hypothesis_names.is_empty() {
        p.push_str(&format!(
            "\n{}\n",
            crate::plugins::support::form::IDENTITY_CARD_FRAMING
        ));
        p.push_str("\nHypothesis entities (from the query that found this article):\n");
        for e in hypothesis_names.iter().take(hypothesis_count) {
            p.push_str("- ");
            p.push_str(&truncate(e, hypothesis_cap));
            p.push('\n');
        }
    }
    p.push_str("\nArticle text:\n");
    p.push_str(text);
    p.push_str("\n\nReturn the JSON object now.");
    p
}

pub fn build_editor_prompt_parts(
    source: &str,
    title: &str,
    description: &str,
    text: &str,
    hypothesis_names: &[String],
) -> String {
    let normalized = normalize_space(text);
    let input_limit = super::EDITOR_NUM_CTX
        .saturating_sub(super::EDITOR_NUM_PREDICT)
        .saturating_sub(EDITOR_CONTEXT_SAFETY_TOKENS as i32) as usize;
    let full_fixed =
        render_editor_user_prompt(source, title, description, "", hypothesis_names, false);
    let compact_metadata =
        estimate_editor_input_tokens(EDITOR_SYSTEM_PROMPT, &full_fixed) > input_limit;

    // Search the largest UTF-8-safe article prefix whose complete system + user conversation
    // still leaves the configured output reservation. This replaces the old body-only cap.
    let mut low = 0usize;
    let mut high = normalized.len();
    while low < high {
        let candidate = (low + high).div_ceil(2);
        let body = truncate(&normalized, candidate);
        let prompt = render_editor_user_prompt(
            source,
            title,
            description,
            &body,
            hypothesis_names,
            compact_metadata,
        );
        if estimate_editor_input_tokens(EDITOR_SYSTEM_PROMPT, &prompt) <= input_limit {
            low = candidate;
        } else {
            high = candidate - 1;
        }
    }
    let prompt = render_editor_user_prompt(
        source,
        title,
        description,
        &truncate(&normalized, low),
        hypothesis_names,
        compact_metadata,
    );
    debug_assert!(
        estimate_editor_input_tokens(EDITOR_SYSTEM_PROMPT, &prompt) <= input_limit,
        "the Editor's fixed prompt exceeds its input budget"
    );
    prompt
}

/// normalize_space collapses the fetched body's whitespace so the prompt spends its character
/// budget on words rather than the publisher's markup residue.
fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

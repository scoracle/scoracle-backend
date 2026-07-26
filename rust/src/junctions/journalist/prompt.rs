//! # THE JOURNALIST — the one at the table who has read everything
//!
//! The news rail's hub. Given one entity and its vetted corpus, this junction decides what the
//! *stories* are: it groups the day's articles into the distinct storylines actually developing,
//! and files them as the record everyone downstream argues from.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::NarrativeLogic` |
//! | **Contract** | `n13` |
//! | **Reads** | the vetted corpus, The Reader's evidence cards, The Insider's vetted transfer heat, the relational memory card, its own prior card reads |
//! | **Feeds** | The Influencer, The Oracle, and `narrative_threads`, via `news_summaries` |
//!
//! ## Authority — it defines what counts as a story
//!
//! No other junction gets to say what the storylines are. The Influencer reads emotion *off* this
//! record, The Oracle renders a verdict *over* it, and thread identity is built *from* it. That
//! makes The Journalist the most load-bearing voice in the rail: a storyline it fails to notice is
//! one nothing downstream can recover, and a storyline it invents propagates unchallenged.
//!
//! It also owns two verdicts beyond the prose. Every numbered article is labeled
//! transfer/trade-related or not, which is what routes material to The Insider. And since n12 it
//! returns a required 1–99 `card_score` — the busyness of the entity's news right now — rendered
//! last in the prompt so the verdict lands after the corpus has been read.
//!
//! ## What it does not decide
//!
//! Relevance. Since 2026-07-25 that belongs entirely to The Reader, and The Journalist never
//! re-litigates it: an article that arrives here has already earned its place. What it may see is
//! The Reader's *evidence card* — a summary drawn from the publisher's body text rather than the
//! headline — and n13's one change is to prefer that card when it exists. Articles The Reader never
//! reached still arrive, on their headline alone. Reading is an upgrade, never a precondition.
//!
//! Transfer facts, likewise, are handed down vetted. The heat section renders known direction and
//! stage as ground truth; the instruction is to ground any transfer storyline in those facts and
//! never contradict them. The whole section is omitted when there is no heat — not rendered as
//! "(none)" the way The Influencer's is — because Go's original `if len(heat) > 0` did that, and
//! this prompt is byte-compatible with it.
//!
//! ## Memory is continuity, not corroboration
//!
//! Two blocks are rendered into the prompt and deliberately kept OUT of the `input_hash`: the
//! relational memory card (n8, mig 163) and the junction's own prior card reads. Both move on their
//! own; hashing either would make the stage re-trigger on its own history. And both carry the
//! echo-chamber rule explicitly — a prior story frames the arc a narrative sits in, and is never
//! itself evidence for a new claim.
//!
//! ## A note on bytes
//!
//! This builder is byte-for-byte identical to Go's `buildNarrativesPrompt` while `full_text` is
//! NULL and no `score_context` is given. The em-dash (U+2014) separators are load-bearing. Change
//! the spacing here and you have changed the contract, whether or not you meant to.

use super::{CorpusItem, NarrativesReq, article_context};
use crate::corpus::{HeatItem, write_heat_lines};
use crate::util::truncate_bytes;

/// System prompt for The Journalist (n11): group recent vetted news into distinct storylines, label
/// each article transfer/non-transfer (the `article_buckets` section that routes the transfers
/// stage), and voice the relational memory's episode heat + new/ongoing state. The candle hands
/// narratives a widened, pre-deduplicated corpus (the source-aware novelty gate runs at the tip of
/// the spear), so the pre-n9 per-article relevance tags are gone.
///
/// n11 keeps the n10 Journalist voice pass and n9 JSON contract, but tells the model to read
/// multilingual sources internally and emit English storylines. Same JSON schema, same
/// storyline/bucket rules, same credibility guards.
///
/// n12 adds THE CARD SCORE (tarot deck, Phase 4): after filing the storylines and labeling the
/// buckets, the Journalist lands a required 1-99 busyness verdict — volume-of-noise, not
/// good-vs-bad. Grounded by the deterministic SIGNALS line and the prior-card-reads memory the
/// user prompt renders (both prompt-only, outside the input_hash).
pub const NARRATIVES_SYSTEM_PROMPT: &str = r#"Task: you are The Journalist — the one at the table who has read everything. Your beat is ONE sports entity. File the record: group the recent vetted news into the distinct storylines actually developing around this entity, and label every numbered article as transfer/trade-related or not.

Voice: informed, sourced, measured. You quote nothing out of context and you never write past your sourcing. You notice how widely a story is actually reported — a single-source whisper is not a chorus — and you say the difference plainly. Freshness, stakes, and trajectory are your native vocabulary: a story is NEW or CONTINUING, and its coverage is heating up, cooling, or steady. No hype, no source lists, no invented facts.

Language handling: numbered articles may be in English, Spanish, French, German, Italian, Portuguese, Dutch, or another language, and one corpus can mix languages. Read each source in its language, translate meaning internally, and write every title, body, and other generated prose in English. Keep proper names, player names, club names, source names, and stated money/pick details exact or canonical. Never quote non-English headlines verbatim; paraphrase in English.

Return STRICT JSON only (no markdown fences, no text before or after):
{"narratives": [{"title": "<headline>", "body": "<write-up>", "articles": [<article numbers>]}, ...], "article_buckets": [{"article": <article number>, "transfer": <true|false>}, ...], "card_score": <integer 1-99>}

Storyline discipline:
- Return at most 6 storylines, most consequential first.
- One story is one storyline: do not split it across entries, and do not merge unrelated stories.
- A quiet cycle is an honest answer: one storyline or none.
- Pass over vague hype that never names who, what, and where — restraint is credibility.
- Pass over articles that are not actually about this entity.

For each storyline:
- title: short and specific, naming the key people/clubs; never generic like "Transfer news".
- body: what is happening, who is involved, and where it stands — the filed piece, not a headline echo. Most run one or two sentences; give more column inches only to a genuinely major, multi-source story. Place the story in its arc using the relational memory below: NEW or CONTINUING, and heating up, cooling, or steady. Keep any coverage/likelihood figures qualitative — the raw numbers are internal notes, never copy.
- articles: the article numbers behind that storyline.

article_buckets — label EVERY numbered article exactly once:
- {"article": <its number>, "transfer": true} when the article is itself about a transfer, trade, signing, loan, or contract move (into or out of a club), otherwise "transfer": false.
- Judge each article on its own substance. Another team scheming around this entity is not this entity moving.

THEN, THE CARD SCORE — an integer 1 to 99, your one-number read of how BUSY this entity's news cycle is:
- Volume of noise, not good news versus bad: 1 = a silent week, ~50 = a steady beat, 85+ = a feeding frenzy the desk can barely file fast enough.
- Score the cycle you just filed: how many storylines are live, how widely they are sourced, how fresh the freshest coverage is. The SIGNALS line is your deterministic tally; your storyline judgment refines it, never contradicts it wholesale.
- YOUR PRIOR CARD READS is memory, not a reset: move deliberately from your previous card score, and hold unless the corpus justifies a change.
- A quiet week is an honest answer: filing zero storylines earns a low card score, never a missing one.

If a "Known transfer/trade activity" list is given, treat it as vetted truth for transfer/trade storylines: take counterparties, direction, and stage from it, never contradict it, and never report a more advanced stage than it shows. The word "heat" and its numbers are internal; never mention them.

The relational memory is your own archive — use it for arc and continuity only (what fizzled before, what is live now, what actually happened). A prior story is never evidence for a new one: today's claims stand on today's sources or they do not run.

Do not turn a story about another team drafting, signing, or scheming around someone alongside/against this entity into a storyline about this entity moving teams or entering a draft. Never quote headlines verbatim, dump source names or URLs, or state anything the sources do not."#;

/// Bump when the prompt materially changes (traced in `news_summaries.prompt_version`).
/// Rollout is free: prompt_version sits inside the generation `input_hash`, so an n-bump forces
/// exactly one regen per news-active entity on the next sweep — no reconcile binary.
pub const NARRATIVES_PROMPT_VERSION: &str = "n13"; // n13: prefer Article Reader evidence cards when present; n12: the Journalist's card_score (tarot deck) — required 1-99 busyness verdict after the storylines

/// The JSON schema Ollama's constrained decoding enforces on the narratives reply (Phase 5).
/// Grammar-level guarantees the free-text contract could only ask for: the top-level object
/// cannot be prose-wrapped, `narratives` must exist, and every item carries title/body/articles.
/// n9 adds the `article_buckets` section — the Journalist's per-article transfer/non-transfer label
/// (own section, never bunched into a storyline); `transfer:true` ⇒ `news_articles.bucket='transfer'`.
/// n12 adds required `card_score` — the Journalist's 1-99 busyness verdict — ordered AFTER
/// narratives/buckets (sigil doctrine: read the signs first, land the verdict second).
/// The tolerant balanced-brace salvager stays as the parse path — schema output is a strict
/// subset of what it accepts, and it remains the safety net for the offline/parity bins.
pub fn narratives_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "narratives": {
                "type": "array",
                "maxItems": 6,
                "items": {
                    "type": "object",
                    "properties": {
                        "title":    { "type": "string" },
                        "body":     { "type": "string" },
                        "articles": { "type": "array", "items": { "type": "integer" } }
                    },
                    "required": ["title", "body", "articles"]
                }
            },
            "article_buckets": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "article":  { "type": "integer" },
                        "transfer": { "type": "boolean" }
                    },
                    "required": ["article", "transfer"]
                }
            },
            "card_score": { "type": "integer", "minimum": 1, "maximum": 99 }
        },
        "required": ["narratives", "article_buckets", "card_score"]
    })
}

/// build_narratives_prompt assembles the user prompt, byte-for-byte the same as Go's
/// `buildNarrativesPrompt` while `full_text` is NULL (the current state) and no `score_context`
/// is given. The `—` (U+2014) bytes are significant. The heat section is OMITTED entirely when
/// there is no transfer heat (unlike vibe's "(none)" line), matching Go's `if len(heat) > 0`.
/// `score_context` (n12) is the pre-rendered SIGNALS line + prior-card-reads memory block that
/// grounds the card score — rendered last, just before the reply instruction, so the verdict
/// lands after the signs are read. Like the relational memory it is prompt-only: deliberately
/// NOT part of the input_hash (the score always moves; hashing it would self-trigger).
pub fn build_narratives_prompt(
    req: &NarrativesReq,
    news: &[CorpusItem],
    heat: &[HeatItem],
    memory: Option<&str>,
    score_context: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {} ({} {})\n",
        req.entity_name, req.sport, req.entity_type
    ));
    b.push_str("\nRecent news (numbered):\n");
    for (i, n) in news.iter().enumerate() {
        b.push_str(&format!("{}. ", i + 1));
        if !n.source.is_empty() {
            b.push_str(&format!("[{}] ", n.source));
        }
        b.push_str(&n.title);
        let (body, body_cap) = article_context(n);
        if !body.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(body, body_cap));
        }
        b.push('\n');
    }
    // Vetted transfer facts (when any) — the structured truth behind any transfer storyline. The
    // narrator uses these names/direction/stage rather than guessing from a headline. The whole
    // section is omitted when empty (Go's `if len(heat) > 0`).
    if !heat.is_empty() {
        b.push_str("\nKnown transfer/trade activity (vetted facts — ground any transfer storyline in these, do not contradict them):\n");
        write_heat_lines(&mut b, heat);
    }
    // Relational memory card (n8, mig 163): the graph's per-entity history — prior
    // stories with outcomes, current stories with likelihood, ground-truth moves.
    // CONTINUITY, NOT CORROBORATION (the echo-chamber rule): memory frames the arc a
    // narrative sits in; it is never itself evidence for a new claim. Rendered only
    // when the graph holds memory; deliberately NOT part of the input_hash.
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this entity — use for arc and continuity: what fizzled before, what is live now, what actually happened; do NOT treat a prior story as evidence for a new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }
    // Card-score grounding (n12): the deterministic SIGNALS tally + the Journalist's own prior
    // card reads, rendered LAST so the busyness verdict lands after the corpus is read.
    if let Some(sc) = score_context.filter(|s| !s.trim().is_empty()) {
        b.push('\n');
        b.push_str(sc);
        if !sc.ends_with('\n') {
            b.push('\n');
        }
    }
    b.push_str("\nReturn the JSON object now.");
    b
}

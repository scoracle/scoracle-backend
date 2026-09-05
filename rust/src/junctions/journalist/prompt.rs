//! # THE JOURNALIST — the one at the table who has read everything
//!
//! The news rail's hub. Given one entity and its vetted corpus, this junction decides what the
//! *stories* are: it groups the day's articles into the distinct storylines actually developing,
//! and files them as the record everyone downstream argues from.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::NarrativeLogic` |
//! | **Contract** | [`NARRATIVES_PROMPT_VERSION`] (this table said `n16` until the 08-23 review pass caught it five bumps stale — the constant is the truth) |
//! | **Reads** | the vetted corpus, The Editor's evidence cards, The Insider's vetted transfer heat, the relational memory card, its own prior card reads |
//! | **Feeds** | The Influencer, The Oracle, and the entity's parts in its storylines, via `news_summaries` |
//!
//! ## Authority — it defines what counts as a story
//!
//! No other junction gets to say what the storylines are. The Influencer reads emotion *off* this
//! record, The Oracle renders a verdict *over* it, and thread identity is built *from* it. That
//! makes The Journalist the most load-bearing voice in the rail: a storyline it fails to notice is
//! one nothing downstream can recover, and a storyline it invents propagates unchallenged.
//!
//! It also owns a verdict beyond the prose: since n12 it returns a required 1–99 `card_score` —
//! the busyness of the entity's news right now — rendered last in the prompt so the verdict lands
//! after the corpus has been read.
//!
//! Until n16 it owned a second one, labelling every numbered article transfer/trade-related to
//! route material to The Insider. That is The Editor's now. It was never really a verdict: it was
//! sorting, priced as though it were storytelling. It made this generation's length scale with the
//! CORPUS rather than with the story, on the host with no headroom, from a 900-byte blurb of a body
//! The Editor had already read in full.
//!
//! ## What it does not decide
//!
//! Relevance. Since 2026-07-25 that belongs entirely to The Editor, and The Journalist never
//! re-litigates it: an article that arrives here has already earned its place. What it may see is
//! The Editor's *evidence card* — a summary drawn from the publisher's body text rather than the
//! headline — and n13's one change is to prefer that card when it exists. Articles The Editor never
//! reached still arrive, on their headline alone. Reading is an upgrade, never a precondition.
//!
//! Transfer truth, since n17, is not this seat's input at all. The Insider owns the transfer
//! rail end-to-end; the Journalist files a transfer story from the news corpus exactly as it
//! files any other story. (The n16-era heat section rendered the Insider's vetted
//! direction/stage as ground truth here — the last cross-seat coupling, removed as the
//! separation pass.)
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

use super::{article_context, CorpusItem, NarrativesReq};
use crate::util::truncate_bytes;

/// System prompt for The Journalist: group recent vetted news into distinct storylines and voice
/// the relational memory's episode heat + new/ongoing state. The candle hands narratives a
/// widened, pre-deduplicated corpus (the source-aware novelty gate runs at the tip of the spear),
/// so the pre-n9 per-article relevance tags are gone.
///
/// **n16 removed the per-article transfer labelling** (the n9 `article_buckets` section). Sorting
/// articles is assignment-desk work and belongs to The Editor, which emits `story_type` from the
/// full body. What is left here is the one thing only this character can do: voice the developing
/// story.
///
/// n11 keeps the n10 Journalist voice pass, but tells the model to read multilingual sources
/// internally and emit English storylines. Same storyline rules, same credibility guards.
///
/// n12 adds THE CARD SCORE (tarot deck, Phase 4): after filing the storylines, the Journalist
/// lands a required 1-99 busyness verdict — volume-of-noise, not
/// good-vs-bad. Grounded by the deterministic SIGNALS line and the prior-card-reads memory the
/// user prompt renders (both prompt-only, outside the input_hash).
pub static NARRATIVES_SYSTEM_PROMPT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        r#"Task: you are The Journalist — the seasoned writer at the table. Your beat is ONE sports entity and your column is the developing narratives around it. Group the recent vetted news into the distinct storylines actually developing.

Voice: the dedicated beat writer, at the top of the craft. The facts ARE the story; every line earns its place by carrying one. No hype, no URLs, no invented facts.

ATTRIBUTION IS PART OF THE STORY. You notice how widely a story is actually reported — a single-source whisper is not a chorus — and you credit the publications by name, woven into the sentence: "first reported by ESPN, since matched by Marca". Never a list bolted on, never a headline quoted verbatim.

{selection}

{wire}

Storylines: at most 6, most consequential first — a storyline IS a claim, and the selection question above is your assignment desk. One story is one storyline — never split it, never merge unrelated ones. A quiet cycle is an honest answer: one storyline or none. Pass over vague hype that never names who, what and where, and over articles not actually about this entity. Never turn a story about another club drafting or signing AROUND this entity into a story about this entity moving.

For each: `title` short and specific, naming the key people and clubs, never generic like "Transfer news" — it is the claim, stated. `body` is the claim told in full: what is happening stated first, the evidence next — who is involved, with the publications credited in prose — and where it stands to close. The filed piece, not a headline echo. Place it in its arc using the relational memory — new or continuing, heating up, cooling or steady — in your own words, never as a pasted label. Keep coverage and likelihood figures qualitative; the raw numbers are internal notes. `articles` are the article numbers behind it.

THE CARD IS THE PAGE. Everything you file prints onto ONE tarot card, and every storyline shares it: your budget is EIGHT SENTENCES TOTAL across all storylines, never eight each. Spend it like a front page — the lead earns real inches, secondary items a line, a name that just cleared the bar a clause. Under budget is fine and often right. Over budget does not fit any card ever printed.

THE HEADLINE: after the storylines, `headline` is the front-page hook for the WHOLE edition — a tweet, 140 characters at most, and shorter lands harder. It is your read of this entity's whole cycle, never a storyline title repeated and never generic like "Latest news". A busy day earns a busy-day hook naming what made it busy; a quiet week says so plainly and honestly. Present tense, no caps-lock. The one thing it may not do is run past the card.

CARD SCORE, 1-99: how BUSY this cycle is, volume not sentiment. 1 is a silent week, 50 a steady beat, 85+ a frenzy. It must MATCH the edition you just filed — several sourced storylines is never a silent-week score, an empty edition is never a frenzy. The headline and the score answer the same question in two registers, so they must agree. The SIGNALS line is your floor; your read refines it, never contradicts it. A prior card read is memory: move from it deliberately.

Relational memory is your archive — arc and continuity only. A prior story is never evidence for a new one: today's claims stand on today's sources or they do not run.

One filed storyline, as the register only (never the subject):
{{"title": "Fee gap is all that holds up Carvalho's move to Leeds", "body": "Leeds and Braga have agreed personal terms on Rui Carvalho, and what began as a Record whisper ten days ago is now carried by ESPN and The Guardian — coverage with real weight behind it, and heating up. Only the fee separates the clubs, and talks continue this week.", "articles": [2, 4, 5]}}"#,
        selection = crate::junctions::form::CLAIM_SELECTION,
        wire = crate::junctions::form::WIRE_COPY
    )
});

/// Bump when the prompt materially changes (traced in `news_summaries.prompt_version`).
/// Rollout is free: prompt_version sits inside the generation `input_hash`, so an n-bump forces
/// exactly one regen per news-active entity on the next sweep — no reconcile binary.
pub const NARRATIVES_PROMPT_VERSION: &str = "n22"; // n22, THE FORM + THE WAVE (2026-08-25): composes CLAIM_SELECTION + WIRE_COPY from junctions::form — a storyline IS a claim and the selection question is the assignment desk (her native shape, now named); each body is the claim told in full (stated first, evidence with credited publications next, where-it-stands close). Part of the deliberate five-voice bump for the granite+form regen wave (momentum-s21's note has the full rationale): the articulator corpus gate requires n22+ AND model_version=granite4.2:3b. // THE HEADLINE (2026-08-24) — contract changed, version deliberately NOT bumped (the v23/Twitter-rule precedent): the reply gains a required entity-level `headline` (the tweet hook, the uniform score+headline+body card contract), but an n-bump reopens every news-active entity and Scott ruled no corpus regeneration — the hook reaches entities as their material moves, and pre-headline rows stay NULL (mig 232's lazy-invalidation norm). SAME COST AS v23: two contracts share "n21" and generated_at is the only discriminator — cut at 2026-08-24. The parser tolerates a missing headline (best-effort, the card_score pattern), so both contracts parse. // n21 — THE COMPACT-WIRE PASS (MLX cutover day, 2026-08-19): the openai path carries no grammar, and the 8B's unconstrained edition arrived fenced and pretty-printed — structural tokens the ollama grammar never spent — exhausting the 700-token packet reservation mid-first-narrative (parsed to zero storylines, entity 1123, first hour of the cutover). The contract line now demands COMPACT single-line JSON and names why (formatting spends the edition's own budget — the card-fit doctrine applied to syntax), and the packet reservation moves 700→900 as the margin. // n20 — THE NEWS-BUDGET PASS (the ctx-budget doctrine, Scott 2026-08-15). MAX_PACKETS_PER_ENTITY bounded stories; a mega-storyline is ONE story with a hundred member articles, and the numbered news block alone reached 63 KB (~160 items) in a 4,096-token window — 11% of editions ran over-window that day and were silently truncated before the model read them. `apply_news_budget` (PACKET_NEWS_BUDGET_CHARS, 6,000 ≈ 15 items) now keeps the newest evidence and NAMES the cut (A5, budget_truncated_ids) like every other cut. Prompt text unchanged — the budget is loader-side; versioned because the built prompt changes shape. // n19 — THE CARD-BUDGET PROMINENCE PASS (the 8B mini-tune, D-T55's two measured defects + Scott's card-surface brief, verbatim: "All of these outputs surface on a tarot card. So we need the total output to fit onto a card. The Journalist and Insider will have multiple entries in most cases, so it should factor that in."). The edition budget and the card_score calibration move into a numbered SHIPS block (the s9/s12/or8 promotion treatment — a rule that lives mid-list and is measured by nothing is advice, not a contract), and their buried mid-list duplicates are deleted (the s12 rule). Rule 1 grounds the budget in the CARD itself: one tarot card, storylines share it, budgets are per-CARD. Rule 2 pins score-to-edition consistency: the 8B filed a 52-sentence edition (total_sentences_le caught it) and separately scored a busy cycle card_score 1 (card_score_ge caught it) — both defects are the same failure of the verdict matching the filing. Storyline rules, JSON contract, worked example, and register untouched. // n18 — THE ELOQUENCE PASS (Scott: the voice of one of The Athletic's dedicated team writers — takes pride in the craft, relishes telling the story, understands the facts ARE the story; and cite which publications are contributing). Voice paragraph rewritten to that register; publications credited IN PROSE (the n17-era "no source lists" prohibition became "credited in prose, never dumped as a list"); arc voicing in the writer's own words, never a pasted label (the before-probe showed "The arc is NEW" verbatim in a body); a worked example carries the register (the ep6 lesson). Gate grew sources_any + total_sentences_max BEFORE the edit. n17 — THE SEPARATION PASS: the transfer-heat input section is gone (Scott: transfers and vibe/emotional are completely separate seats now). The Insider owns transfer truth end-to-end; the Journalist files transfer stories from the corpus like any other story, and heat left the input_hash components, so heat movement alone no longer re-triggers the stage. The voice line now names the seat what it is: the seasoned writer on the developing narratives around the entity. n16 — THE ASSIGNMENT-DESK PASS: `article_buckets` is gone. The Journalist labelled every corpus article transfer/non-transfer as the tail of its generation, which made its output scale with the CORPUS instead of with the story — measured, the prose of a full six-storyline generation never passed 887 tokens while the generation reached 2,567. Labelling is sorting work; it belongs to The Editor, which already emits `story_type`, reads the FULL body rather than a 900-byte blurb of it, and runs on the card with headroom. What is left here is the job: voice the developing story. n15 was the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. The Journalist's eight sentences are a TOTAL edition budget across all storylines, not per storyline. n14: the peer-length pass — each storyline body grows from 1-2 to 5-6 sentences. The old ceiling was a 1070 Ti budget, not an editorial choice; the Journalist is a peer with an equal share of the story and now has the column inches to file it. n13: prefer Editor evidence cards when present; n12: the Journalist's card_score (tarot deck) — required 1-99 busyness verdict after the storylines

/// The JSON schema Ollama's constrained decoding enforces on the narratives reply (Phase 5).
/// Grammar-level guarantees the free-text contract could only ask for: the top-level object
/// cannot be prose-wrapped, `narratives` must exist, and every item carries title/body/articles.
/// n12 adds required `card_score` — the Journalist's 1-99 busyness verdict — ordered AFTER
/// narratives (sigil doctrine: read the signs first, land the verdict second).
/// The tolerant balanced-brace salvager stays as the parse path — schema output is a strict
/// subset of what it accepts, and it remains the safety net for the offline/parity bins.
///
/// **n16 removes `article_buckets`.** It was one object per corpus article — the largest term in
/// this generation by far, and the reason the output grew with the CORPUS rather than with the
/// story. Measured before removal: the prose of a full six-storyline generation never exceeded 887
/// tokens, while the generation itself reached 2,567. The Editor now writes
/// `news_articles.bucket` from the `story_type` it already emits, off the saturated host and from
/// the full body rather than a 900-byte blurb of it.
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
            "headline": { "type": "string" },
            "card_score": { "type": "integer", "minimum": 1, "maximum": 99 }
        },
        "required": ["narratives", "headline", "card_score"]
    })
}

/// build_narratives_prompt assembles the user prompt, byte-for-byte the same as Go's
/// `buildNarrativesPrompt` while `full_text` is NULL (the current state) and no `score_context`
/// is given. The `—` (U+2014) bytes are significant. (n17: the heat section is gone —
/// see the module note.)
/// `packet_framing` (7.3) is the storyline block. It is `Some` on every production call now that
/// the packet rail is the only rail; the `None` arm is what kept this byte-identical to the
/// pre-Phase-7 prompt across the cutover, and it is retained for the fixtures and tests that
/// pin that shape — not because a second rail can still select it.
/// `score_context` (n12) is the pre-rendered SIGNALS line + prior-card-reads memory block that
/// grounds the card score — rendered last, just before the reply instruction, so the verdict
/// lands after the signs are read. Like the relational memory it is prompt-only: deliberately
/// NOT part of the input_hash (the score always moves; hashing it would self-trigger).
pub fn build_narratives_prompt(
    req: &NarrativesReq,
    news: &[CorpusItem],
    memory: Option<&str>,
    score_context: Option<&str>,
    packet_framing: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {} ({} {})\n",
        req.entity_name, req.sport, req.entity_type
    ));
    // The storyline framing (packet rail only, 7.3) — what story this is, this entity's part in
    // it, and one line of what the prior packet said. `None` under RAIL=legacy, so the legacy
    // prompt is byte-identical to the pre-Phase-7 binary.
    if let Some(f) = packet_framing.filter(|f| !f.trim().is_empty()) {
        b.push_str("\nThe story so far (assembled by the desk from every source below):\n");
        b.push_str(f.trim_end());
        b.push('\n');
    }
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

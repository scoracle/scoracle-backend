//! # THE READER — the pipeline's sole relevance judge
//!
//! The only junction that reads the publisher's body text, and therefore the only one that ever
//! answers "is this article actually about this entity?" from evidence rather than inference.
//!
//! | | |
//! |---|---|
//! | **Seat** | `gemma3:4b` — extraction, not voice (2026-07-25) |
//! | **Contract** | `ar6` — describes the page; relevance is DERIVED, never asked |
//! | **Reads** | the fetched publisher page |
//! | **Feeds** | The Journalist (`narratives`), via `news_article_readings` |
//! | **Budget** | top-K per entity by Google's `feed_rank` (mig 194) |
//!
//! ## Authority
//!
//! Its verdict is final and nothing downstream re-litigates it. It can reject a whole article
//! after fetching (mig 190), which clears the sport's vetted links so The Journalist never sees the
//! false positive. It also promotes or rejects Go's co-mention candidates from the full text,
//! so co-mention verdicts come from the body rather than a headline.
//!
//! That authority was consolidated here on 2026-07-25. Before then a GPU gate in `scrub` ruled on
//! relevance from the headline alone; it was measured rejecting ~1 in 3 links at a rate
//! UNCORRELATED with relevance, and was deleted rather than repaired. Upstream of The Reader there
//! is now no opinion at all — Go records only whether the entity is *named* in the text, which is a
//! fact, and Google decides what order to offer things in.
//!
//! ## Why this seat is not a voice
//!
//! The character junctions (The Journalist, The Oracle, The Insider, The Influencer, The Analyst,
//! The Scout) are tuned for voice, and deviations from the doctrine model have to re-earn their
//! place. The Reader is different in kind: it extracts a summary and a set of verdicts from text
//! that already exists. Nothing it writes is read aloud. That is why it can sit on a smaller,
//! faster model than the characters without the same gate — and why it should, since it is the
//! pipeline's throughput bottleneck and every article it cannot reach falls back to its headline.
//!
//! ## The budget it lives under
//!
//! Ingest admits ~6,700 articles/day; this junction sustains far less. An article earns a call by
//! being in the top K Google returned for at least one entity it is linked to; everything else
//! reaches The Journalist on its headline alone. Reading is an upgrade applied to the material that
//! most deserves it, never a precondition for carrying it.

use crate::junctions::reader::{ArticleReadEntities, ArticleRow, CoMentionCandidate, ARTICLE_MAX_MODEL_CHARS};
use crate::util::truncate;

/// The Reader's contract version. Bumping this invalidates every cached reading whose
/// `prompt_version` differs, so a re-read is forced on the next pass. Invalidation is LAZY — an
/// article re-reads when it is next enqueued, not all at once.
///
/// **ar4 (2026-07-26) — the verdict moved to LAST.** ar3 asked for `relevant` as the first field
/// of the reply, so the model rendered its verdict before writing a single word of analysis;
/// measured, gemma3:4b accepted 99.1% of articles against mistral's 82.5% on the identical
/// prompt. ar4 reorders the contract (`story_type` → facts → card → `relevant`) so the judgment
/// is conditioned on text the model has already committed to, drops ar3's "already-relevance-
/// vetted" framing — which told the model the question was settled and then asked it anyway —
/// and promotes the measured rejection classes out of a 12-item bullet list.
///
/// **ar5 (2026-07-26) — classify, then derive.** ar4's reorder alone was MEASURED NEUTRAL
/// (13/16 both ways): with the verdict last, gemma wrote `story_type:"score"` and eight lines of
/// stat-table facts and *still* answered `relevant:true`. The reasoning was on the line above the
/// verdict and went unused, which refutes "it cannot reason before answering".
///
/// What ar4 got wrong was mine: it named the reject classes in prose but never made them values
/// of `story_type` and never stated the mapping, so the model still had to re-derive a bare
/// boolean — the one thing it reliably fails. ar5 makes the reject classes first-class enum
/// values, grammar-enforced in `article_read_format_schema`, and states the verdict as a lookup
/// from the classification rather than a judgment.
///
/// ar5 was refuted too, and most sharply: with the classes grammar-enforced, gemma labelled the
/// boxscore `score_stub` and the broadcast page `broadcast_listing` — the exact reject classes,
/// mapping stated directly above, classification emitted first — and still said `relevant:true`.
/// Deriving the verdict in the parser took fixtures 13/16 → 15/16, but production told a second
/// story: `story_type` collapsed to the `general` catch-all on **84% of reads**, so no reject
/// class ever fired and the live rate stayed at 0.0%.
///
/// **ar6 (2026-07-26) — describe the page; the system decides.** `relevant` is GONE from the
/// schema: the model is never given the question. It answers two extractive ones instead —
/// `page_kind` (what shape is this page) and `entity_roles` (what part each vetted entity plays)
/// — and `derive_relevance` computes the verdict. `story_type` reverts to pure topic, because
/// overloading one field with "what shape" and "what about" is what produced the `general`
/// collapse. `entity_roles` is also the first mechanism that can reach an opponent-only story,
/// which no page-shape signal ever catches.
pub const ARTICLE_READ_PROMPT_VERSION: &str = "ar6";

pub fn build_article_read_prompt(
    article: &ArticleRow,
    text: &str,
    entities: &ArticleReadEntities,
) -> String {
    build_article_read_prompt_parts(
        &article.source,
        &article.title,
        &article.description,
        text,
        &entities.vetted_names,
        &entities.co_mentions,
    )
}

/// The same builder over plain data, so a fixture generator outside the crate can render the EXACT
/// live prompt without a database row (the `build_graph_prompt` shape). `build_article_read_prompt`
/// is a thin adapter onto this, so the two cannot drift.
pub fn build_article_read_prompt_parts(
    source: &str,
    title: &str,
    description: &str,
    text: &str,
    vetted_names: &[String],
    co_mentions: &[CoMentionCandidate],
) -> String {
    let mut p = String::new();
    p.push_str(&format!("Source: {source}\n"));
    p.push_str(&format!("Title: {title}\n"));
    if !description.trim().is_empty() {
        p.push_str(&format!("RSS description: {description}\n"));
    }
    if !vetted_names.is_empty() {
        p.push_str("\nKnown vetted entities:\n");
        for e in vetted_names {
            p.push_str("- ");
            p.push_str(e);
            p.push('\n');
        }
    }
    if !co_mentions.is_empty() {
        p.push_str("\nCo-mention candidates to verify from full text:\n");
        for c in co_mentions {
            p.push_str(&format!(
                "{}. {} ({} {}, {})\n",
                c.number,
                c.name,
                c.entity_type,
                c.entity_id,
                co_mention_identity(c)
            ));
        }
    }
    p.push_str("\nArticle text:\n");
    p.push_str(&truncate(&normalize_space(text), ARTICLE_MAX_MODEL_CHARS));
    p.push_str("\n\nReturn the JSON object now.");
    p
}
fn co_mention_identity(c: &CoMentionCandidate) -> String {
    let mut parts = Vec::new();
    if !c.position.is_empty() {
        parts.push(c.position.as_str());
    }
    if !c.current_club.is_empty() {
        parts.push(c.current_club.as_str());
    }
    if !c.nationality.is_empty() {
        parts.push(c.nationality.as_str());
    }
    if parts.is_empty() {
        "no identity card".to_string()
    } else {
        parts.join(", ")
    }
}
/// normalize_space collapses the fetched body's whitespace so the prompt spends its character
/// budget on words rather than the publisher's markup residue.
fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

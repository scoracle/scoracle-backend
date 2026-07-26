//! # THE READER — the pipeline's sole relevance judge
//!
//! The only junction that reads the publisher's body text, and therefore the only one that ever
//! answers "is this article actually about this entity?" from evidence rather than inference.
//!
//! | | |
//! |---|---|
//! | **Seat** | `gemma3:4b` — extraction, not voice (2026-07-25) |
//! | **Contract** | `ar3` |
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

use crate::reader::{ArticleReadEntities, ArticleRow, CoMentionCandidate, ARTICLE_MAX_MODEL_CHARS};
use crate::util::truncate;

/// The Reader's contract version. Bumping this invalidates every cached reading whose
/// `prompt_version` differs, so a re-read is forced on the next pass.
pub const ARTICLE_READ_PROMPT_VERSION: &str = "ar3";

pub fn build_article_read_prompt(
    article: &ArticleRow,
    text: &str,
    entities: &ArticleReadEntities,
) -> String {
    let mut p = String::new();
    p.push_str(&format!("Source: {}\n", article.source));
    p.push_str(&format!("Title: {}\n", article.title));
    if !article.description.trim().is_empty() {
        p.push_str(&format!("RSS description: {}\n", article.description));
    }
    if !entities.vetted_names.is_empty() {
        p.push_str("\nKnown vetted entities:\n");
        for e in &entities.vetted_names {
            p.push_str("- ");
            p.push_str(e);
            p.push('\n');
        }
    }
    if !entities.co_mentions.is_empty() {
        p.push_str("\nCo-mention candidates to verify from full text:\n");
        for c in &entities.co_mentions {
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

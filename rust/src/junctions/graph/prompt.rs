//! # The graph extractor — typed relations, no persona
//!
//! This junction calls a model, but it is NOT a character and has no seat at the table. It reads
//! one article and returns typed structure: relations among already-vetted entities, and people
//! named in the text who are not yet in the entity world. Nothing it writes is read aloud, so
//! there is no voice here to tune and none should be invented.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::EmotionalNews` — a route, not an identity |
//! | **Contract** | `g3` |
//! | **Reads** | one article (The Reader's summary first, when there is one) plus its vetted entities |
//! | **Feeds** | `narrative_events` and `narrative_persons` candidates — the relational memory the characters later read |
//!
//! ## Authority — deliberately almost none
//!
//! **CLOSED CANDIDATE LIST.** The model never resolves free-text entity names. It picks subjects
//! and objects BY NUMBER from the vetted list it was given. That single constraint is what removes
//! entity resolution from this stage's risk surface entirely: the resolving already happened
//! upstream, and the only novel names the model may emit are person discoveries — which land as
//! `narrative_persons` CANDIDATES under evidence-gated promotion, never as direct entityhood.
//!
//! It was reordered to run BEHIND The Reader on 2026-07-25 (mig 193) so it extracts from the body
//! summary rather than the headline when one exists.
//!
//! ## Fail closed
//!
//! An unparseable reply is `None` and the caller writes nothing. Individually invalid relations or
//! persons are dropped, never repaired — though a partial salvage of the valid entries from a
//! valid JSON body is allowed, mirroring the scrub parser's out-of-range index handling.

use super::GraphCandidate;

pub const GRAPH_SYSTEM_PROMPT: &str = r#"Task: extract structured narrative relations from one sports article.

You are given the article and a NUMBERED list of known entities the article is about (already verified). Extract:

Language handling: the article title/text may be in English, Spanish, French, German, Italian, Portuguese, Dutch, or another language. Read it in the source language and translate meaning internally. Output JSON keys/enums exactly as specified in English. Any generated prose/string explanation must be English, while person names and proper names stay as written in the article. Do not drop relations or persons because the article is non-English.

1. "relations": relations the article STATES OR CLEARLY IMPLIES between listed entities, or about one listed entity alone. Use entity NUMBERS only. Allowed predicates: trade_rumor, trade_confirmed, injury, contract_dispute, praise, criticism.
   - subject: entity number (required). object: the number of the OTHER listed entity the relation is actually WITH -- for a transfer, the club the player is joining or leaving per THIS article, not just any club mentioned. Use null ONLY when no listed entity is the counterparty.
   - sentiment: -1.0 (very negative for the subject) to 1.0 (very positive).
   - confidence: "speculative" (unsourced/rumored), "reported" (attributed to a source), "confirmed" (official/announced).
   - Extract only what the text supports. No relation is the correct output for many articles.

2. "persons": EVERY person named in the article text who is NOT on the entity list and is not a player. Head coaches and managers named in the text ALWAYS belong here (e.g. a manager mentioned in a transfer story). So do agents, sporting directors/executives, and family members. Use their name exactly as written.
   - kind: coach | agent | executive | family | other.
   - team_context: the number of the listed TEAM they are tied to, or null.
   Never list players here; never list people not named in the text. An empty persons list is WRONG whenever a coach or manager is named in the text.

Return ONLY this JSON object, no commentary:
{"relations":[{"subject":1,"predicate":"trade_rumor","object":2,"sentiment":0.0,"confidence":"reported"}],"persons":[{"name":"...","kind":"coach","team_context":2}]}"#;

pub const GRAPH_PROMPT_VERSION: &str = "g3"; // g3: multilingual article handling + English-only generated strings; g2: person-extraction emphasis + object-attachment rule

/// build_graph_prompt lays out the article + numbered candidates (1-indexed, matching
/// the reply contract).
pub fn build_graph_prompt(
    source: &str,
    published: &str,
    title: &str,
    description: &str,
    candidates: &[GraphCandidate],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Article source: {source}\nPublished: {published}\n"
    ));
    b.push_str(&format!("Title: {title}\n"));
    if !description.trim().is_empty() {
        b.push_str(&format!("Text: {description}\n"));
    }
    b.push_str("\nKnown entities (use these numbers):\n");
    for (i, c) in candidates.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, c.descriptor));
    }
    b.push_str("\nReturn the JSON now.");
    b
}

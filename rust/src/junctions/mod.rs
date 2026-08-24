//! # The junctions — every seat in this pipeline that calls a model.
//!
//! A *junction* is a named seat with an identity, an authority, and a versioned contract. Six of
//! them are **characters**: they are tuned for voice, their words reach the reader, and a deviation
//! from the doctrine model has to re-earn its place. [`graph`] calls a model too, but it is typed
//! extraction with no persona and no seat at the table.
//!
//! **The legacy reader is gone.** `article_reader` (stage `article_read`, contract `ar7`) held the
//! relevance judgment on the old rail; it was demolished in Phase 9 (9.1) once `RAIL=packet` made
//! [`editor`] the sole reader. Two of its functions outlived it inside [`journalist`] — they had
//! become part of the narratives debounce key — and its `num_ctx` anchor moved to
//! `route::LOCAL_STAGE_NUM_CTX`.
//!
//! | junction | module | seat | contract (see each `prompt.rs` — versions rot in copies) |
//! |---|---|---|---|
//! | **The Editor** | [`editor`] | the rail's one reader (relevance, evidence, routing) | `*_PROMPT_VERSION` in `editor/prompt.rs` |
//! | **The Investigator** | [`investigator`] | verification: box scores + entity discovery (no memory, no voice) | `investigator/…` |
//! | **The Journalist** | [`journalist`] | narrative memory + the card score | `journalist/prompt.rs` |
//! | **The Oracle** | [`oracle`] | the sigil card's voice — blind to memories since or9: five cards + omen, whole | `oracle/prompt.rs` |
//! | **The Insider** | [`insider`] | transfer/trade vetting | `insider/prompt.rs` |
//! | **The Influencer** | [`influencer`] | the felt read (SCORE/HOOK/VIBE) | `influencer/prompt.rs` |
//! | **The Analyst** | [`analyst`] | momentum — speaks "the form"/"the mood", never product names (s15) | `analyst/prompt.rs` |
//! | **The Scout** | [`scout`] | stat commentary — `divined_peak` is code-owned since s18 | `scout/prompt.rs` |
//! | *(not a character)* | [`graph`] | typed entity extraction | `graph/…` |
//!
//! (This table stopped copying version strings on 2026-08-10: it had rotted six-for-six —
//! `ep1/or5/n13/v14/momentum-s7/s14` against live `ep6/or9/n19/v18/momentum-s15/s18` — and a
//! roster that lies about versions is worse than one that names where the truth lives.)
//!
//! ## THE CARD CONTRACT — score + headline + body, every consumer seat (Scott, 2026-08-24)
//!
//! Every direct-to-consumer character's product is one card face with exactly three parts:
//!
//! - **score** — the seat's own number, top-middle of the card. Its scale is the seat's
//!   (higher-is-better for some, mid-is-quiet for others) and each seat's prompt owns the
//!   calibration.
//! - **headline** — the tweet hook: 140 characters, `guards::settle_title` is the mechanical
//!   floor (a junk title costs the title, never the card). It is the seat's read of the WHOLE
//!   entity-cycle, not an item title promoted — a busy day of narratives is the Journalist's
//!   headline theme; a quiet wire is a legitimate Insider headline, not a missing one.
//! - **body** — the seat's voice under the fit-on-card constraint, `guards::clean_served_prose`
//!   as the floor.
//!
//! Where each triple lives:
//!
//! | seat | score | headline | body |
//! |---|---|---|---|
//! | The Scout | rating | `stat_summaries.headline` (mig 226) | `stat_summaries` body |
//! | The Journalist | `news_summaries.card_score` | `news_summaries.headline` (mig 232, generation-level) | the storylines |
//! | The Insider | `insider_scores.score` | `insider_scores.headline` (mig 232) | `insider_scores.read` + the rumor rows |
//! | The Influencer | `vibe_scores.sentiment` | `vibe_scores.hook` | `vibe_scores.prompt` |
//! | The Analyst | momentum score | `momentum_summaries.headline` (mig 226) | `momentum_summaries` blurb |
//! | The Oracle | crown score | `sigil_synthesis.headline` (mig 226) | the reading |
//!
//! Per-item titles (`narrative_title`, the rumor `model_summary`) are NOT the headline — they
//! are body furniture. The card-level hook is entity-level, one per generation.
//!
//! ## What lives here, and what does not
//!
//! Each junction owns one directory: `mod.rs` holds the machinery — claiming work, loading inputs,
//! calling the model, parsing, persisting — and `prompt.rs` holds the contract with the model, that
//! junction's `*_PROMPT_VERSION` and its prompt builder, and nothing else.
//!
//! That split exists because a prompt change *is* a behaviour change and should read like one.
//! These contracts used to sit buried mid-file inside 2,000-line stage modules, which made the most
//! consequential text in the system the hardest to find and the hardest to diff. Now changing what
//! a character is asked is a one-file diff, and `git log src/junctions/oracle/prompt.rs` is the
//! honest history of The Oracle's voice.
//!
//! Each `mod.rs` re-exports its prompt module's public items, so call sites outside this tree never
//! need to know which of the two files a symbol lives in.
//!
//! Infrastructure (`harness`, `route`, `ollama`, `work`, `worker`, `stage`, `db`, `config`,
//! `ledger`, `embed`) and shared primitives (`threads`, `trajectory`, `bucket`, `corpus`, `util`)
//! stay at the crate root. (`novelty` was the scrub gate's and died with it in Phase 9.)
//! A junction may depend on those; nothing there may depend on a junction.

pub mod analyst;
pub mod editor;
pub mod graph;
pub mod influencer;
pub mod insider;
pub mod investigator;
pub mod journalist;
pub mod oracle;
pub mod scout;

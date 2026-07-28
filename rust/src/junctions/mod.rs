//! # The junctions — every seat in this pipeline that calls a model.
//!
//! A *junction* is a named seat with an identity, an authority, and a versioned contract. Six of
//! them are **characters**: they are tuned for voice, their words reach the reader, and a deviation
//! from the doctrine model has to re-earn its place. The seventh, [`editor`], is extraction rather
//! than voice. [`graph`] calls a model too, but it is typed extraction with no persona and no seat
//! at the table.
//!
//! | junction | module | seat | contract |
//! |---|---|---|---|
//! | **The Editor** | [`editor`] | sole relevance judge | `ar6` |
//! | **The Journalist** | [`journalist`] | narrative memory | `n13` |
//! | **The Oracle** | [`oracle`] | the sigil card's voice | `or5` |
//! | **The Insider** | [`insider`] | transfer/trade vetting | `t11` + `is1` |
//! | **The Influencer** | [`influencer`] | emotional/news rail | `v14` |
//! | **The Analyst** | [`analyst`] | momentum | `momentum-s7` |
//! | **The Scout** | [`scout`] | stat commentary | `s14` |
//! | *(not a character)* | [`graph`] | typed entity extraction | `g3` |
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
//! `ledger`, `embed`) and shared primitives (`novelty`, `threads`, `trajectory`, `bucket`,
//! `corpus`, `util`) stay at the crate root. A junction may depend on those; nothing there may
//! depend on a junction.

pub mod analyst;
pub mod editor;
pub mod graph;
pub mod influencer;
pub mod insider;
pub mod journalist;
pub mod oracle;
pub mod scout;

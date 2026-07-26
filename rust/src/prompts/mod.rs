//! Prompts — one file per LLM junction, each holding that junction's contract with its model.
//!
//! Every stage in this pipeline that calls a model is a *junction*: a named seat with an identity,
//! an authority, and a versioned contract. Those contracts used to live buried mid-file inside
//! 1,500-line stage modules, which made the most consequential text in the system the hardest to
//! find and the hardest to diff. A prompt change is a behaviour change; it should read like one.
//!
//! Each file here owns exactly one junction's `*_PROMPT_VERSION` and its prompt builder, so
//! changing what a character is asked shows up as a one-file diff. The stage module keeps the
//! machinery — claiming work, fetching, parsing, persisting — and re-exports the prompt so call
//! sites are unchanged.
//!
//! | junction | file | seat | version |
//! |---|---|---|---|
//! | **The Reader** | [`reader`] | extraction, not voice | `ar3` |
//! | **The Journalist** | `journalist` | narrative memory | `n13` |
//! | **The Oracle** | `oracle` | the sigil card's voice | `or5` |
//! | **The Insider** | `insider` | transfer/trade vetting | `t11` |
//! | **The Influencer** | `influencer` | emotional/news rail | `v14` |
//! | **The Analyst** | `analyst` | momentum | `momentum-s7` |
//! | **The Scout** | `scout` | stat commentary | `s14` |
//!
//! Rows without a linked module have not been migrated yet — the stage file still owns that
//! prompt. Move them here rather than adding a second home for prompts.

pub mod reader;

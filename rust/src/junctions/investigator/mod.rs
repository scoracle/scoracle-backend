//! The Investigator verifies entities and public-event facts.
//!
//! The Editor nominates; the Investigator verifies; search discovers; sources prove. This
//! junction owns demand-led box-score and entity acquisition.
//!
//! Structured claims are interpreted in code. The prose fallback asks the model for
//! verbatim observations, then code verifies and decides. Every accepted fact cites a
//! `source_documents` row; the Investigator writes facts and provenance, never memories.

pub mod boxscore;
pub mod discover;
pub mod entity;
pub mod gate;
pub mod prompt;
#[cfg(test)]
mod tests;

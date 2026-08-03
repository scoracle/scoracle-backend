//! # The Investigator — the verification junction (PLAN-one-rail Phases 4–5).
//!
//! The Editor nominates; the Investigator verifies; search discovers; sources prove. This
//! junction owns the demand-led acquisition rail: box scores first (Phase 4, [`boxscore`] —
//! stage `fixture_boxscore`, a live wire name that predates the character and does NOT rename),
//! entity discovery second (Phase 5, stage `investigate_entity`).
//!
//! `Role::Investigator` rides the pinned `gemma3:4b` on archbox (§3 — `MAX_LOADED_MODELS=1`
//! makes that a hardware constraint, not a style choice). Its only v1 model calls are
//! describe-only page triage: a model may describe an unfamiliar page layout, but numbers
//! enter rows through DOM/JSON parsers alone, and every accepted fact cites a
//! `source_documents` row. The Investigator writes facts and provenance, never memories —
//! statelessness is the objectivity guarantee (§4).

pub mod boxscore;
pub mod discover;
pub mod entity;
pub mod gate;
#[cfg(test)]
mod tests;

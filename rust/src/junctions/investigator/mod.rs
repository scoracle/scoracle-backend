//! # The Investigator — the verification junction (PLAN-one-rail Phases 4–5).
//!
//! The Editor nominates; the Investigator verifies; search discovers; sources prove. This
//! junction owns the demand-led acquisition rail: box scores first (Phase 4, [`boxscore`] —
//! stage `fixture_boxscore`, a live wire name that predates the character and does NOT rename),
//! entity discovery second (Phase 5, stage `investigate_entity`).
//!
//! `Role::Investigator` is pinned to `ministral-3:14b` on the Mac host
//! (`COGNITION_ROUTE_INVESTIGATOR`, 2026-08-09 — deliberately NOT the archbox 3B). The seat
//! idles in v1: every live decision is code over Wikidata's structured claims, zero model
//! calls. The seat's first real load is 5.4's deferred prose-triage arm (D-T8's class — names
//! Wikimedia knows under a different legal name), and the design rule for it is already fixed
//! (PLAN-one-rail T2 note): the model QUOTES the occupation phrase verbatim, code matches it
//! against sport vocabulary and thresholds on independent-source count — never self-reported
//! confidence. Numbers enter rows through DOM/JSON parsers alone, and every accepted fact
//! cites a `source_documents` row. The Investigator writes facts and provenance, never
//! memories — statelessness is the objectivity guarantee (§4).

pub mod boxscore;
pub mod discover;
pub mod entity;
pub mod gate;
#[cfg(test)]
mod tests;

//! Scoracle Studio runs capture and character skills through the shared harness.
//! Each skill owns its evidence, output contract and publication policy.
//! Offline evaluation uses the same request builders as production.

/// Offline evaluation and editorial review tools.
pub mod evaluation;
/// Durable execution, skill packages, model transports and publication.
#[path = "runtime.rs"]
pub mod harness;
/// Shared identity, source acquisition, form decoding and prose validation.
pub mod tools;
/// Pure text, rounding and fingerprint helpers.
pub mod util;

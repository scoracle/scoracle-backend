//! Durable work coordination and exact-claim publication obligations.

mod coverage;
#[cfg(test)]
#[path = "../../tests/internal/queue/coverage_tests.rs"]
mod coverage_tests;
pub mod outbox;
pub(crate) mod publication;
pub mod work;
pub mod worker;

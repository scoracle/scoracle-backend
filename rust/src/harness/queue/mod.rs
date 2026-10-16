//! Durable work coordination and exact-claim publication obligations.

mod coverage;
#[cfg(test)]
mod coverage_tests;
pub mod outbox;
pub(crate) mod publication;
pub mod work;
pub mod worker;

//! Durable work coordination and exact-claim publication obligations.

pub mod outbox;
pub(crate) mod publication;
pub mod work;
pub mod worker;

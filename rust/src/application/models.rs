//! Application-selected model routes and generation limits. No storage or queue access.
use crate::runtime::route::Router;
use std::time::Duration;

pub struct Models {
    pub router: Router,
    /// Cooperative ceiling for multi-call assignments; zero means unbounded.
    pub handler_budget: Duration,
    pub voice_num_ctx: i32,
}

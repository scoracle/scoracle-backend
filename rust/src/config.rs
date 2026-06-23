//! Environment configuration. Variable names mirror the Go backend
//! (`go/internal/config/config.go`) so the Rust scrubber and the Go API read
//! the same `.env.local`. DB URL precedence matches Go: DATABASE_PRIVATE_URL
//! wins over DATABASE_URL.

use anyhow::{anyhow, Result};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub db_max_conns: u32,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout: Duration,
    /// Periodic drain even without a NOTIFY (Go worker default: 30s).
    pub safety_net: Duration,
    /// A 'running' row idle longer than this is recovered to 'pending'.
    /// TODO(phase-1): align with the Go `derive.StaleLease` value before any
    /// shared-queue run, or the two workers can disagree on what's stale.
    pub stale_lease: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env_opt("DATABASE_PRIVATE_URL")
            .or_else(|| env_opt("DATABASE_URL"))
            .ok_or_else(|| anyhow!("DATABASE_PRIVATE_URL or DATABASE_URL must be set"))?;

        Ok(Self {
            database_url,
            db_max_conns: env_int("SCRUBBER_DB_MAX_CONNS", 5) as u32,
            ollama_base_url: env_or("OLLAMA_BASE_URL", "http://localhost:11434"),
            ollama_model: env_or("OLLAMA_MODEL", "gemma4:e4b"),
            ollama_timeout: Duration::from_secs(env_int("OLLAMA_TIMEOUT_SECONDS", 60) as u64),
            safety_net: Duration::from_secs(env_int("SCRUBBER_SAFETY_NET_SECONDS", 30) as u64),
            stale_lease: Duration::from_secs(env_int("SCRUBBER_STALE_LEASE_SECONDS", 600) as u64),
        })
    }
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

fn env_int(key: &str, default: i64) -> i64 {
    env_opt(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

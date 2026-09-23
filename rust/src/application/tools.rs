//! Application-provided concrete workspace capabilities for Studio plugins.
//!
//! Studio defines the tool vocabulary and validates each plugin's declared reach. This
//! adapter supplies the actual HTTP, provenance, and database machinery once per process;
//! it is shared by every plugin that needs web access and is never constructed by a plugin.

use crate::evidence::fetch::{BudgetedFetchError, BudgetedFetcher, FetchPolicy, SourceFetch};
use crate::studio::plugin::PluginManifest;
use crate::studio::tools::{DomainClass, ToolRefusal};
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};

/// One recorded tool call. Provider failures remain distinguishable from model failures.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub tool: &'static str,
    pub url: String,
    pub outcome: &'static str,
    pub elapsed_ms: u64,
}

/// Per-run call ledger. The application decides whether to log or persist it; the workspace
/// guarantees that every attempted provider reach is recorded.
#[derive(Default)]
pub struct ToolLedger {
    calls: std::sync::Mutex<Vec<ToolCall>>,
}

impl ToolLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, call: ToolCall) {
        self.calls.lock().expect("tool ledger poisoned").push(call);
    }

    pub fn calls(&self) -> Vec<ToolCall> {
        self.calls.lock().expect("tool ledger poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.calls.lock().expect("tool ledger poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The process-wide web workspace. One `BudgetedFetcher` owns domain spacing, circuits,
/// Retry-After holds, caching, and source-document provenance for the entire fleet.
pub struct WebBroker {
    fetcher: BudgetedFetcher,
    /// Per-run ceiling on brokered calls. Zero means unlimited; handler timeout remains the
    /// outer bound.
    max_calls: u32,
}

impl WebBroker {
    pub fn new(max_calls: u32) -> Result<Self> {
        Ok(Self {
            fetcher: BudgetedFetcher::new()?,
            max_calls,
        })
    }

    /// Open one run's web scope. Reach is derived exclusively from the plugin manifest.
    pub fn scope<'a>(
        &'a self,
        pool: &'a PgPool,
        plugin: &'a PluginManifest,
        ledger: &'a ToolLedger,
    ) -> ScopedWeb<'a> {
        ScopedWeb {
            broker: self,
            pool,
            plugin,
            ledger,
            calls: AtomicU64::new(0),
        }
    }
}

/// One plugin run's access to the shared workspace.
pub struct ScopedWeb<'a> {
    broker: &'a WebBroker,
    pool: &'a PgPool,
    plugin: &'a PluginManifest,
    ledger: &'a ToolLedger,
    calls: AtomicU64,
}

impl<'a> ScopedWeb<'a> {
    fn check_grant(&self, class: DomainClass, url: &str) -> Result<()> {
        if !self.plugin.grants_web(class) {
            return Err(anyhow!(
                "{}",
                ToolRefusal::UndeclaredDomain {
                    url: url.to_string(),
                    class: class.as_str(),
                }
            ));
        }
        Ok(())
    }

    fn check_budget(&self) -> Result<()> {
        if self.broker.max_calls > 0 {
            let used = self.calls.fetch_add(1, Ordering::Relaxed);
            if used >= u64::from(self.broker.max_calls) {
                return Err(anyhow!(
                    "tool broker: run exceeded its web-call budget ({} calls)",
                    self.broker.max_calls
                ));
            }
        }
        Ok(())
    }

    /// Fetch a Wikimedia URL under the shared workspace. The class is derived from its URL,
    /// so a Wikimedia grant cannot be spent on another host.
    pub async fn fetch_wikimedia(&self, url: &str, policy: &FetchPolicy) -> Result<SourceFetch> {
        let class = DomainClass::domain_of_url(url).ok_or_else(|| {
            anyhow!(
                "{}",
                ToolRefusal::UnknownDomain {
                    url: url.to_string(),
                }
            )
        })?;
        self.fetch_for_class(class, url, policy).await
    }

    /// Fetch an application-registered URL class. Source registry attribution is input to this
    /// adapter; the manifest gate remains the authority over whether the plugin may use it.
    pub async fn fetch_for_class(
        &self,
        class: DomainClass,
        url: &str,
        policy: &FetchPolicy,
    ) -> Result<SourceFetch> {
        self.check_grant(class, url)?;
        self.check_budget()?;
        let started = std::time::Instant::now();
        let result = self.broker.fetcher.fetch(self.pool, url, policy).await;
        let elapsed = started.elapsed();
        let outcome = match &result {
            Ok(f) if f.from_cache => "cache_hit",
            Ok(_) => "fetched",
            Err(BudgetedFetchError::DomainSkipped { .. }) => "domain_skipped",
            Err(BudgetedFetchError::Http { .. }) => "http_rejected",
            Err(BudgetedFetchError::Other(_)) => "transport_failed",
        };
        self.ledger.record(ToolCall {
            tool: "web.fetch",
            url: url.to_string(),
            outcome,
            elapsed_ms: elapsed.as_millis() as u64,
        });
        result.map_err(|e| anyhow!("{e}"))
    }

    /// Fetch one already-curated article through the Editor's existing retrieval path.
    /// The provider implementation is intentionally unchanged; this boundary adds the
    /// manifest gate, run budget, and call ledger around it.
    pub async fn fetch_curated_article(
        &self,
        url: &str,
    ) -> Result<crate::evidence::fetch::FetchedArticle> {
        self.check_grant(DomainClass::CuratedArticles, url)?;
        self.check_budget()?;
        let started = std::time::Instant::now();
        let result = crate::evidence::fetch::fetch_article(url).await;
        self.ledger.record(ToolCall {
            tool: "web.fetch_curated_article",
            url: url.to_string(),
            outcome: if result.is_ok() {
                "fetched"
            } else {
                "transport_failed"
            },
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
        result
    }
}

#[cfg(test)]
mod tests;

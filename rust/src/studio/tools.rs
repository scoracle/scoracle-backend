//! Studio's tool vocabulary and refusal policy.
//!
//! A plugin brings its own *tools* — its readers, its source policies, its domain
//! knowledge — but never its own *workspace*. The workspace is the room's: one shared
//! budgeted fetcher with per-domain spacing, circuit breaking, Retry-After holds, and
//! provenance. The broker is the boundary between the two:
//!
//! - **Deny by default.** A plugin may call only the tool classes and domain classes its
//!   manifest declares. An undeclared reach is a hard error, not a warning.
//! - **Preparation-class.** Every tool in this slice runs *before* inference, inside a
//!   context recipe. A model never chooses to browse mid-read: input hashes stay
//!   computable before the call, and the debounce economy survives.
//! - **Recorded.** Every brokered call lands in the run's call ledger with its outcome,
//!   so provider failures stay distinguishable from model failures.
//!
//! Concrete workspace adapters live in `application::tools`: they receive this policy plus
//! application-owned storage and provider dependencies. Studio itself remains independent of
//! SQL, HTTP, queues, and provider implementations.

/// A class of external domain a plugin may fetch. Deny-by-default: a plugin may fetch
/// only the classes its manifest declares, and a class grants *sources within that
/// class*, never "the web".
///
/// Adding a class is adding a vocabulary entry plus its routing in `domain_of_url`;
/// a class is not a permission to invent endpoints at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainClass {
    /// Wikipedia/Wikidata API surfaces (`wikipedia.org`, `wikidata.org`).
    Wikimedia,
    /// The registered RSS/news corpus the Editor's funnel curates.
    NewsRss,
    /// The registered box-score sources configured in `boxscore_sources`.
    BoxscoreSources,
    /// Article bodies behind already-curated URLs (the scoped re-read path for the
    /// news characters). Not a license to follow arbitrary links.
    CuratedArticles,
}

impl DomainClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DomainClass::Wikimedia => "wikimedia",
            DomainClass::NewsRss => "news_rss",
            DomainClass::BoxscoreSources => "boxscore_sources",
            DomainClass::CuratedArticles => "curated_articles",
        }
    }

    /// Route a URL to its domain class. `None` means the URL belongs to no declared
    /// class — the broker refuses it regardless of grants.
    pub fn domain_of_url(url: &str) -> Option<DomainClass> {
        let host = host_of(url)?;
        if is_under(host, "wikidata.org")
            || is_under(host, "wikipedia.org")
            || is_under(host, "wikimedia.org")
        {
            return Some(DomainClass::Wikimedia);
        }
        // Box-score and RSS sources are registered in configuration, not guessed from
        // the URL; the workspace callers know their source's class and pass it. URL
        // sniffing beyond Wikimedia would let a curated URL masquerade as another class.
        None
    }
}

/// Exact domain or subdomain — `notwikidata.org` is NOT under `wikidata.org`.
fn is_under(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

/// Why the broker refused a call. Distinct from a fetch failure: the reach never
/// happened, and the plugin's manifest is the thing to fix.
#[derive(Debug, PartialEq, Eq)]
pub enum ToolRefusal {
    /// The plugin's manifest does not declare this tool class.
    UndeclaredTool { tool: &'static str },
    /// The URL's domain class is not among the plugin's declared web domains.
    UndeclaredDomain { url: String, class: &'static str },
    /// The URL belongs to no declared domain class at all.
    UnknownDomain { url: String },
}

impl std::fmt::Display for ToolRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolRefusal::UndeclaredTool { tool } => {
                write!(f, "tool broker: plugin does not declare tool '{tool}'")
            }
            ToolRefusal::UndeclaredDomain { url, class } => {
                write!(
                    f,
                    "tool broker: domain class '{class}' not granted for {url}"
                )
            }
            ToolRefusal::UnknownDomain { url } => {
                write!(f, "tool broker: {url} belongs to no declared domain class")
            }
        }
    }
}

#[cfg(test)]
mod tests;

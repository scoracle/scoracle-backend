//! Shared article fetcher — resolve, fetch, and clean one publisher page.
//!
//! Extracted from the legacy reader junction (`junctions/article_reader`, formerly `editor/`) in
//! Phase 3.1 of PLAN-one-rail so the greenfield Editor and the legacy `article_read` stage fetch
//! through ONE implementation: Google-News wrapper resolution, the HTTP fetch, the
//! headless-Chrome fallback, and HTML-to-text cleaning. Behavior is identical to the code it was
//! cut from; both stages call these functions.
//!
//! Infrastructure rule (see `junctions/mod.rs`): junctions may depend on this module; this module
//! must not depend on any junction.

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::process::Command;
use std::time::Duration;
use tracing::warn;

const ARTICLE_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// The floor under which a fetched body is not worth a model call — and the threshold that
/// triggers the headless-Chrome fallback inside [`fetch_article`].
pub const ARTICLE_MIN_WORDS: usize = 80;
const ARTICLE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; ScoracleBot/1.0; +https://scoracle.com)";
const GOOGLE_NEWS_BATCH_URL: &str =
    "https://news.google.com/_/DotsSplashUi/data/batchexecute?rpcids=Fbv4je";

#[derive(Debug)]
pub struct FetchedArticle {
    pub final_url: String,
    pub final_domain: Option<String>,
    pub text: String,
}

pub async fn fetch_article(raw_url: &str) -> Result<FetchedArticle> {
    let client = reqwest::Client::builder()
        .timeout(ARTICLE_FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(ARTICLE_FETCH_USER_AGENT)
        .build()
        .context("build article fetch client")?;

    let fetch_url = match resolve_google_news_article_url(&client, raw_url).await {
        Ok(Some(resolved)) => resolved,
        Ok(None) => raw_url.to_string(),
        Err(e) => {
            warn!(url = raw_url, error = %format!("{e:#}"), "google news url resolution failed");
            raw_url.to_string()
        }
    };

    let resp = client
        .get(&fetch_url)
        .send()
        .await
        .context("fetch article")?;
    let final_url = resp.url().to_string();
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(anyhow!("article HTTP {}", status.as_u16()));
    }
    if !status.is_success() {
        return Err(anyhow!("article HTTP {}", status.as_u16()));
    }
    let html = resp.text().await.context("read article body")?;
    let mut text = extract_article_text(&html);
    if count_words(&text) < ARTICLE_MIN_WORDS {
        if let Some(rendered) = fetch_with_chrome(&fetch_url) {
            let rendered_text = extract_article_text(&rendered);
            if count_words(&rendered_text) > count_words(&text) {
                text = rendered_text;
            }
        }
    }
    Ok(FetchedArticle {
        final_domain: domain_of(&final_url),
        final_url,
        text,
    })
}

fn fetch_with_chrome(raw_url: &str) -> Option<String> {
    if std::env::var("ARTICLE_READ_CHROME_ENABLED").ok().as_deref() != Some("1") {
        return None;
    }
    let output = Command::new("timeout")
        .arg("20s")
        .arg("google-chrome-stable")
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--dump-dom")
        .arg(raw_url)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub fn content_hash(text: &str) -> String {
    let digest = Sha256::digest(normalize_space(text).as_bytes());
    hex::encode(&digest[..16])
}

pub fn domain_of(raw_url: &str) -> Option<String> {
    reqwest::Url::parse(raw_url).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_lowercase())
    })
}

async fn resolve_google_news_article_url(
    client: &reqwest::Client,
    raw_url: &str,
) -> Result<Option<String>> {
    let Some(article_id) = google_news_article_id(raw_url) else {
        return Ok(None);
    };

    let html = client
        .get(raw_url)
        .send()
        .await
        .context("fetch google news wrapper")?
        .text()
        .await
        .context("read google news wrapper")?;
    let resolved_id = html_attr(&html, "data-n-a-id").unwrap_or(article_id);
    let Some(timestamp) = html_attr(&html, "data-n-a-ts").and_then(|v| v.parse::<i64>().ok())
    else {
        return Ok(None);
    };
    let Some(signature) = html_attr(&html, "data-n-a-sg") else {
        return Ok(None);
    };
    let payload = google_news_resolve_payload(&resolved_id, timestamp, &signature);
    let body = client
        .post(GOOGLE_NEWS_BATCH_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded;charset=utf-8",
        )
        .form(&[("f.req", payload)])
        .send()
        .await
        .context("post google news resolver")?
        .text()
        .await
        .context("read google news resolver")?;
    Ok(parse_google_news_resolver_response(&body))
}

fn google_news_article_id(raw_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw_url).ok()?;
    if url.host_str()? != "news.google.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    if segments.next()? != "rss" || segments.next()? != "articles" {
        return None;
    }
    let id = segments.next()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn html_attr(html: &str, attr: &str) -> Option<String> {
    let needle = format!(r#"{attr}=""#);
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find('"')?;
    Some(decode_entities(&html[start..start + end]))
}

fn google_news_resolve_payload(article_id: &str, timestamp: i64, signature: &str) -> String {
    let request = json!([
        "garturlreq",
        [
            [
                "en-US",
                "US",
                [
                    "FINANCE_TOP_INDICES",
                    "GENESIS_PUBLISHER_SECTION",
                    "WEB_TEST_1_0_0"
                ],
                null,
                null,
                1,
                1,
                "US:en",
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                0,
                5
            ],
            "en-US",
            "US",
            1,
            [2, 3, 4, 8],
            1,
            0,
            "655000234",
            0,
            0,
            null,
            0
        ],
        article_id,
        timestamp,
        signature
    ])
    .to_string();
    json!([[["Fbv4je", request, null, "generic"]]]).to_string()
}

fn parse_google_news_resolver_response(body: &str) -> Option<String> {
    let start = body.find("[[")?;
    let outer: serde_json::Value = serde_json::from_str(&body[start..]).ok()?;
    for row in outer.as_array()? {
        let row = row.as_array()?;
        if row.first()?.as_str()? != "wrb.fr" || row.get(1)?.as_str()? != "Fbv4je" {
            continue;
        }
        let inner: serde_json::Value = serde_json::from_str(row.get(2)?.as_str()?).ok()?;
        let inner = inner.as_array()?;
        if inner.first()?.as_str()? != "garturlres" {
            continue;
        }
        let url = inner.get(1)?.as_str()?.trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(url.to_string());
        }
    }
    None
}

pub fn count_words(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.len() > 1).count()
}

pub fn looks_paywalled(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("subscribe")
        || lower.contains("subscription")
        || lower.contains("sign in")
        || lower.contains("sign up")
        || lower.contains("register to continue")
}

/// Elements whose text is SITE CHROME, never article prose: navigation, promo rails, footers,
/// cookie forms, share widgets. Stripped whole, tag and contents together.
///
/// `header`/`footer` are in the list even though an `<article>` may carry its own: the headline
/// and byline reach the model as `Title` in the user prompt already, so losing them costs nothing
/// and dropping page furniture is worth far more.
const NON_CONTENT_TAGS: &[&str] = &[
    "script", "style", "nav", "header", "footer", "aside", "form", "noscript", "svg", "iframe",
    "button", "select", "textarea", "template", "figure",
];

/// extract_article_text pulls the READING BODY out of a publisher page.
///
/// **Why this exists, measured 2026-08-09.** [`clean_html`] strips `<script>`/`<style>` and then
/// removes tags while keeping EVERY remaining text node — so the model was handed the whole page.
/// A representative 7,922-char Editor prompt carried **~2,700 chars of article** inside ~5,200
/// chars of betting-site menus, a country list, "Related To This Article", "Popular News", and the
/// publisher's street address. **Two thirds of the article budget was site furniture**, on a runner
/// whose window D-T40 showed was already overflowing on the common case.
///
/// Two passes, cheapest first:
/// 1. delete every [`NON_CONTENT_TAGS`] element, contents included;
/// 2. prefer the LARGEST `<article>` element, then `<main>`. Largest matters because related-post
///    cards are themselves `<article>` on most CMS themes — the real story is the big one.
///
/// ⛔ **It can only ever SHRINK the body, so it fails safe:** any result under
/// [`ARTICLE_MIN_WORDS`] is discarded and the caller keeps the full-page text, which is exactly
/// today's behaviour. A site this cannot parse is no worse off than before.
pub fn extract_article_text(html: &str) -> String {
    let mut doc = html.to_string();
    for tag in NON_CONTENT_TAGS {
        doc = strip_element_blocks(&doc, tag);
    }
    for tag in ["article", "main"] {
        if let Some(inner) = largest_element_inner(&doc, tag) {
            let text = clean_html(&inner);
            if count_words(&text) >= ARTICLE_MIN_WORDS {
                return text;
            }
        }
    }
    clean_html(&doc)
}

/// largest_element_inner returns the inner HTML of the LONGEST `<tag>…</tag>` span, nested spans
/// included as candidates (a related-post `<article>` inside the main one is a real layout).
fn largest_element_inner(html: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut best: Option<&str> = None;
    let mut pos = 0usize;
    while let Some(start) = find_ascii_ci(html, &open, pos) {
        let Some(gt) = html[start..].find('>').map(|i| start + i + 1) else {
            break;
        };
        let Some(end) = find_ascii_ci(html, &close, gt) else {
            break;
        };
        let inner = &html[gt..end];
        if best.is_none_or(|b| inner.len() > b.len()) {
            best = Some(inner);
        }
        pos = gt;
    }
    best.map(str::to_string)
}

pub fn clean_html(html: &str) -> String {
    let without_scripts = strip_element_blocks(html, "script");
    let without_styles = strip_element_blocks(&without_scripts, "style");
    let mut out = String::with_capacity(without_styles.len());
    let mut in_tag = false;
    for c in without_styles.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&normalize_space(&out))
}

/// Case-insensitive ASCII search for `needle` in `haystack`, starting at byte offset `from`
/// and returning an offset into `haystack` itself.
///
/// This exists because the obvious version — search a `to_lowercase()` copy, then index the
/// original with the result — is only sound while lowercasing preserves byte length, and
/// Unicode does not guarantee that. `İ` (U+0130, 2 bytes) lowercases to `i̇` (U+0069 U+0307,
/// 3 bytes), so every offset past the first one drifts by a byte. A Galatasaray match report
/// with 11 of them made the lowercase copy 11 bytes longer than the original and panicked the
/// whole harness on `&html[pos..]` (2026-07-26, `start byte index 1040186 is out of bounds for
/// string of length 1040175`).
///
/// HTML tag names are ASCII, so ASCII-case-insensitive matching is both sufficient here and
/// length-preserving by construction. Every returned offset points at an ASCII byte, which is
/// always a char boundary in UTF-8 — so the slices built from it cannot panic either.
fn find_ascii_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || from > h.len() || h.len() - from < n.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn strip_element_blocks(html: &str, tag: &str) -> String {
    let mut out = String::new();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut pos = 0usize;
    while let Some(start) = find_ascii_ci(html, &open, pos) {
        out.push_str(&html[pos..start]);
        // An unclosed block swallows the rest of the document, as before: better to drop a
        // trailing tail than to emit raw script source as article text.
        match find_ascii_ci(html, &close, start) {
            Some(end) => pos = end + close.len(),
            None => {
                pos = html.len();
                break;
            }
        }
    }
    out.push_str(&html[pos..]);
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

pub fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// The budgeted fetcher (PLAN-one-rail 4.2) — the Investigator's polite front door.
//
// Shared substrate: founded in Phase 4 (box scores), reused by Phase 5 (entity discovery).
// Every retrieval the Investigator makes goes through ONE `BudgetedFetcher`, which enforces,
// per domain: concurrency 1, a minimum spacing between requests, 429/Retry-After respected
// as a hold, and a circuit breaker after repeated failures. A domain that blocks direct
// fetch is a domain we skip — never stealth, no browser automation on this path.
//
// Every successful fetch is recorded as a NEW `source_documents` row (provenance is a point
// in time, not a link — mig 205); within `cache_ttl` a same-URL row is reused instead of
// re-fetching, so replays and retries do not hammer sources.
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// How many consecutive failures on one domain open its circuit.
const CIRCUIT_FAILURE_THRESHOLD: u32 = 4;
/// How long an opened circuit stays open before the domain may be tried again.
const CIRCUIT_OPEN: Duration = Duration::from_secs(15 * 60);
/// Longest Retry-After hold we will honor; anything larger is treated as this.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(300);
/// Provenance excerpt bound for `source_documents.retained_excerpt` (chars). Large enough
/// that a JSON payload or article body read back within `cache_ttl` is intact in practice;
/// bounded so provenance can never grow a row without limit.
const RETAINED_EXCERPT_MAX_CHARS: usize = 100_000;

/// Per-domain budget knobs, loaded from `boxscore_sources.fetch_policy` (or defaults).
#[derive(Clone, Debug)]
pub struct FetchPolicy {
    /// Minimum spacing between two requests to one domain. Floor 2s (the 4.2 law) is
    /// applied in `new`, so a config cannot accidentally go faster.
    pub min_spacing: Duration,
    /// Reuse window: a `source_documents` row for the same URL younger than this is
    /// returned instead of re-fetching. Zero disables reuse (always fetch).
    pub cache_ttl: Duration,
}

impl FetchPolicy {
    pub fn new(min_spacing: Duration, cache_ttl: Duration) -> Self {
        Self {
            min_spacing: min_spacing.max(Duration::from_secs(2)),
            cache_ttl,
        }
    }
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self::new(Duration::from_secs(2), Duration::from_secs(6 * 3600))
    }
}

/// One retrieved document: the `source_documents` provenance row id plus the body the
/// caller parses. `from_cache` distinguishes a reused row from a live fetch.
#[derive(Debug)]
pub struct SourceFetch {
    pub document_id: i64,
    pub url: String,
    pub final_url: String,
    pub domain: Option<String>,
    pub http_status: u16,
    pub content_hash: String,
    pub body: String,
    pub from_cache: bool,
}

/// Why a budgeted fetch did not produce a document. `DomainSkipped` is a first-class
/// outcome, not an error to retry: the circuit is open or the domain told us to hold.
#[derive(Debug)]
pub enum BudgetedFetchError {
    /// Circuit open or Retry-After hold in force; contains when the domain may be retried.
    DomainSkipped { domain: String, until_secs: u64 },
    /// HTTP-level rejection (non-2xx). 429 also arms a hold on the domain.
    Http { status: u16, final_url: String },
    /// Transport / build / DB failure.
    Other(anyhow::Error),
}

impl std::fmt::Display for BudgetedFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DomainSkipped { domain, until_secs } => {
                write!(f, "domain {domain} skipped (retry in {until_secs}s)")
            }
            Self::Http { status, final_url } => write!(f, "HTTP {status} from {final_url}"),
            Self::Other(e) => write!(f, "{e:#}"),
        }
    }
}

#[derive(Debug, Default)]
struct DomainState {
    last_request_at: Option<Instant>,
    consecutive_failures: u32,
    /// Circuit-open or Retry-After hold: no requests to this domain before this instant.
    hold_until: Option<Instant>,
}

impl DomainState {
    /// Seconds until this domain may be tried, if it is currently held.
    fn held_for(&self, now: Instant) -> Option<u64> {
        self.hold_until
            .filter(|until| *until > now)
            .map(|until| (until - now).as_secs().max(1))
    }

    /// How long a caller must wait to honor min_spacing (0 if free).
    fn spacing_wait(&self, now: Instant, min_spacing: Duration) -> Duration {
        match self.last_request_at {
            Some(last) if now < last + min_spacing => (last + min_spacing) - now,
            _ => Duration::ZERO,
        }
    }

    fn record_success(&mut self, now: Instant) {
        self.last_request_at = Some(now);
        self.consecutive_failures = 0;
        self.hold_until = None;
    }

    fn record_failure(&mut self, now: Instant, retry_after: Option<Duration>) {
        self.last_request_at = Some(now);
        self.consecutive_failures += 1;
        let mut hold = retry_after.map(|d| d.min(MAX_RETRY_AFTER));
        if self.consecutive_failures >= CIRCUIT_FAILURE_THRESHOLD {
            hold = Some(hold.unwrap_or(Duration::ZERO).max(CIRCUIT_OPEN));
        }
        if let Some(hold) = hold {
            self.hold_until = Some(now + hold);
        }
    }
}

/// The per-process budget ledger. One instance per worker (the handler owns it); the
/// per-domain async mutex is what makes "concurrency 1 per domain" structural — a second
/// task on the same domain queues on the lock rather than racing the spacing check.
pub struct BudgetedFetcher {
    client: reqwest::Client,
    domains: Mutex<HashMap<String, std::sync::Arc<tokio::sync::Mutex<DomainState>>>>,
}

impl BudgetedFetcher {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(ARTICLE_FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(ARTICLE_FETCH_USER_AGENT)
            .build()
            .context("build budgeted fetch client")?;
        Ok(Self {
            client,
            domains: Mutex::new(HashMap::new()),
        })
    }

    fn domain_state(&self, domain: &str) -> std::sync::Arc<tokio::sync::Mutex<DomainState>> {
        let mut map = self.domains.lock().expect("domain budget map poisoned");
        std::sync::Arc::clone(map.entry(domain.to_string()).or_default())
    }

    /// Fetch one URL under the domain budget, recording provenance into `source_documents`.
    ///
    /// Order of operations: cache probe (no budget spent on a hit) → hold/circuit check →
    /// spacing sleep → GET → provenance insert. A non-2xx records a failure against the
    /// domain (429 additionally arms its Retry-After hold) and returns `Http`; transport
    /// errors count against the circuit too.
    pub async fn fetch(
        &self,
        pool: &sqlx::PgPool,
        url: &str,
        policy: &FetchPolicy,
    ) -> std::result::Result<SourceFetch, BudgetedFetchError> {
        let domain = domain_of(url).unwrap_or_else(|| "unknown".to_string());

        if policy.cache_ttl > Duration::ZERO {
            if let Some(hit) = self
                .cached(pool, url, policy.cache_ttl)
                .await
                .map_err(BudgetedFetchError::Other)?
            {
                return Ok(hit);
            }
        }

        let state = self.domain_state(&domain);
        let mut state = state.lock().await;

        let now = Instant::now();
        if let Some(until_secs) = state.held_for(now) {
            return Err(BudgetedFetchError::DomainSkipped {
                domain,
                until_secs,
            });
        }
        let wait = state.spacing_wait(now, policy.min_spacing);
        if wait > Duration::ZERO {
            tokio::time::sleep(wait).await;
        }

        let sent_at = Instant::now();
        let resp = match self.client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                state.record_failure(sent_at, None);
                return Err(BudgetedFetchError::Other(
                    anyhow!(e).context(format!("budgeted fetch {url}")),
                ));
            }
        };
        let final_url = resp.url().to_string();
        let status = resp.status();
        if !status.is_success() {
            let retry_after = (status.as_u16() == 429)
                .then(|| parse_retry_after(resp.headers()))
                .flatten();
            state.record_failure(sent_at, retry_after);
            return Err(BudgetedFetchError::Http {
                status: status.as_u16(),
                final_url,
            });
        }

        let headers = provenance_headers(resp.headers());
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => {
                state.record_failure(sent_at, None);
                return Err(BudgetedFetchError::Other(
                    anyhow!(e).context(format!("read budgeted fetch body {url}")),
                ));
            }
        };
        state.record_success(sent_at);
        drop(state);

        let hash = content_hash(&body);
        let title = html_title(&body);
        let excerpt = bounded_excerpt(&body, RETAINED_EXCERPT_MAX_CHARS);
        let document_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO public.source_documents
                (url, final_url, domain, content_hash, http_status, title, retained_excerpt, headers)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb)
            RETURNING id
            "#,
        )
        .bind(url)
        .bind(&final_url)
        .bind(domain_of(&final_url))
        .bind(&hash)
        .bind(status.as_u16() as i32)
        .bind(title.as_deref())
        .bind(&excerpt)
        .bind(&headers)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            BudgetedFetchError::Other(anyhow!(e).context("insert source_documents row"))
        })?;

        Ok(SourceFetch {
            document_id,
            url: url.to_string(),
            final_url: final_url.clone(),
            domain: domain_of(&final_url),
            http_status: status.as_u16(),
            content_hash: hash,
            body,
            from_cache: false,
        })
    }

    async fn cached(
        &self,
        pool: &sqlx::PgPool,
        url: &str,
        ttl: Duration,
    ) -> Result<Option<SourceFetch>> {
        let row = sqlx::query(
            r#"
            SELECT id, final_url, domain, content_hash, http_status, retained_excerpt
            FROM public.source_documents
            WHERE url = $1
              AND fetched_at > NOW() - make_interval(secs => $2)
              AND http_status BETWEEN 200 AND 299
            ORDER BY fetched_at DESC
            LIMIT 1
            "#,
        )
        .bind(url)
        .bind(ttl.as_secs_f64())
        .fetch_optional(pool)
        .await
        .context("probe source_documents cache")?;
        let Some(row) = row else { return Ok(None) };
        let final_url: Option<String> = row.get("final_url");
        let final_url = final_url.unwrap_or_else(|| url.to_string());
        Ok(Some(SourceFetch {
            document_id: row.get::<i64, _>("id"),
            url: url.to_string(),
            final_url,
            domain: row.get("domain"),
            http_status: row.get::<Option<i32>, _>("http_status").unwrap_or(200) as u16,
            content_hash: row
                .get::<Option<String>, _>("content_hash")
                .unwrap_or_default(),
            body: row
                .get::<Option<String>, _>("retained_excerpt")
                .unwrap_or_default(),
            from_cache: true,
        }))
    }
}

/// Retry-After per RFC 9110. delay-seconds is honored exactly; the HTTP-date form is rare
/// enough that it takes a flat 60s hold rather than a date-parsing dependency.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    match raw.trim().parse::<u64>() {
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => Some(Duration::from_secs(60)),
    }
}

/// The provenance subset of response headers worth keeping (mig 205 `headers jsonb`).
fn provenance_headers(headers: &reqwest::header::HeaderMap) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for key in ["content-type", "etag", "last-modified", "cache-control"] {
        if let Some(v) = headers.get(key).and_then(|v| v.to_str().ok()) {
            obj.insert(key.to_string(), json!(v));
        }
    }
    serde_json::Value::Object(obj)
}

fn html_title(body: &str) -> Option<String> {
    let start = find_ascii_ci(body, "<title", 0)?;
    let open_end = body[start..].find('>')? + start + 1;
    let close = find_ascii_ci(body, "</title>", open_end)?;
    let title = normalize_space(&decode_entities(&body[open_end..close]));
    if title.is_empty() {
        None
    } else {
        Some(truncate_chars(&title, 500))
    }
}

/// Char-boundary-safe excerpt bound (a byte slice could split a UTF-8 char).
fn bounded_excerpt(body: &str, max_chars: usize) -> String {
    truncate_chars(body, max_chars)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_html_removes_tags_scripts_and_normalizes_space() {
        let html = "<html><script>bad()</script><body><h1>Title</h1><p>A&nbsp;B &amp; C.</p></body></html>";
        assert_eq!(clean_html(html), "Title A B & C.");
    }

    /// Regression for the 2026-07-26 harness panic: a page whose text lowercases to a *longer*
    /// byte string than the original. `İ` (U+0130, 2 bytes) becomes `i̇` (3 bytes), so the old
    /// search-the-lowercase-copy-then-index-the-original approach drifted one byte per occurrence
    /// and eventually sliced past the end of `html`. A Galatasaray report with 11 of them took the
    /// whole cognition service down with `start byte index 1040186 is out of bounds for string of
    /// length 1040175`.
    ///
    /// The tag being stripped is deliberately placed *after* the drifting characters, since that is
    /// the only arrangement in which the offsets have diverged by the time they are used.
    #[test]
    fn clean_html_survives_text_whose_lowercase_is_longer() {
        let turkish = "İstanbul İzmir İnönü İlkay İsmail İbrahim İdris İlhan İnan İpek İrem";
        assert!(
            turkish.to_lowercase().len() > turkish.len(),
            "precondition: this text must expand when lowercased"
        );

        let html = format!("<p>{turkish}</p><script>bad()</script><p>Tail.</p>");
        let cleaned = clean_html(&html);

        assert!(!cleaned.contains("bad()"), "script block must still be stripped");
        assert!(cleaned.contains("Tail."), "content after the script must survive");
        assert!(cleaned.contains("İstanbul"), "original casing must be preserved");
    }

    /// Tag matching is case-insensitive, and must stay so now that it no longer goes through
    /// `to_lowercase`.
    #[test]
    fn clean_html_strips_uppercase_tags() {
        assert_eq!(clean_html("<P>A</P><SCRIPT>bad()</SCRIPT><P>B</P>"), "A B");
    }

    /// An unclosed block swallows the remainder — preserved from the previous implementation.
    #[test]
    fn clean_html_drops_tail_of_unclosed_script() {
        assert_eq!(clean_html("<p>Kept</p><script>oops"), "Kept");
    }

    #[test]
    fn google_news_article_id_extracts_rss_token() {
        let url = "https://news.google.com/rss/articles/CBMiabc123?oc=5&hl=en-US";
        assert_eq!(google_news_article_id(url).as_deref(), Some("CBMiabc123"));
        assert!(google_news_article_id("https://example.com/rss/articles/CBMiabc123").is_none());
    }

    #[test]
    fn html_attr_extracts_google_news_tokens() {
        let html =
            r#"<div data-n-a-id="CBMiabc" data-n-a-ts="1784915408" data-n-a-sg="A&amp;B"></div>"#;
        assert_eq!(html_attr(html, "data-n-a-id").as_deref(), Some("CBMiabc"));
        assert_eq!(
            html_attr(html, "data-n-a-ts").as_deref(),
            Some("1784915408")
        );
        assert_eq!(html_attr(html, "data-n-a-sg").as_deref(), Some("A&B"));
    }

    // --- Budgeted fetcher (4.2): the pure budget mechanics, no network -------------------

    #[test]
    fn fetch_policy_floors_spacing_at_two_seconds() {
        let p = FetchPolicy::new(Duration::from_millis(100), Duration::ZERO);
        assert_eq!(p.min_spacing, Duration::from_secs(2));
        let p = FetchPolicy::new(Duration::from_secs(5), Duration::ZERO);
        assert_eq!(p.min_spacing, Duration::from_secs(5));
    }

    #[test]
    fn domain_state_enforces_spacing() {
        let mut s = DomainState::default();
        let t0 = Instant::now();
        assert_eq!(s.spacing_wait(t0, Duration::from_secs(2)), Duration::ZERO);
        s.record_success(t0);
        let wait = s.spacing_wait(t0 + Duration::from_millis(500), Duration::from_secs(2));
        assert_eq!(wait, Duration::from_millis(1500));
        assert_eq!(
            s.spacing_wait(t0 + Duration::from_secs(3), Duration::from_secs(2)),
            Duration::ZERO
        );
    }

    #[test]
    fn circuit_opens_after_repeated_failures_and_success_resets() {
        let mut s = DomainState::default();
        let t0 = Instant::now();
        for _ in 0..CIRCUIT_FAILURE_THRESHOLD - 1 {
            s.record_failure(t0, None);
        }
        assert!(s.held_for(t0).is_none(), "below threshold: no hold");
        s.record_failure(t0, None);
        let held = s.held_for(t0).expect("circuit must open at threshold");
        assert!(held >= CIRCUIT_OPEN.as_secs() - 1);
        // A success (e.g. after the hold expires) closes everything.
        s.record_success(t0 + CIRCUIT_OPEN + Duration::from_secs(1));
        assert!(s.held_for(t0 + CIRCUIT_OPEN + Duration::from_secs(2)).is_none());
        assert_eq!(s.consecutive_failures, 0);
    }

    #[test]
    fn retry_after_arms_a_hold_and_is_capped() {
        let mut s = DomainState::default();
        let t0 = Instant::now();
        s.record_failure(t0, Some(Duration::from_secs(30)));
        let held = s.held_for(t0).unwrap();
        assert!((29..=30).contains(&held));
        // A huge Retry-After is capped at MAX_RETRY_AFTER.
        let mut s = DomainState::default();
        s.record_failure(t0, Some(Duration::from_secs(86_400)));
        assert!(s.held_for(t0).unwrap() <= MAX_RETRY_AFTER.as_secs());
    }

    #[test]
    fn retry_after_header_parses_seconds_and_defaults_dates() {
        let mut h = reqwest::header::HeaderMap::new();
        assert!(parse_retry_after(&h).is_none());
        h.insert(reqwest::header::RETRY_AFTER, "17".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(17)));
        h.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(60)));
    }

    #[test]
    fn html_title_and_excerpt_are_bounded_and_boundary_safe() {
        let body = "<html><head><title> Real Madrid 2-1 Arsenal &amp; more </title></head>";
        assert_eq!(
            html_title(body).as_deref(),
            Some("Real Madrid 2-1 Arsenal & more")
        );
        assert!(html_title("<p>no title</p>").is_none());
        // Multibyte chars must not panic the bound.
        let s = "É".repeat(10);
        assert_eq!(truncate_chars(&s, 4).chars().count(), 4);
        assert_eq!(truncate_chars("short", 100), "short");
    }

    /// The measured 2026-08-09 case: a real page is ~2/3 site furniture, and the old whole-page
    /// clean handed all of it to the model. Pins BOTH halves of the contract — the furniture goes,
    /// and the prose survives intact.
    #[test]
    fn extract_article_text_drops_site_furniture_and_keeps_the_prose() {
        let prose = "Barcelona were beaten by Udinese in the final minute of the Friuli Venezia \
             Giulia Cup, and Hans Flick's side lacked penetration in the final third all evening \
             despite controlling possession for long spells against a deep Udinese block. ";
        let html = format!(
            "<html><head><style>.a{{}}</style></head><body>\
             <nav>Live Scores Betting Sites Megapari App Betpawa App Fortebet Ghana Nigeria</nav>\
             <header>Skip to content</header>\
             <main><article><p>{}</p></article>\
             <article><p>Related: Savinho set for Tottenham move</p></article></main>\
             <aside>Popular News Trending Riyad Mahrez weighs Saudi moves</aside>\
             <footer>Contact GSDS Sports Data Services Accra Ghana Privacy Policy</footer>\
             </body></html>",
            prose.repeat(3)
        );
        let extracted = extract_article_text(&html);
        assert!(
            extracted.contains("lacked penetration in the final third"),
            "the article prose must survive: {extracted}"
        );
        for furniture in [
            "Megapari",
            "Privacy Policy",
            "Popular News",
            "Skip to content",
            "Savinho",
        ] {
            assert!(
                !extracted.contains(furniture),
                "{furniture} is site furniture and must not reach the model: {extracted}"
            );
        }
        assert!(
            extracted.len() < clean_html(&html).len(),
            "extraction must shrink the body"
        );
    }

    /// Fails SAFE: a page the extractor cannot parse must come back whole, never empty — an
    /// over-aggressive strip that silently returned nothing would lose articles.
    #[test]
    fn extract_article_text_falls_back_to_the_whole_page_when_it_cannot_parse() {
        let body = "word ".repeat(ARTICLE_MIN_WORDS * 2);
        let html = format!("<html><body><div id=post>{body}</div></body></html>");
        let extracted = extract_article_text(&html);
        assert!(count_words(&extracted) >= ARTICLE_MIN_WORDS, "{extracted}");
    }

    /// A short `<article>` (a stub card) must not win over the real page body.
    #[test]
    fn extract_article_text_ignores_an_article_element_below_the_word_floor() {
        let body = "sentence about the match ".repeat(ARTICLE_MIN_WORDS);
        let html =
            format!("<html><body><article>too short</article><div>{body}</div></body></html>");
        assert!(count_words(&extract_article_text(&html)) >= ARTICLE_MIN_WORDS);
    }

    #[test]
    fn google_news_resolver_response_extracts_publisher_url() {
        let body = r#")]}'

[["wrb.fr","Fbv4je","[\"garturlres\",\"https://www.goal.com/en/news/example\",1]",null,null,null,"generic"],["di",23]]"#;
        assert_eq!(
            parse_google_news_resolver_response(body).as_deref(),
            Some("https://www.goal.com/en/news/example")
        );
    }
}

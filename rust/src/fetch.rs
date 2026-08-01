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
    let mut text = clean_html(&html);
    if count_words(&text) < ARTICLE_MIN_WORDS {
        if let Some(rendered) = fetch_with_chrome(&fetch_url) {
            let rendered_text = clean_html(&rendered);
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

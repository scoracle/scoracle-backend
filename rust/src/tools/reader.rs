//! One URL reader for acquisition and its retained source consumers.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::time::Duration;

pub const VERSION: &str = "jina-content-v1";

#[derive(Clone)]
pub struct Reader {
    endpoint: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct Article {
    pub url: String,
    pub title: String,
    #[serde(rename = "content")]
    pub body: String,
    #[serde(skip)]
    pub selector: String,
}

impl Reader {
    pub fn new(endpoint: &str) -> Result<Self> {
        let url = reqwest::Url::parse(endpoint)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "invalid reader endpoint"
        );
        Ok(Self {
            endpoint: format!("{}/", endpoint.trim_end_matches('/')),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }

    pub(crate) fn from_env() -> Result<Option<Self>> {
        std::env::var("ARTICLE_READER_URL")
            .ok()
            .map(|url| Self::new(&url))
            .transpose()
    }

    pub async fn read(&self, url: &str) -> Result<Article> {
        super::fetch::http::checked_url(url)?;
        for selector in ["article", "main"] {
            let response = self
                .client
                .get(format!("{}{url}", self.endpoint))
                .header("Accept", "application/json")
                .header("x-engine", "browser")
                .header("x-proxy-url", "http://127.0.0.1:8302")
                .header("x-target-selector", selector)
                .header("x-remove-overlay", "true")
                .header("x-detach-invisibles", "true")
                .header("x-retain-links", "text")
                .header("x-retain-images", "none")
                .header("x-retain-media", "none")
                .header("x-assert-status-code", "200")
                // Reject oversized output instead of silently dropping late corrections.
                .header("x-token-budget", "6500")
                .send()
                .await?;
            let http_error = response.error_for_status_ref().err();
            let body = super::fetch::http::read_body(response, 1024 * 1024).await?;
            if let Some(error) = http_error {
                let envelope: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                if selector == "article" && envelope["status"] == 42206 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(error.into());
            }
            let mut article = decode(&body)?;
            article.selector = selector.into();
            return Ok(article);
        }
        anyhow::bail!("reader found no semantic article body")
    }
}

fn decode(body: &str) -> Result<Article> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    ensure!(
        value["code"] == 200,
        "reader rejected publisher: {}",
        value["readableMessage"]
    );
    let mut article: Article =
        serde_json::from_value(value["data"].clone()).context("reader article")?;
    super::fetch::http::checked_url(&article.url)?;
    ensure!(
        !article.body.contains('\0')
            && super::fetch::count_words(&article.body) >= super::fetch::ARTICLE_MIN_WORDS,
        "reader did not return a usable article"
    );
    if !article.title.trim().is_empty() {
        article.body = format!("{}\n\n{}", article.title.trim(), article.body);
    }
    Ok(article)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn semantic_fallback_preserves_the_headline_and_late_correction() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let reader = Reader::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            for selector in ["article", "main"] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 4096];
                let n = stream.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                assert!(request.contains(&format!("x-target-selector: {selector}")));
                assert!(
                    request.contains("x-token-budget: 6500") && !request.contains("x-max-tokens")
                );
                let (status, body) = if selector == "article" {
                    (
                        "422 Unprocessable Entity",
                        r#"{"code":422,"status":42206}"#.to_string(),
                    )
                } else {
                    ("200 OK",serde_json::json!({"code":200,"data":{"url":"https://example.com/news","title":"Headline",
                        "content":format!("{} Late correction: available.","Full reporting. ".repeat(80))}}).to_string())
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let article = tokio::time::timeout(
            Duration::from_secs(10),
            reader.read("https://example.com/news"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(article.selector, "main");
        assert!(
            article.body.starts_with("Headline\n\n")
                && article.body.ends_with("Late correction: available.")
        );
        server.await.unwrap();
    }

    #[test]
    fn rejects_error_envelopes_empty_articles_and_private_targets() {
        for body in [
            r#"{"code":401,"data":null}"#,
            r#"{"code":200,"data":{"url":"https://example.com","title":"News","content":""}}"#,
        ] {
            assert!(decode(body).is_err());
        }
        let mut value = serde_json::json!({"code":200,"data":{"url":"https://example.com/news","title":"News","content":"Full reporting. ".repeat(80)}});
        assert!(decode(&value.to_string()).is_ok());
        value["data"]["url"] = serde_json::json!("http://127.0.0.1/private");
        assert!(decode(&value.to_string()).is_err());
        assert!(Reader::new("file:///tmp/reader").is_err());
    }
}

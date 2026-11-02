//! Score and prose form one card; only an explicit all-null reply abstains.
use crate::harness::Parser;
use crate::tools::form::{normalize_body, validate_body, validate_hook};
use anyhow::{ensure, Result};

pub struct VibeReply {
    pub score: i32,
    pub headline: String,
    pub body: String,
}
pub struct VibeParser;
impl Parser<VibeReply> for VibeParser {
    fn parse(&self, raw: &str) -> Result<Option<VibeReply>> {
        let value: serde_json::Value = serde_json::from_str(raw)?;
        let map = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Vibe must be an object"))?;
        ensure!(
            map.len() == 3
                && ["score", "headline", "body"]
                    .iter()
                    .all(|k| map.contains_key(*k)),
            "Vibe requires score, headline and body"
        );
        if map.values().all(serde_json::Value::is_null) {
            return Ok(None);
        }
        let score = value["score"]
            .as_i64()
            .filter(|n| (0..=100).contains(n))
            .ok_or_else(|| anyhow::anyhow!("Vibe score must be an integer from 0 to 100"))?
            as i32;
        let headline = value["headline"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Vibe headline missing"))?;
        let body = value["body"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Vibe body missing"))?;
        let headline = crate::util::strip_markdown_emphasis(headline);
        ensure!(
            !headline.contains(['\n', '\r']),
            "Vibe headline must be one line"
        );
        validate_hook(Some(&headline))?;
        let body = normalize_body(&crate::util::strip_markdown_emphasis(body));
        validate_body(&body)?;
        ensure!(
            body.chars().count() <= super::request::BODY_MAX_CHARS,
            "Influencer body exceeds the reading budget"
        );
        Ok(Some(VibeReply {
            score,
            headline,
            body,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_preserves_multiple_paragraphs_with_one_score_and_hook() {
        let paragraph = "Example Wire reports that supporters welcomed the news. ".repeat(12);
        let body = [paragraph.trim(); 3].join("\n\n");
        assert!(body.chars().count() > 1200);
        let raw =
            serde_json::json!({"body":body,"score":75,"headline":"Supporters welcome the news"});
        let reply = VibeParser.parse(&raw.to_string()).unwrap().unwrap();
        assert_eq!(reply.body, body);
        assert_eq!(reply.score, 75);
        assert_eq!(reply.headline, "Supporters welcome the news");
        let mut oversized = raw;
        oversized["body"] = "x".repeat(super::super::request::BODY_MAX_CHARS + 1).into();
        assert!(VibeParser.parse(&oversized.to_string()).is_err());
        assert_eq!(
            super::super::request::schema()["oneOf"][0]["properties"]["body"]["maxLength"],
            super::super::request::BODY_MAX_CHARS
        );
    }
}

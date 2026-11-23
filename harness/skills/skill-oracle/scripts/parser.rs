//! Decode the Oracle reading.
use crate::harness::Parser;
use anyhow::Result;

pub(crate) struct ReadingParser;

impl Parser<(String, String)> for ReadingParser {
    fn flagged_output(&self, raw: &str) -> Result<Option<(String, String)>> {
        let (body, headline) = crate::tools::form::flagged_prose(raw, "reading");
        Ok(Some((body, headline)))
    }

    fn parse(&self, raw: &str) -> Result<Option<(String, String)>> {
        let prose = super::request::prose();
        let map = crate::tools::form::parse_prose_map(raw, &prose.keys)?;
        let Some(reading) = map.get("reading") else {
            anyhow::ensure!(
                map.get("headline").is_none(),
                "headline supplied without card body"
            );
            return Ok(None);
        };
        let reading =
            crate::tools::guards::clean_served_prose(&crate::tools::form::normalize_body(reading));
        crate::tools::form::validate_body(&reading)?;
        let headline = crate::tools::form::card_hook(
            map.get("headline")
                .ok_or_else(|| anyhow::anyhow!("Oracle hook missing"))?,
        );
        Ok(Some((reading, headline)))
    }
}

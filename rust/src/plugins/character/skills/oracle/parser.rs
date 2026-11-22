//! Decode the Oracle reading.
use crate::harness::Parser;
use anyhow::Result;

pub(crate) struct ReadingParser;

impl Parser<String> for ReadingParser {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let prose = super::request::prose();
        let map = crate::tools::form::parse_prose_map(raw, &prose.keys)?;
        let Some(reading) = map.get("reading") else {
            return Ok(None);
        };
        let reading =
            crate::tools::guards::clean_served_prose(&crate::tools::form::normalize_body(reading));
        crate::tools::form::validate_body(&reading)?;
        Ok(Some(reading))
    }
}

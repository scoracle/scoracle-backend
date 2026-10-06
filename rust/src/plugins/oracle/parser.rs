//! Served reading decoding and prose guards.
use crate::studio::Parser;
use anyhow::{bail, Result};

pub(super) struct ReadingParser;

impl Parser<String> for ReadingParser {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let prose = super::prompt::prose();
        let map = crate::plugins::form::parse_prose_map(raw, &prose.keys, prose.dims)?;
        let Some(reading) = map.get("reading") else {
            return Ok(None);
        };
        let reading = crate::plugins::support::guards::clean_served_prose(
            &crate::plugins::form::normalize_body(reading),
        );
        crate::plugins::form::validate_body(&reading)?;
        if crate::plugins::support::guards::has_bookkeeping_citation(&reading)
            || crate::plugins::support::guards::first_product_name(&reading).is_some()
            || crate::plugins::support::guards::has_foreign_script(&reading)
        {
            bail!("crown: reading violates served prose guard");
        }
        Ok(Some(reading))
    }
}

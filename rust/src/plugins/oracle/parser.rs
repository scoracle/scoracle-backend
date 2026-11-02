//! Served reading decoding and prose guards.
use crate::harness::Parser;
use anyhow::{bail, Result};

pub(crate) struct ReadingParser;

impl Parser<String> for ReadingParser {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let prose = super::prompt::prose();
        let map = crate::tools::form::parse_prose_map(raw, &prose.keys)?;
        let Some(reading) = map.get("reading") else {
            return Ok(None);
        };
        let reading =
            crate::tools::guards::clean_served_prose(&crate::tools::form::normalize_body(reading));
        crate::tools::form::validate_body(&reading)?;
        if crate::tools::guards::has_bookkeeping_citation(&reading)
            || crate::tools::guards::first_product_name(&reading).is_some()
            || crate::tools::guards::has_foreign_script(&reading)
        {
            bail!("crown: reading violates served prose guard");
        }
        Ok(Some(reading))
    }
}

/// Evidence-aware parser shared by production and prepared-fixture qualification.
pub(crate) struct RequestParser<'a>(pub &'a str);
impl Parser<String> for RequestParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let Some(reply) = ReadingParser.parse(raw)? else {
            return Ok(None);
        };
        crate::tools::fresh::validate_peer_reading(&reply, self.0)?;
        let package: serde_json::Value = serde_json::from_str(self.0)?;
        let name = package["meta"]["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing Oracle subject"))?;
        anyhow::ensure!(
            crate::tools::guards::title_names_entity(&reply, name),
            "crown: reading does not name entity {name:?}"
        );
        Ok(Some(reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_enforces_the_same_subject_guard_as_publication() {
        let prompt = r#"{"meta":{"name":"Harbor"},"fresh":{"scout":{"body":"Harbor has a strong recorded profile."}}}"#;
        assert!(RequestParser(prompt)
            .parse(r#"{"reading":"The recorded profile is strong."}"#)
            .is_err());
        assert!(RequestParser(prompt)
            .parse(r#"{"reading":"Harbor has a strong recorded profile."}"#)
            .is_ok());
    }
}

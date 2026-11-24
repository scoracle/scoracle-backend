//! Strict prose decoding and served-product guards.
use crate::harness::Parser;
use anyhow::{Context, Result};

/// Parsed model prose. Direction and conviction are deterministic product fields.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The Analyst's read.
    pub blurb: String,
    pub headline: String,
}

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn flagged_output(&self, raw: &str) -> Result<Option<MomentumReply>> {
        let (body, headline) = crate::tools::form::flagged_prose(raw, "blurb");
        Ok(Some(MomentumReply {
            blurb: body,
            headline,
        }))
    }

    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        let prose = super::request::prose();
        let map = crate::tools::form::parse_prose_map(raw, &prose.keys)?;
        Ok(Some(MomentumReply {
            blurb: crate::tools::form::normalize_body(
                map.get("blurb").context("Analyst blurb missing")?,
            ),
            headline: crate::tools::form::card_hook(
                map.get("headline").context("Analyst headline missing")?,
            ),
        }))
    }
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    MomentumParser.parse(raw).ok().flatten()
}

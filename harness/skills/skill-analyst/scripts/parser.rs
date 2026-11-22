//! Strict prose decoding and served-product guards.
use crate::harness::Parser;
use anyhow::{anyhow, Result};

/// Parsed model prose. Direction and conviction are deterministic product fields.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The Analyst's read.
    pub blurb: String,
    pub headline: String,
}

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        // Keep a bounded raw excerpt so malformed output is diagnosable.
        let reply = parse_momentum_reply(raw).ok_or_else(|| {
            anyhow!(
                "momentum: invalid response (raw={:?})",
                crate::util::truncate_bytes(raw.trim(), 160)
            )
        })?;
        Ok(Some(reply))
    }
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    let prose = super::request::prose();
    let map = crate::tools::form::parse_prose_map(raw, &prose.keys).ok()?;
    let blurb = crate::tools::form::normalize_body(map.get("blurb")?);
    if blurb.is_empty() {
        return None;
    }
    Some(MomentumReply {
        blurb,
        headline: crate::tools::form::card_hook(map.get("headline")?),
    })
}

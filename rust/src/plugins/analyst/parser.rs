//! Strict prose decoding and served-product guards.
use crate::studio::Parser;
use anyhow::{anyhow, Result};

/// Parsed model prose. Direction and conviction are deterministic product fields.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The Analyst's read.
    pub blurb: String,
    /// Optional card title.
    pub headline: Option<String>,
}

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        // Keep a bounded raw excerpt so malformed output is diagnosable.
        let mut reply = parse_momentum_reply(raw).ok_or_else(|| {
            anyhow!(
                "momentum: invalid response (raw={:?})",
                crate::util::truncate_bytes(raw.trim(), 160)
            )
        })?;
        // Production guards live at the Parser seam; eval can still inspect the raw parse.
        crate::plugins::support::form::validate_body(&reply.blurb)?;
        crate::plugins::support::form::validate_hook(reply.headline.as_deref())?;
        if let Some(p) = crate::plugins::support::guards::first_banned_phrase(
            &reply.blurb,
            crate::plugins::support::guards::MOMENTUM_BANNED_PHRASES,
        ) {
            tracing::warn!(
                guard = "momentum_banned_phrase",
                phrase = p,
                "momentum READ rejected"
            );
            anyhow::bail!("momentum: READ carries banned phrase {p:?}");
        }
        if let Some(p) = crate::plugins::support::guards::first_product_name(&reply.blurb) {
            tracing::warn!(guard = "product_name", name = p, "momentum READ rejected");
            anyhow::bail!("momentum: READ names product {p:?}");
        }
        // Sporting numbers are evidence; internal field citations leak the input contract.
        if crate::plugins::support::guards::has_bookkeeping_citation(&reply.blurb) {
            tracing::warn!(guard = "bookkeeping_citation", "momentum READ rejected");
            anyhow::bail!("momentum: READ carries a bookkeeping citation");
        }
        // A bad optional title degrades to NULL without costing the read.
        reply.headline =
            crate::plugins::support::guards::settle_title("analyst", reply.headline.as_deref());
        Ok(Some(reply))
    }
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    let prose = super::prompt::prose();
    let map = crate::plugins::support::form::parse_prose_map(raw, &prose.keys, prose.dims).ok()?;
    let blurb = crate::plugins::support::form::normalize_body(map.get("blurb")?);
    if blurb.is_empty() {
        return None;
    }
    // Reject a foreign-script generation so the work item retries.
    if crate::plugins::support::guards::has_foreign_script(&blurb) {
        return None;
    }
    Some(MomentumReply {
        blurb,
        headline: None,
    })
}

//! Decode the Scout’s performance profile.
use crate::harness::Parser;
use anyhow::Result;

pub fn prose() -> crate::tools::form::Prose {
    crate::tools::form::Prose::new(&["body", "headline"])
}

/// Output contract captured separately in the diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v8-parts";

/// Backstops against internal notation and unsupported claims from stored coverage.
pub const RATING_BODY_BANS: &[&str] = &[
    " · ",
    "exact labels and bands",
    "mid-season",
    "printed measurements",
    "limited playing time",
    "playing time is likely limited",
    "reduced playing time",
    "reduced minutes",
    "fewer minutes",
    "lower total minutes",
    "constrained role",
    "shift in role",
    "tactical adjustments",
    "positional or tactical changes",
    "substituted early",
    "typical team averages",
    "only verified fixture",
];

/// The model writes the profile and its hook in one call.
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub body: String,
    pub headline: String,
}

/// An explicit JSON null is a completed pass. Empty or invalid cards remain errors.
pub struct RatingParser;

impl Parser<RatingReply> for RatingParser {
    fn flagged_output(&self, raw: &str) -> Result<Option<RatingReply>> {
        let (body, headline) = crate::tools::form::flagged_prose(raw, "body");
        Ok(Some(RatingReply { body, headline }))
    }

    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        let map = crate::tools::form::decode_prose_map(raw, &prose().keys)?;
        let Some(text) = map.get("body") else {
            return Ok(None);
        };
        let body = clean_commentary(text);
        crate::tools::form::validate_body(&body)?;
        let headline = crate::tools::form::card_hook(
            map.get("headline")
                .ok_or_else(|| anyhow::anyhow!("Scout hook missing"))?,
        );
        Ok(Some(RatingReply { body, headline }))
    }
}

/// Normalize the served prose and remove an accidental wrapping code fence.
pub(super) fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    crate::tools::guards::clean_served_prose(s)
}

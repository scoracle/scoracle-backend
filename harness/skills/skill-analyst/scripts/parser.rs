//! Decode the measured reading or the Analyst's snapshot estimate.
use super::request::Snapshot;
use crate::harness::Parser;
use crate::tools::form::{card_hook, flagged_prose, normalize_body, validate_body};
use anyhow::{ensure, Context, Result};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct MomentumReply {
    pub blurb: String,
    pub headline: String,
    pub estimated_score: Option<i16>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MomentumParser {
    Measured,
    Estimated,
}

impl Snapshot {
    pub fn parser(&self) -> MomentumParser {
        if self.momentum_score.is_some() {
            MomentumParser::Measured
        } else {
            MomentumParser::Estimated
        }
    }
}

fn estimate(value: &Value) -> Option<i16> {
    value["estimated_score"]
        .as_i64()
        .filter(|s| (0..=100).contains(s))
        .map(|s| s as i16)
}

impl Parser<MomentumReply> for MomentumParser {
    fn flagged_output(&self, raw: &str) -> Result<Option<MomentumReply>> {
        let (blurb, headline) = flagged_prose(raw, "blurb");
        let estimated_score = match self {
            Self::Estimated => serde_json::from_str(crate::tools::form::answer_text(raw))
                .ok()
                .and_then(|v| estimate(&v)),
            Self::Measured => None,
        };
        Ok(Some(MomentumReply {
            blurb,
            headline,
            estimated_score,
        }))
    }

    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        let card: Value = serde_json::from_str(raw)?;
        let fields = card.as_object().context("Analyst card must be an object")?;
        let expected = if *self == Self::Estimated { 3 } else { 2 };
        ensure!(fields.len() == expected, "unexpected Analyst card fields");
        let blurb = normalize_body(card["blurb"].as_str().context("Analyst blurb missing")?);
        validate_body(&blurb)?;
        let headline = card_hook(
            card["headline"]
                .as_str()
                .context("Analyst headline missing")?,
        );
        validate_body(&headline)?;
        let estimated_score = match self {
            Self::Estimated => Some(
                estimate(&card)
                    .context("Analyst estimated_score must be an integer from 0 to 100")?,
            ),
            Self::Measured => None,
        };
        Ok(Some(MomentumReply {
            blurb,
            headline,
            estimated_score,
        }))
    }
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    MomentumParser::Measured.parse(raw).ok().flatten()
}

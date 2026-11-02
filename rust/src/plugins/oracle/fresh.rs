//! Current daily peer readings, projected without historical context.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "new_content contains prepared perspectives about the target, possibly sharing sources. Preserve each claim's actor, measure and qualifications; missing perspectives are unknown.",
    fields: &[
        ("journalist", "reported stories; title labels a story and body contains its qualified claims."),
        ("journalist.generated_at", "reading creation date, not event date."),
        ("journalist.source_count", "linked items, not independent confirmations."),
        ("journalist.source_age_days", "age of source reporting."),
        ("journalist.trajectory", "summary label, not dated measurements or causes."),
        ("scout", "measured performance; each number and band belongs only to its named measure."),
        ("scout.generated_at", "reading creation date, not event date."),
        ("scout.direction", "summary label, not dated measurements or changes in every attribute."),
        ("influencer", "coverage tone or attributed reactions."),
        ("influencer.generated_at", "reading creation date, not event date."),
        ("analyst", "performance/coverage relationship."),
        ("analyst.generated_at", "reading creation date, not event date."),
        ("analyst.direction", "summary label, not dated measurements or changes in every attribute."),
        ("insider", "movement or appointment reporting."),
        ("insider.generated_at", "reading creation date, not event date."),
    ],
};

use super::request::{Cards, SynthNarrative};
use serde::Serialize;

#[derive(Serialize)]
struct Narrative<'a> {
    generated_at: Option<&'a str>,
    title: &'a str,
    body: &'a str,
    trajectory: &'a str,
    source_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_age_days: Option<i32>,
}

#[derive(Serialize)]
struct Reading<'a> {
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_at: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct Fresh<'a> {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    journalist: Vec<Narrative<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scout: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    influencer: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    analyst: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insider: Option<Reading<'a>>,
}

pub(super) fn readings(cards: &Cards) -> Fresh<'_> {
    let mut narratives: Vec<&SynthNarrative> = cards.narratives.iter().collect();
    narratives.sort_by(|a, b| b.impact.total_cmp(&a.impact));
    let journalist = narratives
        .into_iter()
        .take(3)
        .map(|n| Narrative {
            generated_at: n.generated_at.as_deref(),
            title: &n.title,
            body: &n.body,
            trajectory: &n.trajectory,
            source_count: n.source_count,
            source_age_days: n.source_age_days,
        })
        .collect();
    Fresh {
        journalist,
        scout: cards.rating.as_ref().map(|r| Reading {
            body: &r.body,
            direction: Some(&r.rating_trajectory),
            generated_at: r.generated_at.as_deref(),
        }),
        influencer: cards.vibe.as_ref().map(|r| Reading {
            body: &r.prompt,
            direction: None,
            generated_at: r.generated_at.as_deref(),
        }),
        analyst: cards.momentum.blurb.as_deref().map(|body| Reading {
            body,
            direction: cards.momentum.direction.as_deref(),
            generated_at: cards.momentum.generated_at.as_deref(),
        }),
        insider: cards.insider.as_ref().map(|r| Reading {
            body: &r.body,
            direction: None,
            generated_at: r.generated_at.as_deref(),
        }),
    }
}

pub(super) fn context(cards: &Cards) -> crate::tools::prompt::Context<Fresh<'_>> {
    crate::tools::prompt::Context {
        guide: GUIDE,
        data: readings(cards),
    }
}

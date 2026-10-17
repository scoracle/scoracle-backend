//! Current daily peer readings, projected without historical context.
use super::prompt::{Cards, SynthNarrative};
use serde::Serialize;

#[derive(Serialize)]
struct Narrative<'a> {
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
            generated_at: None,
        }),
        influencer: cards.vibe.as_ref().map(|r| Reading {
            body: &r.prompt,
            direction: None,
            generated_at: None,
        }),
        analyst: cards.momentum.blurb.as_deref().map(|body| Reading {
            body,
            direction: cards.momentum.direction.as_deref(),
            generated_at: None,
        }),
        insider: cards.insider.as_ref().map(|r| Reading {
            body: &r.body,
            direction: None,
            generated_at: r.generated_at.as_deref(),
        }),
    }
}

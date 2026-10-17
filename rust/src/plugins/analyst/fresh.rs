//! Current daily peer readings, projected without historical context.
use super::prompt::{Form, Mood};
use serde::Serialize;

#[derive(Serialize)]
struct Reading<'a> {
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_at: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct Fresh<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    scout: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    influencer: Option<Reading<'a>>,
}

pub(super) fn readings<'a>(scout: Option<&'a Form>, influencer: Option<&'a Mood>) -> Fresh<'a> {
    Fresh {
        scout: scout.map(|r| Reading {
            body: &r.body,
            season: r.season,
            generated_at: r.generated_at.as_deref(),
        }),
        influencer: influencer.map(|r| Reading {
            body: &r.body,
            season: None,
            generated_at: r.generated_at.as_deref(),
        }),
    }
}

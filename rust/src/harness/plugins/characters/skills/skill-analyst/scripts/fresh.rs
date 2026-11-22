//! Current peer readings and dated computed comparisons, without historical prose.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "Scout measures on-field performance. Influencer measures coverage tone. Comparisons show how each changed between its two reporting days. Current scores show where those readings stand. When there are no comparisons, describe today’s snapshot.",
    fields: &[
        ("scout.score", "Scout rates on-field performance from 0 to 100. Higher is stronger."),
        ("scout.season", "season covered by the performance reading."),
        ("influencer.score", "Influencer rates coverage tone. 0 is strongly negative, 50 is balanced, 100 is strongly positive."),
        ("comparisons", "each change is the newer score minus the older score on the two dates shown, in score points. A zero change means those two readings matched."),
    ],
};

use super::request::{Form, Mood, Snapshot};
use serde::Serialize;

#[derive(Serialize)]
struct Reading<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_at: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct Fresh<'a> {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    comparisons: Vec<Comparison<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scout: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    influencer: Option<Reading<'a>>,
}

pub(super) fn readings<'a>(
    scout: Option<&'a Form>,
    influencer: Option<&'a Mood>,
    snapshot: &'a Snapshot,
) -> Fresh<'a> {
    let comparisons = comparisons(snapshot);
    let include =
        |measure| comparisons.is_empty() || comparisons.iter().any(|c| c.measure == measure);
    Fresh {
        scout: scout.filter(|_| include("performance")).map(|r| Reading {
            body: r.card_score.is_none().then_some(r.body.as_str()),
            score: r.card_score,
            season: r.season,
            generated_at: r.generated_at.as_deref(),
        }),
        influencer: influencer
            .filter(|_| include("coverage_tone"))
            .map(|r| Reading {
                body: r.sentiment.is_none().then_some(r.body.as_str()),
                score: r.sentiment.map(f64::from),
                season: None,
                generated_at: r.generated_at.as_deref(),
            }),
        comparisons,
    }
}

#[derive(Serialize)]
struct Comparison<'a> {
    measure: &'static str,
    change: f64,
    direction: &'static str,
    window_start: &'a str,
    window_end: &'a str,
}

fn comparisons(snapshot: &Snapshot) -> Vec<Comparison<'_>> {
    [
        (
            "performance",
            snapshot.rating_slope,
            snapshot.rating_samples,
            snapshot.rating_window_start.as_deref(),
            snapshot.rating_window_end.as_deref(),
        ),
        (
            "coverage_tone",
            snapshot.vibe_slope,
            snapshot.vibe_samples,
            snapshot.vibe_window_start.as_deref(),
            snapshot.vibe_window_end.as_deref(),
        ),
    ]
    .into_iter()
    .filter_map(|(measure, change, samples, start, end)| {
        let (change, start, end) = (change?, start?, end?);
        if !change.is_finite() || samples < 2 || start.is_empty() || end.is_empty() || start >= end
        {
            return None;
        }
        Some(Comparison {
            measure,
            change,
            direction: if change > 0.0 {
                "increased"
            } else if change < 0.0 {
                "decreased"
            } else {
                "unchanged"
            },
            window_start: start,
            window_end: end,
        })
    })
    .collect()
}

pub fn has_comparison(snapshot: &Snapshot) -> bool {
    !comparisons(snapshot).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_requires_measurement_sample_and_dates() {
        let mut s = Snapshot {
            rating_slope: Some(8.0),
            rating_samples: 2,
            rating_window_start: Some("2026-10-01".into()),
            rating_window_end: Some("2026-10-02".into()),
            ..Snapshot::default()
        };
        let value = serde_json::to_value(readings(None, None, &s)).unwrap();
        assert_eq!(value["comparisons"][0]["measure"], "performance");
        assert_eq!(value["comparisons"][0]["change"], 8.0);
        assert_eq!(value["comparisons"][0]["direction"], "increased");
        assert_eq!(value["comparisons"].as_array().unwrap().len(), 1);
        s.rating_samples = 1;
        assert!(!has_comparison(&s));
        s.rating_samples = 2;
        s.rating_window_end = None;
        assert!(!has_comparison(&s));
    }
}

pub(super) fn context<'a>(
    scout: Option<&'a Form>,
    influencer: Option<&'a Mood>,
    snapshot: &'a Snapshot,
) -> crate::tools::prompt::Context<Fresh<'a>> {
    crate::tools::prompt::Context {
        guide: GUIDE,
        data: readings(scout, influencer, snapshot),
    }
}

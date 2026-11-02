//! Current peer readings and dated computed comparisons, without historical prose.
use super::prompt::{Form, Mood, Snapshot};
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
    comparisons: Vec<Comparison<'a>>,
}

pub(super) fn readings<'a>(
    scout: Option<&'a Form>,
    influencer: Option<&'a Mood>,
    snapshot: &'a Snapshot,
) -> Fresh<'a> {
    Fresh {
        comparisons: comparisons(snapshot),
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

#[derive(Serialize)]
struct Comparison<'a> {
    measure: &'static str,
    change: f64,
    unit: &'static str,
    direction: &'static str,
    samples: i32,
    window_start: &'a str,
    window_end: &'a str,
    computed_at: Option<&'a str>,
    limit: &'static str,
}

fn comparisons(snapshot: &Snapshot) -> Vec<Comparison<'_>> {
    [
        ("rating_notability", snapshot.rating_slope, snapshot.rating_samples,
            snapshot.rating_window_start.as_deref(), snapshot.rating_window_end.as_deref()),
        ("coverage_tone", snapshot.vibe_slope, snapshot.vibe_samples,
            snapshot.vibe_window_start.as_deref(), snapshot.vibe_window_end.as_deref()),
    ].into_iter().filter_map(|(measure, change, samples, start, end)| {
        let (change, start, end) = (change?, start?, end?);
        if !change.is_finite() || samples < 2 || start.is_empty() || end.is_empty() || start >= end {
            return None;
        }
        Some(Comparison {
            measure, change, unit: "score points between dated observations",
            direction: if change > 0.0 { "increased" } else if change < 0.0 { "decreased" } else { "unchanged" },
            samples, window_start: start, window_end: end, computed_at: snapshot.generated_at.as_deref(),
            limit: "Descriptive comparison only; distinctiveness is not skill, coverage tone is not population sentiment.",
        })
    }).collect()
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
        assert_eq!(value["comparisons"][0]["measure"], "rating_notability");
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

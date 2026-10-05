//! The Analyst's finished readings and dated study, without upstream bookkeeping.

use super::{Form, Mood, Snapshot};
use crate::plugins::meta::EntityMeta;
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
struct Rail<'a> {
    slope: f64,
    samples: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    through: Option<&'a str>,
}

#[derive(Serialize)]
struct Study<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    computed_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    form: Option<Rail<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mood: Option<Rail<'a>>,
}

#[derive(Serialize)]
struct Fresh<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    scout: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    influencer: Option<Reading<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trajectory: Option<Study<'a>>,
}

pub fn assemble(
    subject: &EntityMeta,
    scout: Option<&Form>,
    influencer: Option<&Mood>,
    snapshot: &Snapshot,
) -> String {
    let form = snapshot.rating_slope.map(|slope| Rail {
        slope,
        samples: snapshot.rating_samples,
        from: snapshot.rating_window_start.as_deref(),
        through: snapshot.rating_window_end.as_deref(),
    });
    let mood = snapshot.vibe_slope.map(|slope| Rail {
        slope,
        samples: snapshot.vibe_samples,
        from: snapshot.vibe_window_start.as_deref(),
        through: snapshot.vibe_window_end.as_deref(),
    });
    let trajectory = (form.is_some() || mood.is_some()).then_some(Study {
        computed_at: snapshot.generated_at.as_deref(),
        form,
        mood,
    });
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::plugins::meta::WritingIdentity<'a>,
        fresh: Fresh<'a>,
        voice: &'static str,
        form: serde_json::Value,
    }
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: Fresh {
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
            trajectory,
        },
        voice: crate::plugins::analyst::voice::VOICE,
        form: prose().form(),
    })
    .expect("analyst world serializes")
}

pub fn prose() -> crate::plugins::cognition::prose::Prose {
    crate::plugins::cognition::prose::Prose::new(
        &["blurb"],
        crate::plugins::cognition::prose::Dimensions::new(
            crate::plugins::support::form::BODY_MAX_CHARS,
            None,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_readings_and_study_without_scores_or_hashes() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let scout = Form {
            body: "Full measured reading. Its qualifier stays here.".into(),
            headline: None,
            season: Some(2026),
            generated_at: Some("2026-10-01".into()),
            input_hash: Some("hidden".into()),
        };
        let snapshot = Snapshot {
            rating_slope: Some(2.0),
            rating_samples: 4,
            rating_window_start: Some("2026-09-01".into()),
            rating_window_end: Some("2026-09-30".into()),
            momentum_score: Some(71.0),
            ..Snapshot::default()
        };
        let world = assemble(&subject, Some(&scout), None, &snapshot);
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert!(world.starts_with(r#"{"meta":"#));
        assert_eq!(value["fresh"]["scout"]["body"], scout.body);
        assert_eq!(value["fresh"]["trajectory"]["form"]["samples"], 4);
        assert!(value["fresh"].get("influencer").is_none());
        assert!(world.find("hidden").is_none());
        assert!(world.find("momentum_score").is_none());
    }
}

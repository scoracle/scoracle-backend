//! Five finished cards are the Oracle's entire evidence world.

use super::{Cards, Subject, SynthNarrative};
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
struct Fresh<'a> {
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

pub fn assemble(subject: &Subject, cards: &Cards) -> String {
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
    let meta = crate::plugins::meta::EntityMeta {
        name: subject.entity_name.clone(),
        entity_type: subject.entity_type.clone(),
        entity_id: subject.entity_id,
        sport: subject.sport.clone(),
    };
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::plugins::meta::WritingIdentity<'a>,
        fresh: Fresh<'a>,
        voice: &'static str,
        form: serde_json::Value,
    }
    serde_json::to_string(&Input {
        meta: meta.for_writing(),
        fresh: Fresh {
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
        },
        voice: crate::plugins::oracle::voice::VOICE,
        form: prose().form(),
    })
    .expect("oracle world serializes")
}

pub fn prose() -> crate::plugins::cognition::prose::Prose {
    crate::plugins::cognition::prose::Prose::new(
        &["reading"],
        crate::plugins::cognition::prose::Dimensions::new(
            crate::plugins::support::form::ORACLE_READING_MAX_CHARS,
            None,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_receives_five_cards_and_no_memory() {
        let subject = Subject {
            entity_id: 7,
            entity_type: "team".into(),
            entity_name: "Cleveland Browns".into(),
            sport: "NFL".into(),
        };
        let cards = Cards {
            insider: Some(super::super::SynthInsider {
                body: "The Browns are linked with a receiver, according to Wire.".into(),
                score: 70,
                generated_at: Some("2026-10-02".into()),
            }),
            ..Cards::default()
        };
        let world = assemble(&subject, &cards);
        assert!(world.starts_with(r#"{"meta":"#));
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert!(value.get("memories").is_none());
        assert_eq!(
            value["fresh"]["insider"]["body"],
            cards.insider.unwrap().body
        );
        assert!(world.find("score").is_none());
    }
}

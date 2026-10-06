//! Finished-card retrieval, readiness, request projection and provenance.
use crate::application::queue::work::Item;
use crate::evidence::trajectory::DEFAULT_TRAJECTORY;
use crate::studio::model::GenerateOptions;
use crate::util::{hash_components, round1};
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::PgPool;

pub const ORACLE_PROMPT_VERSION: &str = "or26";

pub const ORACLE_SYSTEM_PROMPT: &str = "Synthesize only the five supplied finished character cards into one current reading. Show where they agree or pull apart. A missing card is unknown; do not fill it from memory or guess. These cards may share underlying sources, so repetition is not independent confirmation. Attribute reported claims, preserve qualifications and dates, and do not forecast. Write only the declared JSON reading.";

/// Production crown temperature. Fixtures pin zero.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Runtime headroom for the JSON response, not a requested prose length.
pub const ORACLE_NUM_PREDICT: i32 = 700;

pub fn generation_options(temperature: f64, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: ORACLE_NUM_PREDICT,
        num_ctx,
        json_mode: false,
        format_schema: Some(prose().schema()),
        format_schema_raw: None,
    }
}

/// One narrative from the entity's latest generation. `impact` is widened from a database
/// integer and rendered without a fractional part.
#[derive(Clone, Debug)]
pub struct SynthNarrative {
    pub title: String,
    pub body: String,
    pub impact: f64,
    pub trajectory: String,
    /// Prompt-only corroboration and freshness; excluded from the material hash.
    pub source_count: i32,
    pub source_age_days: Option<i32>,
}

/// The Scout's rating pillar. A latest NULL body suppresses the pillar.
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub body: String,
    pub notability: i32,
    pub rating_trajectory: String,
    pub rating_trajectory_label: String,
}

/// The vibe pillar (P3): the latest felt-read product, distinct from the Momentum trajectory.
#[derive(Clone, Debug)]
pub struct SynthVibe {
    pub sentiment: i32,
    pub prompt: String,
}

/// The momentum pillar (P4): durable trajectory values from `momentum_scores`.
#[derive(Clone, Debug, Default)]
pub struct SynthMomentum {
    pub direction: Option<String>,
    pub blurb: Option<String>,
    pub input_hash: Option<String>,
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub momentum_score: Option<f64>,
}

/// The Insider's finished card. Its score is kept for product math, not prose input.
#[derive(Clone, Debug)]
pub struct SynthInsider {
    pub body: String,
    pub score: i32,
    pub generated_at: Option<String>,
}

/// The five cards handed to the Oracle. Missing cards remain explicit rather than being filled
/// from older products or raw evidence.
#[derive(Clone, Debug, Default)]
pub struct Cards {
    pub narratives: Vec<SynthNarrative>,
    pub rating: Option<SynthRating>,
    pub vibe: Option<SynthVibe>,
    pub momentum: SynthMomentum,
    pub insider: Option<SynthInsider>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pillar {
    Narratives,
    Rating,
    Vibe,
    Momentum,
    Insider,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Empty,
    Partial { missing: Vec<Pillar> },
    Complete,
}

impl Cards {
    pub fn readiness(&self) -> Readiness {
        let mut missing = Vec::new();
        if self.narratives.is_empty() {
            missing.push(Pillar::Narratives);
        }
        if self.rating.is_none() {
            missing.push(Pillar::Rating);
        }
        if self.vibe.is_none() {
            missing.push(Pillar::Vibe);
        }
        if self.momentum.empty() {
            missing.push(Pillar::Momentum);
        }
        if self.insider.is_none() {
            missing.push(Pillar::Insider);
        }
        if missing.len() == 5 {
            Readiness::Empty
        } else if missing.is_empty() {
            Readiness::Complete
        } else {
            Readiness::Partial { missing }
        }
    }
}

/// Subject of an Oracle assignment. Durable ids and work revisions stay outside Studio.
#[derive(Clone, Debug)]
pub struct Subject {
    pub entity_id: i32,
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
}

/// Complete prepared assignment for one crown.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: Subject,
    pub season: i32,
    pub cards: Cards,
    pub input_components_json: String,
    pub input_hash: String,
    pub options: GenerateOptions,
}

impl SynthMomentum {
    /// empty mirrors `synthMomentum.empty()`: no momentum signal at all.
    pub fn empty(&self) -> bool {
        self.direction.is_none()
            && self.blurb.is_none()
            && self.vibe_slope.is_none()
            && self.rating_slope.is_none()
            && self.momentum_score.is_none()
    }
}

/// Build canonical input-components JSON. Narrative keys are always present; the rest are
/// conditional.
pub fn build_synthesis_input_components(cards: &Cards) -> String {
    let mut narratives: Vec<_> = cards
        .narratives
        .iter()
        .map(|n| {
            serde_json::json!({
                "title": n.title, "body": n.body, "impact": n.impact,
                "trajectory": n.trajectory, "source_count": n.source_count,
                "source_age_days": n.source_age_days,
            })
        })
        .collect();
    narratives.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    serde_json::json!({
        "prompt_version": ORACLE_PROMPT_VERSION,
        "narratives": narratives,
        "rating": cards.rating.as_ref().map(|r| serde_json::json!({
            "body": r.body, "notability": r.notability,
            "trajectory": r.rating_trajectory,
            "trajectory_label": r.rating_trajectory_label,
        })),
        "vibe": cards.vibe.as_ref().map(|v| serde_json::json!({
            "body": v.prompt, "sentiment": v.sentiment,
        })),
        "momentum": serde_json::json!({
            "body": cards.momentum.blurb,
            "direction": cards.momentum.direction,
            "input_hash": cards.momentum.input_hash,
            "rating_slope": cards.momentum.rating_slope.map(round1),
            "rating_samples": cards.momentum.rating_samples,
            "vibe_slope": cards.momentum.vibe_slope.map(round1),
            "vibe_samples": cards.momentum.vibe_samples,
            "score": cards.momentum.momentum_score.map(round1),
        }),
        "insider": cards.insider.as_ref().map(|r| serde_json::json!({
            "body": r.body, "score": r.score, "generated_at": r.generated_at,
        })),
    })
    .to_string()
}

pub async fn resolve_season(pool: &PgPool, sport: &str, want: Option<i32>) -> Result<i32> {
    if let Some(season) = want {
        return Ok(season);
    }
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("resolve current_season for {sport}"))
}

pub async fn load_narrative_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SynthNarrative>> {
    let rows: Vec<(String, String, i32, String, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), COALESCE(trajectory, $4),
               COALESCE(source_count, 0),
               EXTRACT(day FROM NOW() - source_latest_at)::int
          FROM news_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND body IS NOT NULL
           AND generated_at = (
               SELECT max(generated_at) FROM news_summaries
                WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           )
         ORDER BY impact DESC NULLS LAST
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(DEFAULT_TRAJECTORY)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load narrative pillar {entity_type}/{entity_id}"))?;
    Ok(rows
        .into_iter()
        .map(
            |(title, body, impact, trajectory, source_count, source_age_days)| SynthNarrative {
                title,
                body,
                impact: f64::from(impact),
                trajectory,
                source_count,
                source_age_days,
            },
        )
        .collect())
}

pub async fn load_rating_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<SynthRating>> {
    let row: Option<(Option<String>, i32, String, String)> = sqlx::query_as(
        r#"
        SELECT body, COALESCE(notability, 0),
               COALESCE(rating_trajectory, 'steady'), COALESCE(rating_trajectory_label, '')
          FROM stat_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND ($4::int IS NULL OR season = $4)
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load rating pillar {entity_type}/{entity_id}"))?;
    Ok(match row {
        Some((Some(body), notability, rating_trajectory, rating_trajectory_label)) => {
            Some(SynthRating {
                body,
                notability,
                rating_trajectory,
                rating_trajectory_label,
            })
        }
        _ => None,
    })
}

pub async fn load_vibe_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<SynthVibe>> {
    let row: Option<(Option<i16>, String)> = sqlx::query_as(
        r#"
        SELECT sentiment, COALESCE(prompt, '')
          FROM vibe_scores
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load vibe pillar {entity_type}/{entity_id}"))?;
    Ok(match row {
        Some((Some(sentiment), prompt)) => Some(SynthVibe {
            sentiment: i32::from(sentiment),
            prompt,
        }),
        _ => None,
    })
}

pub async fn load_momentum_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"
        SELECT direction, score, blurb, input_hash, COALESCE(input_components, '{}'::jsonb)
          FROM momentum_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND ($4::int IS NULL OR season = $4)
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load momentum pillar {entity_type}/{entity_id}"))?;
    let Some((direction, score, blurb, input_hash, components)) = row else {
        return Ok(SynthMomentum::default());
    };
    Ok(SynthMomentum {
        direction: direction.filter(|value| !value.trim().is_empty()),
        blurb: blurb.filter(|value| !value.trim().is_empty()),
        input_hash,
        vibe_slope: components
            .get("momentum_vibe_slope")
            .and_then(serde_json::Value::as_f64),
        vibe_samples: components
            .get("momentum_vibe_samples")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default() as i32,
        rating_slope: components
            .get("momentum_rating_slope")
            .and_then(serde_json::Value::as_f64),
        rating_samples: components
            .get("momentum_rating_samples")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default() as i32,
        momentum_score: score.map(f64::from),
    })
}

pub async fn load_insider_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<SynthInsider>> {
    let row: Option<(Option<String>, i16, String)> = sqlx::query_as(
        "SELECT read,score,generated_at::date::text FROM public.insider_scores \
         WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 \
         ORDER BY generated_at DESC,id DESC LIMIT 1",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(body, score, generated_at)| {
        body.filter(|text| !text.trim().is_empty())
            .map(|body| SynthInsider {
                body,
                score: i32::from(score),
                generated_at: Some(generated_at),
            })
    }))
}

pub async fn load_pillars(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(i32, Cards)> {
    let season = resolve_season(pool, sport, None).await?;
    let (narratives, rating, vibe, momentum, insider) = tokio::try_join!(
        load_narrative_pillar(pool, entity_type, entity_id, sport),
        load_rating_pillar(pool, entity_type, entity_id, sport, Some(season)),
        load_vibe_pillar(pool, entity_type, entity_id, sport),
        load_momentum_pillar(pool, entity_type, entity_id, sport, Some(season)),
        load_insider_pillar(pool, entity_type, entity_id, sport),
    )?;
    Ok((
        season,
        Cards {
            narratives,
            rating,
            vibe,
            momentum,
            insider,
        },
    ))
}

/// Read-only materials for production creation, including the no-card marker's
/// empty fingerprint. Backend binding and debounce policy belong to execution.
pub(super) async fn load_assignment(
    pool: &PgPool,
    item: &Item,
    num_ctx: i32,
) -> Result<Assignment> {
    let entity_id = item.entity_id_i32()?;
    let name = crate::evidence::corpus::lookup_entity_name(
        pool,
        &item.entity_type,
        entity_id,
        &item.sport,
    )
    .await?;
    let sport = item.sport.to_uppercase();
    let (season, cards) = load_pillars(pool, &item.entity_type, entity_id, &sport).await?;
    let (input_components_json, input_hash) = if cards.readiness() == Readiness::Empty {
        ("{}".to_string(), String::new())
    } else {
        let components = build_synthesis_input_components(&cards);
        let hash = hash_components(&components);
        (components, hash)
    };
    Ok(Assignment {
        subject: Subject {
            entity_id,
            entity_type: item.entity_type.clone(),
            entity_name: name,
            sport: item.sport.clone(),
        },
        season,
        cards,
        input_components_json,
        input_hash,
        options: generation_options(ORACLE_TEMPERATURE, num_ctx),
    })
}

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
            insider: Some(SynthInsider {
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

//! Source-level Scout review before measured-profile publication.
use super::prompt::{RatingBuild, RATING_TEMPERATURE};
use super::publish::{insert_stat_summary, record_ledger, LedgerSubject};
use super::{create, RatingOutput};
use crate::harness::model::Inference;
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::Studio;
use crate::plugins::classifier::delivery::load_for_character;
use crate::plugins::scout::performance::current_season;
use crate::plugins::scout::prompt::{build_rating_request, RatingReq};
use crate::tools::source::SourceContext;
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool};

const POLICY_VERSION: &str = "scout-source-v7-article-spectrum";

/// Only a source-linked structured record can turn roster/availability reporting
/// into a Scout rating trigger. A Classifier proposal or model quote alone is not enough.
pub(crate) async fn structured_record(
    connection: &mut PgConnection,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    article_id: i64,
    kind: SourceKind,
) -> Result<Option<i64>> {
    match kind {
        SourceKind::Roster => Ok(sqlx::query_scalar(
            "SELECT id FROM public.transfer_identity_applications \
                 WHERE sport=$1 AND status='applied' AND reverted_at IS NULL \
                   AND evidence->'identity_evidence_article_ids' @> jsonb_build_array($4::bigint) \
                   AND (($2='player' AND player_id=$3) OR \
                        ($2='team' AND (old_team_id=$3 OR new_team_id=$3))) \
                 ORDER BY applied_at DESC,id DESC LIMIT 1 FOR SHARE NOWAIT",
        )
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(article_id)
        .fetch_optional(&mut *connection)
        .await?),
        SourceKind::Availability => Ok(sqlx::query_scalar(
            "SELECT id FROM public.player_availability \
                 WHERE sport=$1 AND status='applied' AND reverted_at IS NULL \
                   AND source_article_id=$4 \
                   AND (($2='player' AND player_id=$3) OR \
                        ($2='team' AND team_id=$3)) \
                 ORDER BY applied_at DESC,id DESC LIMIT 1 FOR SHARE NOWAIT",
        )
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(article_id)
        .fetch_optional(&mut *connection)
        .await?),
        _ => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    None,
    Roster,
    Availability,
}

#[derive(Debug, PartialEq)]
struct SourceDecision {
    kind: SourceKind,
    identity_resolved: bool,
    structured_record: Option<i64>,
    provenance: serde_json::Value,
}

fn source_eligible(world: Option<&serde_json::Value>) -> bool {
    world
        .and_then(|world| world["spectrum"]["scout"].as_f64())
        .unwrap_or(0.0)
        >= 70.0
}

async fn decide(
    connection: &mut PgConnection,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    source: &SourceContext,
) -> Result<SourceDecision> {
    let native = crate::plugins::classifier::delivery::load_spectrum_source_on(
        connection,
        source.classification_id,
    )
    .await?;
    let identity_resolved = native.provenance["identity_candidates"]
        .as_array()
        .is_some_and(|candidates| {
            candidates.iter().any(|candidate| {
                candidate["entity_type"] == entity_type && candidate["entity_id"] == entity_id
            })
        });
    // The article score nominates review; the applied record establishes its factual kind.
    let (mut kind, mut structured_record) = (SourceKind::None, None);
    if source_eligible(source.classifier_world.as_ref()) {
        for candidate in [SourceKind::Availability, SourceKind::Roster] {
            if let Some(record) = self::structured_record(
                connection,
                sport,
                entity_type,
                entity_id,
                source.article_id,
                candidate,
            )
            .await?
            {
                kind = candidate;
                structured_record = Some(record);
                break;
            }
        }
    }
    Ok(SourceDecision {
        kind,
        identity_resolved,
        structured_record,
        provenance: json!({"policy": POLICY_VERSION, "classifier_world": source.classifier_world}),
    })
}

pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let entity_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let plugin_id = super::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    let mut decisions = Vec::with_capacity(sources.len());
    for source in &sources {
        decisions.push(
            decide(
                &mut *pool.acquire().await?,
                &item.entity_type,
                entity_id,
                &sport,
                source,
            )
            .await?,
        );
    }
    let now = crate::plugins::influencer::now();
    let invalid = |source: &SourceContext| {
        !source.published_at_epoch.is_some_and(|t| t <= now)
            || super::super::influencer::prompt::source_disposition(&source.context).is_some()
    };
    let overflow = serde_json::to_vec(&sources)?.len() > 24000;
    let reporting: Vec<_> = sources
        .iter()
        .zip(&decisions)
        .filter(|(s, d)| !invalid(s) && d.identity_resolved)
        .map(|(s, _)| s.clone())
        .collect();
    let reporting_ids: Vec<_> = reporting.iter().map(|s| s.article_id).collect();
    let stat_trigger = !item
        .input_version
        .as_deref()
        .is_some_and(|v| v.starts_with(crate::plugins::classifier::CONTRACT));
    let grounded_trigger = decisions
        .iter()
        .any(|d| d.identity_resolved && d.structured_record.is_some());
    let trigger_payload = json!({"daily_sources":sources.iter().zip(&decisions).map(|(s,d)|json!({
        "article_id":s.article_id,"classification_id":s.classification_id,
        "identity_resolved":d.identity_resolved,"structured_record_id":d.structured_record,
        "source_decision":d.provenance})).collect::<Vec<_>>()});
    let mut output: Option<RatingOutput> = None;
    if !overflow && (stat_trigger || grounded_trigger) {
        let name =
            crate::tools::meta::lookup_entity_name(pool, &item.entity_type, entity_id, &sport)
                .await?;
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            trigger_type: "daily".into(),
            season: Some(current_season(pool, &sport).await?),
        };
        if let RatingBuild::Ready(mut assignment) =
            build_rating_request(pool, voice_num_ctx, &req, RATING_TEMPERATURE, false).await?
        {
            assignment.parts.reporting = reporting;
            assignment.built_prompt = assignment.parts.render();
            let mut components: serde_json::Value =
                serde_json::from_str(&assignment.input_components)?;
            components["daily_source_decisions"] = trigger_payload.clone();
            components["world"] = serde_json::from_str(&assignment.built_prompt)?;
            assignment.input_components = components.to_string();
            assignment.input_hash = crate::util::hash_components(&assignment.input_components);
            let mut generated = create(&Studio::new(backend), *assignment).await?;
            generated.provenance.input_ids = reporting_ids;
            output = Some(generated);
        }
    }
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    crate::plugins::classifier::delivery::validate_for_publication(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
        &sources,
    )
    .await?;
    for (source, decision) in sources.iter().zip(&decisions) {
        ensure!(
            *decision
                == decide(
                    publication.transaction(),
                    &item.entity_type,
                    entity_id,
                    &sport,
                    source
                )
                .await?,
            "Scout trigger evidence changed during daily articulation"
        );
    }
    let product_id = if let Some(out) = &output {
        Some(
            insert_stat_summary(
                publication.transaction(),
                &item.entity_type,
                entity_id,
                &sport,
                "daily",
                &trigger_payload,
                out,
            )
            .await?,
        )
    } else {
        None
    };
    for (source, decision) in sources.iter().zip(&decisions) {
        let reason = if overflow {
            Some("daily_package_exceeds_context_budget")
        } else if invalid(source) {
            Some("complete_source_unavailable")
        } else if !decision.identity_resolved {
            Some("unresolved_target_identity")
        } else if decision.structured_record.is_none() {
            Some("no_applied_source_linked_record")
        } else if product_id.is_none() {
            Some("no_measured_profile")
        } else {
            None
        };
        crate::plugins::classifier::delivery::settle(
            publication.transaction(),
            plugin_id,
            &[source.classification_id],
            if reason.is_none() { "used" } else { "held" },
            reason,
            json!({"stat_summary_id":product_id,"structured_record_id":decision.structured_record,
                "source_quote":source.context,"source_decision":decision.provenance}),
        )
        .await?;
    }
    super::record_rating_completed(publication.transaction(), item, product_id.is_some()).await?;
    publication.commit_final().await?;
    if let (Some(id), Some(out)) = (product_id, output.as_ref()) {
        record_ledger(
            pool,
            &LedgerSubject {
                entity_type: &item.entity_type,
                entity_id,
                sport: &sport,
                trigger_type: "daily",
                trigger_payload: &trigger_payload,
            },
            id,
            out,
        )
        .await?;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn article_score_nominates_scout_without_claiming_an_applied_change() {
        for (score, expected) in [(70.0, true), (69.9, false), (0.9, false)] {
            assert_eq!(
                source_eligible(Some(&json!({"spectrum":{"scout":score}}))),
                expected
            );
        }
        assert!(!source_eligible(None));
        assert!(!source_eligible(Some(
            &json!({"spectrum":{"availability":99}})
        )));
    }
}

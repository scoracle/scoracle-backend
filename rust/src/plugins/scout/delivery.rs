//! Source-level Scout review before measured-profile publication.
use super::prompt::{RatingBuild, RATING_TEMPERATURE};
use super::publish::{insert_stat_summary, record_ledger, LedgerSubject};
use super::{create, record_rating_completed, RatingOutput};
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
use std::time::Duration;

const POLICY_VERSION: &str = "scout-source-v5-classifier-world";

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
    Performance,
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

async fn decide(
    connection: &mut PgConnection,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    source: &SourceContext,
) -> Result<SourceDecision> {
    let (native, _, _) = crate::plugins::classifier::adapter::load_measurement_on(
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
    let selected = source
        .classifier_world
        .as_ref()
        .and_then(|world| world["selection"]["kind"].as_str());
    let kind = match selected {
        Some("performance") => SourceKind::Performance,
        Some("roster") => SourceKind::Roster,
        Some("availability") => SourceKind::Availability,
        _ => SourceKind::None,
    };
    let structured_record = structured_record(
        connection,
        sport,
        entity_type,
        entity_id,
        source.article_id,
        kind,
    )
    .await?;
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
    let plugin_id = crate::plugins::scout::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    let Some(source) = sources.last() else {
        let Some(publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    };
    let name =
        crate::tools::meta::lookup_entity_name(pool, &item.entity_type, entity_id, &sport).await?;
    let decision = decide(
        &mut *pool.acquire().await?,
        &item.entity_type,
        entity_id,
        &sport,
        source,
    )
    .await?;
    let identity_resolved = decision.identity_resolved;
    let corroborating_record = decision.structured_record;
    // Preserve a complete attributed source unit; no model extracts a new quote.
    let quote = &source.context;
    let input_invalid = if !source
        .published_at_epoch
        .is_some_and(|date| date <= crate::plugins::influencer::now())
    {
        Some("Publication time unresolved")
    } else if crate::plugins::influencer::prompt::source_disposition(&source.context).is_some()
        || serde_json::to_vec(source)?.len() > 24000
    {
        Some("Complete source input unavailable or exceeds reading budget")
    } else {
        None
    };
    let mut output: Option<RatingOutput> = None;
    if input_invalid.is_none()
        && identity_resolved
        && (decision.kind == SourceKind::Performance || corroborating_record.is_some())
    {
        let season = current_season(pool, &sport).await?;
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            trigger_type: "periodic".into(),
            season: Some(season),
        };
        if let RatingBuild::Ready(mut assignment) =
            build_rating_request(pool, voice_num_ctx, &req, RATING_TEMPERATURE, false).await?
        {
            let mut components: serde_json::Value =
                serde_json::from_str(&assignment.input_components)?;
            components["classifier_trigger"] = json!({
                "source_decision": decision.provenance,
                "article_id": source.article_id,
                "classification_id": source.classification_id,
                "source_role": "attributed_reporting",
                "scout_kind": match decision.kind { SourceKind::Performance => "performance", SourceKind::Roster => "roster", SourceKind::Availability => "availability", SourceKind::None => "none" },
                "source_quote": quote,
                "structured_record_id": corroborating_record,
            });
            assignment.parts.reporting = vec![source.clone()];
            assignment.built_prompt = assignment.parts.render();
            assignment.opts = assignment
                .parts
                .generation_options(voice_num_ctx, RATING_TEMPERATURE);
            components["world"] =
                serde_json::from_str::<serde_json::Value>(&assignment.built_prompt)?;
            assignment.input_components = components.to_string();
            assignment.input_hash = crate::util::hash_components(&assignment.input_components);
            let mut generated = create(&Studio::new(backend), *assignment).await?;
            generated.provenance.input_ids = vec![source.article_id];
            output = Some(generated);
        }
    }
    let status = if output.is_some() { "used" } else { "held" };
    let reason = if let Some(reason) = input_invalid {
        Some(reason)
    } else if !identity_resolved {
        Some("No independently resolved article-to-entity link")
    } else if decision.kind == SourceKind::None {
        Some("Unavailable: no current performance predicate or applied source-linked roster/availability record")
    } else if output.is_none() {
        Some("No measured profile available for a rating")
    } else {
        None
    };
    let trigger_payload = json!({
        "source_article_id": source.article_id,
        "classification_id": source.classification_id,
        "source_role": "attributed_reporting",
        "identity_resolved": identity_resolved,
        "structured_record_id": corroborating_record,
        "scout_kind": match decision.kind {
            SourceKind::None => "none", SourceKind::Performance => "performance",
            SourceKind::Roster => "roster", SourceKind::Availability => "availability",
        },
        "evidence_quote": quote,
    });
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    crate::plugins::classifier::delivery::validate_for_publication(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
        std::slice::from_ref(source),
    )
    .await?;
    ensure!(
        decision
            == decide(
                publication.transaction(),
                &item.entity_type,
                entity_id,
                &sport,
                source
            )
            .await?,
        "Scout trigger evidence changed during articulation"
    );
    let product_row_id = if let Some(out) = output.as_ref() {
        let id = insert_stat_summary(
            publication.transaction(),
            &item.entity_type,
            entity_id,
            &sport,
            "periodic",
            &trigger_payload,
            out,
        )
        .await?;
        record_rating_completed(publication.transaction(), item, true).await?;
        Some(id)
    } else {
        None
    };
    let changed = sqlx::query(
        "UPDATE public.classifier_deliveries SET status=$3,production_eligible=($3<>'held'),reason=$4,product_ref=$5,updated_at=now() \
         WHERE measurement_id=$1 AND plugin_id=$2 AND status='pending' \
           AND production_eligible",
    )
    .bind(source.classification_id).bind(plugin_id).bind(status).bind(reason)
    .bind(json!({
        "source_article_id": source.article_id,
        "source_role": "attributed_reporting",
        "identity_resolved": identity_resolved,
        "structured_record_id": corroborating_record,
        "source_decision": decision.provenance,
        "source_kind": match decision.kind {
            SourceKind::None => "none", SourceKind::Performance => "performance",
            SourceKind::Roster => "roster", SourceKind::Availability => "availability",
        },
        "source_quote": quote,
        "stat_summary_id": product_row_id,
    }))
    .execute(&mut **publication.transaction()).await?;
    ensure!(
        changed.rows_affected() == 1,
        "Scout source assignment changed during call"
    );
    let remaining = crate::plugins::classifier::delivery::undelivered_count(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
    )
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        if let (Some(id), Some(out)) = (product_row_id, output.as_ref()) {
            record_ledger(
                pool,
                &LedgerSubject {
                    entity_type: &item.entity_type,
                    entity_id,
                    sport: &sport,
                    trigger_type: "periodic",
                    trigger_payload: &trigger_payload,
                },
                id,
                out,
            )
            .await?;
        }
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Classifier source contexts remain for Scout"),
            Duration::from_secs(1),
        ));
    }
    publication.commit_final().await?;
    if let (Some(id), Some(out)) = (product_row_id, output.as_ref()) {
        record_ledger(
            pool,
            &LedgerSubject {
                entity_type: &item.entity_type,
                entity_id,
                sport: &sport,
                trigger_type: "periodic",
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
#[path = "delivery_tests.rs"]
mod delivery_tests;

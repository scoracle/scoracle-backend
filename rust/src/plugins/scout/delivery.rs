//! Source-level Scout review before measured-profile publication.
use super::prompt::{RatingBuild, RATING_TEMPERATURE};
use super::publish::{insert_stat_summary, record_ledger, LedgerSubject};
use super::{create, record_rating_completed, RatingOutput};
use crate::harness::model::Inference;
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::Studio;
use crate::plugins::harvester::delivery::load_for_character;
use crate::plugins::scout::performance::current_season;
use crate::plugins::scout::prompt::{build_rating_request, RatingReq};
use crate::tools::source::SourceContext;
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use std::time::Duration;

const POLICY_VERSION: &str = "scout-source-v4-durable-evidence";

/// Only a source-linked structured record can turn roster/availability reporting
/// into a Scout rating trigger. A Harvester tag or model quote alone is not enough.
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
                 ORDER BY applied_at DESC,id DESC LIMIT 1 FOR SHARE",
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
                 ORDER BY applied_at DESC,id DESC LIMIT 1 FOR SHARE",
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

fn performance_supported(contract: &str, scores: &serde_json::Value) -> bool {
    // Only the current receipt has the measured scalar vocabulary. Old receipts
    // without source-linked records remain unavailable, never a guessed negative.
    contract == crate::plugins::harvester::context::CONTRACT
        && crate::plugins::harvester::policy::CHARACTER_ROUTES
            .iter()
            .filter(|route| route.destination.id == crate::plugins::scout::manifest::MANIFEST.id)
            .flat_map(|route| route.predicates)
            .find(|predicate| predicate.key == "performance")
            .is_some_and(|predicate| {
                scores[predicate.key].as_f64().is_some_and(|score| {
                    score.is_finite() && score <= 1.0 && score >= predicate.threshold
                })
            })
}

fn select_kind(
    performance: bool,
    roster: Option<i64>,
    availability: Option<i64>,
) -> (SourceKind, Option<i64>) {
    if let Some(id) = roster {
        (SourceKind::Roster, Some(id))
    } else if let Some(id) = availability {
        (SourceKind::Availability, Some(id))
    } else if performance {
        (SourceKind::Performance, None)
    } else {
        (SourceKind::None, None)
    }
}

async fn decide(
    connection: &mut PgConnection,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    source: &SourceContext,
) -> Result<SourceDecision> {
    let (contract, scores, provenance): (String, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT contract_version, distributions, model_provenance FROM public.harvester_classifications WHERE id=$1 FOR SHARE",
    ).bind(source.classification_id).fetch_one(&mut *connection).await?;
    let identity_resolved = sqlx::query_scalar::<_, i64>(
        "SELECT l.article_id FROM public.harvester_resolved_links l \
         JOIN public.harvester_classifications c ON c.id=$5 AND c.article_id=l.article_id \
         WHERE l.article_id=$1 AND l.entity_type=$2 AND l.entity_id=$3 AND l.sport=$4 \
           AND l.body_sha256=c.body_sha256 FOR SHARE OF l",
    )
    .bind(source.article_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(source.classification_id)
    .fetch_optional(&mut *connection)
    .await?
    .is_some();
    let roster = structured_record(
        connection,
        sport,
        entity_type,
        entity_id,
        source.article_id,
        SourceKind::Roster,
    )
    .await?;
    let availability = structured_record(
        connection,
        sport,
        entity_type,
        entity_id,
        source.article_id,
        SourceKind::Availability,
    )
    .await?;
    let (kind, structured_record) = select_kind(
        performance_supported(&contract, &scores),
        roster,
        availability,
    );
    Ok(SourceDecision {
        kind,
        identity_resolved,
        structured_record,
        provenance: json!({"policy": POLICY_VERSION, "contract": contract,
            "scores": scores, "harvester": provenance}),
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
    let mut output: Option<RatingOutput> = None;
    if identity_resolved
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
            components["harvester_trigger"] = json!({
                "source_decision": decision.provenance,
                "article_id": source.article_id,
                "classification_id": source.classification_id,
                "source_role": "trigger_only",
                "scout_kind": match decision.kind { SourceKind::Performance => "performance", SourceKind::Roster => "roster", SourceKind::Availability => "availability", SourceKind::None => "none" },
                "source_quote": quote,
                "structured_record_id": corroborating_record,
            });
            assignment.input_components = components.to_string();
            assignment.input_hash = crate::util::hash_components(&assignment.input_components);
            let mut generated = create(&Studio::new(backend), *assignment).await?;
            generated.provenance.input_ids = vec![source.article_id];
            output = Some(generated);
        }
    }
    let status = if output.is_some() {
        "used"
    } else {
        "relevant_but_unused"
    };
    let reason = if !identity_resolved {
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
        "source_role": "trigger_only",
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
    crate::plugins::harvester::delivery::validate_for_publication(
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
        "UPDATE public.harvester_assignments SET status=$3,reason=$4,product_ref=$5,updated_at=now() \
         WHERE classification_id=$1 AND plugin_id=$2 AND status='pending' \
           AND reason IS DISTINCT FROM 'delivery_held'",
    )
    .bind(source.classification_id).bind(plugin_id).bind(status).bind(reason)
    .bind(json!({
        "source_article_id": source.article_id,
        "source_role": "trigger_only",
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
    let remaining = crate::plugins::harvester::delivery::undelivered_count(
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
            format!("{remaining} Harvester source contexts remain for Scout"),
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
mod tests {
    use super::*;

    #[test]
    fn triggers_use_current_policy_or_applied_records_without_a_model() {
        let contract = crate::plugins::harvester::context::CONTRACT;
        assert!(performance_supported(
            contract,
            &json!({"performance": 0.7})
        ));
        for scores in [
            json!({}),
            json!({"performance": 0.69}),
            json!({"performance": 1.1}),
            json!({"fitness": 1.0}),
        ] {
            assert!(!performance_supported(contract, &scores));
        }
        assert!(!performance_supported(
            "harvest-context-v1",
            &json!({"performance": 1.0})
        ));
        assert_eq!(
            select_kind(true, None, None),
            (SourceKind::Performance, None)
        );
        assert_eq!(
            select_kind(false, Some(7), None),
            (SourceKind::Roster, Some(7))
        );
        assert_eq!(
            select_kind(false, None, Some(9)),
            (SourceKind::Availability, Some(9))
        );
        assert_eq!(select_kind(false, None, None), (SourceKind::None, None));
    }
}

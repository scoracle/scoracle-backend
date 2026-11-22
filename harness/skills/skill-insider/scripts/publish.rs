//! Claim-fenced rumors, scores, source receipts, completion and ledger.
use super::fresh::{Match, Material};
use super::request;
use super::{record_transfer_event, Finding, Reply, Status, TRANSFER_PUBLISHED};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::skill::SkillOutcome;
use crate::harness::skills::classifier::delivery::validate_for_publication;
use crate::harness::Generation;
use crate::tools::source::SourceContext;
use anyhow::{ensure, Context, Result};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashSet;

fn direction_for(relationship: &str) -> &'static str {
    if relationship == "current" {
        "outgoing"
    } else {
        "incoming"
    }
}

fn counterparty<'a>(material: &'a Material, finding: &Finding) -> Result<&'a Match> {
    let candidates = material
        .mentions
        .get(finding.report_index)
        .context("finding report was not delivered")?;
    let matches = candidates
        .iter()
        .filter(|m| {
            m.name == finding.counterparty
                && (if material.subject.entity_type == "team" {
                    matches!(m.entity_type.as_str(), "player" | "person")
                } else {
                    m.entity_type == "team"
                })
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "finding counterparty is not a unique named move partner"
    );
    Ok(matches[0])
}

async fn rumor_heat(
    connection: &mut PgConnection,
    reply: &Reply,
    sources: &[SourceContext],
) -> Result<i16> {
    let findings = reply
        .findings
        .iter()
        .map(|finding| {
            let source = sources
                .get(finding.report_index)
                .context("finding report was not delivered")?;
            // Preserve existing Rust Unicode casing and whitespace identity.
            Ok(serde_json::json!({
                "counterparty": finding.counterparty, "report_index": finding.report_index,
                "status": finding.status, "stage": finding.stage,
                "publisher": source.source.to_lowercase(),
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    sqlx::query_scalar(include_str!("activity.sql"))
        .bind(serde_json::Value::Array(findings))
        .fetch_one(connection)
        .await
        .context("calculate source-grounded rumor heat")
}

#[cfg(test)]
fn reference_rumor_heat(reply: &Reply, sources: &[SourceContext]) -> i16 {
    let mut by_counterparty: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in &reply.findings {
        by_counterparty
            .entry(&finding.counterparty)
            .or_default()
            .push(finding);
    }
    let active = by_counterparty
        .values()
        .flat_map(|findings| {
            let (lead, run) = current_run(findings);
            if lead.status == Status::Reported {
                run
            } else {
                Vec::new()
            }
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return 1;
    }
    let stage = active
        .iter()
        .map(|f| match f.stage.as_deref() {
            Some("here_we_go") => 88,
            Some("advanced_talks") => 72,
            Some("concrete_interest") => 48,
            _ => 28,
        })
        .max()
        .unwrap_or(1);
    let publishers = active
        .iter()
        .map(|f| sources[f.report_index].source.to_lowercase())
        .collect::<HashSet<_>>()
        .len();
    (stage + (publishers.saturating_sub(1) as i32 * 4)).min(99) as i16
}

fn current_run<'a>(findings: &[&'a Finding]) -> (&'a Finding, Vec<&'a Finding>) {
    let mut ordered = findings.to_vec();
    ordered.sort_by_key(|f| f.report_index);
    let lead = ordered[0];
    let selected = ordered
        .into_iter()
        .take_while(|f| f.status == lead.status)
        .collect();
    (lead, selected)
}

async fn insert_rumors(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    material: &Material,
    reply: &Reply,
    generation: &crate::harness::Generation<Reply>,
) -> Result<Vec<i64>> {
    if material.subject.entity_type == "team" {
        return Ok(Vec::new());
    }
    let mut groups: BTreeMap<i32, Vec<&Finding>> = BTreeMap::new();
    for finding in &reply.findings {
        if let Ok(other) = counterparty(material, finding) {
            groups.entry(other.entity_id).or_default().push(finding);
        }
    }
    let mut ids = Vec::new();
    for (team_id, findings) in groups {
        let (lead, selected) = current_run(&findings);
        let mut news_ids = Vec::new();
        let mut names = Vec::new();
        let mut epochs = Vec::new();
        // ponytail: O(n²) contains-scan over a handful of reports per counterparty.
        // HashSet if report counts ever grow.
        for finding in &selected {
            let source = &material.sources[finding.report_index];
            if !news_ids.contains(&source.article_id) {
                news_ids.push(source.article_id);
            }
            if !names.contains(&source.source) {
                names.push(source.source.clone());
            }
            if let Some(epoch) = source.published_at_epoch {
                epochs.push(epoch);
            }
        }
        let direction = if lead.status == Status::Denied {
            None
        } else if material.subject.entity_type == "player" {
            let row = sqlx::query(
                "SELECT COALESCE((SELECT team_id=$3 FROM public.player_current_identity \
                 WHERE player_id=$1 AND sport=$2),false) AS current, \
                 COALESCE((SELECT bool_or(team_id=$3) FROM public.player_stats \
                 WHERE player_id=$1 AND sport=$2),false) AS former",
            )
            .bind(material.subject.entity_id)
            .bind(&material.subject.sport)
            .bind(team_id)
            .fetch_one(&mut **tx)
            .await?;
            if row.get::<bool, _>("current") {
                Some("current")
            } else if row.get::<bool, _>("former") {
                Some("former")
            } else {
                Some("none")
            }
        } else {
            let current: Option<i32> =
                sqlx::query_scalar("SELECT team_id FROM public.persons WHERE id=$1 AND sport=$2")
                    .bind(material.subject.entity_id)
                    .bind(&material.subject.sport)
                    .fetch_one(&mut **tx)
                    .await?;
            if current == Some(team_id) {
                Some("current")
            } else {
                Some("none")
            }
        };
        let source = &material.sources[lead.report_index];
        let heat = if lead.status == Status::Denied {
            0
        } else {
            rumor_heat(
                &mut **tx,
                &Reply {
                    headline: String::new(),
                    card_score: 0,
                    body: String::new(),
                    findings: selected.iter().map(|f| (*f).clone()).collect(),
                },
                &material.sources,
            )
            .await?
        };
        let payload = serde_json::json!({
            "subject": material.subject.name,
            "findings": findings.iter().map(|f| {
                let s = &material.sources[f.report_index];
                serde_json::json!({"article_id":s.article_id,"publisher":s.source,"status":f.status,"evidence_quote":f.evidence_quote})
            }).collect::<Vec<_>>()
        });
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO public.transfer_rumors \
             (team_id,player_id,sport,trigger_type,trigger_payload,heat,heat_components, \
              is_rumor,direction,stage,model_summary,source_attribution,input_news_ids, \
              model_version,prompt_version,rumor_updated_at,source_count,source_names, \
              source_latest_at,source_oldest_at,input_hash,subject_type) \
             VALUES($1,$2,$3,'classifier',$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14, \
                    COALESCE(to_timestamp($15::double precision),now()),$16,$17, \
                    to_timestamp($15::double precision),to_timestamp($18::double precision),$19,$20) \
             RETURNING id"
        )
        .bind(team_id).bind(material.subject.entity_id).bind(&material.subject.sport)
        .bind(payload).bind(heat)
        .bind(serde_json::json!({"source_count":names.len(),"status":lead.status,"stage":lead.stage}))
        .bind(lead.status == Status::Reported)
        .bind(direction.map(direction_for))
        .bind(lead.stage.as_deref())
        .bind(format!("{}: {}", source.source, lead.evidence_quote))
        .bind(&source.source).bind(&news_ids)
        .bind(&generation.provenance.model_version).bind(generation.provenance.prompt_version)
        .bind(epochs.iter().max().map(|epoch| *epoch as f64)).bind(names.len() as i32).bind(&names)
        .bind(epochs.iter().min().map(|epoch| *epoch as f64))
        .bind(generation.provenance.input_hash.as_deref()).bind(&material.subject.entity_type)
        .fetch_one(&mut **tx).await?;
        ids.push(id);
    }
    Ok(ids)
}

pub(super) async fn commit(
    pool: &PgPool,
    item: &Item,
    material: &Material,
    generation: &Generation<Reply>,
) -> Result<SkillOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(SkillOutcome::Superseded);
    };
    validate_for_publication(
        publication.transaction(),
        crate::harness::skills::insider::manifest::MANIFEST
            .id
            .as_str(),
        &item.entity_type,
        item.entity_id_i32()?,
        &material.subject.sport,
        &material.sources,
    )
    .await?;
    let score = generation.product.card_score;
    let rumor_ids = insert_rumors(
        publication.transaction(),
        material,
        &generation.product,
        generation,
    )
    .await?;
    let score_id = {
        let previous: Option<i16> = sqlx::query_scalar(
            "SELECT score FROM public.insider_scores WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 AND scoring_version=$4 ORDER BY generated_at DESC,id DESC LIMIT 1"
        ).bind(&item.entity_type).bind(item.entity_id_i32()?).bind(&material.subject.sport)
            .bind(crate::tools::source::ACTIVITY_SCORING_VERSION)
            .fetch_optional(&mut **publication.transaction()).await?;
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO public.insider_scores \
             (sport,entity_type,entity_id,score,previous_score,read,headline,model_version,prompt_version,input_hash,scoring_version) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id"
        ).bind(&material.subject.sport).bind(&item.entity_type).bind(item.entity_id_i32()?)
            .bind(score).bind(previous).bind(&generation.product.body)
            .bind(&generation.product.headline)
            .bind(&generation.provenance.model_version).bind(generation.provenance.prompt_version)
            .bind(generation.provenance.input_hash.as_deref())
            .bind(crate::tools::source::ACTIVITY_SCORING_VERSION)
            .fetch_one(&mut **publication.transaction()).await?
    };
    for (index, source) in material.sources.iter().enumerate() {
        let has_finding = generation
            .product
            .findings
            .iter()
            .any(|f| f.report_index == index);
        crate::harness::skills::classifier::delivery::settle(
            publication.transaction(),
            crate::harness::skills::insider::manifest::MANIFEST
                .id
                .as_str(),
            &[source.classification_id],
            if has_finding { "used" } else { "abstained" },
            if has_finding {
                None
            } else {
                Some("No source-grounded move for this entity")
            },
            serde_json::json!({"score_id":score_id,"rumor_ids":rumor_ids}),
        )
        .await?;
    }
    record_transfer_event(
        publication.transaction(),
        item,
        TRANSFER_PUBLISHED,
        &item.entity_type,
        item.entity_id_i32()?,
        Some(&score_id.to_string()),
    )
    .await?;
    let remaining = crate::harness::skills::classifier::delivery::undelivered_count(
        publication.transaction(),
        crate::harness::skills::insider::manifest::MANIFEST
            .id
            .as_str(),
        &item.entity_type,
        item.entity_id_i32()?,
        &material.subject.sport,
    )
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
    } else {
        publication.commit_final().await?;
    }
    let product_ids = std::iter::once(score_id)
        .chain(rumor_ids)
        .collect::<Vec<_>>();
    insert_generation_ledger_best_effort(pool, generation,
            LedgerSpec {
                plugin_id: crate::harness::skills::insider::manifest::MANIFEST.plugin_id,
                stage: "transfers", lens: "insider",
                role: crate::harness::skills::insider::manifest::ROUTE,
                product_table: "insider_scores",
                output_contract_version: request::OUTPUT_CONTRACT_VERSION,
            },
            LedgerEvent {
                entity_type: &item.entity_type, entity_id: item.entity_id_i32()?, sport: &material.subject.sport,
                pair_entity: None, trigger_type: "classifier", trigger_payload: serde_json::json!({}),
                product_row_ids: product_ids,
                included_evidence: serde_json::json!({"article_ids":material.sources.iter().map(|s| s.article_id).collect::<Vec<_>>()}),
                excluded_evidence: serde_json::json!([]),
                context_budget: generation.context_budget(serde_json::json!({"num_predict":request::NUM_PREDICT})),
                parser_outcome: "parsed",
            }).await;
    Ok(if remaining > 0 {
        SkillOutcome::deferred(
            "more complete Insider reports remain",
            std::time::Duration::from_secs(1),
        )
    } else {
        SkillOutcome::Committed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::meta::EntityMeta;

    include!("../../../tests/internal/insider/publish.rs");

    #[test]
    fn counterparties_require_one_matching_identity_in_the_delivered_report() {
        let mut material = Material {
            database_record: None,
            held: vec![],
            subject: EntityMeta {
                name: "Jordan Sample".into(),
                entity_type: "player".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            sources: vec![],
            reports: vec![],
            mentions: vec![vec![Match {
                name: "Test Club".into(),
                entity_type: "team".into(),
                entity_id: 2,
            }]],
        };
        let mut finding = Finding {
            report_index: 0,
            counterparty: "Test Club".into(),
            status: Status::Reported,
            stage: Some("speculation".into()),
            evidence_quote: "Publisher quote".into(),
        };
        assert_eq!(counterparty(&material, &finding).unwrap().entity_id, 2);
        finding.counterparty = "Other Club".into();
        assert!(counterparty(&material, &finding).is_err());
        finding.counterparty = "Test Club".into();
        material.mentions[0].push(Match {
            name: "Test Club".into(),
            entity_type: "team".into(),
            entity_id: 3,
        });
        assert!(counterparty(&material, &finding).is_err());
        material.mentions[0].pop();
        material.mentions[0][0].entity_type = "player".into();
        assert!(counterparty(&material, &finding).is_err());
        finding.report_index = 1;
        assert!(counterparty(&material, &finding).is_err());
    }

    #[test]
    fn newest_denial_retires_older_reports_without_reusing_stale_corroboration() {
        let finding = |report_index, status, stage| Finding {
            report_index,
            counterparty: "Cleveland Browns".into(),
            status,
            stage,
            evidence_quote: "Exact source quote".into(),
        };
        let newest = finding(0, Status::Reported, Some("concrete_interest".into()));
        let intervening = finding(1, Status::Denied, None);
        let oldest = finding(2, Status::Reported, Some("speculation".into()));
        let (lead, selected) = current_run(&[&oldest, &newest, &intervening]);
        assert_eq!(lead.status, Status::Reported);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].report_index, 0);
        let (lead, selected) = current_run(&[&oldest, &intervening]);
        assert_eq!(lead.status, Status::Denied);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].report_index, 1);
        let sources = (0..3)
            .map(|article_id| SourceContext {
                classifier_world: None,
                classification_id: article_id,
                article_id,
                headline: String::new(),
                context: String::new(),
                source: format!("Wire {article_id}"),
                published_at_epoch: None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            reference_rumor_heat(
                &Reply {
                    headline: String::new(),
                    card_score: 0,
                    body: String::new(),
                    findings: vec![oldest.clone(), intervening.clone(), newest],
                },
                &sources,
            ),
            48
        );
        assert_eq!(
            reference_rumor_heat(
                &Reply {
                    headline: String::new(),
                    card_score: 0,
                    body: String::new(),
                    findings: vec![oldest, intervening],
                },
                &sources,
            ),
            1
        );
    }
}

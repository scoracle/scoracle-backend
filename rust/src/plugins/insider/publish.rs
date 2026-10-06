//! Claim-fenced rumors, scores, source receipts, completion and ledger.
use super::prompt::{self, Match, Material};
use super::{record_transfer_event, Finding, Reply, Status, TRANSFER_PUBLISHED};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::Generation;
use crate::plugins::harvester::delivery::{
    validate_for_publication, validate_insider_subject_for_publication, SourceContext,
};
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
    let expected = if material.subject.entity_type == "team" {
        "subject"
    } else {
        "team"
    };
    let candidates = material
        .mentions
        .get(finding.report_index)
        .context("finding report was not delivered")?;
    let matches = candidates
        .iter()
        .filter(|m| {
            m.name == finding.counterparty
                && (if expected == "team" {
                    m.entity_type == "team"
                } else {
                    matches!(m.entity_type.as_str(), "player" | "person")
                })
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "finding counterparty is not a unique named move partner"
    );
    Ok(matches[0])
}

async fn activity_score(
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
        .context("calculate Insider source-grounded activity")
}

#[cfg(test)]
fn reference_activity_score(reply: &Reply, sources: &[SourceContext]) -> i16 {
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
        let other = counterparty(material, finding)?;
        groups.entry(other.entity_id).or_default().push(finding);
    }
    let mut ids = Vec::new();
    for (team_id, findings) in groups {
        let (lead, selected) = current_run(&findings);
        let mut news_ids = Vec::new();
        let mut names = Vec::new();
        let mut epochs = Vec::new();
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
            activity_score(
                &mut **tx,
                &Reply {
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
             VALUES($1,$2,$3,'harvester',$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14, \
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
) -> Result<PluginOutcome> {
    // Resolve every named partner before opening a publication transaction. Unknown
    // or ambiguous co-mentions fail the claim rather than inventing a DB identity.
    for finding in &generation.product.findings {
        counterparty(material, finding)?;
    }
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    if item.entity_type == "team" {
        validate_for_publication(
            publication.transaction(),
            crate::plugins::insider::manifest::MANIFEST.id.as_str(),
            "team",
            item.entity_id_i32()?,
            &material.subject.sport,
            &material.sources,
        )
        .await?;
    } else {
        validate_insider_subject_for_publication(
            publication.transaction(),
            &item.entity_type,
            item.entity_id_i32()?,
            &material.subject.sport,
            &material.sources,
        )
        .await?;
    }
    let score = activity_score(
        publication.transaction(),
        &generation.product,
        &material.sources,
    )
    .await?;
    let rumor_ids = insert_rumors(
        publication.transaction(),
        material,
        &generation.product,
        generation,
    )
    .await?;
    let score_id = if item.entity_type != "person" {
        let previous: Option<i16> = sqlx::query_scalar(
            "SELECT score FROM public.insider_scores WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 ORDER BY generated_at DESC,id DESC LIMIT 1"
        ).bind(&item.entity_type).bind(item.entity_id_i32()?).bind(&material.subject.sport)
            .fetch_optional(&mut **publication.transaction()).await?;
        Some(sqlx::query_scalar::<_, i64>(
            "INSERT INTO public.insider_scores \
             (sport,entity_type,entity_id,score,previous_score,read,headline,model_version,prompt_version,input_hash) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id"
        ).bind(&material.subject.sport).bind(&item.entity_type).bind(item.entity_id_i32()?)
            .bind(score).bind(previous).bind(&generation.product.body)
            .bind(format!("{}: transfer picture", material.subject.name))
            .bind(&generation.provenance.model_version).bind(generation.provenance.prompt_version)
            .bind(generation.provenance.input_hash.as_deref())
            .fetch_one(&mut **publication.transaction()).await?)
    } else {
        None
    };
    for (index, source) in material.sources.iter().enumerate() {
        let has_finding = generation
            .product
            .findings
            .iter()
            .any(|f| f.report_index == index);
        if item.entity_type == "team" {
            let changed = sqlx::query(
                "UPDATE public.harvester_assignments SET status=$3,reason=$4,product_ref=$5,updated_at=now() \
                 WHERE classification_id=$1 AND plugin_id=$2 AND status='pending'"
            ).bind(source.classification_id).bind(crate::plugins::insider::manifest::MANIFEST.id.as_str())
                .bind(if has_finding { "used" } else { "abstained" })
                .bind(if has_finding { None } else { Some("No source-grounded move for this team") })
                .bind(serde_json::json!({"score_id":score_id,"rumor_ids":rumor_ids}))
                .execute(&mut **publication.transaction()).await?;
            ensure!(
                changed.rows_affected() == 1,
                "Insider assignment changed during call"
            );
        } else {
            let has_reported = generation
                .product
                .findings
                .iter()
                .any(|f| f.report_index == index && f.status == Status::Reported);
            let changed = sqlx::query(
                "UPDATE public.harvester_insider_pairs p SET status=$4,product_ref=$5,updated_at=now() \
                 FROM public.harvester_classifications c \
                 WHERE p.classification_id=c.id AND c.article_id=$1 AND c.sport=$6 \
                   AND p.subject_type=$2 AND p.subject_id=$3 AND p.status='pending'"
            ).bind(source.article_id).bind(&item.entity_type).bind(item.entity_id_i32()?)
                .bind(if has_reported { "rumor" } else { "cleared" })
                .bind(serde_json::json!({
                    "score_id":score_id,"rumor_ids":rumor_ids,"body":generation.product.body,
                    "model_version":generation.provenance.model_version,
                    "input_hash":generation.provenance.input_hash,
                })).bind(&material.subject.sport)
                .execute(&mut **publication.transaction()).await?;
            ensure!(
                changed.rows_affected() >= 1,
                "Insider subject obligation changed during call"
            );
        }
    }
    if let Some(score_id) = score_id {
        record_transfer_event(
            publication.transaction(),
            item,
            TRANSFER_PUBLISHED,
            &item.entity_type,
            item.entity_id_i32()?,
            Some(&score_id.to_string()),
        )
        .await?;
    }
    publication.commit_final().await?;
    let product_ids = score_id.into_iter().chain(rumor_ids).collect::<Vec<_>>();
    if !product_ids.is_empty() {
        insert_generation_ledger_best_effort(pool, generation,
            LedgerSpec {
                plugin_id: crate::plugins::insider::manifest::MANIFEST.id.as_str(),
                stage: "transfers", lens: "insider",
                role: crate::plugins::insider::manifest::ROUTE,
                product_table: if item.entity_type == "person" { "transfer_rumors" } else { "insider_scores" },
                output_contract_version: prompt::OUTPUT_CONTRACT_VERSION,
            },
            LedgerEvent {
                entity_type: &item.entity_type, entity_id: item.entity_id_i32()?, sport: &material.subject.sport,
                pair_entity: None, trigger_type: "harvester", trigger_payload: serde_json::json!({}),
                product_row_ids: product_ids,
                included_evidence: serde_json::json!({"article_ids":material.sources.iter().map(|s| s.article_id).collect::<Vec<_>>()}),
                excluded_evidence: serde_json::json!([]),
                context_budget: generation.context_budget(serde_json::json!({"num_predict":prompt::NUM_PREDICT})),
                parser_outcome: "parsed",
            }).await;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::meta::EntityMeta;

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL; pure read-only SQL, no schema needed"]
    async fn sql_activity_matches_legacy_runs_stages_publishers_and_caps() -> Result<()> {
        let mut connection = sqlx::Connection::connect(
            &std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"),
        )
        .await?;
        let stages = [
            "speculation",
            "concrete_interest",
            "advanced_talks",
            "here_we_go",
        ];
        let names = [
            "Wire", "WIRE", " Wire ", "ΣΟΣ", "σος", "İ", "i\u{307}", "Other",
        ];
        let sources = (0..32)
            .map(|i| SourceContext {
                classification_id: i,
                article_id: i,
                headline: String::new(),
                context: String::new(),
                source: names[i as usize % names.len()].into(),
                published_at_epoch: Some(100 - i),
            })
            .collect::<Vec<_>>();
        let mut cases = 0;
        // Every denial/report pattern through six reports, including a restart
        // after a denial; model finding order must never replace source order.
        for count in 0..=6 {
            for mask in 0..(1 << count) {
                for stage in 0..stages.len() {
                    let findings = (0..count)
                        .map(|i| {
                            let status = if mask & (1 << i) == 0 {
                                Status::Reported
                            } else {
                                Status::Denied
                            };
                            Finding {
                                report_index: i,
                                counterparty: "Club".into(),
                                status,
                                stage: (status == Status::Reported)
                                    .then(|| stages[(stage + i) % stages.len()].into()),
                                evidence_quote: "Exact quote".into(),
                            }
                        })
                        .collect::<Vec<_>>();
                    for order in 0..3 {
                        let mut findings = findings.clone();
                        if order == 1 {
                            findings.reverse();
                        }
                        if order == 2 && findings.len() > 1 {
                            findings.rotate_left(1);
                        }
                        let reply = Reply {
                            body: String::new(),
                            findings,
                        };
                        assert_eq!(
                            activity_score(&mut connection, &reply, &sources).await?,
                            reference_activity_score(&reply, &sources),
                            "count={count} mask={mask} stage={stage} order={order}"
                        );
                        cases += 1;
                    }
                }
            }
        }
        // Publisher bonus/cap and identity match the original lowercase-only
        // comparison; whitespace and Unicode are deliberately not SQL-normalized.
        for count in 1..=32 {
            for stage in stages {
                for unique in [false, true] {
                    let mut sources = sources.clone();
                    if unique {
                        for (i, source) in sources.iter_mut().enumerate() {
                            source.source = format!("Wire {i}");
                        }
                    }
                    let mut findings = (0..count)
                        .map(|i| Finding {
                            report_index: i,
                            counterparty: "Club".into(),
                            status: Status::Reported,
                            stage: Some(stage.into()),
                            evidence_quote: "Exact quote".into(),
                        })
                        .collect::<Vec<_>>();
                    // These are separate exact-name counterparties, each with
                    // its own denial cutoff, but publisher bonus is global.
                    findings.extend([
                        Finding {
                            report_index: 0,
                            counterparty: "club".into(),
                            status: Status::Denied,
                            stage: None,
                            evidence_quote: "Exact quote".into(),
                        },
                        Finding {
                            report_index: 1,
                            counterparty: "club".into(),
                            status: Status::Reported,
                            stage: Some("here_we_go".into()),
                            evidence_quote: "Exact quote".into(),
                        },
                        Finding {
                            report_index: 2,
                            counterparty: "Club ".into(),
                            status: Status::Reported,
                            stage: Some(stage.into()),
                            evidence_quote: "Exact quote".into(),
                        },
                    ]);
                    findings.reverse();
                    let reply = Reply {
                        body: String::new(),
                        findings,
                    };
                    assert_eq!(
                        activity_score(&mut connection, &reply, &sources).await?,
                        reference_activity_score(&reply, &sources)
                    );
                    let group = reply
                        .findings
                        .iter()
                        .filter(|f| f.counterparty == "Club")
                        .collect::<Vec<_>>();
                    let (lead, selected) = current_run(&group);
                    assert_eq!(lead.status, Status::Reported);
                    let selected_reply = Reply {
                        body: String::new(),
                        findings: selected.into_iter().cloned().collect(),
                    };
                    assert_eq!(
                        activity_score(&mut connection, &selected_reply, &sources).await?,
                        reference_activity_score(&selected_reply, &sources)
                    );
                    cases += 2;
                }
            }
        }
        let missing_source = Reply {
            body: String::new(),
            findings: vec![Finding {
                report_index: sources.len(),
                counterparty: "Club".into(),
                status: Status::Reported,
                stage: Some("speculation".into()),
                evidence_quote: "Exact quote".into(),
            }],
        };
        assert!(activity_score(&mut connection, &missing_source, &sources)
            .await
            .is_err());
        eprintln!("{cases} SQL/legacy parity cases verified; missing source rejected");
        Ok(())
    }

    #[test]
    fn counterparties_require_one_matching_identity_in_the_delivered_report() {
        let mut material = Material {
            subject: EntityMeta {
                name: "Jordan Sample".into(),
                entity_type: "player".into(),
                entity_id: 1,
                sport: "NFL".into(),
            },
            sources: vec![],
            reports: vec![],
            history: vec![],
            source_records: vec![],
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
                classification_id: article_id,
                article_id,
                headline: String::new(),
                context: String::new(),
                source: format!("Wire {article_id}"),
                published_at_epoch: None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            reference_activity_score(
                &Reply {
                    body: String::new(),
                    findings: vec![oldest.clone(), intervening.clone(), newest],
                },
                &sources,
            ),
            48
        );
        assert_eq!(
            reference_activity_score(
                &Reply {
                    body: String::new(),
                    findings: vec![oldest, intervening],
                },
                &sources,
            ),
            1
        );
    }
}

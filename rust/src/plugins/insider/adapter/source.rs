//! One Harvester source package, one Insider call, one fenced publication per entity.
use super::*;
use crate::plugins::harvester::delivery::{
    load_for_character, load_for_insider_subject, validate_for_publication,
    validate_insider_subject_for_publication, SourceContext,
};
use crate::plugins::insider::cognition::{
    self as insider, ContextMention, ContextReport, SourceFinding, SourceReply, SourceStatus,
};
use crate::plugins::memories::{self as study, HistoryItem, ReportingHistory};
use crate::plugins::meta::EntityMeta;
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::studio::model::Inference;
use crate::studio::Studio;
use anyhow::ensure;
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, HashSet};

const HISTORY: ReportingHistory = ReportingHistory {
    lookback_seconds: 90 * 24 * 60 * 60,
    max_reports: 6,
    budget_bytes: 1800,
    grouped: false,
};

#[derive(Clone)]
struct Match {
    name: String,
    entity_type: String,
    entity_id: i32,
}

struct Material {
    subject: EntityMeta,
    sources: Vec<SourceContext>,
    reports: Vec<ContextReport>,
    mentions: Vec<Vec<Match>>,
    history: Vec<HistoryItem>,
    source_records: Vec<study::SourceRecord>,
}

async fn load_material(pool: &PgPool, item: &Item) -> Result<Material> {
    let sport = item.sport.to_uppercase();
    let entity_id = item.entity_id_i32()?;
    let name: String = if item.entity_type == "person" {
        sqlx::query_scalar(
            "SELECT full_name FROM public.persons WHERE id=$1 AND sport=$2 AND kind='coach'",
        )
        .bind(entity_id)
        .bind(&sport)
        .fetch_one(pool)
        .await?
    } else {
        crate::evidence::corpus::lookup_entity_name(pool, &item.entity_type, entity_id, &sport)
            .await?
    };
    let subject = EntityMeta {
        name,
        entity_type: item.entity_type.clone(),
        entity_id,
        sport: sport.clone(),
    };
    let sources = if item.entity_type == "team" {
        load_for_character(
            pool,
            crate::plugins::insider::manifest::MANIFEST.id.as_str(),
            "team",
            entity_id,
            &sport,
        )
        .await?
    } else {
        load_for_insider_subject(pool, &item.entity_type, entity_id, &sport).await?
    };
    let article_ids: Vec<i64> = sources.iter().map(|s| s.article_id).collect();
    let rows = sqlx::query(
        "SELECT m.article_id,m.entity_type,m.entity_id,COALESCE(t.name,p.name,pp.full_name) AS name \
         FROM public.harvester_entity_mentions m \
         LEFT JOIN public.teams t ON m.entity_type='team' AND t.id=m.entity_id AND t.sport=m.sport \
         LEFT JOIN public.players p ON m.entity_type='player' AND p.id=m.entity_id AND p.sport=m.sport \
         LEFT JOIN public.persons pp ON m.entity_type='person' AND pp.id=m.entity_id AND pp.sport=m.sport \
         WHERE m.article_id=ANY($1) AND m.sport=$2 AND (m.entity_type,m.entity_id)<>($3,$4) \
         ORDER BY m.article_id,m.entity_type,name"
    ).bind(&article_ids).bind(&sport).bind(&item.entity_type).bind(entity_id).fetch_all(pool).await?;
    let mut by_article: HashMap<i64, Vec<Match>> = HashMap::new();
    for row in rows {
        if let Some(name) = row.get::<Option<String>, _>("name") {
            let found = by_article.entry(row.get("article_id")).or_default();
            let mention = Match {
                name,
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
            };
            if !found
                .iter()
                .any(|x| x.entity_type == mention.entity_type && x.entity_id == mention.entity_id)
            {
                found.push(mention);
            }
        }
    }
    let mut reports = Vec::with_capacity(sources.len());
    let mut mentions = Vec::with_capacity(sources.len());
    for source in &sources {
        let found = by_article.remove(&source.article_id).unwrap_or_default();
        reports.push(ContextReport {
            publisher: source.source.clone(),
            published_at: source.published_at_epoch.map(crate::util::utc_timestamp),
            headline: source.headline.clone(),
            publisher_excerpt: source.context.clone(),
            co_mentions: found
                .iter()
                .map(|m| ContextMention {
                    name: m.name.clone(),
                    entity_type: m.entity_type.clone(),
                })
                .collect(),
        });
        mentions.push(found);
    }
    let history = load_history(pool, &subject, &sources).await?;
    let publishers = sources
        .iter()
        .map(|source| source.source.clone())
        .collect::<Vec<_>>();
    let source_records = study::source_records(pool, &sport, &publishers).await?;
    Ok(Material {
        subject,
        sources,
        reports,
        mentions,
        history,
        source_records,
    })
}

pub(crate) async fn preview(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<String>> {
    let item = Item {
        stage: crate::plugins::insider::manifest::TASK,
        entity_type: entity_type.into(),
        entity_id: i64::from(entity_id),
        sport: sport.into(),
        input_version: None,
        attempts: 0,
        claim_token: None,
    };
    let material = load_material(pool, &item).await?;
    Ok((!material.sources.is_empty()).then(|| {
        insider::assemble_context(
            &material.subject,
            &material.reports,
            &material.history,
            &material.source_records,
        )
        .render()
    }))
}

async fn load_history(
    pool: &PgPool,
    subject: &EntityMeta,
    sources: &[SourceContext],
) -> Result<Vec<HistoryItem>> {
    if sources.is_empty() || subject.entity_type == "person" {
        return Ok(Vec::new());
    }
    let before = sources.iter().filter_map(|s| s.published_at_epoch).min();
    let Some(before) = before else {
        return Ok(Vec::new());
    };
    let excluded: Vec<i64> = sources.iter().map(|s| s.article_id).collect();
    let prior: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT u.article_id FROM public.transfer_rumors r \
         CROSS JOIN LATERAL unnest(r.input_news_ids) AS u(article_id) \
         WHERE r.sport=$1 AND r.is_rumor IS NOT NULL \
           AND (($2='team' AND r.team_id=$3) OR ($2='player' AND r.subject_type='player' AND r.player_id=$3)) \
         ORDER BY u.article_id LIMIT 200"
    ).bind(&subject.sport).bind(&subject.entity_type).bind(subject.entity_id).fetch_all(pool).await?;
    if prior.is_empty() {
        return Ok(Vec::new());
    }
    let report = study::reporting_scope(
        pool,
        subject,
        before - HISTORY.lookback_seconds,
        before,
        &excluded,
        HISTORY.max_reports,
        &prior,
        None,
    )
    .await?;
    HISTORY.select(&report, subject, Some(before), &excluded, |_| true)
}

fn counterparty<'a>(material: &'a Material, finding: &SourceFinding) -> Result<&'a Match> {
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

fn activity_score(reply: &SourceReply, sources: &[SourceContext]) -> i16 {
    let mut by_counterparty: BTreeMap<&str, Vec<&SourceFinding>> = BTreeMap::new();
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
            if lead.status == SourceStatus::Reported {
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

fn current_run<'a>(findings: &[&'a SourceFinding]) -> (&'a SourceFinding, Vec<&'a SourceFinding>) {
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
    reply: &SourceReply,
    generation: &crate::studio::Generation<SourceReply>,
) -> Result<Vec<i64>> {
    if material.subject.entity_type == "team" {
        return Ok(Vec::new());
    }
    let mut groups: BTreeMap<i32, Vec<&SourceFinding>> = BTreeMap::new();
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
        let direction = if lead.status == SourceStatus::Denied {
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
        let heat = if lead.status == SourceStatus::Denied {
            0
        } else {
            activity_score(
                &SourceReply {
                    body: String::new(),
                    findings: selected.iter().map(|f| (*f).clone()).collect(),
                },
                &material.sources,
            )
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
        .bind(lead.status == SourceStatus::Reported)
        .bind(direction.map(crate::plugins::insider::cognition::direction_for))
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

pub(super) async fn execute(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<PluginOutcome> {
    let material = load_material(pool, item).await?;
    if material.sources.is_empty() {
        let Some(publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    let backend = models.inference(crate::plugins::insider::manifest::ROUTE)?;
    execute_prepared(pool, backend.as_ref(), models.voice_num_ctx, item, material).await
}

#[cfg(test)]
pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let material = load_material(pool, item).await?;
    execute_prepared(pool, backend, num_ctx, item, material).await
}

async fn execute_prepared(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
    material: Material,
) -> Result<PluginOutcome> {
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.pipeline_work \
         WHERE stage=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4 \
           AND status='running' AND claim_token=$5::uuid \
           AND running_input_version IS NOT DISTINCT FROM $6)",
    )
    .bind(item.stage.as_str())
    .bind(&item.entity_type)
    .bind(item.entity_id)
    .bind(&item.sport)
    .bind(item.require_claim_token()?)
    .bind(item.input_version.as_deref())
    .fetch_one(pool)
    .await?;
    if !current {
        return Ok(PluginOutcome::Superseded);
    }
    let generation = insider::create_reading(
        &Studio::new(backend),
        &material.subject,
        &material.reports,
        &material.history,
        &material.source_records,
        num_ctx,
    )
    .await?;
    // Resolve every named partner before opening a publication transaction. Unknown
    // or ambiguous co-mentions fail the claim rather than inventing a DB identity.
    for finding in &generation.product.findings {
        counterparty(&material, finding)?;
    }
    let score = activity_score(&generation.product, &material.sources);
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
    let rumor_ids = insert_rumors(
        publication.transaction(),
        &material,
        &generation.product,
        &generation,
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
                .any(|f| f.report_index == index && f.status == SourceStatus::Reported);
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
        insert_generation_ledger_best_effort(pool, &generation,
            LedgerSpec {
                plugin_id: crate::plugins::insider::manifest::MANIFEST.id.as_str(),
                stage: "transfers", lens: "insider",
                role: crate::plugins::insider::manifest::ROUTE,
                product_table: if item.entity_type == "person" { "transfer_rumors" } else { "insider_scores" },
                output_contract_version: insider::READING_OUTPUT_CONTRACT_VERSION,
            },
            LedgerEvent {
                entity_type: &item.entity_type, entity_id: item.entity_id_i32()?, sport: &material.subject.sport,
                pair_entity: None, trigger_type: "harvester", trigger_payload: serde_json::json!({}),
                product_row_ids: product_ids,
                included_evidence: serde_json::json!({"article_ids":material.sources.iter().map(|s| s.article_id).collect::<Vec<_>>()}),
                excluded_evidence: serde_json::json!([]),
                context_budget: generation.context_budget(serde_json::json!({"num_predict":insider::READING_NUM_PREDICT})),
                parser_outcome: "parsed",
            }).await;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_denial_retires_older_reports_without_reusing_stale_corroboration() {
        let finding = |report_index, status, stage| SourceFinding {
            report_index,
            counterparty: "Cleveland Browns".into(),
            status,
            stage,
            evidence_quote: "Exact source quote".into(),
        };
        let newest = finding(0, SourceStatus::Reported, Some("concrete_interest".into()));
        let intervening = finding(1, SourceStatus::Denied, None);
        let oldest = finding(2, SourceStatus::Reported, Some("speculation".into()));
        let (lead, selected) = current_run(&[&oldest, &newest, &intervening]);
        assert_eq!(lead.status, SourceStatus::Reported);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].report_index, 0);
        let (lead, selected) = current_run(&[&oldest, &intervening]);
        assert_eq!(lead.status, SourceStatus::Denied);
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
            activity_score(
                &SourceReply {
                    body: String::new(),
                    findings: vec![oldest.clone(), intervening.clone(), newest],
                },
                &sources,
            ),
            48
        );
        assert_eq!(
            activity_score(
                &SourceReply {
                    body: String::new(),
                    findings: vec![oldest, intervening],
                },
                &sources,
            ),
            1
        );
    }
}

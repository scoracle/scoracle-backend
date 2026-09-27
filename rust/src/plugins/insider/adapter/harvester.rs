//! Packet-free source preparation for the claimed Harvester Insider path.
use super::*;
use crate::plugins::harvester::delivery::load_for_character;
use crate::plugins::harvester::delivery::SourceContext;
use crate::studio::model::Inference;
use crate::studio::{Generation, GenerationCall, Parser};
use anyhow::ensure;
use serde::Deserialize;

mod identity;
mod wrap;

const SOURCE_PROMPT_VERSION: &str = "transfer-source-v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceVerdict {
    is_rumor: bool,
    subject: String,
    stage: String,
    evidence_quote: String,
}

struct SourceVerdictParser<'a> {
    source: &'a SourceContext,
    candidate: &'a TransferCandidate,
}

impl Parser<SourceVerdict> for SourceVerdictParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<SourceVerdict>> {
        let verdict: SourceVerdict = serde_json::from_str(raw)?;
        ensure!(
            verdict
                .subject
                .trim()
                .eq_ignore_ascii_case(&self.candidate.player_name),
            "transfer verdict changed the resolved subject"
        );
        if verdict.is_rumor {
            ensure!(
                matches!(
                    verdict.stage.as_str(),
                    "speculation" | "concrete_interest" | "advanced_talks" | "here_we_go"
                ),
                "transfer verdict has invalid stage"
            );
            ensure!(
                !verdict.evidence_quote.trim().is_empty()
                    && verdict.evidence_quote.chars().count() <= 500
                    && (self.source.context.contains(&verdict.evidence_quote)
                        || self.source.headline.contains(&verdict.evidence_quote)),
                "transfer evidence quote is not an exact bounded publisher span"
            );
        } else {
            ensure!(
                verdict.stage.is_empty() && verdict.evidence_quote.is_empty(),
                "cleared transfer verdict must not claim a stage or supporting quote"
            );
        }
        Ok(Some(verdict))
    }
}

fn verdict_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object", "additionalProperties": false,
        "required": ["is_rumor","subject","stage","evidence_quote"],
        "properties": {
            "is_rumor": {"type":"boolean"},
            "subject": {"type":"string"},
            "stage": {"type":"string"},
            "evidence_quote": {"type":"string"}
        }
    })
}

fn source_prompt(
    team_name: &str,
    candidate: &TransferCandidate,
    sport: &str,
    relationship: &str,
    source: &SourceContext,
    memory: &str,
) -> String {
    format!(
        "Sport: {sport}\nTeam: {team_name}\nResolved subject: {} {}\nCurrent relationship: {relationship}\n\nIdentity and historical memory (context, not proof of a move):\n{memory}\n\nPublisher: {}\nSource article ID: {}\nExact headline: {}\nExact publisher opening (unchanged):\n{}\n\nDecide whether this source reports this exact subject joining or leaving this team now. Co-mentions, old affiliations, and a coach discussing recruitment are not moves. Return JSON with is_rumor, subject (the resolved name), stage (speculation, concrete_interest, advanced_talks, or here_we_go when true; empty when false), and evidence_quote (an exact continuous span from the headline or opening that supports the move when true; empty when false). Return only the JSON object.",
        candidate.subject_type,
        candidate.player_name,
        source.source,
        source.article_id,
        source.headline,
        source.context
    )
}

pub(super) async fn create_source_pair(
    backend: &dyn Inference,
    mut assignment: PairAssignment,
    source: &SourceContext,
    candidate: &TransferCandidate,
) -> Result<TransferPairOutput> {
    assignment.options.system = Some("Judge one exact publisher source for one resolved transfer subject. Identity, heat, and memory are context, not evidence of a move. Cite an exact source span for every positive. Never invent a move or quote.".into());
    assignment.options.format_schema = Some(verdict_schema());
    let extracted = crate::studio::Studio::new(backend)
        .extract(
            &assignment.prompt,
            &assignment.options,
            &SourceVerdictParser { source, candidate },
            crate::plugins::support::form::structured_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let verdict = extracted
        .value
        .context("transfer source verdict did not commit")?;
    let row = if verdict.is_rumor {
        TransferRow {
            is_rumor: Some(true),
            direction: Some(
                crate::plugins::insider::cognition::direction_for(&assignment.relationship).into(),
            ),
            stage: Some(verdict.stage),
            summary: Some(format!(
                "{} reports: {}",
                source.source, verdict.evidence_quote
            )),
            attribution: Some(source.source.clone()),
            confidence: None,
            model: Some(extracted.model.clone()),
            trigger_payload: serde_json::json!({
                "subject": candidate.player_name,
                "evidence_quote": verdict.evidence_quote,
                "article_id": source.article_id,
                "classification_id": source.classification_id,
            })
            .to_string(),
        }
    } else {
        TransferRow {
            is_rumor: Some(false),
            direction: None,
            stage: None,
            summary: None,
            attribution: Some(source.source.clone()),
            confidence: None,
            model: Some(extracted.model.clone()),
            trigger_payload: serde_json::json!({
                "subject": candidate.player_name,
                "article_id": source.article_id,
                "classification_id": source.classification_id,
                "decision": "cleared",
            })
            .to_string(),
        }
    };
    let outcome = if verdict.is_rumor {
        Outcome::Rumor
    } else {
        Outcome::Cleared
    };
    Ok(Generation::called(
        crate::plugins::insider::cognition::TransferPairProduct {
            player_id: candidate.player_id,
            subject_type: candidate.subject_type.clone(),
            heat: Some(assignment.heat),
            components: assignment.components,
            news_ids: vec![source.article_id],
            prompted_news_ids: vec![source.article_id],
            stale_news_ids: Vec::new(),
            outcome,
            row: Some(row),
            identity_apply_news: assignment.news,
        },
        extracted.model,
        SOURCE_PROMPT_VERSION,
        vec![source.article_id],
        Some(assignment.input_hash),
        call,
    ))
}

/// Build one pair from resolved identities, deterministic Harvester heat, and
/// one hash-verified publisher opening. `None` means the pair has no recent
/// source corpus, never a model rejection.
pub(super) async fn prepare_pair(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    source: &SourceContext,
    candidate: &TransferCandidate,
) -> Result<Option<PairAssignment>> {
    let team_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let (heat, components, _corpus_ids): (Option<i16>, serde_json::Value, Vec<i64>) =
        sqlx::query_as("SELECT heat,components,news_ids FROM public.compute_harvester_transfer_heat($1,$2,$3,$4)")
            .bind(team_id)
            .bind(candidate.player_id)
            .bind(&sport)
            .bind(&candidate.subject_type)
            .fetch_one(pool)
            .await
            .context("compute Harvester transfer heat")?;
    let Some(heat) = heat else {
        return Ok(None);
    };
    let team_name =
        crate::evidence::corpus::lookup_entity_name(pool, "team", team_id, &sport).await?;
    let relationship = match candidate.relationship_override.as_deref() {
        Some(value) => value.to_string(),
        None => team_relationship(pool, team_id, candidate.player_id, &sport).await?,
    };
    let article_ids = [source.article_id];
    let mut request = MemoryRequest::new(
        Mission::Insider,
        &candidate.subject_type,
        candidate.player_id,
        &sport,
    );
    request.pair_team_id = Some(team_id);
    request.current_article_ids = &article_ids;
    request.include_storyline_history = false;
    let memories = memories::load(pool, request).await?;
    let team_identity =
        crate::evidence::memories::load_identity_record(pool, "team", team_id, &sport).await?;
    let mut input: serde_json::Value = serde_json::from_str(&memories.with_input_components(
        &build_transfer_input_components(&article_ids, &components.to_string(), &relationship),
    )?)?;
    input["harvester_source"] = serde_json::json!({
        "contract": crate::plugins::harvester::context::CONTRACT,
        "prompt_version": SOURCE_PROMPT_VERSION,
        "classification_id": source.classification_id,
        "article_id": source.article_id,
        "headline": source.headline,
        "publisher_opening": source.context,
        "source": source.source,
        "team_identity": team_identity,
    });
    let input_components = input.to_string();
    let input_hash = hash_components(&input_components);
    let memory = memories.render_for_model()?;
    let prompt = source_prompt(
        &team_name,
        candidate,
        &sport,
        &relationship,
        source,
        &memory,
    );
    let options = crate::studio::model::GenerateOptions {
        system: Some("Judge one exact publisher source for one resolved transfer subject. Cite an exact span for every positive.".into()),
        temperature: Some(TRANSFER_TEMPERATURE),
        num_predict: TRANSFER_NUM_PREDICT,
        num_ctx: voice_num_ctx,
        json_mode: true,
        format_schema: Some(verdict_schema()),
        format_schema_raw: None,
    };
    let failed_request_body = backend.request_body(&prompt, &options);
    Ok(Some(PairAssignment {
        player_id: candidate.player_id,
        player_name: candidate.player_name.clone(),
        team_name,
        subject_type: candidate.subject_type.clone(),
        heat,
        components: components.to_string(),
        news_ids: article_ids.to_vec(),
        prompted_news_ids: article_ids.to_vec(),
        stale_news_ids: Vec::new(),
        news: vec![NewsItem {
            id: source.article_id,
            title: source.headline.clone(),
            description: source.context.clone(),
            source: source.source.clone(),
        }],
        relationship,
        attribution: source.source.clone(),
        prompt,
        options,
        model_configured: backend.model().to_string(),
        failed_request_body,
        input_hash,
    }))
}

async fn settle_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &Item,
    source: &SourceContext,
) -> Result<i64> {
    let plugin_id = crate::plugins::insider::manifest::MANIFEST.id.as_str();
    let (pending_pairs, rumors, total_pairs): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status='pending'), \
                count(*) FILTER (WHERE status='rumor'), count(*) \
         FROM public.harvester_insider_pairs WHERE classification_id=$1",
    )
    .bind(source.classification_id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        pending_pairs == 0,
        "Insider source still has pending subjects"
    );
    let reason = if rumors > 0 {
        None
    } else if total_pairs == 0 {
        Some("No resolved transfer subject in the delivered source")
    } else {
        Some("No source-grounded transfer move after pair review")
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_assignments SET status=$3,reason=$4,product_ref=$5,updated_at=now() \
         WHERE classification_id=$1 AND plugin_id=$2 AND status='pending'",
    )
    .bind(source.classification_id)
    .bind(plugin_id)
    .bind(if rumors > 0 { "used" } else { "abstained" })
    .bind(reason)
    .bind(serde_json::json!({
        "article_id": source.article_id,
        "rumor_pair_count": rumors,
        "reviewed_pair_count": total_pairs,
    }))
    .execute(&mut **tx)
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "Insider source disposition changed during claim"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND c.entity_type=$2 \
           AND c.entity_id=$3 AND c.sport=$4",
    )
    .bind(plugin_id)
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(item.sport.to_uppercase())
    .fetch_one(&mut **tx)
    .await?;
    Ok(remaining)
}

async fn finish_source(
    pool: &PgPool,
    item: &Item,
    source: &SourceContext,
) -> Result<PluginOutcome> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let remaining = settle_source(publication.transaction(), item, source).await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Harvester source contexts remain for Insider"),
            Duration::from_secs(1),
        ));
    }
    publication.commit_progress().await?;
    Ok(PluginOutcome::deferred(
        "Insider source reviews complete; scored-board wraps remain",
        Duration::from_secs(1),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn insert_source_pair_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &Item,
    candidate: &TransferCandidate,
    output: &TransferPairOutput,
    source_count: i32,
    source_names: &[String],
    latest_epoch: Option<i64>,
    oldest_epoch: Option<i64>,
    trajectory: &str,
    trajectory_components: &serde_json::Value,
) -> Result<i64> {
    let row = output
        .row
        .as_ref()
        .context("Insider source verdict has no row")?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO public.transfer_rumors (
            team_id,player_id,sport,trigger_type,heat,heat_components,
            is_rumor,direction,stage,model_summary,source_attribution,confidence,
            input_news_ids,rumor_updated_at,source_count,source_names,
            source_latest_at,source_oldest_at,trajectory,trajectory_components,
            model_version,prompt_version,trigger_payload,input_hash,subject_type
        ) VALUES (
            $1,$2,$3,'harvester',$4,$5::jsonb,$6,$7,$8,$9,$10,$11::float8::numeric,
            $12,COALESCE(to_timestamp($13::double precision),now()),$14,$15,
            to_timestamp($16::double precision),to_timestamp($17::double precision),
            $18,$19::jsonb,$20,$21,$22::jsonb,$23,$24
        ) RETURNING id
        "#,
    )
    .bind(item.entity_id_i32()?)
    .bind(candidate.player_id)
    .bind(item.sport.to_uppercase())
    .bind(output.heat)
    .bind(&output.components)
    .bind(row.is_rumor)
    .bind(row.direction.as_deref())
    .bind(row.stage.as_deref())
    .bind(row.summary.as_deref())
    .bind(row.attribution.as_deref())
    .bind(row.confidence)
    .bind(&output.news_ids)
    .bind(latest_epoch)
    .bind(source_count)
    .bind(source_names)
    .bind(latest_epoch)
    .bind(oldest_epoch)
    .bind(trajectory)
    .bind(trajectory_components.to_string())
    .bind(row.model.as_deref())
    .bind(output.provenance.prompt_version)
    .bind(&row.trigger_payload)
    .bind(output.provenance.input_hash.as_deref())
    .bind(&candidate.subject_type)
    .fetch_one(&mut **tx)
    .await?;
    Ok(inserted.get("id"))
}

async fn bank_source_junction_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &Item,
    source: &SourceContext,
    candidate: &TransferCandidate,
    stage: &str,
    model: &str,
) -> Result<()> {
    if candidate.subject_type != "player" {
        return Ok(());
    }
    let (predicate, confidence) = match stage {
        "here_we_go" => ("trade_confirmed", "confirmed"),
        "advanced_talks" | "concrete_interest" => ("trade_rumor", "reported"),
        _ => ("trade_rumor", "speculative"),
    };
    sqlx::query(
        "INSERT INTO public.narrative_events \
         (sport,subject_type,subject_id,predicate,object_type,object_id, \
          sentiment,confidence,article_id,event_date,source,model_version,prompt_version,origin) \
         SELECT $1,'player',$2,$3,'team',$4,NULL,$5,a.id,NOW(),a.source,$6,$7,'junction' \
           FROM public.news_articles a WHERE a.id=$8 \
         ON CONFLICT (article_id,sport,subject_type,subject_id,predicate, \
                      COALESCE(object_type,''),COALESCE(object_id,0),origin) \
         DO UPDATE SET confidence=EXCLUDED.confidence,event_date=NOW(), \
                       model_version=EXCLUDED.model_version,extracted_at=NOW()",
    )
    .bind(item.sport.to_uppercase())
    .bind(candidate.player_id)
    .bind(predicate)
    .bind(item.entity_id_i32()?)
    .bind(confidence)
    .bind(model)
    .bind(SOURCE_PROMPT_VERSION)
    .bind(source.article_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn execute(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<PluginOutcome> {
    let backend = models.inference(crate::plugins::insider::manifest::ROUTE)?;
    execute_with_models(
        pool,
        backend.as_ref(),
        models.voice_num_ctx,
        item,
        Some(models),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    execute_with_models(pool, backend, voice_num_ctx, item, None).await
}

async fn execute_with_models(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    models: Option<&ExecutionCapabilities>,
) -> Result<PluginOutcome> {
    ensure!(
        item.entity_type == "team",
        "Insider Harvester work requires a team"
    );
    let plugin_id = crate::plugins::insider::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(
        pool,
        plugin_id,
        &item.entity_type,
        item.entity_id_i32()?,
        &item.sport.to_uppercase(),
    )
    .await?;
    let Some(source) = sources.last() else {
        return wrap::execute(pool, backend, voice_num_ctx, item, models).await;
    };
    let next_pair: Option<(String, i32)> = sqlx::query_as(
        "SELECT subject_type,subject_id FROM public.harvester_insider_pairs \
         WHERE classification_id=$1 AND status='pending' \
         ORDER BY subject_type,subject_id LIMIT 1",
    )
    .bind(source.classification_id)
    .fetch_optional(pool)
    .await?;
    let Some((subject_type, subject_id)) = next_pair else {
        return finish_source(pool, item, source).await;
    };
    let candidates = load_harvester_source_candidates(
        pool,
        source.article_id,
        item.entity_id_i32()?,
        &item.sport.to_uppercase(),
    )
    .await?;
    let candidate = candidates
        .into_iter()
        .find(|c| c.subject_type == subject_type && c.player_id == subject_id)
        .context("Insider pair identity no longer resolves from the source")?;
    let prepared = prepare_pair(pool, backend, voice_num_ctx, item, source, &candidate).await?;
    let output = if let Some(prepared) = prepared {
        Some(create_source_pair(backend, prepared, source, &candidate).await?)
    } else {
        None
    };
    let metadata = if let Some(output) = output.as_ref() {
        let source_metadata = load_transfer_source_metadata(pool, &output.news_ids).await?;
        let row = output
            .row
            .as_ref()
            .context("Insider source verdict has no row")?;
        let trajectory = classify_transfer_trajectory(
            pool,
            item.entity_id_i32()?,
            candidate.player_id,
            &item.sport.to_uppercase(),
            output,
            row,
        )
        .await?;
        Some((source_metadata, trajectory))
    } else {
        None
    };
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let product_row_id = if let (Some(output), Some((source_metadata, trajectory))) =
        (output.as_ref(), metadata.as_ref())
    {
        let (count, names, latest, oldest) = source_metadata;
        let (trajectory, components) = trajectory;
        let id = insert_source_pair_row(
            publication.transaction(),
            item,
            &candidate,
            output,
            *count,
            names,
            *latest,
            *oldest,
            trajectory,
            components,
        )
        .await?;
        if candidate.subject_type == "player" {
            sqlx::query(
                "INSERT INTO public.harvester_insider_identity_reviews \
                 (transfer_rumor_id,classification_id,article_id,team_id,player_id,sport) \
                 VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(source.classification_id)
            .bind(source.article_id)
            .bind(item.entity_id_i32()?)
            .bind(candidate.player_id)
            .bind(item.sport.to_uppercase())
            .execute(&mut **publication.transaction())
            .await?;
        }
        if output.outcome == Outcome::Rumor {
            record_transfer_event(
                publication.transaction(),
                item,
                TRANSFER_PUBLISHED,
                &candidate.subject_type,
                candidate.player_id,
                Some(&id.to_string()),
            )
            .await?;
            if let Some(stage) = output.row.as_ref().and_then(|row| row.stage.as_deref()) {
                bank_source_junction_event(
                    publication.transaction(),
                    item,
                    source,
                    &candidate,
                    stage,
                    &output.provenance.model_version,
                )
                .await?;
            }
        }
        Some(id)
    } else {
        None
    };
    let pair_status = match output.as_ref().map(|out| out.outcome) {
        Some(Outcome::Rumor) => "rumor",
        Some(Outcome::Cleared) => "cleared",
        None => "abstained",
        _ => anyhow::bail!("Insider source pair did not reach a terminal verdict"),
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_insider_pairs SET status=$4,product_ref=$5,updated_at=now() \
         WHERE classification_id=$1 AND subject_type=$2 AND subject_id=$3 AND status='pending'",
    )
    .bind(source.classification_id)
    .bind(&candidate.subject_type)
    .bind(candidate.player_id)
    .bind(pair_status)
    .bind(serde_json::json!({
        "article_id": source.article_id,
        "transfer_rumor_id": product_row_id,
        "model_version": output.as_ref().map(|out| out.provenance.model_version.as_str()),
        "prompt_version": output.as_ref().map(|out| out.provenance.prompt_version),
        "input_hash": output.as_ref().and_then(|out| out.provenance.input_hash.as_deref()),
        "eval_count": output.as_ref().and_then(|out| out.call.as_ref()).and_then(|call| call.eval_count),
        "wall_ms": output.as_ref().and_then(|out| out.call.as_ref()).and_then(|call| call.wall_ms),
        "reason": if output.is_none() { Some("no_recent_pair_corpus") } else { None },
    }))
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "Insider pair changed during source call"
    );
    let pending_pairs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_insider_pairs WHERE classification_id=$1 AND status='pending'"
    ).bind(source.classification_id)
        .fetch_one(&mut **publication.transaction()).await?;
    if pending_pairs > 0 {
        publication.commit_progress().await?;
        return Ok(PluginOutcome::deferred(
            format!("{pending_pairs} Insider source pairs remain"),
            Duration::from_secs(1),
        ));
    }
    let remaining = settle_source(publication.transaction(), item, source).await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Harvester source contexts remain for Insider"),
            Duration::from_secs(1),
        ));
    }
    publication.commit_progress().await?;
    Ok(PluginOutcome::deferred(
        "Insider source reviews complete; scored-board wraps remain",
        Duration::from_secs(1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::model::{GenerateOptions, GenerateResult};
    use std::time::Duration;

    struct FakeVerdict(&'static str);

    #[async_trait]
    impl Inference for FakeVerdict {
        async fn generate(
            &self,
            prompt: &str,
            options: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            ensure!(prompt.contains("Exact publisher opening"));
            ensure!(options.format_schema.is_some());
            Ok((
                GenerateResult {
                    response: self.0.into(),
                    thinking: String::new(),
                    model: "smoke-insider".into(),
                    total_duration: Duration::from_millis(4),
                    prompt_eval_count: 8,
                    eval_count: 9,
                    completion_reason: Some("stop".into()),
                    raw_response_body: self.0.into(),
                },
                serde_json::json!({"prompt":prompt}),
            ))
        }
        fn model(&self) -> &str {
            "smoke-insider"
        }
        fn request_body(&self, prompt: &str, _: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({"prompt":prompt})
        }
    }

    fn pair_fixture() -> (SourceContext, TransferCandidate, PairAssignment) {
        let source = SourceContext {
            classification_id: 2,
            article_id: 41,
            headline: "Example Club pursuing Morgan Example".into(),
            context: "Example Club is pursuing Morgan Example after opening talks.".into(),
            source: "Wire".into(),
            published_at_epoch: None,
        };
        let candidate = TransferCandidate {
            player_id: 7,
            player_name: "Morgan Example".into(),
            nationality: String::new(),
            current_club: String::new(),
            position: String::new(),
            subject_type: "player".into(),
            relationship_override: None,
        };
        let prompt = source_prompt("Example Club", &candidate, "FOOTBALL", "none", &source, "");
        let assignment = PairAssignment {
            player_id: 7,
            player_name: candidate.player_name.clone(),
            team_name: "Example Club".into(),
            subject_type: "player".into(),
            heat: 30,
            components: "{}".into(),
            news_ids: vec![41],
            prompted_news_ids: vec![41],
            stale_news_ids: Vec::new(),
            news: vec![NewsItem {
                id: 41,
                title: source.headline.clone(),
                description: source.context.clone(),
                source: source.source.clone(),
            }],
            relationship: "none".into(),
            attribution: "Wire".into(),
            prompt,
            options: GenerateOptions::default(),
            model_configured: "smoke-insider".into(),
            failed_request_body: serde_json::json!({}),
            input_hash: "hash".into(),
        };
        (source, candidate, assignment)
    }

    #[test]
    fn source_prompt_keeps_the_entire_exact_opening_without_packets() {
        let opening = "This is the exact publisher opening. ".repeat(12);
        let source = SourceContext {
            classification_id: 2,
            article_id: 41,
            headline: "Club reports a move".into(),
            context: opening.clone(),
            source: "Wire".into(),
            published_at_epoch: None,
        };
        let candidate = TransferCandidate {
            player_id: 7,
            player_name: "Morgan Example".into(),
            nationality: String::new(),
            current_club: String::new(),
            position: String::new(),
            subject_type: "player".into(),
            relationship_override: None,
        };
        let prompt = source_prompt("Example Club", &candidate, "FOOTBALL", "none", &source, "");
        assert!(prompt.contains(&opening));
        assert!(prompt.contains("Source article ID: 41"));
        assert!(!prompt.contains("storyline"));
        assert!(!prompt.contains("packet"));
    }

    #[tokio::test]
    async fn positive_requires_an_exact_source_quote_and_retains_one_article_id() {
        let (source, candidate, assignment) = pair_fixture();
        let raw = r#"{"is_rumor":true,"subject":"Morgan Example","stage":"advanced_talks","evidence_quote":"Example Club is pursuing Morgan Example after opening talks."}"#;
        let output = create_source_pair(&FakeVerdict(raw), assignment, &source, &candidate)
            .await
            .unwrap();
        assert_eq!(output.outcome, Outcome::Rumor);
        assert_eq!(output.provenance.input_ids, vec![41]);
        assert_eq!(output.provenance.prompt_version, SOURCE_PROMPT_VERSION);
        assert_eq!(
            output.row.as_ref().unwrap().summary.as_deref(),
            Some("Wire reports: Example Club is pursuing Morgan Example after opening talks.")
        );
        let (source, candidate, assignment) = pair_fixture();
        let invented = r#"{"is_rumor":true,"subject":"Morgan Example","stage":"advanced_talks","evidence_quote":"The club signed Morgan Example yesterday."}"#;
        assert!(
            create_source_pair(&FakeVerdict(invented), assignment, &source, &candidate)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn explicit_negative_has_no_published_rumor_summary() {
        let (source, candidate, assignment) = pair_fixture();
        let raw = r#"{"is_rumor":false,"subject":"Morgan Example","stage":"","evidence_quote":""}"#;
        let output = create_source_pair(&FakeVerdict(raw), assignment, &source, &candidate)
            .await
            .unwrap();
        assert_eq!(output.outcome, Outcome::Cleared);
        assert_eq!(output.row.as_ref().unwrap().is_rumor, Some(false));
        assert!(output.row.as_ref().unwrap().summary.is_none());
    }

    #[tokio::test]
    #[ignore = "requires a local Ollama model"]
    async fn local_real_model_pins_a_transfer_verdict_to_source_words() -> Result<()> {
        let backend = crate::runtime::providers::ollama::OllamaClient::with_think(
            &std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            &std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite4.2:3b".into()),
            Duration::from_secs(600),
            Some(false),
        )?;
        let (mut source, candidate, mut assignment) = pair_fixture();
        source.headline = "Example Club agrees deal for Morgan Example".into();
        source.context = "Example Club agreed a deal to sign Morgan Example today. The player will join when the transfer window opens next week.".into();
        assignment.prompt =
            source_prompt("Example Club", &candidate, "FOOTBALL", "none", &source, "");
        assignment.options.num_ctx = 4096;
        assignment.options.num_predict = 220;
        assignment.options.temperature = Some(0.0);
        assignment.options.json_mode = true;
        let output = create_source_pair(&backend, assignment, &source, &candidate).await?;
        ensure!(
            output.outcome == Outcome::Rumor,
            "clear transfer report was missed"
        );
        let row = output
            .row
            .as_ref()
            .context("transfer verdict has no source row")?;
        let evidence: serde_json::Value = serde_json::from_str(&row.trigger_payload)?;
        let quote = evidence["evidence_quote"]
            .as_str()
            .context("transfer verdict has no exact quote")?;
        ensure!(
            source.headline.contains(quote) || source.context.contains(quote),
            "real-model transfer quote is not exact"
        );
        let (mut source, candidate, mut assignment) = pair_fixture();
        source.headline = "Morgan Example faces Example Club".into();
        source.context = "Morgan Example played against Example Club on Friday. The club's coach praised the player's defending after the match.".into();
        assignment.prompt =
            source_prompt("Example Club", &candidate, "FOOTBALL", "none", &source, "");
        assignment.options.num_ctx = 4096;
        assignment.options.num_predict = 220;
        assignment.options.temperature = Some(0.0);
        assignment.options.json_mode = true;
        let cleared = create_source_pair(&backend, assignment, &source, &candidate).await?;
        ensure!(
            cleared.outcome == Outcome::Cleared,
            "non-transfer co-mention became a move"
        );
        ensure!(
            cleared
                .row
                .as_ref()
                .is_some_and(|row| row.summary.is_none()),
            "cleared co-mention has a rumor summary"
        );
        Ok(())
    }
}

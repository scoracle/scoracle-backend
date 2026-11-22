//! Read-only league throughput replay. Creates local receipts; never claims or publishes.
use anyhow::{ensure, Result};
use async_trait::async_trait;
use scoracle_cognition::{
    evaluation::tasks::{resolve_task, EntitySpec},
    harness::{
        config::Config,
        model::{ContextExceeded, GenerateOptions, GenerateResult, Inference, ResponseFailure},
        models::Models,
        route::{RouteKey, Router},
        Parser, Studio,
    },
    plugins::{analyst, influencer, insider, journalist, oracle, scout},
    tools::meta::EntityMeta,
};
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

fn save(root: &Path, name: &str, value: &Value) -> Result<()> {
    fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}
struct Recorded {
    inner: Arc<dyn Inference>,
    calls: Mutex<Vec<Value>>,
}
#[async_trait]
impl Inference for Recorded {
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn request_body(&self, prompt: &str, options: &GenerateOptions) -> Value {
        self.inner.request_body(prompt, options)
    }
    async fn generate(
        &self,
        prompt: &str,
        options: &GenerateOptions,
    ) -> Result<(GenerateResult, Value)> {
        let start = Instant::now();
        let result = self.inner.generate(prompt, options).await;
        let mut call = json!({"elapsed_ms_including_governor":start.elapsed().as_millis(),"prepared_prompt":prompt,"num_ctx":options.num_ctx,"reserved_output":options.num_predict});
        match &result {
            Ok((g, wire)) => {
                call["request"] = wire.clone();
                call["raw_response"] = json!(g.raw_response_body);
                call["input_tokens"] = json!(g.prompt_eval_count);
                call["output_tokens"] = json!(g.eval_count);
                call["provider_wall_ms"] = json!(g.total_duration.as_millis());
            }
            Err(e) => {
                call["error"] = json!(format!("{e:#}"));
                call["context_exceeded"] = json!(e.is::<ContextExceeded>());
                if let Some(f) = e.downcast_ref::<ResponseFailure>() {
                    call["raw_response"] = json!(f.raw_response_body);
                }
            }
        }
        self.calls.lock().unwrap().push(call);
        result
    }
}
struct Run {
    pool: PgPool,
    models: Models,
    root: PathBuf,
    events: Mutex<File>,
    now: i64,
    season: i32,
    timestamp: String,
    start: Instant,
}
impl Run {
    fn backend(&self, role: RouteKey) -> Recorded {
        Recorded {
            inner: self.models.router.for_route(role),
            calls: Mutex::new(vec![]),
        }
    }
    fn finish(
        &self,
        subject: &EntityMeta,
        stage: &str,
        start: Instant,
        backend: &Recorded,
        result: &Result<Value>,
    ) -> Result<()> {
        let calls = backend.calls.lock().unwrap();
        let status = match result {
            Ok(v) => v["status"].as_str().unwrap_or("generated"),
            Err(e) if e.is::<ContextExceeded>() => "held_token_budget",
            Err(_) => "failed",
        };
        let event = json!({"entity_type":subject.entity_type,"entity_id":subject.entity_id,"name":subject.name,"stage":stage,"status":status,"elapsed_seconds":start.elapsed().as_secs_f64(),"run_elapsed_seconds":self.start.elapsed().as_secs_f64(),"attempts":calls.len(),"model_calls":calls.iter().filter(|c| c.get("input_tokens").is_some()).count(),"input_tokens":calls.iter().filter_map(|c|c["input_tokens"].as_u64()).sum::<u64>(),"output_tokens":calls.iter().filter_map(|c|c["output_tokens"].as_u64()).sum::<u64>()});
        let folder = self
            .root
            .join(format!("{}-{}", subject.entity_type, subject.entity_id));
        fs::create_dir_all(&folder)?;
        let receipt = match result {
            Ok(v) => json!({"event":event,"product":v,"calls":*calls}),
            Err(e) => json!({"event":event,"error":format!("{e:#}"),"calls":*calls}),
        };
        save(&folder, stage, &receipt)?;
        writeln!(self.events.lock().unwrap(), "{event}")?;
        println!("{event}");
        Ok(())
    }
}
struct Prepared {
    subject: EntityMeta,
    journalist: Result<journalist::prompt::Assignment>,
    influencer: Result<influencer::prompt::Assignment>,
    scout: Result<scout::prompt::RatingBuild>,
    insider: Result<Option<insider::prompt::Parts>>,
}
struct InsiderParser(insider::prompt::Parts);
impl Parser<insider::Reply> for InsiderParser {
    fn parse(&self, raw: &str) -> Result<Option<insider::Reply>> {
        self.0.parse(raw)
    }
}
// Simulate only production's pre-generation held-source settlement, without writes.
async fn prepare_influencer(
    run: &Run,
    subject: EntityMeta,
) -> Result<influencer::prompt::Assignment> {
    let (mut assignment, _) =
        influencer::fresh::prepare_assignment(&run.pool, subject.clone(), run.now).await?;
    let unresolved = |e: &Value| {
        matches!(
            e["reason"].as_str(),
            Some(
                "unresolved_target_identity"
                    | "unknown_publication_time"
                    | "future_publication_time"
            )
        )
    };
    let settled: Vec<_> = assignment
        .parts
        .excluded
        .iter()
        .filter(|e| unresolved(e))
        .cloned()
        .collect();
    if !settled.is_empty() {
        let folder = run
            .root
            .join(format!("{}-{}", subject.entity_type, subject.entity_id));
        save(
            &folder,
            "influencer-readonly-settlement",
            &json!({"writes":false,"held_before_preparation":settled}),
        )?;
        assignment.parts.excluded.retain(|e| !unresolved(e));
        assignment.parts.memories = scoracle_cognition::plugins::classifier::memories::load_on(
            &mut *run.pool.acquire().await?,
            &subject,
            &assignment.parts.sources,
            "influencer",
        )
        .await?;
        while !assignment.parts.memories.is_empty()
            && assignment.parts.assemble().len() > influencer::prompt::SOURCE_BUDGET_BYTES
        {
            assignment.parts.memories.pop();
        }
        assignment = influencer::prompt::Assignment::from_parts(assignment.parts)?;
    }
    Ok(assignment)
}
async fn journalist_read(
    run: &Run,
    subject: &EntityMeta,
    input: Result<journalist::prompt::Assignment>,
) -> Result<Vec<oracle::prompt::SynthNarrative>> {
    let backend = run.backend(journalist::prompt::MODEL);
    let start = Instant::now();
    let mut cards = vec![];
    let result:Result<Value>=async {
        let mut assignment=input?;
        let folder=run.root.join(format!("{}-{}",subject.entity_type,subject.entity_id));
        save(&folder,"journalist-input",&json!({"prompt":journalist::prompt::prompt(&assignment),"selected":assignment.selected.len(),"dispositions":assignment.dispositions.iter().map(|d|json!({"article_id":d.article_id,"reason":d.reason})).collect::<Vec<_>>()}))?;
        let output=loop {
            match journalist::create(&run.pool,&Studio::new(&backend),&assignment,run.now,run.models.voice_num_ctx).await {
                Err(e) if e.is::<ContextExceeded>() && !assignment.memories.is_empty()=>assignment.memories.clear(),
                result=>break result?,
            }
        };
        cards=output.narratives.iter().map(|n|oracle::prompt::SynthNarrative {title:n.title.clone(),body:n.body.clone(),impact:f64::from(n.impact),trajectory:scoracle_cognition::tools::source::DEFAULT_TRAJECTORY.into(),source_count:n.source_count,source_age_days:n.source_latest_epoch.map(|t|((run.now-t)/86400) as i32)}).collect();
        Ok(json!({"status":if output.was_called(){"generated"}else{"no_call"},"narratives":cards.iter().map(|n|json!({"title":n.title,"body":n.body,"impact":n.impact})).collect::<Vec<_>>()}))
    }.await;
    run.finish(subject, "journalist", start, &backend, &result)?;
    result?;
    Ok(cards)
}
async fn influencer_read(
    run: &Run,
    subject: &EntityMeta,
    input: Result<influencer::prompt::Assignment>,
) -> Result<Option<analyst::prompt::Mood>> {
    let backend = run.backend(influencer::prompt::MODEL);
    let start = Instant::now();
    let mut mood = None;
    let result:Result<Value>=async {
        let mut assignment=input?;
        let (output,receipt)=loop { let value=influencer::create(&backend,&assignment,run.models.voice_num_ctx).await?;
            if value.1["context_exceeded"]==true && !assignment.parts.memories.is_empty(){assignment.parts.memories.clear();continue}break value;
        };
        if receipt["context_exceeded"]==true {return Ok(json!({"status":"held_token_budget","receipt":receipt}))}
        if let Some(error)=receipt.get("error"){anyhow::bail!("{error}")}
        if let Some(g)=output { if let Some(body)=&g.vibe_prompt { mood=Some(analyst::prompt::Mood {body:body.clone(),headline:g.hook.clone(),sentiment:g.sentiment,generated_at:Some(run.timestamp.clone()),input_hash:g.provenance.input_hash.clone()}); } }
        Ok(json!({"status":if mood.is_some(){"generated"}else if receipt.get("hold_reason").is_some(){"held_packet_budget"}else if receipt.get("raw_response").is_some(){"abstained"}else{"no_call"},"receipt":receipt}))
    }.await;
    run.finish(subject, "influencer", start, &backend, &result)?;
    result?;
    Ok(mood)
}
struct ScoutRead {
    form: analyst::prompt::Form,
    rating: oracle::prompt::SynthRating,
}
async fn scout_read(
    run: &Run,
    subject: &EntityMeta,
    input: Result<scout::prompt::RatingBuild>,
) -> Result<Option<ScoutRead>> {
    let backend = run.backend(scout::prompt::MODEL);
    let start = Instant::now();
    let mut reading = None;
    let result:Result<Value>=async {
        match input? {
            scout::prompt::RatingBuild::NoStats{season}=>Ok(json!({"status":"no_stats","season":season})),
            scout::prompt::RatingBuild::Ready(assignment)=> {

                let g=scout::create(&Studio::new(&backend),*assignment).await?;
                if let Some(body)=&g.body {reading=Some(ScoutRead {form:analyst::prompt::Form{body:body.clone(),headline:g.headline.clone(),season:Some(g.season),generated_at:Some(run.timestamp.clone()),input_hash:g.provenance.input_hash.clone()},rating:oracle::prompt::SynthRating{body:body.clone(),notability:g.notability.unwrap_or(0),rating_trajectory:g.rating_trajectory.clone().unwrap_or_else(||"steady".into()),rating_trajectory_label:g.rating_trajectory_label.clone().unwrap_or_default()}});}
                Ok(json!({"status":if g.was_called(){"generated"}else{"no_call"},"body":g.body,"notability":g.notability,"abstained":g.abstained}))
            }
        }
    }.await;
    run.finish(subject, "scout", start, &backend, &result)?;
    result?;
    Ok(reading)
}
async fn insider_read(
    run: &Run,
    subject: &EntityMeta,
    input: Result<Option<insider::prompt::Parts>>,
) -> Result<Option<oracle::prompt::SynthInsider>> {
    let backend = run.backend(insider::prompt::MODEL);
    let start = Instant::now();
    let mut card = None;
    let result:Result<Value>=async {
        let Some(mut parts)=input? else{return Ok(json!({"status":"no_call"}))};
        let extracted=loop { let opts=insider::prompt::options_for_reports(run.models.voice_num_ctx,subject,&parts.reports);
            let result=Studio::new(&backend).extract(&parts.assemble(),&opts,&InsiderParser(parts.clone()),|_|None).await;
            match result {Err(e) if e.is::<ContextExceeded>() && !parts.memories.is_empty()=>parts.memories.clear(),result=>break result?}
        };
        let reply=extracted.value.ok_or_else(||anyhow::anyhow!("Insider returned no reply"))?;
        let findings=reply.findings.iter().map(|f|json!({"counterparty":f.counterparty,"report_index":f.report_index,"status":f.status,"stage":f.stage,"publisher":parts.reports[f.report_index].publisher.to_lowercase()})).collect::<Vec<_>>();
        let heat:i16=sqlx::query_scalar(include_str!("../src/plugins/insider/activity.sql")).bind(json!(findings)).fetch_one(&run.pool).await?;
        card=Some(oracle::prompt::SynthInsider{body:reply.body.clone(),score:i32::from(heat),generated_at:Some(run.timestamp.clone())});
        Ok(json!({"status":"generated","reply":reply,"activity_score":heat}))
    }.await;
    run.finish(subject, "insider", start, &backend, &result)?;
    result?;
    Ok(card)
}
async fn entity(run: Arc<Run>, prepared: Prepared) -> Result<()> {
    let Prepared {
        subject,
        journalist,
        influencer,
        scout,
        insider,
    } = prepared;
    let (news, mood, form, movement) = tokio::join!(
        journalist_read(&run, &subject, journalist),
        influencer_read(&run, &subject, influencer),
        scout_read(&run, &subject, scout),
        insider_read(&run, &subject, insider)
    );
    let mut cards = oracle::prompt::Cards::default();
    cards.narratives = news.unwrap_or_default();
    cards.insider = movement.unwrap_or_default();
    let mood = mood.unwrap_or_default();
    let form = form.unwrap_or_default();
    cards.vibe = mood.as_ref().and_then(|m| {
        m.sentiment.map(|s| oracle::prompt::SynthVibe {
            sentiment: s,
            prompt: m.body.clone(),
        })
    });
    cards.rating = form.as_ref().map(|s| s.rating.clone());
    let backend = run.backend(analyst::prompt::MODEL);
    let start = Instant::now();
    let context = analyst::prompt::MomentumContext::new(
        run.season,
        form.map(|s| s.form),
        mood,
        analyst::prompt::Snapshot::default(),
    );
    let assignment = analyst::prompt::Assignment {
        entity_id: subject.entity_id,
        entity_type: subject.entity_type.clone(),
        entity_name: subject.name.clone(),
        sport: subject.sport.clone(),
        context,
        voice_num_ctx: run.models.voice_num_ctx,
    };
    let result:Result<Value>=async {
        match analyst::create(&run.pool,&Studio::new(&backend),&assignment).await? {
            Some(g)=> {cards.momentum=oracle::prompt::SynthMomentum{direction:Some(g.direction.clone()),blurb:Some(g.blurb.clone()),input_hash:g.provenance.input_hash.clone(),..Default::default()};Ok(json!({"status":if g.was_called(){"generated"}else{"no_call"},"body":g.blurb,"headline":g.headline,"direction":g.direction,"score":g.score}))}
            None=>Ok(json!({"status":"no_call"})),
        }
    }.await;
    run.finish(&subject, "analyst", start, &backend, &result)?;
    let backend = run.backend(oracle::prompt::MODEL);
    let start = Instant::now();
    let components = oracle::prompt::build_synthesis_input_components(&cards);
    let assignment = oracle::prompt::Assignment {
        subject: oracle::prompt::Subject {
            entity_id: subject.entity_id,
            entity_type: subject.entity_type.clone(),
            entity_name: subject.name.clone(),
            sport: subject.sport.clone(),
        },
        season: run.season,
        cards,
        input_hash: scoracle_cognition::util::hash_components(&components),
        input_components_json: components,
        options: oracle::prompt::generation_options(
            oracle::prompt::ORACLE_TEMPERATURE,
            run.models.voice_num_ctx,
        ),
    };
    let result:Result<Value>=async { let g=oracle::create(&run.pool,&Studio::new(&backend),&assignment).await?;Ok(json!({"status":if g.was_called(){"generated"}else{"no_call"},"reading":g.reading,"headline":g.headline,"score":g.score,"peer_scope":"this_replay_only"})) }.await;
    run.finish(&subject, "oracle", start, &backend, &result)?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("new output directory required"),
    );
    fs::create_dir(&root)?;
    let cfg = Config::from_env()?;
    ensure!(
        cfg.voice_num_ctx == 10000,
        "benchmark expects deployed 10k cap"
    );
    let opts: PgConnectOptions = cfg.database_url.parse()?;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(
            opts.application_name("premier-league-readonly-replay")
                .options([
                    ("default_transaction_read_only", "on"),
                    ("statement_timeout", "30000"),
                ]),
        )
        .await?;
    let read_only: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&pool)
        .await?;
    ensure!(read_only == "on");
    let season: i32 = sqlx::query_scalar("SELECT current_season FROM sports WHERE id='FOOTBALL'")
        .fetch_one(&pool)
        .await?;
    let rows:Vec<(String,i32,String)>=sqlx::query_as("WITH clubs AS (SELECT DISTINCT t.id,t.name FROM team_stats s JOIN teams t ON t.id=s.team_id AND t.sport=s.sport WHERE s.sport='FOOTBALL' AND s.league_id=8 AND s.season=$1), members AS(SELECT 'team'::text AS kind,id,name FROM clubs UNION SELECT 'player',p.id,p.name FROM player_stats s JOIN players p ON p.id=s.player_id AND p.sport=s.sport WHERE s.sport='FOOTBALL' AND s.league_id=8 AND s.season=$1) SELECT kind,id,name FROM members ORDER BY CASE kind WHEN 'team' THEN 0 ELSE 1 END,name").bind(season).fetch_all(&pool).await?;
    ensure!(
        rows.iter().filter(|r| r.0 == "team").count() == 20,
        "current-season club population must be 20"
    );
    let roster = rows
        .iter()
        .map(|(kind, id, name)| json!({"entity_type":kind,"entity_id":id,"name":name}))
        .collect::<Vec<_>>();
    save(&root, "roster", &json!(roster))?;
    let total = rows.len();
    let started = Instant::now();
    let now = influencer::now();
    let timestamp = sqlx::query_scalar(
        "SELECT to_char(now() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
    )
    .fetch_one(&pool)
    .await?;
    let models = Models {
        router: Router::from_config(&cfg.route, cfg.model_timeout, cfg.model_max_concurrent)?,
        voice_num_ctx: cfg.voice_num_ctx,
    };
    let run = Arc::new(Run {
        pool,
        models,
        root: root.clone(),
        events: Mutex::new(
            OpenOptions::new()
                .create_new(true)
                .append(true)
                .open(root.join("events.jsonl"))?,
        ),
        now,
        season,
        timestamp,
        start: started,
    });
    save(
        &root,
        "run",
        &json!({"season":season,"subjects":total,"expected_stages":total*6,"read_only":true,"publishes":false,"logical_cap":10000,"concurrency":4,"started_at":run.timestamp,"qualification":"Creator/parser replay, not a durable queue/publication benchmark; no stored peer prose. Publication metrics are computed read-only; no product reuse or queue-level retry backoff."}),
    )?;
    let mut prepared = vec![];
    for (kind, id, name) in rows {
        let subject = EntityMeta {
            entity_type: kind,
            entity_id: id,
            name,
            sport: "FOOTBALL".into(),
        };
        let folder = root.join(format!("{}-{}", subject.entity_type, subject.entity_id));
        fs::create_dir(&folder)?;
        let spec = EntitySpec {
            entity_type: subject.entity_type.clone(),
            entity_id: id,
            sport: "FOOTBALL".into(),
            pair_player_id: None,
        };
        let task = resolve_task("transfer").unwrap();
        let req = scout::prompt::RatingReq {
            entity_type: subject.entity_type.clone(),
            entity_id: id,
            entity_name: subject.name.clone(),
            sport: "FOOTBALL".into(),
            season: Some(season),
            trigger_type: "daily".into(),
        };
        let (j, v, s, i) = tokio::join!(
            journalist::fresh::load_narratives_material(&run.pool, subject.clone(), now),
            prepare_influencer(&run, subject.clone()),
            scout::prompt::build_rating_request(
                &run.pool,
                10000,
                &req,
                scout::prompt::RATING_TEMPERATURE,
                false
            ),
            task.build_request(&run.pool, &run.models, &spec)
        );
        let insider = i.and_then(|p| {
            p.map(|p| {
                serde_json::from_value(
                    p.parts
                        .ok_or_else(|| anyhow::anyhow!("Insider parts missing"))?,
                )
                .map_err(anyhow::Error::from)
            })
            .transpose()
        });
        prepared.push(Prepared {
            subject,
            journalist: j.map(|m| m.assignment),
            influencer: v,
            scout: s,
            insider,
        });
        if prepared.len() % 20 == 0 {
            println!(
                "PREPARED {}/{} elapsed={:.1}s",
                prepared.len(),
                total,
                started.elapsed().as_secs_f64()
            )
        }
    }
    let preparation = started.elapsed().as_secs_f64();
    save(
        &root,
        "preparation",
        &json!({"seconds":preparation,"subjects":total}),
    )?;
    let mut tasks = tokio::task::JoinSet::new();
    for p in prepared {
        if tasks.len() >= 8 {
            tasks.join_next().await.unwrap()??;
        }
        tasks.spawn(entity(run.clone(), p));
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    let events = fs::read_to_string(root.join("events.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(events.len() == total * 6, "missing stage receipts");
    save(
        &root,
        "summary",
        &json!({"subjects":total,"stage_results":events.len(),"preparation_seconds":preparation,"generation_seconds":started.elapsed().as_secs_f64()-preparation,"total_seconds":started.elapsed().as_secs_f64(),"model_calls":events.iter().filter_map(|e|e["model_calls"].as_u64()).sum::<u64>(),"publishes":false,"read_only":true}),
    )?;
    println!(
        "FINISHED {}",
        fs::read_to_string(root.join("summary.json"))?
    );
    Ok(())
}

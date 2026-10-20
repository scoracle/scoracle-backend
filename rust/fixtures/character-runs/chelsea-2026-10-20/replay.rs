//! One-team read-only diagnostic run. Uses production creators; never publishes.
use anyhow::{ensure, Result};
use scoracle_cognition::{
    harness::{
        model::{GenerateOptions, GenerateResult, Inference, ResponseFailure},
        providers::llama::LlamaClient,
        Parser, Studio,
    },
    plugins::{analyst, influencer, insider, journalist, scout},
    tools::meta::EntityMeta,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
struct Recorded {
    inner: LlamaClient,
    calls: Mutex<Vec<Value>>,
}
#[async_trait::async_trait]
impl Inference for Recorded {
    async fn generate(&self, p: &str, o: &GenerateOptions) -> Result<(GenerateResult, Value)> {
        let started = Instant::now();
        let result = self.inner.generate(p, o).await;
        let mut v =
            json!({"request":self.inner.request_body(p,o),"wall_ms":started.elapsed().as_millis()});
        match &result {
            Ok((g, r)) => {
                v["request"] = r.clone();
                v["response"] = json!(g.response);
                v["raw_response_body"] = json!(g.raw_response_body);
                v["input_tokens"] = json!(g.prompt_eval_count);
                v["output_tokens"] = json!(g.eval_count);
            }
            Err(e) => {
                v["error"] = json!(format!("{e:#}"));
                if let Some(f) = e.downcast_ref::<ResponseFailure>() {
                    v["raw_response_body"] = json!(f.raw_response_body);
                }
            }
        }
        self.calls.lock().unwrap().push(v);
        result
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn request_body(&self, p: &str, o: &GenerateOptions) -> Value {
        self.inner.request_body(p, o)
    }
}
impl Recorded {
    fn new(url: &str, model: &str) -> Result<Self> {
        Ok(Self {
            inner: LlamaClient::new(url, model, Duration::from_secs(180), Some(false))?,
            calls: Mutex::new(vec![]),
        })
    }
    fn receipt(&self) -> Value {
        json!(self.calls.lock().unwrap().drain(..).collect::<Vec<_>>())
    }
}
fn save(root: &Path, name: &str, v: &Value) -> Result<()> {
    std::fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_string_pretty(v)? + "\n",
    )?;
    Ok(())
}
struct InsiderParser(insider::prompt::Parts);
impl Parser<insider::Reply> for InsiderParser {
    fn parse(&self, raw: &str) -> Result<Option<insider::Reply>> {
        self.0.parse(raw)
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let root = PathBuf::from(std::env::args().nth(1).expect("output dir required"));
    std::fs::create_dir(&root)?;
    let opts: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")?.parse()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(opts.options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", "20000"),
        ]))
        .await?;
    let ro: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&pool)
        .await?;
    ensure!(ro == "on");
    let subject = EntityMeta {
        name: scoracle_cognition::tools::meta::lookup_entity_name(&pool, "team", 18, "FOOTBALL")
            .await?,
        entity_type: "team".into(),
        entity_id: 18,
        sport: "FOOTBALL".into(),
    };
    ensure!(subject.name == "Chelsea");
    let smol = Recorded::new("http://127.0.0.1:8080", "alibayram/smollm3")?;
    let qwen = Recorded::new("http://127.0.0.1:8081", "Qwen3.5-4B-Q4_K_M")?;
    let now = influencer::now();
    let timestamp: String = sqlx::query_scalar(
        "SELECT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
    )
    .fetch_one(&pool)
    .await?;
    save(
        &root,
        "run",
        &json!({"runtime_source":"49d27ca9","captured_at":now,"subject":subject,"database_read_only":true,"publishes":false,"num_ctx":32768,"analyst_inputs":"fresh replay outputs; no stored peer fallback"}),
    )?;
    let material = journalist::fresh::load_narratives_material(&pool, subject.clone(), now).await?;
    let all = material
        .sources
        .iter()
        .map(journalist::prompt::CorpusItem::from)
        .collect::<Vec<_>>();
    save(
        &root,
        "journalist-baseline",
        &json!({"sources":material.sources,"selected":material.assignment.selected,"dispositions":material.assignment.dispositions.iter().map(|d|json!({"article_id":d.article_id,"reason":d.reason})).collect::<Vec<_>>(),"memories":material.assignment.memories}),
    )?;
    let mut assignment = material.assignment;
    let mut scope = "current_complete_assignment";
    if assignment.selected.is_empty() {
        // shortcut: bounded diagnostic subset exercises prose only; a full-day package needs a production budget fix.
        let mut subset = vec![];
        for r in all {
            let mut next = subset.clone();
            next.push(r);
            if journalist::prompt::assemble(&subject, &next).len()
                > journalist::prompt::SOURCE_BUDGET_BYTES
            {
                break;
            }
            subset = next;
        }
        assignment = journalist::fresh::prepare(subject.clone(), subset, &[], now)?;
        scope = "newest_bounded_debug_subset_not_publishable_daily_run";
    }
    save(
        &root,
        "journalist-input",
        &json!({"scope":scope,"subject":assignment.subject,"reports":assignment.selected,"memories":assignment.memories,"prompt":journalist::prompt::prompt(&assignment)}),
    )?;
    let r = journalist::create(&pool, &Studio::new(&smol), &assignment, now, 32768).await;
    let v = match r {
        Ok(g) => {
            json!({"scope":scope,"model":g.provenance.model_version,"headline":g.headline,"card_score":g.card_score,"narratives":g.narratives.iter().map(|n|json!({"headline":n.title,"body":n.body,"impact":n.impact,"input_news_ids":n.input_news_ids})).collect::<Vec<_>>()})
        }
        Err(e) => json!({"error":format!("{e:#}")}),
    };
    save(
        &root,
        "journalist-output",
        &json!({"product":v,"calls":smol.receipt()}),
    )?;
    println!("Journalist: {v}");
    let (assignment, disposition) =
        influencer::fresh::prepare_assignment(&pool, subject.clone(), now).await?;
    save(
        &root,
        "influencer-input",
        &json!({"parts":assignment.parts,"disposition":disposition}),
    )?;
    let result = influencer::create(&qwen, &assignment, 32768).await;
    let mut mood = None;
    let v = match result {
        Ok((Some(g), receipt)) => {
            mood = Some(analyst::prompt::Mood {
                body: g.vibe_prompt.clone().unwrap(),
                headline: g.hook.clone(),
                sentiment: g.sentiment,
                generated_at: Some(timestamp.clone()),
                input_hash: g.provenance.input_hash.clone(),
            });
            json!({"score":g.sentiment,"headline":g.hook,"body":g.vibe_prompt,"receipt":receipt})
        }
        Ok((None, receipt)) => json!({"no_product":true,"receipt":receipt}),
        Err(e) => json!({"error":format!("{e:#}")}),
    };
    save(
        &root,
        "influencer-output",
        &json!({"product":v,"calls":qwen.receipt()}),
    )?;
    println!("Influencer: {}", v.get("body").unwrap_or(&v));
    let req = scout::prompt::RatingReq {
        entity_type: "team".into(),
        entity_id: 18,
        entity_name: subject.name.clone(),
        sport: "FOOTBALL".into(),
        season: None,
        trigger_type: "periodic".into(),
    };
    let built = scout::prompt::build_rating_request(
        &pool,
        32768,
        &req,
        scout::prompt::RATING_TEMPERATURE,
        false,
    )
    .await?;
    let mut form = None;
    let mut season = 2026;
    let v = match built {
        scout::prompt::RatingBuild::NoStats { season } => json!({"no_stats":true,"season":season}),
        scout::prompt::RatingBuild::Ready(a) => {
            save(
                &root,
                "scout-input",
                &json!({"assignment":a,"request":smol.request_body(&a.built_prompt,&a.opts)}),
            )?;
            season = a.season;
            match scout::create(&Studio::new(&smol), *a).await {
                Ok(g) => {
                    if let Some(body) = g.body.clone() {
                        form = Some(analyst::prompt::Form {
                            body,
                            headline: g.headline.clone(),
                            season: Some(g.season),
                            generated_at: Some(timestamp.clone()),
                            input_hash: g.provenance.input_hash.clone(),
                        });
                    }
                    json!({"body":g.body,"headline":g.headline,"notability":g.notability,"season":g.season,"abstained":g.abstained})
                }
                Err(e) => json!({"error":format!("{e:#}")}),
            }
        }
    };
    save(
        &root,
        "scout-output",
        &json!({"product":v,"calls":smol.receipt()}),
    )?;
    println!("Scout: {v}");
    let task = scoracle_cognition::evaluation::tasks::resolve_task("transfer").unwrap();
    let models = scoracle_cognition::harness::models::Models {
        router: scoracle_cognition::harness::route::Router::from_config(
            &scoracle_cognition::harness::config::Config::from_env()?.route,
            Duration::from_secs(180),
            4,
        )?,
        voice_num_ctx: 32768,
    };
    let e = scoracle_cognition::evaluation::tasks::EntitySpec {
        entity_type: "team".into(),
        entity_id: 18,
        sport: "FOOTBALL".into(),
        pair_player_id: None,
    };
    let v = if let Some(p) = task.build_request(&pool, &models, &e).await? {
        let parts: insider::prompt::Parts = serde_json::from_value(p.parts.clone().unwrap())?;
        let opts = insider::prompt::options_for_reports(32768, &parts.subject, &parts.reports);
        save(
            &root,
            "insider-input",
            &json!({"parts":parts,"prompt":p.user_prompt}),
        )?;
        match Studio::new(&smol)
            .extract(&p.user_prompt, &opts, &InsiderParser(parts), |_| None)
            .await
        {
            Ok(g) => json!({"reply":g.value}),
            Err(e) => json!({"error":format!("{e:#}")}),
        }
    } else {
        json!({"no_corpus":true})
    };
    save(
        &root,
        "insider-output",
        &json!({"product":v,"calls":smol.receipt()}),
    )?;
    println!("Insider: {v}");
    let context = analyst::prompt::MomentumContext::new(
        season,
        form,
        mood,
        analyst::prompt::Snapshot::default(),
    );
    let assignment = analyst::prompt::Assignment {
        entity_id: 18,
        entity_type: "team".into(),
        entity_name: subject.name.clone(),
        sport: "FOOTBALL".into(),
        context,
        voice_num_ctx: 32768,
    };
    save(
        &root,
        "analyst-input",
        &json!({"scope":"fresh_replay_peer_outputs","components":serde_json::from_str::<Value>(&assignment.context.input_components_json)?,"prompt":analyst::prompt::assemble(&subject,assignment.context.rating.as_ref(),assignment.context.vibe.as_ref(),&assignment.context.snapshot)}),
    )?;
    let v = match analyst::create(&pool, &Studio::new(&smol), &assignment).await {
        Ok(Some(g)) => {
            json!({"body":g.blurb,"headline":g.headline,"direction":g.direction,"conviction":g.score})
        }
        Ok(None) => json!({"no_corpus":true}),
        Err(e) => json!({"error":format!("{e:#}")}),
    };
    save(
        &root,
        "analyst-output",
        &json!({"product":v,"calls":smol.receipt()}),
    )?;
    println!("Analyst: {v}");
    Ok(())
}

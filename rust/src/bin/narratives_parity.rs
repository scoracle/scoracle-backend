//! L13 narratives parity harness — deterministic request dump for the Rust port.
//!
//! This writes source='rust' rows to `news_summaries_shadow` for the deterministic
//! axes only: built_prompt, ollama_request, model_version, prompt_version. It does
//! not call the model unless `NARRATIVES_PARITY_VET=1`, and the default harness has
//! `embedder: None`, so the Rust candle dedup is the identity and the prompt is
//! directly comparable to Go for a fixed corpus.

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::narratives::{
    build_narratives_request, generate_narratives, NarrativesBuild, NarrativesOutput,
    NarrativesReady, NarrativesReq, NARRATIVES_PROMPT_VERSION,
};
use scoracle_cognition::ollama::OllamaClient;
use scoracle_cognition::route::Router;
use scoracle_cognition::{corpus, db};
use sqlx::PgPool;

const PARITY_TEMPERATURE: f64 = 0.0;

#[derive(Clone, Debug)]
struct EntitySpec {
    entity_type: String,
    entity_id: i32,
    sport: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let vet = std::env::var("NARRATIVES_PARITY_VET").is_ok_and(|v| !v.is_empty() && v != "0");
    if vet {
        let ollama =
            OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
        ollama
            .ping()
            .await
            .context("ollama must be reachable for NARRATIVES_PARITY_VET=1")?;
    }

    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, 1)?,
        embedder: None,
        resolve: cfg.resolve.clone(),
        scrub: cfg.scrub.clone(),
    };

    let specs = match parse_specs(std::env::args().skip(1))? {
        v if !v.is_empty() => v,
        _ => {
            let limit: i64 = std::env::var("NARRATIVES_PARITY_ENTITIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5);
            auto_select(&harness.pool, limit).await?
        }
    };
    if specs.is_empty() {
        return Err(anyhow!(
            "no entities to process — pass entity_type:id:sport specs, or ensure recent vetted corpus exists"
        ));
    }

    println!(
        "narratives parity harness — source=rust, prompt_version={NARRATIVES_PROMPT_VERSION}, temp={PARITY_TEMPERATURE}, vet={vet}, {} entit(ies) → news_summaries_shadow",
        specs.len()
    );

    let mut written = 0usize;
    for s in &specs {
        match run_entity(&harness, s, vet).await {
            Ok(true) => written += 1,
            Ok(false) => println!(
                "  · {}/{} ({}) → skipped (no vetted recent corpus)",
                s.entity_type, s.entity_id, s.sport
            ),
            Err(e) => println!(
                "  ✗ {}/{} ({}) → {e:#}",
                s.entity_type, s.entity_id, s.sport
            ),
        }
    }

    println!("done — {written} entit(ies) written to news_summaries_shadow (source=rust)");
    Ok(())
}

async fn run_entity(hx: &Harness, s: &EntitySpec, vet: bool) -> Result<bool> {
    let name = corpus::lookup_entity_name(&hx.pool, &s.entity_type, s.entity_id, &s.sport).await?;
    let req = NarrativesReq {
        entity_type: s.entity_type.clone(),
        entity_id: s.entity_id,
        entity_name: name.clone(),
        sport: s.sport.clone(),
        trigger_type: "periodic".to_string(),
    };

    let ready = match build_narratives_request(hx, &req, PARITY_TEMPERATURE).await? {
        NarrativesBuild::NoCorpus { .. } => return Ok(false),
        NarrativesBuild::Ready(r) => *r,
    };
    let vetted = if vet {
        Some(generate_narratives(hx, &req, PARITY_TEMPERATURE, now_unix()).await?)
    } else {
        None
    };

    persist_shadow(&hx.pool, s, &ready, vetted.as_ref()).await?;
    let label = vetted
        .as_ref()
        .map(|v| format!("{} grounded narrative(s)", v.narratives.len()))
        .unwrap_or_else(|| "deterministic".to_string());
    println!(
        "  ✓ {}/{} {} ({}) → corpus {}/{} | {} | {} bytes prompt",
        s.entity_type,
        s.entity_id,
        name,
        s.sport,
        ready.deduped_corpus_size(),
        ready.original_corpus_size,
        label,
        ready.built_prompt.len(),
    );
    Ok(true)
}

async fn persist_shadow(
    pool: &PgPool,
    s: &EntitySpec,
    ready: &NarrativesReady,
    vetted: Option<&NarrativesOutput>,
) -> Result<()> {
    let request_json = ready.request_body.to_string();
    let news_ids: Vec<i64> = ready.corpus.iter().map(|c| c.id).collect();

    if let Some(v) = vetted {
        if !v.narratives.is_empty() {
            for n in &v.narratives {
                let row = ShadowNarrativeRow {
                    title: Some(&n.title),
                    body: Some(&n.body),
                    impact: Some(n.impact as i16),
                    impact_components: n.impact_components.to_string(),
                    input_news_ids: &n.input_news_ids,
                };
                insert_shadow(pool, s, ready, &row, &request_json).await?;
            }
            return Ok(());
        }
    }

    let row = ShadowNarrativeRow {
        title: None,
        body: None,
        impact: None,
        impact_components: "{}".to_string(),
        input_news_ids: &news_ids,
    };
    insert_shadow(pool, s, ready, &row, &request_json).await
}

struct ShadowNarrativeRow<'a> {
    title: Option<&'a String>,
    body: Option<&'a String>,
    impact: Option<i16>,
    impact_components: String,
    input_news_ids: &'a [i64],
}

async fn insert_shadow(
    pool: &PgPool,
    s: &EntitySpec,
    ready: &NarrativesReady,
    row: &ShadowNarrativeRow<'_>,
    request_json: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO news_summaries_shadow (
            source, entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components, input_news_ids,
            original_corpus_size, deduped_corpus_size,
            model_version, prompt_version, temperature, built_prompt, ollama_request
        ) VALUES ('rust',$1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7::jsonb,$8,$9,$10,$11,$12,$13,$14,$15::jsonb)
        "#,
    )
    .bind(&s.entity_type)
    .bind(s.entity_id)
    .bind(&s.sport)
    .bind(row.title)
    .bind(row.body)
    .bind(row.impact)
    .bind(&row.impact_components)
    .bind(row.input_news_ids)
    .bind(ready.original_corpus_size as i32)
    .bind(ready.corpus.len() as i32)
    .bind(&ready.model_configured)
    .bind(NARRATIVES_PROMPT_VERSION)
    .bind(PARITY_TEMPERATURE as f32)
    .bind(&ready.built_prompt)
    .bind(request_json)
    .execute(pool)
    .await
    .context("persist narratives shadow row")?;
    Ok(())
}

fn parse_specs(args: impl Iterator<Item = String>) -> Result<Vec<EntitySpec>> {
    let mut out = Vec::new();
    for a in args {
        let parts: Vec<&str> = a.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow!("bad spec {a:?}; want entity_type:id:sport"));
        }
        let entity_type = parts[0].to_lowercase();
        if entity_type != "player" && entity_type != "team" {
            return Err(anyhow!("bad spec {a:?}; entity_type must be player|team"));
        }
        let entity_id: i32 = parts[1]
            .parse()
            .with_context(|| format!("bad id in {a:?}"))?;
        out.push(EntitySpec {
            entity_type,
            entity_id,
            sport: parts[2].to_uppercase(),
        });
    }
    Ok(out)
}

async fn auto_select(pool: &PgPool, limit: i64) -> Result<Vec<EntitySpec>> {
    let rows: Vec<(String, i32, String)> = sqlx::query_as(
        r#"
        SELECT nae.entity_type, nae.entity_id, nae.sport
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        WHERE nae.entity_type IN ('player', 'team')
          AND nae.vetted IS TRUE
          AND (a.published_at IS NULL OR a.published_at > NOW() - make_interval(secs => 259200))
        GROUP BY nae.entity_type, nae.entity_id, nae.sport
        ORDER BY COUNT(*) DESC, nae.sport, nae.entity_type, nae.entity_id
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("auto-select narrative parity entities")?;

    Ok(rows
        .into_iter()
        .map(|(entity_type, entity_id, sport)| EntitySpec {
            entity_type,
            entity_id,
            sport: sport.to_uppercase(),
        })
        .collect())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

trait ReadyExt {
    fn deduped_corpus_size(&self) -> usize;
}

impl ReadyExt for NarrativesReady {
    fn deduped_corpus_size(&self) -> usize {
        self.corpus.len()
    }
}

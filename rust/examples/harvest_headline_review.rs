//! Make a blinded, on-host human review packet from a headline replay.
//! Publisher text stays in the output directory; scores stay in manifest.json.
//! cargo run --release --example harvest_headline_review -- REPLAY.json OUTPUT_DIR
use anyhow::{ensure, Context, Result};
use scoracle_cognition::plugins::harvester::cognition::first_paragraphs;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashSet};
use std::fs::{create_dir, read_to_string, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Clone, Deserialize)]
struct Replay {
    rows: Vec<ReplayRow>,
}

#[derive(Clone, Deserialize)]
struct ReplayRow {
    article_id: i64,
    entity_id: i32,
    sport: String,
    old_body_stratum_not_gold: String,
    headline_sha256: String,
    p_relevant: Option<f64>,
}

#[derive(Serialize)]
struct Selection {
    case_id: String,
    split: String,
    article_id: i64,
    entity_id: i32,
    sport: String,
    old_body_stratum_not_gold: String,
    p_relevant: f64,
    headline_sha256: String,
    opening_available: bool,
}

fn write_file(path: &Path, data: &str) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data.as_bytes())?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: harvest_headline_review REPLAY.json OUTPUT_DIR"
    );
    let replay: Replay = serde_json::from_str(&read_to_string(&args[1])?)?;
    let db_url = std::env::var("DATABASE_PRIVATE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("database URL is required")?;
    let pool = PgPool::connect(&db_url).await?;
    let output_dir = Path::new(&args[2]);
    create_dir(output_dir).context("review output directory must not already exist")?;

    // Six cells cover both sides of the current policy boundary in each old
    // body-decision stratum. The old decisions are sampling strata, not gold.
    let mut cells = BTreeMap::<(String, bool), Vec<ReplayRow>>::new();
    for row in replay.rows {
        if let Some(p) = row.p_relevant {
            ensure!((0.0..=1.0).contains(&p), "invalid replay probability");
            cells
                .entry((row.old_body_stratum_not_gold.clone(), p >= 0.25))
                .or_default()
                .push(row);
        }
    }
    ensure!(
        cells.len() == 6,
        "expected six old-stratum by boundary cells"
    );
    let mut chosen = Vec::<(String, ReplayRow)>::new();
    let mut used_articles = HashSet::<i64>::new();
    for ((stratum, above), rows) in &mut cells {
        rows.sort_by_key(|row| {
            hex::encode(Sha256::digest(
                format!("{}:{}:{}", row.article_id, row.entity_id, row.sport).as_bytes(),
            ))
        });
        let mut count = 0;
        for row in rows {
            if used_articles.insert(row.article_id) {
                chosen.push((
                    if count < 6 { "development" } else { "holdout" }.into(),
                    row.clone(),
                ));
                count += 1;
                if count == 12 {
                    break;
                }
            }
        }
        ensure!(
            count == 12,
            "too few unique articles in {stratum} / {above}"
        );
    }
    let mut headline_docs = BTreeMap::<String, String>::new();
    let mut opening_docs = BTreeMap::<String, String>::new();
    let mut label_sheets = BTreeMap::<String, String>::new();
    for split in ["development", "holdout"] {
        headline_docs.insert(split.into(), format!("# Headline review — {split}\n\nJudge only the Google headline and target sports entity. Mark whether opening the publisher article is warranted. Do not consult the opening packet until these labels are set.\n\n"));
        opening_docs.insert(split.into(), format!("# Opening review — {split}\n\nAfter labeling headlines, judge whether the exact available opening supplies useful evidence for each character theme. Empty or polluted openings may be marked unusable.\n\n"));
        label_sheets.insert(split.into(), "case_id,headline_should_read,opening_useful,narrative,emotional_charge,transfers,availability,reviewer,notes\n".into());
    }
    let mut manifest = Vec::<Selection>::new();
    for (index, (split, row)) in chosen.into_iter().enumerate() {
        let case_id = format!("H{:03}", index + 1);
        let source = sqlx::query("SELECT n.title,n.source,n.url,n.full_text,t.name AS team_name FROM public.news_articles n JOIN public.teams t ON t.id=$2 AND t.sport=$3 WHERE n.id=$1")
            .bind(row.article_id)
            .bind(row.entity_id)
            .bind(&row.sport)
            .fetch_one(&pool)
            .await?;
        let headline: String = source.get("title");
        ensure!(
            hex::encode(Sha256::digest(headline.as_bytes())) == row.headline_sha256,
            "headline changed since replay"
        );
        let name: String = source.get("team_name");
        let body: Option<String> = source.get("full_text");
        let opening = body
            .as_deref()
            .map(|b| first_paragraphs(b, 3).text)
            .unwrap_or_default();
        let headline_doc = headline_docs.get_mut(&split).unwrap();
        headline_doc.push_str(&format!("## {case_id}\n\nTarget: {name} ({})\n\nHeadline: {headline}\n\nHeadline warrants publisher read: yes / no / unsure\n\n", row.sport));
        let opening_doc = opening_docs.get_mut(&split).unwrap();
        opening_doc.push_str(&format!("## {case_id}\n\nSource: {}\n\nURL: {}\n\nHeadline: {headline}\n\nTarget: {name} ({})\n\nOpening:\n\n{opening}\n\nUseful opening: yes / no / unsure\n\nNarrative: yes / no / unsure · Emotional charge: yes / no / unsure · Transfers: yes / no / unsure · Availability: yes / no / unsure\n\n", source.get::<Option<String>, _>("source").unwrap_or_default(), source.get::<String, _>("url"), row.sport));
        label_sheets
            .get_mut(&split)
            .unwrap()
            .push_str(&format!("{case_id},,,,,,,,\n"));
        manifest.push(Selection {
            case_id,
            split,
            article_id: row.article_id,
            entity_id: row.entity_id,
            sport: row.sport,
            old_body_stratum_not_gold: row.old_body_stratum_not_gold,
            p_relevant: row.p_relevant.unwrap(),
            headline_sha256: row.headline_sha256,
            opening_available: !opening.trim().is_empty(),
        });
    }
    for split in ["development", "holdout"] {
        write_file(
            &output_dir.join(format!("{split}-headlines.md")),
            &headline_docs[split],
        )?;
        write_file(
            &output_dir.join(format!("{split}-openings.md")),
            &opening_docs[split],
        )?;
        write_file(
            &output_dir.join(format!("{split}-labels.csv")),
            &label_sheets[split],
        )?;
    }
    write_file(
        &output_dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest)?,
    )?;
    println!(
        "{}",
        json!({"cases":manifest.len(),"development":36,"holdout":36,"openings_available":manifest.iter().filter(|s| s.opening_available).count(),"review_dir":output_dir})
    );
    Ok(())
}

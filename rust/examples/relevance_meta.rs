//! Snapshot the existing meta.rs context for an offline relevance replay. No writes to Postgres.
//! RELEVANCE_REPLAY_DATABASE_URL=... cargo run --example relevance_meta -- INPUT.jsonl OUTPUT.jsonl
use anyhow::{ensure, Context, Result};
use scoracle_cognition::tools::meta::{self, EntityMeta};
use serde_json::{json, Value};
use std::io::Write;

fn subject(case: &Value) -> Result<EntityMeta> {
    let name = case["subject"].as_str().context("subject name")?.to_owned();
    let sport = case["sport"].as_str().context("subject sport")?.to_owned();
    let entity_id = match case["kind"].as_str() {
        Some("synthetic") => -1,
        Some("retained-source") => i32::try_from(
            case["entity_id"]
                .as_i64()
                .context("canonical team ID required")?,
        )?,
        _ => anyhow::bail!("unknown calibration case kind"),
    };
    ensure!(
        !name.trim().is_empty() && !sport.trim().is_empty(),
        "empty identity"
    );
    ensure!(
        case["kind"] == "synthetic" || entity_id > 0,
        "invalid canonical ID"
    );
    Ok(EntityMeta {
        name,
        entity_type: "team".into(),
        entity_id,
        sport,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: relevance_meta INPUT.jsonl OUTPUT.jsonl"
    );
    let connect: sqlx::postgres::PgConnectOptions =
        std::env::var("RELEVANCE_REPLAY_DATABASE_URL")?.parse()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(connect.options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", "15000"),
        ]))
        .await?;
    let readonly: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&pool)
        .await?;
    ensure!(
        readonly == "on",
        "metadata replay requires read-only Postgres"
    );
    let cases = std::fs::read_to_string(&args[1])?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    for line in cases.lines().filter(|line| !line.trim().is_empty()) {
        let mut case: Value = serde_json::from_str(line)?;
        let entity = subject(&case)?;
        let synthetic = case["kind"] == "synthetic";
        let record = if synthetic {
            case["fixture_identity_record"].as_str().map(str::to_owned)
        } else {
            let canonical_name =
                meta::lookup_entity_name(&pool, "team", entity.entity_id, &entity.sport).await?;
            ensure!(
                canonical_name == entity.name,
                "subject name/ID mismatch: requested {:?}, canonical {:?}",
                entity.name,
                canonical_name
            );
            Some(
                meta::load_identity_record(&pool, "team", entity.entity_id, &entity.sport)
                    .await?
                    .context("canonical identity record missing")?,
            )
        };
        case["subject"] = json!(entity.name);
        case["subject_reference"] = json!(entity.reference());
        case["meta"] = json!({"identity": entity, "identity_record": record,
            "origin": if synthetic { "synthetic_fixture" } else { "canonical_meta_rs" },
            "evidence_scope": "Subject identity only; not article evidence."});
        writeln!(output, "{case}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_sources_require_canonical_ids_and_fixtures_are_explicit() {
        let mut case = json!({"subject":"Cedar FC", "sport":"FOOTBALL", "kind":"retained-source"});
        assert!(subject(&case).is_err());
        case["entity_id"] = json!(-1);
        assert!(subject(&case).is_err());
        case["kind"] = json!("synthetic");
        let entity = subject(&case).unwrap();
        assert_eq!(entity.entity_id, -1);
        assert_eq!(entity.reference(), "Cedar FC, the FOOTBALL team");
    }
}

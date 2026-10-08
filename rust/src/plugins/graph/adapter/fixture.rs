//! Source-grounded fixture-result nomination owned by Graph from complete native acquisition.
use crate::plugins::graph::cognition::GRAPH_PROMPT_VERSION;
use crate::plugins::graph::result::parse_result_line;
use anyhow::{Context, Result};
use sqlx::PgConnection;

pub(super) async fn review(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
    input_hash: &str,
    model_version: &str,
    result_line: &str,
) -> Result<Option<&'static str>> {
    let source = super::load_source(conn, article_id, sport)
        .await?
        .context("Graph fixture source disappeared")?;
    let headline = source.provenance["title"].as_str().unwrap_or_default();
    let opening = &source.body;

    let mut quote = None;
    let fixture_id = Option::<i32>::None;
    let status = if result_line.trim().is_empty() {
        "no_result"
    } else if result_line.chars().count() > 300
        || !(headline.contains(result_line) || opening.contains(result_line))
    {
        "quote_not_found"
    } else {
        quote = Some(result_line);
        match parse_result_line(result_line) {
            None => "invalid_result",
            Some(parsed) => {
                let home = resolve_team(conn, sport, &parsed.home).await?;
                let away = resolve_team(conn, sport, &parsed.away).await?;
                match (home, away) {
                    (Some(home), Some(away)) if home != away => {
                        // A copied score line does not identify a fixture, its date,
                        // finality or trusted result. Preserve the nomination for review.
                        "extraction_unavailable"
                    }
                    _ => "team_unresolved",
                }
            }
        }
    };
    sqlx::query(
        "INSERT INTO public.harvester_fixture_reviews \
         (article_id,sport,contract_version,input_hash,model_version,result_line,source_quote,status,fixture_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) \
         ON CONFLICT(article_id,sport,contract_version,input_hash) DO UPDATE SET \
           model_version=EXCLUDED.model_version,result_line=EXCLUDED.result_line, \
           source_quote=EXCLUDED.source_quote,status=EXCLUDED.status, \
           fixture_id=EXCLUDED.fixture_id,reviewed_at=now()",
    )
    .bind(article_id)
    .bind(sport)
    .bind(GRAPH_PROMPT_VERSION)
    .bind(input_hash)
    .bind(model_version)
    .bind(result_line)
    .bind(quote)
    .bind(status)
    .bind(fixture_id)
    .execute(&mut *conn)
    .await?;
    Ok(Some(status))
}

async fn resolve_team(conn: &mut PgConnection, sport: &str, name: &str) -> Result<Option<i32>> {
    let matches: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT entity_id FROM public.entity_name_surfaces \
         WHERE sport=$1 AND entity_type='team' AND norm=public.nrm($2) LIMIT 2",
    )
    .bind(sport)
    .bind(name)
    .fetch_all(&mut *conn)
    .await?;
    Ok(if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    })
}

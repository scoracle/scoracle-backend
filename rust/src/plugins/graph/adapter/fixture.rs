//! Source-grounded fixture-result nomination owned by Graph for Harvester articles.
use crate::evidence::news::result::parse_result_line;
use crate::plugins::graph::cognition::GRAPH_PROMPT_VERSION;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

pub(super) async fn review(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
    input_hash: &str,
    model_version: &str,
    result_line: &str,
) -> Result<Option<&'static str>> {
    let source = sqlx::query(
        "SELECT c.headline,c.context_text,c.context_start,c.context_end,c.body_sha256, \
                a.title,a.full_text \
         FROM public.harvester_classifications c \
         JOIN public.news_articles a ON a.id=c.article_id \
         WHERE c.article_id=$1 AND c.sport=$2 \
         ORDER BY c.created_at DESC,c.id DESC LIMIT 1",
    )
    .bind(article_id)
    .bind(sport)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(source) = source else {
        return Ok(None);
    };
    let headline: String = source.get("headline");
    let opening: String = source.get("context_text");
    let body: String = source
        .get::<Option<String>, _>("full_text")
        .context("Harvester fixture review has no retained publisher body")?;
    let start: i32 = source.get("context_start");
    let end: i32 = source.get("context_end");
    let hash: String = source.get("body_sha256");
    let title: String = source.get("title");
    ensure!(
        start >= 0
            && end >= start
            && hex::encode(Sha256::digest(body.as_bytes())) == hash
            && body.get(start as usize..end as usize) == Some(opening.as_str())
            && headline == title,
        "Harvester fixture review source hash or byte range drift"
    );

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

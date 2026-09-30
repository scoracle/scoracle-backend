//! Source-grounded fixture-result nomination owned by Graph for Harvester articles.
use crate::evidence::news::result::{parse_result_line, ParsedResult};
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
    let mut fixture_id = None;
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
                        let (outcome, id) = upsert_fixture(
                            conn,
                            article_id,
                            sport,
                            model_version,
                            result_line,
                            &parsed,
                            home,
                            away,
                        )
                        .await?;
                        fixture_id = Some(id);
                        outcome
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

#[allow(clippy::too_many_arguments)]
async fn upsert_fixture(
    conn: &mut PgConnection,
    article_id: i64,
    sport: &str,
    model_version: &str,
    source_quote: &str,
    parsed: &ParsedResult,
    home_id: i32,
    away_id: i32,
) -> Result<(&'static str, i32)> {
    let meta = serde_json::json!({
        "needs_verification": true,
        "nominated_by": "harvester_graph",
        "article_id": article_id,
        "source_quote": source_quote,
        "model_version": model_version,
        "contract_version": GRAPH_PROMPT_VERSION,
    });
    let existing = sqlx::query(
        "WITH anchor AS ( \
            SELECT COALESCE(published_at,fetched_at) AS at \
              FROM public.news_articles WHERE id=$4 \
         ) SELECT f.id,f.status,f.home_team_id,f.home_score,f.away_score \
           FROM public.fixtures f,anchor \
          WHERE f.sport=$1 \
            AND ((f.home_team_id=$2 AND f.away_team_id=$3) \
              OR (f.home_team_id=$3 AND f.away_team_id=$2)) \
            AND f.start_time BETWEEN anchor.at-interval '2 days' \
                                 AND anchor.at+interval '2 days' \
          ORDER BY abs(extract(epoch FROM (f.start_time-anchor.at))),f.id LIMIT 1",
    )
    .bind(sport)
    .bind(home_id)
    .bind(away_id)
    .bind(article_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = existing {
        let id: i32 = row.get("id");
        let fixture_home: i32 = row.get("home_team_id");
        let (wanted_home, wanted_away) = if fixture_home == home_id {
            (parsed.home_score as i32, parsed.away_score as i32)
        } else {
            (parsed.away_score as i32, parsed.home_score as i32)
        };
        let current_home: Option<i32> = row.get("home_score");
        let current_away: Option<i32> = row.get("away_score");
        let status: String = row.get("status");
        if (status == "completed" || status == "seeded")
            && current_home == Some(wanted_home)
            && current_away == Some(wanted_away)
        {
            return Ok(("already_correct", id));
        }
        sqlx::query(
            "UPDATE public.fixtures SET \
               status=CASE WHEN status='seeded' THEN status ELSE 'completed' END, \
               home_score=$2,away_score=$3,meta=meta || $4::jsonb,updated_at=now() \
             WHERE id=$1",
        )
        .bind(id)
        .bind(wanted_home)
        .bind(wanted_away)
        .bind(meta)
        .execute(&mut *conn)
        .await?;
        return Ok(("corrected", id));
    }
    let id: i32 = sqlx::query_scalar(
        "WITH anchor AS ( \
            SELECT COALESCE(published_at,fetched_at) AS at \
              FROM public.news_articles WHERE id=$7 \
         ) INSERT INTO public.fixtures \
           (sport,season,league_id,home_team_id,away_team_id,start_time, \
            status,home_score,away_score,external_id,meta) \
         SELECT $1,CASE WHEN extract(month FROM anchor.at)>=7 \
                        THEN extract(year FROM anchor.at)::int \
                        ELSE extract(year FROM anchor.at)::int-1 END, \
                CASE WHEN ht.league_id=at.league_id THEN ht.league_id ELSE NULL END, \
                $2,$3,anchor.at,'completed',$4,$5,NULL,$6::jsonb \
           FROM anchor,public.teams ht,public.teams at \
          WHERE ht.id=$2 AND ht.sport=$1 AND at.id=$3 AND at.sport=$1 \
         RETURNING id",
    )
    .bind(sport)
    .bind(home_id)
    .bind(away_id)
    .bind(parsed.home_score as i32)
    .bind(parsed.away_score as i32)
    .bind(meta)
    .bind(article_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(("created", id))
}

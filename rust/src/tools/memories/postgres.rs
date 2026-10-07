//! Read-only exports of unchanged stored records for DuckDB memory studies.
//! This adapter supplies bounded snapshots, never analytical findings.
use super::*;

pub async fn team_matches(
    pool: &PgPool,
    subject: &EntityMeta,
    metric: &str,
    league_id: i32,
    season: i32,
    from: i64,
    split: i64,
    before: i64,
) -> Result<statistic::Study> {
    ensure!(
        subject.entity_type == "team" && from < split && split < before,
        "invalid team statistic scope"
    );
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    let (mvcc_snapshot,captured_at):(String,i64)=sqlx::query_as("SELECT pg_current_snapshot()::text,floor(extract(epoch FROM transaction_timestamp()))::bigint").fetch_one(&mut *tx).await?;
    ensure!(
        before <= captured_at,
        "statistic window extends into future"
    );
    let (unit,measure_label): (String,String)=sqlx::query_as("SELECT COALESCE(unit,''),display_name FROM stat_definitions WHERE sport=$1 AND entity_type='team' AND key_name=$2")
    .bind(&subject.sport).bind(metric).fetch_optional(&mut *tx).await?.context("unregistered team measure")?;
    ensure!(
        unit == "cumulative_total",
        "match-total study requires a registered additive measure"
    );
    let rows:Vec<String>=sqlx::query_scalar(
    "SELECT jsonb_build_object('fixture_id',f.id,'played_at',floor(extract(epoch FROM f.start_time))::bigint,
    'value',CASE WHEN (s.stats->>$4) ~ '^-?[0-9]+([.][0-9]+)?$' THEN (s.stats->>$4)::double precision ELSE NULL END)::text
    FROM fixtures f LEFT JOIN event_team_stats s ON s.fixture_id=f.id AND s.team_id=$2 AND s.sport=f.sport AND s.league_id=$3 AND s.season=$5
    WHERE f.sport=$1 AND $2 IN (f.home_team_id,f.away_team_id) AND f.league_id=$3 AND f.season=$5
    AND f.status IN ('completed','seeded') AND COALESCE(f.meta->>'needs_verification','false')<>'true'
    AND f.start_time>=to_timestamp($6::double precision) AND f.start_time<to_timestamp($7::double precision)
    ORDER BY f.start_time,f.id LIMIT 20001")
    .bind(&subject.sport).bind(subject.entity_id).bind(league_id).bind(metric).bind(season).bind(from).bind(before)
    .fetch_all(&mut *tx).await?;
    ensure!(rows.len() <= 20000, "statistic population exceeds bound");
    let observations = rows
        .iter()
        .map(|r| serde_json::from_str(r))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    tx.commit().await?;
    let req = statistic::Request {
        kind: "statistic".into(),
        version: "match-statistic-v1".into(),
        metric: metric.into(),
        unit,
        from,
        split,
        before,
        observations,
    };
    let input_hash = crate::util::hash_components(&serde_json::to_string(&(
        subject,
        league_id,
        season,
        &measure_label,
        &req,
    ))?);
    let finding = run(&req).await?;
    Ok(statistic::Study {subject:subject.clone(),measure_label,league_id,season,captured_at,mvcc_snapshot,input_hash,version:req.version,
    coverage:"Stored completed verified fixtures in the requested competition and season; absent measures remain missing. Fixture inventory completeness is not established.".into(),finding})
}

/// Export stored plugin scores; DuckDB owns weekly means and window comparisons.
pub async fn score_history(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<ScoreHistory> {
    ensure!(
        matches!(entity_type, "player" | "team") && entity_id > 0,
        "invalid score history subject"
    );
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    let before: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM transaction_timestamp()))::bigint")
            .fetch_one(&mut *tx)
            .await?;
    let rows: Vec<String> = sqlx::query_scalar(
        "WITH scores AS (
           SELECT 'rating' rail,id,notability::float8 value,generated_at,week_season,week_no
           FROM public.stat_summaries
           WHERE sport=$1 AND entity_type=$2 AND entity_id=$3
             AND season IN (SELECT current_season FROM public.sports WHERE id=$1
                            UNION SELECT current_season-1 FROM public.sports WHERE id=$1)
           UNION ALL
           SELECT 'vibe',id,sentiment::float8,generated_at,week_season,week_no
           FROM public.vibe_scores WHERE sport=$1 AND entity_type=$2 AND entity_id=$3
         ) SELECT jsonb_build_object('id',s.id,'rail',s.rail,'value',s.value,
           'observed_at',floor(extract(epoch FROM s.generated_at))::bigint,
           'week_start',floor(extract(epoch FROM sw.starts_at))::bigint)::text
         FROM scores s JOIN public.season_weeks sw
           ON sw.sport=$1 AND sw.season=s.week_season AND sw.week_no=s.week_no
         WHERE s.generated_at<=to_timestamp($4::double precision)
           AND sw.starts_at<=to_timestamp($4::double precision)
           AND (s.rail='rating' OR sw.ends_at>to_timestamp($4::double precision)-interval '21 days')
         ORDER BY s.rail,s.id LIMIT 20001",
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(before)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        rows.len() <= 20_000,
        "score history population exceeds bound"
    );
    tx.commit().await?;
    let observations: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| serde_json::from_str(row))
        .collect::<std::result::Result<_, _>>()?;
    run(&serde_json::json!({
        "kind":"score_history", "version":"score-history-v1", "before":before,
        "rating_weeks":match sport { "NBA"=>3, "NFL"=>2, _=>4 },
        "observations":observations,
    }))
    .await
}

pub async fn source_records(
    pool: &PgPool,
    sport: &str,
    publishers: &[String],
) -> Result<Vec<SourceRecord>> {
    // Postgres exports attributed positive player reports and their stored outcomes.
    // DuckDB owns canonical pair deduplication, sample counts and reliability math.
    let observations: Vec<serde_json::Value> = sqlx::query_scalar::<_, String>(
        "SELECT jsonb_build_object('publisher',source,'player_id',r.player_id, \
         'team_id',r.team_id,'confirmed',g.player_id IS NOT NULL)::text \
         FROM public.transfer_rumors r CROSS JOIN LATERAL unnest(r.source_names) AS s(source) \
         LEFT JOIN public.transfer_ground_truth g \
           ON g.sport=r.sport AND g.player_id=r.player_id AND g.team_id=r.team_id \
         WHERE r.sport=$1 AND r.subject_type='player' AND r.is_rumor IS TRUE \
           AND source=ANY($2) ORDER BY source,r.player_id,r.team_id,r.id LIMIT 20001",
    )
    .bind(sport)
    .bind(publishers)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| serde_json::from_str(row))
    .collect::<std::result::Result<_, _>>()?;
    ensure!(
        observations.len() <= 20_000,
        "publisher record population exceeds bound"
    );
    if observations.is_empty() {
        return Ok(Vec::new());
    }
    run(&serde_json::json!({
        "kind": "source_records", "version": "publisher-outcomes-v1",
        "observations": observations,
    }))
    .await
}

pub async fn reporting_scope(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
    exclude: &[i64],
    limit: usize,
    per_topic: usize,
    include: &[i64],
    topic: Option<Topic<'_>>,
) -> Result<Study> {
    ensure!(
        matches!(subject.entity_type.as_str(), "team" | "player") && subject.entity_id > 0,
        "reporting study requires a canonical team or player; person Graph IDs need reconciliation"
    );
    ensure!(
        include.len() <= 20_000,
        "included article set exceeds bound; resolve a narrower set"
    );
    ensure!(
        from < before && (1..=20).contains(&limit) && (1..=3).contains(&per_topic),
        "invalid memory scope"
    );
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    let (mvcc_snapshot, captured_at): (String, i64) = sqlx::query_as(
        "SELECT pg_current_snapshot()::text,floor(extract(epoch FROM transaction_timestamp()))::bigint",
    ).fetch_one(&mut *tx).await?;
    ensure!(
        before <= captured_at,
        "reporting window extends into future"
    );
    let rows: Vec<String> = sqlx::query_scalar(include_str!("reporting.sql"))
        .bind(&subject.sport)
        .bind(&subject.entity_type)
        .bind(subject.entity_id)
        .bind(&subject.name)
        .bind(from)
        .bind(before)
        .bind(exclude)
        .bind(include)
        .fetch_all(&mut *tx)
        .await?;
    ensure!(
        rows.len() <= 20000,
        "memory population exceeds bound; narrow the timeframe"
    );
    let observations = rows
        .iter()
        .map(|r| serde_json::from_str(r))
        .collect::<std::result::Result<Vec<Observation>, _>>()?;
    tx.commit().await?;
    let mut req = Request {
        version: VERSION.into(),
        from,
        before,
        limit,
        per_topic,
        observations,
    };
    // Grouping is applied before hashing so the receipt covers the groups the
    // findings were actually built from, not the study's default grouping.
    if let Some(topic) = topic {
        apply_topics(&mut req, topic);
    }
    let input_hash =
        crate::util::hash_components(&serde_json::to_string(&(subject, include, &req))?);
    let receipt = Receipt {
        version: VERSION.into(),
        subject: subject.clone(),
        from,
        before,
        input_hash,
        captured_at,
        mvcc_snapshot,
        observed_articles: req.observations.len(),
        included_articles: include.len(),
    };
    let findings = if req.observations.is_empty() {
        Vec::new()
    } else {
        compute(&req).await?
    };
    Ok(Study { receipt, findings })
}

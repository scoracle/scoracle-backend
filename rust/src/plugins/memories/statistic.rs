//! Comparable team match-stat windows, selected at request time.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchObservation {
    pub fixture_id: i64,
    pub played_at: i64,
    pub value: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub kind: String,
    pub version: String,
    pub metric: String,
    pub unit: String,
    pub from: i64,
    pub split: i64,
    pub before: i64,
    pub observations: Vec<MatchObservation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Window {
    pub from: i64,
    pub before: i64,
    pub fixtures: usize,
    pub measured: usize,
    pub total: Option<f64>,
    pub per_match: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub metric: String,
    pub unit: String,
    pub previous: Window,
    pub current: Window,
    pub per_match_change: Option<f64>,
    pub percent_change: Option<f64>,
    pub fixture_ids: Vec<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Study {
    pub measure_label: String,
    pub subject: EntityMeta,
    pub league_id: i32,
    pub season: i32,
    pub captured_at: i64,
    pub mvcc_snapshot: String,
    pub input_hash: String,
    pub version: String,
    pub coverage: String,
    pub finding: Finding,
}

/// The initial statistic adapter is team match totals in one competition and
/// season. A plugin must request a registered measure, not infer units from prose.
pub async fn team_matches(
    pool: &PgPool,
    subject: &EntityMeta,
    metric: &str,
    league_id: i32,
    season: i32,
    from: i64,
    split: i64,
    before: i64,
) -> Result<Study> {
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
    let req = Request {
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
    let finding = super::run(&req).await?;
    Ok(Study {subject:subject.clone(),measure_label,league_id,season,captured_at,mvcc_snapshot,input_hash,version:req.version,
        coverage:"Stored completed verified fixtures in the requested competition and season; absent measures remain missing. Fixture inventory completeness is not established.".into(),finding})
}

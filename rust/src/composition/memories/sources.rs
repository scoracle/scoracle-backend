//! Deterministic reads over existing records. No generated summary table or daily
//! summarizer. Junctions own current evidence; source IDs distinguish earlier
//! reports from independent corroboration.

use super::*;
use anyhow::Context;
use serde_json::json;
use sqlx::{PgPool, Row};

pub struct MemoryRequest<'a> {
    pub mission: Mission,
    pub entity_type: &'a str,
    pub entity_id: i32,
    pub sport: &'a str,
    pub season: Option<i32>,
    pub pair_team_id: Option<i32>,
    pub current_article_ids: &'a [i64],
}

impl<'a> MemoryRequest<'a> {
    pub fn new(mission: Mission, entity_type: &'a str, entity_id: i32, sport: &'a str) -> Self {
        Self {
            mission,
            entity_type,
            entity_id,
            sport,
            season: None,
            pair_team_id: None,
            current_article_ids: &[],
        }
    }
}

fn record(
    section: Section,
    table: &str,
    key: String,
    observed: Option<String>,
    unix: Option<i64>,
    data: Value,
) -> Record {
    Record {
        section,
        sources: vec![SourceRef {
            table: table.into(),
            key,
        }],
        observed_at: observed,
        observed_unix: unix,
        data,
    }
}

fn add(package: &mut Package, id: &str, required: bool, records: Vec<Record>, notes: &[&str]) {
    if !records.is_empty() {
        package.groups.push(EvidenceGroup {
            id: id.into(),
            required,
            records,
            qualifications: notes.iter().map(|s| s.to_string()).collect(),
        });
    }
}

/// One shared budget, independent of output length or tokenizer. Callers may
/// configure it after measuring full requests; records are never clipped to fit.
fn max_bytes() -> usize {
    std::env::var("COGNITION_MEMORY_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(4800)
}

pub async fn load(pool: &PgPool, req: MemoryRequest<'_>) -> Result<Package> {
    let sport = req.sport.to_uppercase();
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '20s'")
        .execute(&mut *tx)
        .await?;
    let (captured_at,captured_unix,current_season):(String,i64,i32)=sqlx::query_as(
        "SELECT now()::text, floor(extract(epoch FROM now()))::bigint, current_season FROM public.sports WHERE id=$1"
    ).bind(&sport).fetch_one(&mut *tx).await.context("load memory clock")?;
    let name_query = match req.entity_type {
        "player" => "SELECT name FROM public.players WHERE id=$1 AND sport=$2",
        "team" => "SELECT name FROM public.teams WHERE id=$1 AND sport=$2",
        "person" => "SELECT full_name FROM public.persons WHERE id=$1 AND sport=$2",
        _ => anyhow::bail!("unsupported memory entity type"),
    };
    let name: String = sqlx::query_scalar(name_query)
        .bind(req.entity_id)
        .bind(&sport)
        .fetch_one(&mut *tx)
        .await
        .context("essential memory identity")?;
    let mut package = Package {
        version: VERSION.into(),
        entity: Entity {
            sport: sport.clone(),
            entity_type: req.entity_type.into(),
            id: req.entity_id,
            name,
        },
        mission: req.mission,
        captured_at,
        captured_unix,
        groups: vec![],
        unknowns: vec![],
        omissions: vec![],
        diagnostics: vec![],
        previous_score: None,
        historical: req.season.is_some_and(|s| s != current_season),
    };
    let historical = package.historical;
    if historical {
        let name = package.entity.name.clone();
        add(
            &mut package,
            "identity",
            true,
            vec![record(
                Section::Identity,
                match req.entity_type {
                    "player" => "players",
                    "team" => "teams",
                    _ => "persons",
                },
                format!("{sport}/{}/{}", req.entity_type, req.entity_id),
                None,
                None,
                json!({"name":name}),
            )],
            &[],
        );
        package.unknowns.push("Historical-season report: current employment, availability and current stories are not supplied as facts of that season.".into());
    } else {
        let identity =
            identity::load_identity_record_on(&mut tx, req.entity_type, req.entity_id, &sport)
                .await?
                .context("essential memory identity absent")?;
        let mut identity_records = vec![record(
            Section::Identity,
            match req.entity_type {
                "player" => "player_current_identity",
                "team" => "teams",
                _ => "persons",
            },
            format!("{sport}/{}/{}", req.entity_type, req.entity_id),
            None,
            None,
            json!({"text":identity}),
        )];
        let relations = sqlx::query(include_str!("relationships.sql"))
            .bind(req.entity_type)
            .bind(req.entity_id)
            .bind(&sport)
            .fetch_all(&mut *tx)
            .await?;
        for row in relations {
            identity_records.push(record(
                Section::Identity,
                "entity_relationships",
                row.get::<i64, _>("id").to_string(),
                row.get("observed_at"),
                row.get("observed_unix"),
                row.get("data"),
            ));
        }
        if req.entity_type == "player" {
            for row in sqlx::query(include_str!("affiliations.sql"))
                .bind(req.entity_id)
                .bind(&sport)
                .bind(current_season)
                .fetch_all(&mut *tx)
                .await?
            {
                let key = format!(
                    "{sport}/{}/{}/{}",
                    req.entity_id,
                    row.get::<i32, _>("season"),
                    row.get::<i32, _>("league_id")
                );
                identity_records.push(record(
                    Section::Identity,
                    "player_stats",
                    key,
                    row.get("observed_at"),
                    row.get("observed_unix"),
                    row.get("data"),
                ));
            }
        }
        add(&mut package,"identity",true,identity_records,&["Dated house records may disagree. A later observation is not itself a signing date. When later records conflict, do not present the older affiliation as current; state that the affiliation is unresolved."]);
        // Calendar evidence is selected from verified fixtures for the actual team,
        // not the sport's reporting grid or unverified article nominations.
        let fixtures = sqlx::query(include_str!("fixtures.sql"))
            .bind(req.entity_type)
            .bind(req.entity_id)
            .bind(&sport)
            .fetch_all(&mut *tx)
            .await?;
        let mut schedule = Vec::new();
        for row in fixtures {
            schedule.push(record(
                Section::CompetitionClock,
                "fixtures",
                row.get::<i32, _>("id").to_string(),
                row.get("observed_at"),
                row.get("observed_unix"),
                row.get("data"),
            ));
        }
        if schedule.is_empty() {
            package.unknowns.push("Competition stage and nearby verified fixtures are unavailable; missing fixtures do not establish an offseason.".into());
        }
        add(&mut package,"nearby fixtures",true,schedule,&["These are selected fixtures, not a complete schedule or a count of this entity's season participation."]);
    }
    if matches!(req.mission, Mission::Scout | Mission::Analyst)
        && matches!(req.entity_type, "player" | "team")
    {
        let season = req.season.unwrap_or(current_season);
        let table = if req.entity_type == "player" {
            "player_stats"
        } else {
            "team_stats"
        };
        let query = include_str!("performance.sql")
            .replace("{table}", table)
            .replace(
                "{id}",
                if req.entity_type == "player" {
                    "player_id"
                } else {
                    "team_id"
                },
            );
        let rows = sqlx::query(&query)
            .bind(&sport)
            .bind(req.entity_id)
            .bind(season)
            .bind(req.entity_type)
            .fetch_all(&mut *tx)
            .await?;
        let mut records = Vec::new();
        let mut has_prior = false;
        for row in rows {
            let row_season: i32 = row.get("season");
            has_prior |= row_season < season;
            let key = format!(
                "{sport}/{}/{}/{}",
                req.entity_id,
                row_season,
                row.get::<i32, _>("league_id")
            );
            let data = super::performance::with_rates(row.get("data"), &sport, req.entity_type);
            records.push(record(
                if row_season < season {
                    Section::EstablishedHistory
                } else {
                    Section::PresentEvidence
                },
                table,
                key,
                row.get("observed_at"),
                row.get("observed_unix"),
                data,
            ));
        }
        if !has_prior {
            package.unknowns.push("No earlier performance snapshot in the selected competition is available for comparison.".into());
        }
        add(&mut package,"performance comparison",false,records,&["Compare the same measure and time basis across these named teams and seasons. Per-90 figures use recorded minutes, not inferred appearances. Scoring and chance creation may change differently; the measurements support the interpretation, not a predetermined role change. Coverage is unknown, so a smaller stored sample does not prove reduced playing time, fitness or ability. Missing values remain unknown; counts of different actions are not interchangeable."]);
    }
    if !historical
        && matches!(
            req.mission,
            Mission::Journalist | Mission::Influencer | Mission::Insider
        )
    {
        let rows = sqlx::query(include_str!("moves.sql"))
            .bind(req.entity_type)
            .bind(req.entity_id)
            .bind(&sport)
            .bind(req.pair_team_id)
            .fetch_all(&mut *tx)
            .await?;
        let mut moves = Vec::new();
        for row in rows {
            moves.push(record(
                Section::EstablishedHistory,
                "transfer_ground_truth",
                row.get("source_key"),
                row.get("observed_at"),
                row.get("observed_unix"),
                row.get("data"),
            ));
        }
        add(&mut package,"recorded moves",false,moves,&["Record/application dates are observation dates, not exact transfer effective dates."]);
        if matches!(
            req.mission,
            Mission::Journalist | Mission::Influencer | Mission::Insider
        ) {
            let rows = sqlx::query(include_str!("stories.sql"))
                .bind(req.entity_type)
                .bind(req.entity_id)
                .bind(&sport)
                .bind(req.current_article_ids)
                .bind(req.pair_team_id)
                .fetch_all(&mut *tx)
                .await?;
            for row in rows {
                let source_id: i64 = row.get("article_id");
                add(&mut package,&format!("story {source_id}"),false,vec![record(Section::DevelopingHistory,"news_articles",source_id.to_string(),row.get("observed_at"),row.get("observed_unix"),row.get("data"))],&["An earlier report is attributed history, not independent confirmation of a new claim."]);
            }
        }
    }
    // At most one whole prior interpretation. It remains outside material hashes,
    // so writing a new card cannot create an endless self-triggering refresh loop.
    if !historical {
        let source = match req.mission {
            Mission::Journalist => Some(("news_summaries", "card_score", "body", "input_news_ids")),
            Mission::Influencer => Some(("vibe_scores", "sentiment", "prompt", "input_news_ids")),
            Mission::Insider if req.pair_team_id.is_none() => {
                Some(("insider_scores", "score", "read", "ARRAY[]::bigint[]"))
            }
            _ => None,
        };
        if let Some((table, score, body, news_ids)) = source {
            let q=format!("SELECT id,{score} AS score,{body} AS body,{news_ids} AS news_ids,generated_at::text AS observed_at,floor(extract(epoch FROM generated_at))::bigint AS observed_unix FROM public.{table} WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 AND {score} IS NOT NULL ORDER BY generated_at DESC,id DESC LIMIT 1");
            if let Some(row) = sqlx::query(&q)
                .bind(req.entity_type)
                .bind(req.entity_id)
                .bind(&sport)
                .fetch_optional(&mut *tx)
                .await?
            {
                package.previous_score = Some(row.get("score"));
                if let Some(body) = row
                    .get::<Option<String>, _>("body")
                    .filter(|b| !b.trim().is_empty())
                {
                    let ids: Vec<i64> = row.get("news_ids");
                    if ids.is_empty() {
                        package.omissions.push(Omission {
                            editorial: true,
                            group: "prior interpretation".into(),
                            reason: "prior prose has no recorded source article IDs".into(),
                            sources: vec![SourceRef {
                                table: table.into(),
                                key: row.get::<i64, _>("id").to_string(),
                            }],
                        });
                    } else {
                        add(
                            &mut package,
                            "prior interpretation",
                            false,
                            vec![record(
                                Section::EditorialMemory,
                                table,
                                row.get::<i64, _>("id").to_string(),
                                row.get("observed_at"),
                                row.get("observed_unix"),
                                json!({"text":body,"evidence_article_ids":ids}),
                            )],
                            &[],
                        );
                    }
                }
            }
        }
    }
    let package = package.within_bytes(max_bytes())?;
    tx.rollback().await?;
    Ok(package)
}

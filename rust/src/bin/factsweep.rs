//! Dynamic person-metadata adjudication sweep.
//!
//! The producer is the news the system already reads: for each news-active person whose
//! affiliation is absent or stale, gather the recent articles that tag them, and ask the
//! resident model ONE adjudication question over those excerpts. The healthy gates:
//!
//! - **Policy-gated**: absent policy means frozen.
//! - **Evidence floor**: fewer than two distinct articles adjudicates nothing.
//! - **Deterministic resolution**: the model must pick from the CANDIDATE TEAMS list (the
//!   clubs actually co-tagged on the evidence) — a name we cannot resolve to a team id is a
//!   null, never a fuzzy match.
//! - **Provenance**: each updated field needs contained quotes from two distinct articles.
//!   Role and affiliation are independent; unknown leaves the existing dated value alone.
//! - **Provenance**: every write supersedes (never deletes) in `entity_facts`, cites a
//!   `source_documents` row for a cited evidence article, and stamps
//!   `meta.affiliation_checked_at` so quiet outcomes debounce.
//!
//! This sweep covers persons of every kind; player identity has its own rail.
//!
//! Usage:
//!   factsweep -sport FOOTBALL [-limit 60] [-dry-run]
//!   factsweep -person 57 -sport FOOTBALL [-dry-run]      # one person, on demand

use anyhow::{anyhow, Result};
use scoracle_cognition::runtime::config::Config;
use scoracle_cognition::runtime::db;
use scoracle_cognition::runtime::route::Router;
use scoracle_cognition::studio::model::GenerateOptions;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

const EVIDENCE_DAYS: i32 = 21;
const ACTIVE_DAYS: i32 = 14;
const RECHECK_DAYS: i32 = 7;
const MAX_EVIDENCE_ARTICLES: i64 = 12;
const MIN_EVIDENCE_ARTICLES: usize = 2;
/// Kinds the adjudicated `role` may set. Deliberately excludes `player` (the player layer is
/// the players table, not a kind flip) and `other`/`unknown` (too weak a signal to overwrite).
const KIND_SET: &[&str] = &["coach", "agent", "owner", "executive", "official"];

struct Args {
    sport: String,
    limit: i64,
    person: Option<i32>,
    dry_run: bool,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args> {
    let mut a = Args {
        sport: String::new(),
        limit: 60,
        person: None,
        dry_run: false,
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "-sport" => a.sport = it.next().ok_or_else(|| anyhow!("-sport needs a value"))?,
            "-limit" => {
                a.limit = it
                    .next()
                    .ok_or_else(|| anyhow!("-limit needs a value"))?
                    .parse()?
            }
            "-person" => {
                a.person = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("-person needs a value"))?
                        .parse()?,
                )
            }
            "-dry-run" => a.dry_run = true,
            other => return Err(anyhow!("unknown flag {other:?}")),
        }
    }
    if a.sport.is_empty() {
        return Err(anyhow!("-sport is required"));
    }
    a.sport = a.sport.to_uppercase();
    Ok(a)
}

struct Candidate {
    id: i32,
    full_name: String,
    kind: String,
    team_id: Option<i32>,
}

struct Evidence {
    published: String,
    source: String,
    title: String,
    description: String,
    url: String,
    teams: Vec<(i32, String)>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Citation {
    article: usize,
    quote: String,
}

#[derive(serde::Deserialize)]
struct Verdict {
    current_team_index: Option<i64>,
    #[serde(default)]
    unaffiliated: bool,
    role: Option<String>,
    confidence: Option<f64>,
    #[serde(default)]
    affiliation_evidence: Vec<Citation>,
    #[serde(default)]
    role_evidence: Vec<Citation>,
}

const ADJUDICATION_SYSTEM: &str = r#"Task: adjudicate the CURRENT club affiliation of one sports figure, strictly from the reporting excerpts supplied.

The team must be one the excerpts state the figure CURRENTLY works for. Past clubs, opponents, rumored destinations and co-mentions are not current affiliation. Set current_team_index=null when unknown. Set unaffiliated=true only for explicit reporting that the figure is now without a club; otherwise false.

Choose the club by its NUMBER from the CANDIDATE TEAMS list, or null when no listed club is clearly current. role is what the excerpts show the figure to be: coach, player, agent, owner, executive, official, other, or unknown. confidence (0.0-1.0) is how explicitly the reporting states the affiliation — reserve 0.9+ for excerpts that state it outright.

For each field, cite at least two distinct numbered articles with an exact quote from their supplied title or excerpt that establishes the CURRENT affiliation or role. A club co-mention, former club or possible destination is not affiliation evidence. Adjudicate role independently: the role can change even when the club is unchanged or unknown. Return empty evidence arrays when the reporting does not establish a field.

Reply with ONLY this JSON object:
{"current_team_index": <number from the list, or null>, "unaffiliated": false, "role": "<role>", "confidence": <0.0-1.0>, "affiliation_evidence": [{"article": 1, "quote": "exact source words"}], "role_evidence": [{"article": 2, "quote": "exact source words"}]}"#;

fn adjudication_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "current_team_index": {"type": ["integer", "null"]},
            "unaffiliated": {"type": "boolean"},
            "role": {"type": "string"},
            "confidence": {"type": "number"},
            "affiliation_evidence": citation_schema(),
            "role_evidence": citation_schema()
        },
        "required": ["current_team_index", "unaffiliated", "role", "confidence", "affiliation_evidence", "role_evidence"]
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let cfg = Config::from_env()?;
    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    let router = Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?;

    // The healthy gate the whole sweep hangs from: no policy row ⇒ frozen ⇒ exit.
    let policy_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.entity_fact_policy
          WHERE entity_type = 'person' AND fact_type = 'team_affiliation')",
    )
    .fetch_one(&pool)
    .await?;
    let role_policy_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.entity_fact_policy
          WHERE entity_type = 'person' AND fact_type = 'role')",
    )
    .fetch_one(&pool)
    .await?;

    if !policy_ok && !role_policy_ok {
        println!("factsweep: no writable person metadata fields");
        return Ok(());
    }
    let candidates = load_candidates(&pool, &args).await?;
    println!(
        "factsweep: {} candidate person(s) in {} (limit {}, dry_run {})",
        candidates.len(),
        args.sport,
        args.limit,
        args.dry_run
    );

    let mut written = 0usize;
    let mut unknown = 0usize;
    let mut thin = 0usize;
    for cand in &candidates {
        stamp_checked(&pool, cand.id, &args.sport, "checking", args.dry_run).await?;
        let ev = load_evidence(&pool, cand.id, &args.sport).await?;
        if ev.len() < MIN_EVIDENCE_ARTICLES {
            thin += 1;
            stamp_checked(&pool, cand.id, &args.sport, "thin evidence", args.dry_run).await?;
            continue;
        }
        // Deterministic candidates: only clubs actually co-tagged on the evidence,
        // presented NUMBERED — the model picks an index, never spells a name, so an
        // embellished names cannot miss the database's canonical name.
        let mut seen: HashMap<i32, ()> = HashMap::new();
        let mut teams: Vec<(i32, String)> = Vec::new();
        for e in &ev {
            for (id, name) in &e.teams {
                if seen.insert(*id, ()).is_none() {
                    teams.push((*id, name.clone()));
                }
            }
        }
        teams.sort_by(|a, b| a.1.cmp(&b.1));
        let prompt = build_prompt(cand, &ev, &teams);
        let opts = GenerateOptions {
            system: Some(ADJUDICATION_SYSTEM.to_string()),
            temperature: Some(0.0),
            num_predict: 700,
            num_ctx: cfg.voice_num_ctx,
            json_mode: true,
            format_schema: Some(adjudication_schema()),
            format_schema_raw: None,
        };
        let client = router.for_route(scoracle_cognition::plugins::investigator::manifest::ROUTE);
        let (result, _body) = match client.generate(&prompt, &opts).await {
            Ok(result) => result,
            Err(error) => {
                unknown += 1;
                eprintln!(
                    "  {} ({}): metadata read failed: {error}",
                    cand.full_name, cand.id
                );
                stamp_checked(
                    &pool,
                    cand.id,
                    &args.sport,
                    "model call failed",
                    args.dry_run,
                )
                .await?;
                continue;
            }
        };
        let verdict: Verdict = match serde_json::from_str(result.response.trim()) {
            Ok(v) => v,
            Err(e) => {
                println!(
                    "  {} ({}): unparseable verdict ({e}); skipped",
                    cand.full_name, cand.id
                );
                continue;
            }
        };

        let confidence = verdict.confidence.unwrap_or(0.0);
        let team = verdict
            .current_team_index
            .and_then(|i| usize::try_from(i.checked_sub(1)?).ok())
            .and_then(|i| teams.get(i).cloned());
        let affiliation_sources = grounded_sources(&verdict.affiliation_evidence, &ev);
        let role_sources = grounded_sources(&verdict.role_evidence, &ev);
        let affiliation_grounded = policy_ok && affiliation_sources.len() >= MIN_EVIDENCE_ARTICLES;
        let clear_team =
            affiliation_grounded && verdict.unaffiliated && verdict.current_team_index.is_none();
        let team = team.filter(|_| affiliation_grounded && !verdict.unaffiliated);
        let role = verdict.role.as_deref().filter(|r| {
            role_policy_ok && KIND_SET.contains(r) && role_sources.len() >= MIN_EVIDENCE_ARTICLES
        });
        if team.is_none() && !clear_team && role.is_none() {
            unknown += 1;
            stamp_checked(
                &pool,
                cand.id,
                &args.sport,
                "no grounded current metadata",
                args.dry_run,
            )
            .await?;
            continue;
        }
        println!(
            "  {} ({}): club {:?}, role {:?}{}",
            cand.full_name,
            cand.id,
            team.as_ref().map(|(_, name)| name),
            role,
            if args.dry_run { " [dry-run]" } else { "" }
        );
        if !args.dry_run {
            apply_metadata(
                &pool,
                cand,
                &args.sport,
                team.as_ref(),
                clear_team,
                role,
                confidence,
                affiliation_sources.first().map(|i| &ev[*i]),
                role_sources.first().map(|i| &ev[*i]),
                citation_provenance(&verdict.affiliation_evidence, &ev),
                citation_provenance(&verdict.role_evidence, &ev),
            )
            .await?;
        }
        written += 1;
    }
    println!(
        "factsweep: {} written, {} unknown, {} thin-evidence, {} scanned",
        written,
        unknown,
        thin,
        candidates.len()
    );
    Ok(())
}

async fn load_candidates(pool: &PgPool, args: &Args) -> Result<Vec<Candidate>> {
    let rows = if let Some(pid) = args.person {
        sqlx::query(
            r#"SELECT p.id, p.full_name, p.kind, p.team_id, 0::bigint AS tags
               FROM public.persons p WHERE p.id = $1 AND p.sport = $2"#,
        )
        .bind(pid)
        .bind(&args.sport)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            WITH active AS (
                SELECT nae.entity_id, count(DISTINCT nae.article_id) AS tags
                  FROM public.news_article_entities nae
                 WHERE nae.entity_type = 'person' AND nae.sport = $1
                   AND nae.created_at > NOW() - make_interval(days => $3::int)
                 GROUP BY 1
            )
            SELECT p.id, p.full_name, p.kind, p.team_id, a.tags
              FROM public.persons p JOIN active a ON a.entity_id = p.id
             WHERE p.sport = $1
               AND COALESCE((p.meta->>'affiliation_checked_at')::timestamptz,
                            'epoch'::timestamptz) < NOW() - make_interval(days => $4::int)
             ORDER BY a.tags DESC
             LIMIT $2
            "#,
        )
        .bind(&args.sport)
        .bind(args.limit)
        .bind(ACTIVE_DAYS)
        .bind(RECHECK_DAYS)
        .fetch_all(pool)
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|r| Candidate {
            id: r.get("id"),
            full_name: r.get("full_name"),
            kind: r.get("kind"),
            team_id: r.get("team_id"),
        })
        .collect())
}

async fn load_evidence(pool: &PgPool, person_id: i32, sport: &str) -> Result<Vec<Evidence>> {
    let rows = sqlx::query(
        r#"
        SELECT na.source, na.title, na.published_at::date::text AS published, COALESCE(na.description, '') AS description, na.url,
               COALESCE((SELECT json_agg(json_build_object('id', t.id, 'name', t.name))
                           FROM public.news_article_entities nt
                           JOIN public.teams t ON t.id = nt.entity_id AND t.sport = nt.sport
                          WHERE nt.article_id = na.id AND nt.entity_type = 'team' AND nt.sport = $2),
                        '[]'::json) AS teams
          FROM public.news_article_entities nae
          JOIN public.news_articles na ON na.id = nae.article_id
         WHERE nae.entity_type = 'person' AND nae.entity_id = $1 AND nae.sport = $2
           AND na.published_at > NOW() - make_interval(days => $3::int)
           AND na.duplicate_of IS NULL
         ORDER BY na.published_at DESC, na.id DESC
         LIMIT $4
        "#,
    )
    .bind(person_id)
    .bind(sport)
    .bind(EVIDENCE_DAYS)
    .bind(MAX_EVIDENCE_ARTICLES)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let teams_json: serde_json::Value = r.get("teams");
            let teams = teams_json
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| {
                            Some((
                                t.get("id")?.as_i64()? as i32,
                                t.get("name")?.as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Evidence {
                published: r.get::<Option<String>, _>("published").unwrap_or_default(),
                source: r.get::<Option<String>, _>("source").unwrap_or_default(),
                title: r.get("title"),
                description: r.get("description"),
                url: r.get("url"),
                teams,
            }
        })
        .collect())
}

fn build_prompt(cand: &Candidate, ev: &[Evidence], teams: &[(i32, String)]) -> String {
    let mut b = format!(
        "Figure: {} (on record as: {})\n\nCANDIDATE TEAMS (choose one by NUMBER, or null):\n",
        cand.full_name, cand.kind
    );
    if let Some(id) = cand.team_id {
        if let Some((_, name)) = teams.iter().find(|(team_id, _)| *team_id == id) {
            b.push_str(&format!("Club on record: {name}\n"));
        }
    }
    for (i, (_, n)) in teams.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, n));
    }
    b.push_str("\nReporting excerpts (newest first):\n");
    for (index, e) in ev.iter().enumerate() {
        b.push_str(&format!("{}. ", index + 1));
        b.push_str(&format!("{} ", e.published));
        if !e.source.is_empty() {
            b.push_str(&format!("[{}] ", e.source));
        }
        b.push_str(&evidence_text(e));
        b.push('\n');
    }
    b.push_str("\nReturn the JSON adjudication now.");
    b
}

fn citation_schema() -> serde_json::Value {
    serde_json::json!({"type":"array","items":{"type":"object","properties":{
        "article":{"type":"integer"}, "quote":{"type":"string"}}, "required":["article","quote"]}})
}

fn evidence_text(e: &Evidence) -> String {
    let excerpt: String = e.description.chars().take(280).collect();
    if excerpt.is_empty() {
        e.title.clone()
    } else {
        format!("{} — {excerpt}", e.title)
    }
}

fn citation_provenance(citations: &[Citation], evidence: &[Evidence]) -> serde_json::Value {
    serde_json::json!(citations.iter().filter_map(|c| {
        let i = grounded_sources(std::slice::from_ref(c), evidence).into_iter().next()?;
        Some(serde_json::json!({"url":evidence[i].url,"published":evidence[i].published,"quote":c.quote}))
    }).collect::<Vec<_>>())
}

/// Citation containment verifies provenance; co-mention frequency is not identity evidence.
fn grounded_sources(citations: &[Citation], evidence: &[Evidence]) -> Vec<usize> {
    let mut sources = Vec::new();
    for citation in citations {
        let Some(i) = citation
            .article
            .checked_sub(1)
            .filter(|i| *i < evidence.len())
        else {
            continue;
        };
        let quote = citation
            .quote
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if quote.is_empty() {
            continue;
        }
        let text = evidence_text(&evidence[i])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.contains(&quote) && !sources.contains(&i) {
            sources.push(i);
        }
    }
    sources
}

#[allow(clippy::too_many_arguments)]
async fn apply_metadata(
    pool: &PgPool,
    cand: &Candidate,
    sport: &str,
    team: Option<&(i32, String)>,
    clear_team: bool,
    role: Option<&str>,
    confidence: f64,
    team_source: Option<&Evidence>,
    role_source: Option<&Evidence>,
    team_provenance: serde_json::Value,
    role_provenance: serde_json::Value,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (current_team, current_role): (Option<i32>, String) = sqlx::query_as(
        "SELECT team_id,kind FROM public.persons WHERE id=$1 AND sport=$2 FOR UPDATE",
    )
    .bind(cand.id)
    .bind(sport)
    .fetch_one(&mut *tx)
    .await?;
    let mut revision_source = None;
    let already_unaffiliated: bool = if clear_team && current_team.is_none() {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM public.entity_facts
            WHERE entity_type='person' AND entity_id=$1 AND sport=$2
              AND fact_type='team_affiliation' AND state='active' AND value_text='unaffiliated')",
        )
        .bind(cand.id)
        .bind(sport)
        .fetch_one(&mut *tx)
        .await?
    } else {
        false
    };
    if clear_team && !already_unaffiliated {
        revision_source = Some(record_revision(&mut tx, cand.id, sport, "team_affiliation", "unaffiliated",
            serde_json::json!({"team_id":null,"confidence":confidence,"evidence":team_provenance}),
            team_source.unwrap()).await?);
        sqlx::query("UPDATE public.persons SET team_id=NULL WHERE id=$1 AND sport=$2")
            .bind(cand.id)
            .bind(sport)
            .execute(&mut *tx)
            .await?;
    }
    if let Some((id, name)) = team.filter(|(id, _)| Some(*id) != current_team) {
        revision_source = Some(record_revision(
            &mut tx,
            cand.id,
            sport,
            "team_affiliation",
            name,
            serde_json::json!({"team_id":id,"confidence":confidence,"evidence":team_provenance}),
            team_source.unwrap(),
        )
        .await?);
        sqlx::query("UPDATE public.persons SET team_id=$2 WHERE id=$1 AND sport=$3")
            .bind(cand.id)
            .bind(id)
            .bind(sport)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(role) = role.filter(|r| *r != current_role) {
        revision_source = Some(
            record_revision(
                &mut tx,
                cand.id,
                sport,
                "role",
                role,
                serde_json::json!({"confidence":confidence,"evidence":role_provenance}),
                role_source.unwrap(),
            )
            .await?,
        );
        sqlx::query("UPDATE public.persons SET kind=$2 WHERE id=$1 AND sport=$3")
            .bind(cand.id)
            .bind(role)
            .bind(sport)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(source) = revision_source {
        let predicate = match role.unwrap_or(&current_role) {
            "coach" => Some("coach_of"),
            "owner" => Some("owner_of"),
            _ => None,
        };
        let next_team = if clear_team {
            None
        } else {
            team.map(|(id, _)| *id).or(current_team)
        };
        sqlx::query(
            "UPDATE public.entity_relationships SET state='superseded'
            WHERE subject_entity_type='person' AND subject_entity_id=$1 AND subject_sport=$2
              AND predicate IN ('coach_of','owner_of') AND state='active'
              AND (predicate IS DISTINCT FROM $3 OR object_entity_id IS DISTINCT FROM $4)",
        )
        .bind(cand.id)
        .bind(sport)
        .bind(predicate)
        .bind(next_team)
        .execute(&mut *tx)
        .await?;
        if let (Some(predicate), Some(team_id)) = (predicate, next_team) {
            sqlx::query("INSERT INTO public.entity_relationships
                (subject_entity_type,subject_entity_id,subject_sport,predicate,object_entity_type,object_entity_id,object_sport,source_document_id,state)
                SELECT 'person',$1,$2,$3,'team',$4,$2,$5,'active'
                WHERE NOT EXISTS (SELECT 1 FROM public.entity_relationships
                    WHERE subject_entity_type='person' AND subject_entity_id=$1 AND subject_sport=$2
                      AND predicate=$3 AND object_entity_type='team' AND object_entity_id=$4 AND state='active')")
                .bind(cand.id).bind(sport).bind(predicate).bind(team_id).bind(source).execute(&mut *tx).await?;
        }
    }
    sqlx::query(
        "UPDATE public.persons SET meta=meta || jsonb_build_object(
        'affiliation_checked_at',NOW(), 'affiliation_check_outcome','grounded metadata checked')
        WHERE id=$1 AND sport=$2",
    )
    .bind(cand.id)
    .bind(sport)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn record_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i32,
    sport: &str,
    field: &str,
    value: &str,
    details: serde_json::Value,
    evidence: &Evidence,
) -> Result<i64> {
    let source: i64 =
        sqlx::query_scalar("INSERT INTO public.source_documents (url) VALUES ($1) RETURNING id")
            .bind(&evidence.url)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query("UPDATE public.entity_facts SET state='superseded',valid_to=NOW()
        WHERE entity_type='person' AND entity_id=$1 AND sport=$2 AND fact_type=$3 AND state='active'")
        .bind(id).bind(sport).bind(field).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO public.entity_facts (entity_type,entity_id,sport,fact_type,value_text,value_jsonb,source_document_id,state,valid_from)
        VALUES ('person',$1,$2,$3,$4,$5,$6,'active',NOW())")
        .bind(id).bind(sport).bind(field).bind(value).bind(sqlx::types::Json(details)).bind(source)
        .execute(&mut **tx).await?;
    Ok(source)
}

async fn stamp_checked(
    pool: &PgPool,
    person_id: i32,
    sport: &str,
    why: &str,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    sqlx::query(
        r#"UPDATE public.persons SET meta = meta || jsonb_build_object(
               'affiliation_checked_at', to_char(NOW() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
               'affiliation_check_outcome', $2::text)
           WHERE id = $1 AND sport = $3"#,
    )
    .bind(person_id)
    .bind(why)
    .bind(sport)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_must_be_in_the_supplied_excerpt_and_independently_sourced() {
        let evidence = vec![Evidence {
            published: "2026-09-13".into(),
            source: "Wire".into(),
            title: "Vale's coach speaks".into(),
            description: format!("{}hidden claim", "x".repeat(280)),
            url: "https://example.test/report".into(),
            teams: vec![],
        }];
        let citations = vec![
            Citation {
                article: 1,
                quote: "Vale's coach speaks".into(),
            },
            Citation {
                article: 1,
                quote: "Vale's coach speaks".into(),
            },
            Citation {
                article: 2,
                quote: "Vale's coach speaks".into(),
            },
            Citation {
                article: 0,
                quote: "".into(),
            },
            Citation {
                article: 1,
                quote: "hidden claim".into(),
            },
        ];
        assert_eq!(grounded_sources(&citations, &evidence), vec![0]);
        let provenance = citation_provenance(&citations, &evidence);
        assert!(provenance
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["quote"] == "Vale's coach speaks"));
    }
}

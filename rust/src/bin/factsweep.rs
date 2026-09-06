//! factsweep — the dynamic-metadata adjudication sweep (mig 236's missing producer).
//!
//! Scott, 2026-09-06: *"All entity metadata should be dynamic. Now there should be healthy
//! gates, but coaches, agents, owners, players, they all are dynamic. If we have a system in
//! place that self heals when something happens, that's the unlock."*
//!
//! The gap this closes, measured on the Iraola/Alonso trail: `entity_fact_policy` declared
//! `person / team_affiliation / adjudicated` on day one (mig 236) and nothing ever produced
//! that fact. The transfer wire correctly rejects "already at the club" coverage as
//! not-a-move, the Investigator's wikidata dossiers carry playing careers rather than current
//! posts, so standing affiliations froze at their seed values — Guardiola "at Barcelona",
//! 462 coach rows of career archaeology, each one misrouting that person's articles through
//! the claim fence.
//!
//! The producer is the news the system already reads: for each news-active person whose
//! affiliation is absent or stale, gather the recent articles that tag them, and ask the
//! resident model ONE adjudication question over those excerpts. The healthy gates:
//!
//! - **Policy-gated**: no `entity_fact_policy` row for the fact type ⇒ frozen, the sweep
//!   never touches it (mig 236's absence-is-frozen rule).
//! - **Evidence floor**: fewer than two distinct articles adjudicates nothing.
//! - **Deterministic resolution**: the model must pick from the CANDIDATE TEAMS list (the
//!   clubs actually co-tagged on the evidence) — a name we cannot resolve to a team id is a
//!   null, never a fuzzy match.
//! - **Confidence floor**: below 0.7 writes nothing.
//! - **Fail-closed**: unknown stays absent — the identity card omits what we do not know
//!   ("a dated snapshot, not gospel"; absence over guess).
//! - **Provenance**: every write supersedes (never deletes) in `entity_facts`, cites a
//!   `source_documents` row for the strongest evidence article, and stamps
//!   `meta.affiliation_checked_at` so quiet outcomes debounce.
//!
//! Players are already dynamic through the transfer-identity rail and the data imports; this
//! sweep covers PERSONS of every kind (coach, agent, owner, executive, …). New fact types
//! later are a policy row + a selection query, not a new pipeline.
//!
//! Usage:
//!   factsweep -sport FOOTBALL [-limit 60] [-dry-run]
//!   factsweep -person 57 -sport FOOTBALL [-dry-run]      # one person, on demand

use anyhow::{anyhow, Context, Result};
use scoracle_cognition::config::Config;
use scoracle_cognition::db;
use scoracle_cognition::ollama::GenerateOptions;
use scoracle_cognition::route::{Role, Router};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

const EVIDENCE_DAYS: i32 = 21;
const ACTIVE_DAYS: i32 = 14;
const RECHECK_DAYS: i32 = 7;
const MAX_EVIDENCE_ARTICLES: i64 = 12;
const MIN_EVIDENCE_ARTICLES: usize = 2;
const MIN_CONFIDENCE: f64 = 0.7;
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
    tags: i64,
}

struct Evidence {
    source: String,
    title: String,
    description: String,
    url: String,
    teams: Vec<(i32, String)>,
}

#[derive(serde::Deserialize)]
struct Verdict {
    current_team_index: Option<i64>,
    role: Option<String>,
    confidence: Option<f64>,
}

const ADJUDICATION_SYSTEM: &str = r#"Task: adjudicate the CURRENT club affiliation of one sports figure, strictly from the reporting excerpts supplied.

The team must be one the excerpts state the figure CURRENTLY works for — managing it, playing for it, owning it, or representing it in the named role. A past club, an opponent, a rumored or linked destination, or a club they are merely discussed alongside is NOT a current affiliation. When the excerpts do not clearly establish a current club, current_team is null — an honest unknown beats a guess.

Choose the club by its NUMBER from the CANDIDATE TEAMS list, or null when no listed club is clearly current. role is what the excerpts show the figure to be: coach, player, agent, owner, executive, official, other, or unknown. confidence (0.0-1.0) is how explicitly the reporting states the affiliation — reserve 0.9+ for excerpts that state it outright.

Reply with ONLY this JSON object:
{"current_team_index": <number from the list, or null>, "role": "<role>", "confidence": <0.0-1.0>}"#;

fn adjudication_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "current_team_index": {"type": ["integer", "null"]},
            "role": {"type": "string"},
            "confidence": {"type": "number"}
        },
        "required": ["current_team_index", "role", "confidence"]
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
    if !policy_ok {
        println!("factsweep: person/team_affiliation is not in entity_fact_policy — frozen, exiting");
        return Ok(());
    }
    let role_policy_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.entity_fact_policy
          WHERE entity_type = 'person' AND fact_type = 'role')",
    )
    .fetch_one(&pool)
    .await?;

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
        let ev = load_evidence(&pool, cand.id, &args.sport).await?;
        if ev.len() < MIN_EVIDENCE_ARTICLES {
            thin += 1;
            stamp_checked(&pool, cand.id, &args.sport, "thin evidence", args.dry_run).await?;
            continue;
        }
        // Deterministic candidates: only clubs actually co-tagged on the evidence,
        // presented NUMBERED — the model picks an index, never spells a name, so an
        // embellished "Liverpool FC" can never miss our "Liverpool" (measured on the
        // first dry-run: three 0.80 verdicts lost to exact-name matching).
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
        // The corroboration gate's deterministic half: how many evidence articles co-tag
        // each club. The MODAL club (strict max; ties disqualify) is the frequency prior.
        let mut counts: HashMap<i32, usize> = HashMap::new();
        for e in &ev {
            for (id, _) in &e.teams {
                *counts.entry(*id).or_insert(0) += 1;
            }
        }
        let modal: Option<i32> = {
            let max = counts.values().copied().max().unwrap_or(0);
            let tops: Vec<i32> = counts
                .iter()
                .filter(|(_, c)| **c == max)
                .map(|(id, _)| *id)
                .collect();
            (tops.len() == 1).then(|| tops[0])
        };
        if teams.is_empty() {
            thin += 1;
            stamp_checked(&pool, cand.id, &args.sport, "no co-tagged teams", args.dry_run).await?;
            continue;
        }

        let prompt = build_prompt(cand, &ev, &teams);
        let opts = GenerateOptions {
            system: Some(ADJUDICATION_SYSTEM.to_string()),
            temperature: Some(0.0),
            num_predict: 200,
            num_ctx: cfg.voice_num_ctx,
            json_mode: true,
            format_schema: Some(adjudication_schema()),
            format_schema_raw: None,
        };
        let client = router.for_role(Role::Investigator);
        let (result, _body) = client.generate(&prompt, &opts).await?;
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
        // THE CORROBORATION GATE (measured on the second dry-run, 2026-09-06): granite chose
        // Ipswich Town for Iraola at 0.80 off "Ipswich 0-2 Liverpool" headlines, and gave two
        // different 0.80 answers for Demichelis across runs — a 3b confidence number is not a
        // gate. The model's pick must AGREE with the deterministic frequency prior (the modal
        // co-tagged club across the evidence); either signal alone can be fooled, agreement
        // rarely is. Displacing an existing NON-NULL affiliation additionally demands 0.85.
        let corroborated = matches!((&team, modal), (Some((id, _)), Some(m)) if *id == m);
        let displacing = matches!(&team, Some((id, _)) if cand.team_id.is_some() && cand.team_id != Some(*id));
        let confident = confidence >= MIN_CONFIDENCE && (!displacing || confidence >= 0.85);
        match (&team, corroborated && confident) {
            (Some((team_id, team_name)), true) => {
                if cand.team_id == Some(*team_id) {
                    println!(
                        "  {} ({}): confirmed at {team_name} (conf {confidence:.2})",
                        cand.full_name, cand.id
                    );
                    stamp_checked(&pool, cand.id, &args.sport, "confirmed", args.dry_run).await?;
                    continue;
                }
                println!(
                    "  {} ({}): {} -> {team_name} (conf {confidence:.2}, {} articles){}",
                    cand.full_name,
                    cand.id,
                    cand.team_id
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "(none)".into()),
                    ev.len(),
                    if args.dry_run { " [dry-run]" } else { "" }
                );
                if !args.dry_run {
                    apply_affiliation(
                        &pool,
                        cand,
                        &args.sport,
                        *team_id,
                        team_name,
                        confidence,
                        verdict.role.as_deref(),
                        role_policy_ok,
                        &ev,
                    )
                    .await?;
                }
                written += 1;
            }
            _ => {
                unknown += 1;
                println!(
                    "  {} ({}): unknown (index {:?}, conf {confidence:.2}, corroborated {corroborated}) — stays absent",
                    cand.full_name, cand.id, verdict.current_team_index
                );
                stamp_checked(&pool, cand.id, &args.sport, "adjudicated unknown", args.dry_run)
                    .await?;
            }
        }
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
            tags: r.get("tags"),
        })
        .collect())
}

async fn load_evidence(pool: &PgPool, person_id: i32, sport: &str) -> Result<Vec<Evidence>> {
    let rows = sqlx::query(
        r#"
        SELECT na.source, na.title, COALESCE(na.description, '') AS description, na.url,
               COALESCE((SELECT json_agg(json_build_object('id', t.id, 'name', t.name))
                           FROM public.news_article_entities nt
                           JOIN public.teams t ON t.id = nt.entity_id AND t.sport = nt.sport
                          WHERE nt.article_id = na.id AND nt.entity_type = 'team'),
                        '[]'::json) AS teams
          FROM public.news_article_entities nae
          JOIN public.news_articles na ON na.id = nae.article_id
         WHERE nae.entity_type = 'person' AND nae.entity_id = $1 AND nae.sport = $2
           AND nae.created_at > NOW() - make_interval(days => $3::int)
           AND na.duplicate_of IS NULL
         ORDER BY na.id DESC
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
    for (i, (_, n)) in teams.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, n));
    }
    b.push_str("\nReporting excerpts (newest first):\n");
    for e in ev {
        b.push_str("- ");
        if !e.source.is_empty() {
            b.push_str(&format!("[{}] ", e.source));
        }
        b.push_str(&e.title);
        if !e.description.is_empty() {
            b.push_str(" — ");
            let d = &e.description;
            let cut = d
                .char_indices()
                .nth(280)
                .map(|(i, _)| i)
                .unwrap_or(d.len());
            b.push_str(&d[..cut]);
        }
        b.push('\n');
    }
    b.push_str("\nReturn the JSON adjudication now.");
    b
}

#[allow(clippy::too_many_arguments)]
async fn apply_affiliation(
    pool: &PgPool,
    cand: &Candidate,
    sport: &str,
    team_id: i32,
    team_name: &str,
    confidence: f64,
    role: Option<&str>,
    role_policy_ok: bool,
    ev: &[Evidence],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    // Provenance: one source document citing the strongest (newest) evidence article.
    let url = ev.first().map(|e| e.url.as_str()).unwrap_or("internal:factsweep");
    let source_doc_id: i64 =
        sqlx::query_scalar("INSERT INTO public.source_documents (url) VALUES ($1) RETURNING id")
            .bind(url)
            .fetch_one(&mut *tx)
            .await?;
    // Supersede-never-delete in the facts ledger.
    sqlx::query(
        r#"UPDATE public.entity_facts SET state = 'superseded'
            WHERE entity_type = 'person' AND entity_id = $1 AND sport = $2
              AND fact_type = 'team_affiliation' AND state = 'active'"#,
    )
    .bind(cand.id)
    .bind(sport)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO public.entity_facts
               (entity_type, entity_id, sport, fact_type, value_text, value_jsonb,
                source_document_id, state)
           VALUES ('person', $1, $2, 'team_affiliation', $3,
                   jsonb_build_object('team_id', $4::int, 'confidence', $5::numeric),
                   $6, 'active')"#,
    )
    .bind(cand.id)
    .bind(sport)
    .bind(team_name)
    .bind(team_id)
    .bind(confidence)
    .bind(source_doc_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"UPDATE public.persons SET team_id = $2,
               meta = meta || jsonb_build_object(
                   'affiliation_checked_at', to_char(NOW() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
                   'team_affiliation_note', 'factsweep ' || CURRENT_DATE || ': ' || $3::text)
             WHERE id = $1"#,
    )
    .bind(cand.id)
    .bind(team_id)
    .bind(team_name)
    .execute(&mut *tx)
    .await?;
    // The adjudicated role may refine kind — within the strong-kind set only, policy-gated.
    if role_policy_ok {
        if let Some(r) = role.filter(|r| KIND_SET.contains(r) && *r != cand.kind) {
            sqlx::query("UPDATE public.persons SET kind = $2 WHERE id = $1")
                .bind(cand.id)
                .bind(r)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn stamp_checked(
    pool: &PgPool,
    person_id: i32,
    _sport: &str,
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
           WHERE id = $1"#,
    )
    .bind(person_id)
    .bind(why)
    .execute(pool)
    .await?;
    Ok(())
}

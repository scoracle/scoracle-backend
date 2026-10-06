//! Subject identity supplied by a plugin from canonical data.
//!
//! This describes who the task concerns. It is separate from source evidence and
//! does not imply affiliations, aliases, or relationships absent from the frame.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EntityMeta {
    pub name: String,
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
}

impl EntityMeta {
    /// Compact identity for a predicate; the canonical ID stays in the receipt.
    pub fn reference(&self) -> String {
        format!("{}, the {} {}", self.name, self.sport, self.entity_type)
    }
}

/// Identity used for prose; database identity remains in plugin provenance.
#[derive(Serialize)]
pub struct WritingIdentity<'a> {
    name: &'a str,
    entity_type: &'a str,
    sport: &'a str,
}
/// Compact identity for candidate lists. Framing belongs once above the list.
pub async fn load_identity_record(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> anyhow::Result<Option<String>> {
    use sqlx::Row;
    let mut connection = pool.acquire().await?;
    let sport_uc = sport.to_uppercase();
    let card = match entity_type {
        "team" => {
            let row = sqlx::query(
                r#"
                SELECT t.name, t.city, t.country, t.venue_name, t.conference, t.division,
                       l.name AS league_name, l.country AS league_country,
                       (SELECT pe.full_name FROM public.persons pe
                         WHERE pe.sport = t.sport AND pe.team_id = t.id AND pe.kind = 'coach'
                         ORDER BY pe.created_at DESC LIMIT 1) AS coach
                  FROM public.teams t
                  LEFT JOIN public.leagues l ON l.id = t.league_id AND l.sport = t.sport
                 WHERE t.id = $1 AND t.sport = $2
                "#,
            )
            .bind(entity_id)
            .bind(&sport_uc)
            .fetch_optional(&mut *connection)
            .await?;
            row.map(|r| {
                let name: String = r.get("name");
                let mut line = name;
                if let Some(league) = r.get::<Option<String>, _>("league_name") {
                    line.push_str(&format!(" — {league}"));
                    if let Some(c) = r.get::<Option<String>, _>("league_country") {
                        line.push_str(&format!(" ({c})"));
                    }
                } else if let Some(conf) = r.get::<Option<String>, _>("conference") {
                    line.push_str(&format!(" — {conf}"));
                    if let Some(div) = r.get::<Option<String>, _>("division") {
                        line.push_str(&format!(" {div}"));
                    }
                }
                if let (Some(venue), Some(city)) = (
                    r.get::<Option<String>, _>("venue_name"),
                    r.get::<Option<String>, _>("city"),
                ) {
                    line.push_str(&format!(". Home: {venue}, {city}"));
                }
                if let Some(coach) = r.get::<Option<String>, _>("coach") {
                    line.push_str(&format!(". Coach on record: {coach}"));
                }
                line.push('.');
                line
            })
        }
        "player" => {
            let row = sqlx::query(
                r#"
                SELECT p.name, p.nationality, pci.source, pci.source_updated_at::date::text AS observed_at,
                       t.name AS team_name,
                       l.name AS league_name,
                       pci.position
                  FROM public.players p
                  LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
                  LEFT JOIN public.teams t ON t.id = pci.team_id AND t.sport = p.sport
                  LEFT JOIN public.leagues l ON l.id = COALESCE(pci.league_id, t.league_id) AND l.sport = p.sport
                 WHERE p.id = $1 AND p.sport = $2
                "#,
            )
            .bind(entity_id)
            .bind(&sport_uc)
            .fetch_optional(&mut *connection)
            .await?;
            row.map(|r| {
                let name: String = r.get("name");
                let mut line = name;
                if let Some(pos) = r.get::<Option<String>, _>("position") {
                    line.push_str(&format!(" — {pos}"));
                }
                if let Some(team) = r.get::<Option<String>, _>("team_name") {
                    line.push_str(&format!(", on record at {team}"));
                    if let Some(league) = r.get::<Option<String>, _>("league_name") {
                        line.push_str(&format!(" ({league})"));
                    }
                }
                if let Some(nat) = r.get::<Option<String>, _>("nationality") {
                    line.push_str(&format!(". Nationality: {nat}"));
                }
                if let Some(source) = r.get::<Option<String>, _>("source") {
                    line.push_str(&format!(". Record: {source}"));
                }
                if let Some(date) = r.get::<Option<String>, _>("observed_at") {
                    line.push_str(&format!("; observed {date}"));
                }
                line.push('.');
                line
            })
        }
        "person" => {
            let row: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT p.full_name, p.kind, t.name, left(p.meta->>'affiliation_checked_at',10)
                   FROM public.persons p
                   LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
                  WHERE p.id = $1 AND p.sport = $2",
            )
            .bind(entity_id)
            .bind(&sport_uc)
            .fetch_optional(&mut *connection)
            .await?;
            row.map(|(name, role, team, checked)| {
                format!(
                    "{name} — {role}; club: {}; checked: {}.",
                    team.as_deref().unwrap_or("unknown"),
                    checked.as_deref().unwrap_or("unknown"),
                )
            })
        }
        _ => None,
    };
    let Some(mut card) = card else {
        return Ok(None);
    };
    // Active facts only. Dates and source IDs stay so an old observation is not current reporting.
    let facts: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (fact_type, COALESCE(value_text, value_jsonb::text, 'unknown'))
                fact_type, COALESCE(value_text, value_jsonb::text, 'unknown'),
                COALESCE(valid_from, created_at)::date::text, source_document_id
           FROM public.entity_facts
          WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND state = 'active'
            AND (valid_from IS NULL OR valid_from <= NOW())
            AND (valid_to IS NULL OR valid_to > NOW())
            AND fact_type IN ('role', 'team_affiliation', 'playing_status', 'date_of_birth')
          ORDER BY fact_type, COALESCE(value_text, value_jsonb::text, 'unknown'), created_at DESC, id DESC",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(&sport_uc)
    .fetch_all(&mut *connection)
    .await?;
    for (kind, value, date, source) in facts {
        card.push_str(&format!("\n{kind}: {value} [{date}; source {source}]"));
    }
    Ok(Some(card))
}

impl EntityMeta {
    pub fn for_writing(&self) -> WritingIdentity<'_> {
        // Storage keeps the competition namespace used by adapters. Articulation
        // receives the underlying sport so identity does not imply a recap genre.
        let sport = match self.sport.as_str() {
            "NBA" => "basketball",
            "NFL" => "American football",
            sport => sport,
        };
        WritingIdentity {
            name: &self.name,
            entity_type: &self.entity_type,
            sport,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_identity_describes_the_sport_not_the_competition_namespace() {
        let identity = EntityMeta {
            name: "Cedar Comets".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        };
        assert_eq!(
            serde_json::to_value(identity.for_writing()).unwrap(),
            serde_json::json!({"name":"Cedar Comets","entity_type":"team","sport":"basketball"})
        );
    }
}

use anyhow::{bail, Context, Result};
use sqlx::PgPool;

/// lookup_entity_name resolves the display name for the prompt. An empty/missing name is an
/// error, so the work item fails and retries rather than generating against a blank prompt.
pub async fn lookup_entity_name(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<String> {
    let query = if entity_type == "player" {
        "SELECT name FROM players WHERE id = $1 AND sport = $2"
    } else {
        "SELECT name FROM teams WHERE id = $1 AND sport = $2"
    };
    let name: String = sqlx::query_scalar(query)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("lookup {entity_type}/{entity_id} ({sport})"))?;
    if name.is_empty() {
        bail!("empty name for {entity_type}/{entity_id} ({sport})");
    }
    Ok(name)
}

//! Shared identity lookup and semantic versioning for writers and extraction.

/// Framing that tells the model to reconcile dated house records with current reporting.
pub const IDENTITY_CARD_FRAMING: &str = "Identity context, not event evidence. Distinguish current roles from career history; dated reporting may supersede these records. Unknown means unknown.";

/// load_identity_card renders the entity's house-record identity line for the prompts: who
/// this is, where they play, and (teams) the coach on record.
/// Shared by article extraction, transfer verification and card writers. Database errors
/// propagate; a failed metadata read must not silently produce a context-free answer.
///
/// `None` when the entity is unknown — an absent card is honest; an empty one is noise.
pub async fn load_identity_card(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> anyhow::Result<Option<String>> {
    Ok(load_identity_record(pool, entity_type, entity_id, sport)
        .await?
        .map(|record| format!("{IDENTITY_CARD_FRAMING}\n{record}")))
}

/// A compact descriptor for numbered candidate lists; framing belongs once above the list.
pub async fn load_identity_record(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> anyhow::Result<Option<String>> {
    let mut connection = pool.acquire().await?;
    load_identity_record_on(&mut connection, entity_type, entity_id, sport).await
}

pub(super) async fn load_identity_record_on(
    connection: &mut sqlx::PgConnection,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> anyhow::Result<Option<String>> {
    use sqlx::Row;
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
    // Only active, currently applicable facts; retain their dates and source IDs so a
    // model can distinguish an old observation from current reporting.
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

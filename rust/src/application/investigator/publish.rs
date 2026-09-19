//! The Investigator world transaction: facts, identities, provenance, cooldown, and exact completion.
use super::*;
type FactPolicy = HashMap<(String, String), String>;

#[allow(clippy::too_many_arguments)]
async fn publish_accept(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    cand: &CandidateRow,
    sport: &str,
    it: &WikidataItem,
    kind: &str,
    role: RoleClass,
    career_team_ids: &[i32],
    run_plan: &serde_json::Value,
) -> Result<()> {
    // Career teams disambiguate identity; their order does not establish a current job.
    let current_teams: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT entity_id FROM public.entity_external_ids
          WHERE entity_type='team' AND sport=$1 AND namespace='wikidata' AND external_id=ANY($2)",
    )
    .bind(sport)
    .bind(&it.coach_of_teams)
    .fetch_all(&mut **tx)
    .await?;
    let team_id = (kind == "coach" && current_teams.len() == 1).then(|| current_teams[0]);
    let policy = load_fact_policy(&mut **tx).await?;

    // Resolve-to-existing FIRST (5.5): an exact-surface person/player whose sport matches.
    // Discriminator for the merge: same team when both sides know one; a player-kind match
    // with no team agreement refuses the merge (new person is NOT created either — that
    // would duplicate; the verdict downgrades to ambiguous).
    let existing = sqlx::query(
        r#"
        SELECT DISTINCT s.entity_type, s.entity_id
        FROM public.entity_name_surfaces s
        WHERE s.sport = $1 AND s.entity_type IN ('player', 'person')
          AND s.norm = public.nrm($2)
        "#,
    )
    .bind(sport)
    .bind(&it.label)
    .fetch_all(&mut **tx)
    .await
    .context("existing entity lookup")?;

    let (resolved_type, resolved_id): (String, i32) = match existing.len() {
        0 => {
            let person_id: i32 = sqlx::query_scalar(
                r#"
                INSERT INTO public.persons (sport, full_name, kind, team_id, search_aliases, meta)
                VALUES ($1, $2, $3, $4, $5,
                        jsonb_strip_nulls(jsonb_build_object('wikidata', NULLIF($6, '')::text)))
                RETURNING id
                "#,
            )
            .bind(sport)
            .bind(&it.label)
            .bind(kind)
            .bind(team_id)
            .bind(&it.aliases)
            .bind(&it.qid)
            .fetch_one(&mut **tx)
            .await
            .context("insert persons row")?;
            ("person".to_string(), person_id)
        }
        1 => {
            let r = &existing[0];
            let etype: String = r.get("entity_type");
            let eid: i32 = r.get("entity_id");
            // Merging onto an existing entity needs the discriminator to AGREE, not merely
            // exist: for players, the caller's resolved career teams must include the
            // player's team. A team-less player on our side refuses (never a guess).
            if etype == "player" {
                let player_team: Option<i32> =
                    sqlx::query_scalar("SELECT team_id FROM public.player_current_identity WHERE player_id = $1 AND sport = $2")
                        .bind(eid)
                        .bind(sport)
                        .fetch_optional(&mut **tx)
                        .await
                        .context("load player team for merge check")?
                        .flatten();
                let agree = player_team.is_some_and(|pt| career_team_ids.contains(&pt));
                if !agree {
                    publish_refusal(
                        tx,
                        cand,
                        "ambiguous",
                        None,
                        run_plan,
                        "surface matches existing player but team discriminator disagrees",
                    )
                    .await?;
                    return Ok(());
                }
            }
            (etype, eid)
        }
        _ => {
            publish_refusal(
                tx,
                cand,
                "ambiguous",
                None,
                run_plan,
                "multiple existing entities share the surface",
            )
            .await?;
            return Ok(());
        }
    };

    // Acquisition seeds identity. The current-reporting sweep owns subsequent role and
    // affiliation changes, so rereading a career biography cannot undo a sourced correction.
    let seed_role = resolved_type == "person"
        && (existing.is_empty()
            || sqlx::query_scalar::<_, bool>(
                "SELECT kind='other' FROM public.persons WHERE id=$1 AND sport=$2 FOR UPDATE",
            )
            .bind(resolved_id)
            .bind(sport)
            .fetch_one(&mut **tx)
            .await?);
    if resolved_type == "person" {
        if seed_role && policy_allows(&policy, "person", "role") {
            sqlx::query(
                "UPDATE public.persons SET kind = $2 WHERE id = $1 AND kind IS DISTINCT FROM $2",
            )
            .bind(resolved_id)
            .bind(kind)
            .execute(&mut **tx)
            .await
            .context("refresh person kind")?;
        }
        if policy_allows(&policy, "person", "team_affiliation") {
            if let Some(team) = team_id {
                sqlx::query(
                    "UPDATE public.persons p SET team_id = $2 WHERE id = $1 AND team_id IS NULL
                     AND NOT EXISTS (SELECT 1 FROM public.entity_facts f
                         WHERE f.entity_type='person' AND f.entity_id=p.id AND f.sport=p.sport
                           AND f.fact_type='team_affiliation' AND f.state='active')",
                )
                .bind(resolved_id)
                .bind(team)
                .execute(&mut **tx)
                .await
                .context("refresh person team affiliation")?;
            }
        }
        // The dossier facts players always had and persons never got.
        let dob = it.date_of_birth.as_deref().and_then(wire_date);
        let photo = it
            .image_file
            .as_deref()
            .and_then(commons_image_url)
            .filter(|_| policy_allows(&policy, "person", "photo_url"));
        for (ft, val) in [
            ("date_of_birth", dob.as_deref()),
            ("photo_url", photo.as_deref()),
        ] {
            let Some(val) = val else { continue };
            if !policy_allows(&policy, "person", ft) {
                continue;
            }
            write_fact_superseding(
                tx,
                "person",
                resolved_id,
                sport,
                ft,
                val,
                it.source_document_id,
            )
            .await?;
        }
        if let Some(p) = photo.as_deref() {
            sqlx::query(
                "UPDATE public.persons SET meta = jsonb_set(COALESCE(meta, '{}'::jsonb), '{photo_url}', to_jsonb($2::text)) WHERE id = $1",
            )
            .bind(resolved_id)
            .bind(p)
            .execute(&mut **tx)
            .await
            .context("refresh person photo meta")?;
        }
    }

    // Aliases (append-only ledger) + direct surface mirror, per name form. The ledger
    // stays append-only, but a re-accept must not re-append forms it already holds —
    // the reopen clock would otherwise duplicate the whole set monthly.
    let mut forms: Vec<String> = vec![it.label.clone()];
    forms.extend(it.aliases.iter().cloned());
    for form in &forms {
        sqlx::query(
            r#"
            INSERT INTO public.entity_aliases
                (entity_type, entity_id, sport, alias, norm_alias, source_document_id, state)
            SELECT $1, $2, $3, $4, public.nrm($4), $5, 'active'
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_aliases
                WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
                  AND norm_alias = public.nrm($4) AND state = 'active'
            )
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(form)
        .bind(it.source_document_id)
        .execute(&mut **tx)
        .await
        .context("insert entity_alias")?;
        sqlx::query(
            r#"
            INSERT INTO public.entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind)
            SELECT $1, $2, $3, public.nrm($4), 'alias'
            WHERE public.nrm($4) <> ''
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(form)
        .execute(&mut **tx)
        .await
        .context("mirror surface")?;
    }

    // External ids + role fact + the coach_of relationship when it applies. The namespaces
    // are per-arm: `wikidata` only with a real QID (the prose arm has none), `enwiki`
    // whenever a page title anchors the identity.
    for (ns, ext) in [
        ("wikidata", (!it.qid.is_empty()).then(|| it.qid.clone())),
        ("enwiki", it.enwiki_title.clone()),
    ] {
        let Some(ext) = ext else { continue };
        sqlx::query(
            r#"
            INSERT INTO public.entity_external_ids
                (entity_type, entity_id, sport, namespace, external_id, source_document_id)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_external_ids
                WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
                  AND namespace = $4 AND external_id = $5
            )
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(ns)
        .bind(&ext)
        .bind(it.source_document_id)
        .execute(&mut **tx)
        .await
        .context("insert person external id")?;
    }

    // Seed person roles with provenance. Subsequent career changes belong to factsweep;
    // being a players-table row already establishes the player role.
    if seed_role && policy_allows(&policy, &resolved_type, "role") {
        write_fact_superseding(
            tx,
            &resolved_type,
            resolved_id,
            sport,
            "role",
            kind,
            it.source_document_id,
        )
        .await?;
    }

    // Structural role → relationship edge, when a team discriminated. A changed team
    // supersedes the old edge — the story moved, the graph moves with it — and an
    // unchanged one is not re-inserted.
    let predicate = match role {
        RoleClass::Coach => Some("coach_of"),
        RoleClass::Owner => Some("owner_of"),
        _ => None,
    };
    if let (Some(predicate), Some(team), true) = (predicate, team_id, existing.is_empty()) {
        sqlx::query(
            r#"
            UPDATE public.entity_relationships SET state = 'superseded'
            WHERE subject_entity_type = $1 AND subject_entity_id = $2 AND subject_sport = $3
              AND predicate = $4 AND state = 'active'
              AND (object_entity_type <> 'team' OR object_entity_id <> $5)
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(predicate)
        .bind(team)
        .execute(&mut **tx)
        .await
        .context("supersede stale role relationship")?;
        sqlx::query(
            r#"
            INSERT INTO public.entity_relationships
                (subject_entity_type, subject_entity_id, subject_sport,
                 predicate, object_entity_type, object_entity_id, object_sport,
                 source_document_id, state)
            SELECT $1, $2, $3, $4, 'team', $5, $3, $6, 'active'
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_relationships
                WHERE subject_entity_type = $1 AND subject_entity_id = $2 AND subject_sport = $3
                  AND predicate = $4 AND object_entity_type = 'team' AND object_entity_id = $5
                  AND state = 'active'
            )
            "#,
        )
        .bind(&resolved_type)
        .bind(resolved_id)
        .bind(sport)
        .bind(predicate)
        .bind(team)
        .bind(it.source_document_id)
        .execute(&mut **tx)
        .await
        .context("insert role relationship")?;
    }

    record_run(tx, cand.id, "accepted", run_plan, None).await?;
    sqlx::query(
        r#"
        UPDATE public.entity_candidates
        SET state = 'accepted', resolved_entity_type = $2, resolved_entity_id = $3,
            decided_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(cand.id)
    .bind(&resolved_type)
    .bind(resolved_id)
    .execute(&mut **tx)
    .await
    .context("mark candidate accepted")?;

    info!(
        candidate_id = cand.id,
        entity_type = %resolved_type,
        entity_id = resolved_id,
        qid = %it.qid,
        kind,
        "investigator accepted candidate"
    );
    Ok(())
}

async fn publish_refusal(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    cand: &CandidateRow,
    state: &str,
    resolved: Option<(String, i32)>,
    run_plan: &serde_json::Value,
    reason: &str,
) -> Result<()> {
    record_run(tx, cand.id, state, run_plan, Some(reason)).await?;
    sqlx::query(
        r#"
        UPDATE public.entity_candidates
        SET state = $2,
            resolved_entity_type = $3, resolved_entity_id = $4,
            decided_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(cand.id)
    .bind(state)
    .bind(resolved.as_ref().map(|(t, _)| t.clone()))
    .bind(resolved.as_ref().map(|(_, i)| *i))
    .execute(&mut **tx)
    .await
    .context("mark candidate decided")?;
    info!(
        candidate_id = cand.id,
        state, reason, "investigator decided candidate"
    );
    Ok(())
}

async fn record_run(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    candidate_id: i64,
    outcome: &str,
    query_plan: &serde_json::Value,
    rejection_reason: Option<&str>,
) -> Result<()> {
    // The prose arm's plan self-describes (`arm: "prose"` carries `model` + `contract`),
    // so the ledger columns come from the plan rather than threading two more parameters
    // through every verdict path. The Wikidata arm's plan has neither key → NULL model,
    // wikidata parser version — exactly the pre-5.4 row shape.
    let model_version = query_plan
        .get("model")
        .and_then(serde_json::Value::as_str)
        .filter(|m| !m.is_empty());
    let parser_version = query_plan
        .get("contract")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(INVESTIGATE_PARSER_VERSION);
    sqlx::query(
        r#"
        INSERT INTO public.acquisition_runs
            (candidate_id, status, query_plan, outcome, rejection_reason,
             model_version, parser_version, started_at, finished_at)
        VALUES ($1, 'completed', $2::jsonb, $3, $4, $5, $6, NOW(), NOW())
        "#,
    )
    .bind(candidate_id)
    .bind(query_plan)
    .bind(outcome)
    .bind(rejection_reason)
    .bind(model_version)
    .bind(parser_version)
    .execute(&mut **tx)
    .await
    .context("record acquisition run")?;
    Ok(())
}

async fn publish_player(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    player_id: i32,
    sport: &str,
    it: &WikidataItem,
) -> Result<()> {
    let dob = it.date_of_birth.as_deref().and_then(wire_date);
    let weight = it.weight_kg.map(|kg| display_weight(sport, kg));
    let height = it.height_cm.map(|cm| display_height(sport, cm));
    // NBA prefers its league headshot; all other cases fall back to Commons.
    let photo = it
        .nba_id
        .as_deref()
        .and_then(nba_headshot_url)
        .filter(|_| sport == "NBA")
        .or_else(|| it.image_file.as_deref().and_then(commons_image_url));

    let policy = load_fact_policy(&mut **tx).await?;

    // Facts with provenance; a correction is a revision — prior active facts of the same
    // type are superseded, never overwritten. Every write asks the
    // policy table first: an unlisted (entity_type, fact_type) is frozen to model paths.
    let weight_fact = it.weight_kg.map(|k| format!("{k}"));
    let height_fact = it.height_cm.map(|c| format!("{c}"));
    for (ft, val) in [
        ("date_of_birth", dob.as_deref()),
        ("weight_kg", weight_fact.as_deref()),
        ("height_cm", height_fact.as_deref()),
        ("photo_url", photo.as_deref()),
    ] {
        let Some(val) = val else { continue };
        if !policy_allows(&policy, "player", ft) {
            continue;
        }
        write_fact_superseding(
            tx,
            "player",
            player_id,
            sport,
            ft,
            val,
            it.source_document_id,
        )
        .await?;
    }

    // External ids (wikidata + the sport's own id when present).
    for (ns, ext) in [
        ("wikidata", Some(it.qid.clone())),
        ("nba", it.nba_id.clone()),
    ] {
        let Some(ext) = ext else { continue };
        sqlx::query(
            r#"
            INSERT INTO public.entity_external_ids
                (entity_type, entity_id, sport, namespace, external_id, source_document_id)
            SELECT 'player', $1, $2, $3, $4, $5
            WHERE NOT EXISTS (
                SELECT 1 FROM public.entity_external_ids
                WHERE entity_type = 'player' AND entity_id = $1 AND sport = $2
                  AND namespace = $3 AND external_id = $4
            )
            "#,
        )
        .bind(player_id)
        .bind(sport)
        .bind(ns)
        .bind(&ext)
        .bind(it.source_document_id)
        .execute(&mut **tx)
        .await
        .context("insert player external id")?;
    }

    // Convenience copies update existing player rows only.
    sqlx::query(
        r#"
        UPDATE public.players
        SET date_of_birth = COALESCE($2::date, date_of_birth),
            weight = COALESCE($3, weight),
            height = COALESCE($4, height),
            photo_url = COALESCE($5, photo_url),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(player_id)
    .bind(
        dob.as_deref()
            .filter(|_| policy_allows(&policy, "player", "date_of_birth")),
    )
    .bind(
        weight
            .as_deref()
            .filter(|_| policy_allows(&policy, "player", "weight_kg")),
    )
    .bind(
        height
            .as_deref()
            .filter(|_| policy_allows(&policy, "player", "height_cm")),
    )
    .bind(
        photo
            .as_deref()
            .filter(|_| policy_allows(&policy, "player", "photo_url")),
    )
    .execute(&mut **tx)
    .await
    .context("update player convenience columns")?;

    Ok(())
}

pub(super) async fn load_fact_policy<'e>(
    pool: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
) -> Result<FactPolicy> {
    let rows = sqlx::query("SELECT entity_type, fact_type, tier FROM public.entity_fact_policy")
        .fetch_all(pool)
        .await
        .context("load entity_fact_policy")?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                (
                    r.get::<String, _>("entity_type"),
                    r.get::<String, _>("fact_type"),
                ),
                r.get::<String, _>("tier"),
            )
        })
        .collect())
}

pub(super) fn policy_allows(policy: &FactPolicy, entity_type: &str, fact_type: &str) -> bool {
    policy.contains_key(&(entity_type.to_string(), fact_type.to_string()))
}

async fn write_fact_superseding(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    fact_type: &str,
    value: &str,
    source_document_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE public.entity_facts SET state = 'superseded'
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND fact_type = $4 AND state = 'active' AND value_text IS DISTINCT FROM $5
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(fact_type)
    .bind(value)
    .execute(&mut **tx)
    .await
    .context("supersede prior fact")?;
    sqlx::query(
        r#"
        INSERT INTO public.entity_facts
            (entity_type, entity_id, sport, fact_type, value_text, source_document_id, state)
        SELECT $1, $2, $3, $4, $5, $6, 'active'
        WHERE NOT EXISTS (
            SELECT 1 FROM public.entity_facts
            WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
              AND fact_type = $4 AND state = 'active' AND value_text = $5
        )
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(fact_type)
    .bind(value)
    .bind(source_document_id)
    .execute(&mut **tx)
    .await
    .context("insert fact")?;
    Ok(())
}

async fn publish_team(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    team_id: i32,
    sport: &str,
    venue: &Option<(String, i64)>,
    logo: &Option<(String, i64)>,
) -> Result<()> {
    if let Some((v, doc)) = venue.as_ref() {
        write_fact_superseding(tx, "team", team_id, sport, "venue_name", v, *doc).await?;
    }
    if let Some((l, doc)) = logo.as_ref() {
        write_fact_superseding(tx, "team", team_id, sport, "logo_url", l, *doc).await?;
    }
    // Convenience copies — dynamism means the current value moves WITH the fact.
    sqlx::query(
        r#"
        UPDATE public.teams
        SET venue_name = COALESCE($2, venue_name),
            logo_url   = COALESCE($3, logo_url),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(team_id)
    .bind(venue.as_ref().map(|(v, _)| v.as_str()))
    .bind(logo.as_ref().map(|(v, _)| v.as_str()))
    .execute(&mut **tx)
    .await
    .context("update team convenience columns")?;
    Ok(())
}

pub(super) async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    mappings: &[TeamMapping],
    decision: &Decision,
) -> Result<HandleOutcome> {
    let mut tx = pool.begin().await?;
    if !work::lock_claim(&mut tx, item).await? {
        tx.rollback().await?;
        return Ok(HandleOutcome::Superseded);
    }
    // A different claim can have resolved the candidate while evidence was being gathered.
    let candidate = match decision {
        Decision::Accept { candidate, .. } | Decision::Refuse { candidate, .. } => Some(candidate),
        _ => None,
    };
    let pending = if let Some(candidate) = candidate {
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM public.entity_candidates WHERE id=$1 FOR UPDATE",
        )
        .bind(candidate.id)
        .fetch_optional(&mut *tx)
        .await?
        .as_deref()
            == Some("pending")
    } else {
        true
    };
    if pending {
        for mapping in mappings {
            sqlx::query("INSERT INTO public.entity_external_ids (entity_type, entity_id, sport, namespace, external_id, source_document_id)
                SELECT 'team', $1, $2, 'wikidata', $3, $4 WHERE NOT EXISTS (
                    SELECT 1 FROM public.entity_external_ids WHERE entity_type='team' AND entity_id=$1 AND sport=$2 AND namespace='wikidata' AND external_id=$3)")
                .bind(mapping.team_id).bind(&mapping.sport).bind(&mapping.qid).bind(mapping.document_id)
                .execute(&mut *tx).await?;
        }
        match decision {
            Decision::Unchanged => {}
            Decision::Accept {
                candidate,
                sport,
                item,
                kind,
                role,
                teams,
                plan,
            } => publish_accept(&mut tx, candidate, sport, item, kind, *role, teams, plan).await?,
            Decision::Refuse {
                candidate,
                state,
                resolved,
                plan,
                reason,
            } => publish_refusal(&mut tx, candidate, state, resolved.clone(), plan, reason).await?,
            Decision::Player {
                player_id,
                sport,
                it,
            } => publish_player(&mut tx, *player_id, sport, it).await?,
            Decision::Team {
                team_id,
                sport,
                venue,
                logo,
            } => publish_team(&mut tx, *team_id, sport, venue, logo).await?,
        }
    }
    // Refusals observe the cooldown too; propagated preparation errors never publish.
    let stamp = match item.entity_type.as_str() {
        "player" => Some("UPDATE public.players SET meta=jsonb_set(COALESCE(meta, '{}'::jsonb), '{investigated_at}', to_jsonb(NOW())) WHERE id=$1 AND sport=$2"),
        "team" => Some("UPDATE public.teams SET meta=jsonb_set(COALESCE(meta, '{}'::jsonb), '{investigated_at}', to_jsonb(NOW())) WHERE id=$1 AND sport=$2"),
        _ => None,
    };
    if let Some(sql) = stamp {
        sqlx::query(sql)
            .bind(item.entity_id_i32()?)
            .bind(item.sport.to_uppercase())
            .execute(&mut *tx)
            .await?;
    }
    anyhow::ensure!(
        work::complete_in_transaction(&mut tx, item).await?,
        "investigator claim changed during publication"
    );
    tx.commit().await?;
    Ok(HandleOutcome::Completed)
}

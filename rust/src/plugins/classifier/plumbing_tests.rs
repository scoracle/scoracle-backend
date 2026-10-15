//! Shared Classifier test schema bootstrap.
use anyhow::{ensure, Result};

pub(crate) async fn setup_disposable(pool: &sqlx::PgPool) -> Result<()> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    ensure!(
        database.starts_with("classifier_test"),
        "use a disposable classifier_test database"
    );
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations(version text PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS teams(id integer,sport text,name text,PRIMARY KEY(id,sport));
        CREATE TABLE IF NOT EXISTS news_articles(id bigint PRIMARY KEY,url text NOT NULL,title text NOT NULL,
            source text,published_at timestamptz,full_text text,duplicate_of bigint,feed_rank integer,url_hash text);
        ALTER TABLE news_articles ADD COLUMN IF NOT EXISTS url_hash text;
        CREATE TABLE IF NOT EXISTS harvester_query_provenance(article_id bigint REFERENCES news_articles(id),
            entity_type text,entity_id integer,sport text,feed_rank integer,PRIMARY KEY(article_id,entity_type,entity_id,sport));
        CREATE TABLE IF NOT EXISTS players(id integer,sport text,name text,PRIMARY KEY(id,sport));
        CREATE TABLE IF NOT EXISTS persons(id integer,sport text,full_name text,kind text,team_id integer,PRIMARY KEY(id,sport));
        CREATE TABLE IF NOT EXISTS entity_name_surfaces(sport text,entity_type text,entity_id integer,norm text,surface_kind text);
        ALTER TABLE entity_name_surfaces ADD COLUMN IF NOT EXISTS surface_kind text;
        DO $fixture$ BEGIN IF to_regprocedure('public.nrm(text)') IS NULL THEN
            EXECUTE 'CREATE FUNCTION public.nrm(text) RETURNS text LANGUAGE sql IMMUTABLE AS $body$SELECT lower($1)$body$';
        END IF; END $fixture$;
        CREATE TABLE IF NOT EXISTS sports(id text PRIMARY KEY,display_name text,current_season integer);
        CREATE TABLE IF NOT EXISTS news_article_entities(article_id bigint REFERENCES news_articles(id) ON DELETE CASCADE,
            entity_type text,entity_id integer,sport text,created_at timestamptz DEFAULT now(),PRIMARY KEY(article_id,entity_type,entity_id,sport));")
        .execute(pool).await?;
    for (version, migration) in [
        (
            "102_pipeline_work",
            include_str!("../../../../sql/migrations/102_pipeline_work.sql"),
        ),
        (
            "109_pipeline_work_article_stage",
            include_str!("../../../../sql/migrations/109_pipeline_work_article_stage.sql"),
        ),
        (
            "256_pipeline_work_claim_fencing",
            include_str!("../../../../sql/migrations/256_pipeline_work_claim_fencing.sql"),
        ),
        (
            "257_influencer_publication_outbox",
            include_str!("../../../../sql/migrations/257_influencer_publication_outbox.sql"),
        ),
        (
            "258_analyst_publication_outbox",
            include_str!("../../../../sql/migrations/258_analyst_publication_outbox.sql"),
        ),
        (
            "259_scout_publication_outbox",
            include_str!("../../../../sql/migrations/259_scout_publication_outbox.sql"),
        ),
        (
            "260_journalist_publication_outbox",
            include_str!("../../../../sql/migrations/260_journalist_publication_outbox.sql"),
        ),
        (
            "291_classifier_plumbing",
            include_str!("../../../../sql/migrations/291_classifier_plumbing.sql"),
        ),
        (
            "292_classifier_acquisition_intake",
            include_str!("../../../../sql/migrations/292_classifier_acquisition_intake.sql"),
        ),
        (
            "293_classifier_character_delivery",
            include_str!("../../../../sql/migrations/293_classifier_character_delivery.sql"),
        ),
        (
            "294_classifier_influencer_delivery",
            include_str!("../../../../sql/migrations/294_classifier_influencer_delivery.sql"),
        ),
        (
            "261_insider_publication_outbox",
            include_str!("../../../../sql/migrations/261_insider_publication_outbox.sql"),
        ),
        (
            "295_classifier_insider_delivery",
            include_str!("../../../../sql/migrations/295_classifier_insider_delivery.sql"),
        ),
        (
            "296_classifier_scout_delivery",
            include_str!("../../../../sql/migrations/296_classifier_scout_delivery.sql"),
        ),
        (
            "298_classifier_article_identities",
            include_str!("../../../../sql/migrations/298_classifier_article_identities.sql"),
        ),
    ] {
        let applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=$1)")
                .bind(version)
                .fetch_one(pool)
                .await?;
        // A newer schema may already contain person claims; never replay the old article-only constraint.
        if !applied && version == "109_pipeline_work_article_stage" {
            let article_supported: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_constraint
                WHERE conrelid='pipeline_work'::regclass AND conname='pipeline_work_entity_type_check'
                AND pg_get_constraintdef(oid) LIKE '%article%')").fetch_one(pool).await?;
            if article_supported {
                sqlx::query(
                    "INSERT INTO schema_migrations(version) VALUES($1) ON CONFLICT DO NOTHING",
                )
                .bind(version)
                .execute(pool)
                .await?;
                continue;
            }
        }
        // Reapply only the newest dispatch implementation while developing it.
        if !applied || version == "296_classifier_scout_delivery" {
            sqlx::raw_sql(migration).execute(pool).await?;
            sqlx::query("INSERT INTO schema_migrations(version) VALUES($1) ON CONFLICT DO NOTHING")
                .bind(version)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

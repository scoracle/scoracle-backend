//! Verified, packet-free publisher context for character adapters.
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceContext {
    pub classification_id: i64,
    pub article_id: i64,
    pub headline: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
}

pub async fn load_for_character(
    pool: &PgPool,
    plugin_id: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SourceContext>> {
    let mut connection = pool.acquire().await?;
    load_on(
        &mut connection,
        plugin_id,
        entity_type,
        entity_id,
        sport,
        false,
        None,
    )
    .await
}

/// A named Insider subject can use the source receipt even after the query-team
/// assignment has finished. Its own pending mention is the work obligation.
pub async fn load_for_insider_subject(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SourceContext>> {
    let mut connection = pool.acquire().await?;
    load_on(
        &mut connection,
        crate::plugins::insider::manifest::MANIFEST.id.as_str(),
        entity_type,
        entity_id,
        sport,
        true,
        None,
    )
    .await
}

/// Accepted entity evidence, independent of generated stories and delivery completion.
pub async fn load_accepted(
    connection: &mut sqlx::PgConnection,
    subject: &crate::tools::meta::EntityMeta,
    from: i64,
    before: i64,
    cutoff: i64,
) -> Result<Vec<SourceContext>> {
    load_on(
        connection,
        crate::plugins::influencer::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
        false,
        Some((from, before, cutoff)),
    )
    .await
}

async fn load_on(
    connection: &mut sqlx::PgConnection,
    plugin_id: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    insider_subject: bool,
    scope: Option<(i64, i64, i64)>,
) -> Result<Vec<SourceContext>> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON (c.article_id) c.id AS classification_id, c.article_id, \
         c.headline, a.title, COALESCE(a.source, '') AS source, \
         EXTRACT(EPOCH FROM a.published_at)::bigint AS published_at_epoch, \
         a.full_text, c.body_sha256, c.context_start, c.context_end, c.context_text, \
         c.contract_version, COALESCE( \
           (c.model_provenance->'source_identity') - 'published_at' = \
             jsonb_build_object('source',COALESCE(a.source,''),'url',a.url) \
           AND c.model_provenance->'source_identity' ? 'published_at' \
           AND ((c.model_provenance->'source_identity'->>'published_at')::timestamptz \
             IS NOT DISTINCT FROM a.published_at),false) AS source_identity_matches \
         FROM public.harvester_classifications c \
         LEFT JOIN public.harvester_assignments d ON d.classification_id=c.id AND d.plugin_id=$1 \
         JOIN public.news_articles a ON a.id=c.article_id \
         WHERE d.reason IS DISTINCT FROM $5 AND c.sport=$4 \
           AND (($7::bigint IS NOT NULL AND c.entity_type=$2 AND c.entity_id=$3 \
             AND c.entity_choice='relevant' AND c.created_at<=to_timestamp($9::double precision) \
             AND NOT EXISTS (SELECT 1 FROM harvester_classifications newer \
               WHERE newer.article_id=c.article_id AND newer.sport=c.sport \
               AND newer.entity_type=c.entity_type AND newer.entity_id=c.entity_id \
               AND (newer.created_at,newer.id)>(c.created_at,c.id) \
               AND newer.created_at<=to_timestamp($9::double precision)) \
             AND a.fetched_at<=to_timestamp($9::double precision) \
             AND COALESCE(a.published_at,a.fetched_at)>=to_timestamp($7::double precision) \
             AND COALESCE(a.published_at,a.fetched_at)<to_timestamp($8::double precision)) \
             OR ($7::bigint IS NULL AND d.status='pending' AND c.entity_type=$2 AND c.entity_id=$3) \
             OR ($6 AND d.plugin_id=$1 AND EXISTS ( \
               SELECT 1 FROM public.harvester_insider_pairs p \
                WHERE p.classification_id=c.id AND p.subject_type=$2 \
                  AND p.subject_id=$3 AND p.status='pending'))) \
         ORDER BY c.article_id, c.created_at DESC, c.id DESC LIMIT 20001",
    )
    .bind(plugin_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(super::adapter::DELIVERY_HELD_REASON)
    .bind(insider_subject)
    .bind(scope.map(|s| s.0))
    .bind(scope.map(|s| s.1))
    .bind(scope.map(|s| s.2))
    .fetch_all(connection)
    .await?;
    ensure!(
        rows.len() <= 20000,
        "accepted source population exceeds bound"
    );
    let mut sources = Vec::with_capacity(rows.len());
    for row in rows {
        let body: Option<String> = row.get("full_text");
        let body = body
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Harvester body missing"))?;
        let hash: String = row.get("body_sha256");
        let start: i32 = row.get("context_start");
        let end: i32 = row.get("context_end");
        let context: String = row.get("context_text");
        let headline: String = row.get("headline");
        let article_title: String = row.get("title");
        ensure!(start >= 0 && end >= start, "invalid Harvester byte range");
        ensure!(
            headline == article_title,
            "Harvester headline no longer matches article"
        );
        ensure!(
            hex::encode(Sha256::digest(body.as_bytes())) == hash,
            "Harvester body hash drift"
        );
        ensure!(
            body.get(start as usize..end as usize) == Some(context.as_str()),
            "Harvester context no longer matches publisher text"
        );
        // Older pending receipts retain their historical contract. V6 additionally
        // binds attribution and date through delivery, not only the body and title.
        if matches!(
            row.get::<String, _>("contract_version").as_str(),
            "harvest-context-v6"
                | "harvest-context-v7"
                | "harvest-context-v8-editor"
                | "harvest-context-v9-entity-vibe"
        ) {
            ensure!(
                row.get::<bool, _>("source_identity_matches"),
                "Harvester source attribution or publication date drift"
            );
        }
        let context = if crate::tools::reader::needs_article(plugin_id) {
            crate::tools::reader::read(body).text
        } else {
            context
        };
        sources.push(SourceContext {
            classification_id: row.get("classification_id"),
            article_id: row.get("article_id"),
            headline,
            context,
            source: row.get("source"),
            published_at_epoch: row.get("published_at_epoch"),
        });
    }
    sources.sort_by(|a, b| {
        b.published_at_epoch
            .cmp(&a.published_at_epoch)
            .then_with(|| b.article_id.cmp(&a.article_id))
    });
    Ok(sources)
}

/// Undelivered Harvester source contexts still owed to this plugin for this entity,
/// excluding the delivery-held receipt. A character publishes only once the drain
/// reaches zero, so every publisher checks this inside its own transaction.
pub async fn undelivered_count(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND d.reason IS DISTINCT FROM $5 \
           AND c.entity_type=$2 AND c.entity_id=$3 AND c.sport=$4",
    )
    .bind(plugin_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(crate::plugins::harvester::adapter::DELIVERY_HELD_REASON)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

/// Lock receipts and publisher rows, then re-run the delivery integrity checks
/// inside the publication transaction. No model or network work occurs here.
pub async fn validate_for_publication(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    sources: &[SourceContext],
) -> Result<()> {
    validate_on(tx, plugin_id, entity_type, entity_id, sport, sources, false).await
}

pub async fn validate_insider_subject_for_publication(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    sources: &[SourceContext],
) -> Result<()> {
    validate_on(
        tx,
        crate::plugins::insider::manifest::MANIFEST.id.as_str(),
        entity_type,
        entity_id,
        sport,
        sources,
        true,
    )
    .await
}

async fn validate_on(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    sources: &[SourceContext],
    insider_subject: bool,
) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = sources.iter().map(|s| s.classification_id).collect();
    sqlx::query(
        "SELECT c.id FROM harvester_classifications c \
        JOIN harvester_assignments d ON d.classification_id=c.id \
        JOIN news_articles a ON a.id=c.article_id \
        WHERE c.id=ANY($1) AND d.plugin_id=$2 ORDER BY c.id FOR SHARE OF a,c,d",
    )
    .bind(&ids)
    .bind(plugin_id)
    .fetch_all(&mut **tx)
    .await?;
    let current = load_on(
        &mut **tx,
        plugin_id,
        entity_type,
        entity_id,
        sport,
        insider_subject,
        None,
    )
    .await?;
    for source in sources {
        ensure!(
            current.iter().any(|s| s == source),
            "source receipt changed during plugin articulation"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL with migration 269"]
    async fn stored_context_is_exact_and_body_drift_fails_closed() {
        const ARTICLE: i64 = 9_690_001;
        const SPORT: &str = "ZZ_HARVESTER_CONTEXT";
        const PLUGIN: &str = "scoracle.character.narrative";
        let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        let body = "First source sentence. Second source sentence. Third source sentence.";
        sqlx::query(
            "INSERT INTO public.news_articles(id,url_hash,url,title,source,description,full_text) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(ARTICLE)
        .bind("harvester-context-test-9690001")
        .bind("https://example.test/harvester-context")
        .bind("Original headline")
        .bind("Example Wire")
        .bind("RSS summary")
        .bind(body)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO public.harvester_query_provenance(article_id,entity_type,entity_id,sport) \
             VALUES ($1,'team',1,$2)",
        )
        .bind(ARTICLE)
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
        let classification_id: i64 = sqlx::query_scalar(
            "INSERT INTO public.harvester_classifications \
             (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
              body_sha256,headline,model_input_start,model_input_end,model_input_text, \
              context_start,context_end,context_text,distributions,model_provenance) \
             VALUES ($1,'team',1,$2,'harvest-context-v1','test-revision','irrelevant', \
                     $3,'Original headline',0,$4,$5,0,$4,$5,'{}'::jsonb,'{}'::jsonb) RETURNING id",
        )
        .bind(ARTICLE)
        .bind(SPORT)
        .bind(hex::encode(Sha256::digest(body.as_bytes())))
        .bind(body.len() as i32)
        .bind(body)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO public.harvester_assignments(classification_id,plugin_id) VALUES($1,$2)",
        )
        .bind(classification_id)
        .bind(PLUGIN)
        .execute(&pool)
        .await
        .unwrap();
        let sources = load_for_character(&pool, PLUGIN, "team", 1, SPORT)
            .await
            .unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].headline, "Original headline");
        assert_eq!(sources[0].context, body);
        sqlx::query("UPDATE public.harvester_assignments SET reason=$2 WHERE classification_id=$1")
            .bind(classification_id)
            .bind(super::super::adapter::DELIVERY_HELD_REASON)
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_for_character(&pool, PLUGIN, "team", 1, SPORT)
            .await
            .unwrap()
            .is_empty());
        sqlx::query(
            "UPDATE public.harvester_assignments SET reason=NULL WHERE classification_id=$1",
        )
        .bind(classification_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE public.news_articles SET published_at='2000-01-01 00:00:00+00' WHERE id=$1",
        )
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE public.harvester_classifications c SET contract_version=$2, \
             model_provenance=jsonb_build_object('source_identity',jsonb_build_object( \
                 'source',a.source,'url',a.url,'published_at','1999-12-31 19:00:00-05')) \
             FROM public.news_articles a WHERE c.article_id=a.id AND c.id=$1",
        )
        .bind(classification_id)
        .bind(super::super::context::CONTRACT)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            load_for_character(&pool, PLUGIN, "team", 1, SPORT)
                .await
                .unwrap()
                .len(),
            1
        );
        for mutation in [
            "UPDATE public.news_articles SET source='Other publisher' WHERE id=$1",
            "UPDATE public.news_articles SET source='Example Wire',published_at=NOW() WHERE id=$1",
        ] {
            sqlx::query(mutation)
                .bind(ARTICLE)
                .execute(&pool)
                .await
                .unwrap();
            assert!(load_for_character(&pool, PLUGIN, "team", 1, SPORT)
                .await
                .unwrap_err()
                .to_string()
                .contains("attribution or publication date drift"));
        }
        sqlx::query("UPDATE public.news_articles SET published_at=NULL WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE public.news_articles SET title='Changed headline' WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_for_character(&pool, PLUGIN, "team", 1, SPORT)
            .await
            .unwrap_err()
            .to_string()
            .contains("headline no longer matches article"));
        sqlx::query("UPDATE public.news_articles SET title='Original headline' WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE public.news_articles SET full_text='altered source' WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_for_character(&pool, PLUGIN, "team", 1, SPORT)
            .await
            .is_err());
        sqlx::query("DELETE FROM public.news_articles WHERE id=$1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
    }
}

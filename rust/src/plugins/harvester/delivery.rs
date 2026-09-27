//! Verified, packet-free publisher context for character adapters.
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

#[derive(Clone, Debug)]
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
    let rows = sqlx::query(
        "SELECT DISTINCT ON (c.article_id) c.id AS classification_id, c.article_id, \
         c.headline, a.title, COALESCE(a.source, '') AS source, \
         EXTRACT(EPOCH FROM a.published_at)::bigint AS published_at_epoch, \
         a.full_text, c.body_sha256, c.context_start, c.context_end, c.context_text \
         FROM public.harvester_classifications c \
         JOIN public.harvester_assignments d ON d.classification_id=c.id \
         JOIN public.news_articles a ON a.id=c.article_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND d.reason IS DISTINCT FROM $5 \
           AND c.entity_type=$2 AND c.entity_id=$3 AND c.sport=$4 \
         ORDER BY c.article_id, c.created_at DESC, c.id DESC",
    )
    .bind(plugin_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(super::adapter::DELIVERY_HELD_REASON)
    .fetch_all(pool)
    .await?;
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

//! Bounded publisher work: one cheap read, one deferred retry, then an explicit skip.
use super::Source;
use crate::harness::{
    plugin::PluginOutcome, queue::publication::ClaimPublication, queue::work::Item,
    tools::WebBroker,
};
use crate::tools::fetch::{self, BudgetedFetchError, FetchPolicy};
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;

pub(super) const POLICY: &str = "article-acquisition-v1";
// shortcut: 24 KB does not prove token fit, add native preflight if smaller inputs still overflow.
const MAX_ARTICLE_BYTES: usize = 24_000;

pub(super) enum Prepared {
    Ready(Source),
    Skipped {
        reason: &'static str,
        detail: String,
    },
    Retry {
        reason: &'static str,
        detail: String,
        delay: Duration,
    },
}

async fn retry(
    pool: &PgPool,
    item: &Item,
    before: &Source,
    reason: &'static str,
    detail: String,
    delay: Duration,
) -> Result<Prepared> {
    let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM classifier_acquisitions
        WHERE article_id=$1 AND sport=$2 AND acquisition_key=$3 AND policy_version=$4 AND status='retryable_error')")
        .bind(item.entity_id).bind(&item.sport).bind(before.provenance["acquisition_key"].as_str().expect("acquisition key"))
        .bind(POLICY).fetch_one(pool).await?;
    Ok(if used {
        Prepared::Skipped {
            reason: "retry_exhausted",
            detail,
        }
    } else {
        Prepared::Retry {
            reason,
            detail,
            delay,
        }
    })
}

pub(super) async fn prepare(
    pool: &PgPool,
    web: &WebBroker,
    item: &Item,
    before: &Source,
) -> Result<Prepared> {
    let skipped: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM classifier_acquisitions
        WHERE article_id=$1 AND sport=$2 AND acquisition_key=$3 AND policy_version=$4 AND status='skipped')")
        .bind(item.entity_id).bind(&item.sport)
        .bind(before.provenance["acquisition_key"].as_str().expect("acquisition key"))
        .bind(POLICY).fetch_one(pool).await?;
    if skipped {
        return Ok(Prepared::Skipped {
            reason: "previously_skipped",
            detail: "This publisher input already has a terminal disposition".into(),
        });
    }
    let mut source = before.clone();
    if web.reader_enabled() {
        // Existing RSS/full_text bytes do not establish that the URL was browser-read.
        source.body.clear();
    }
    if source.body.trim().is_empty() {
        // shortcut: unchanged discovery keys reuse text indefinitely, add expiry if correction polling is required.
        let retained: Option<Value> = sqlx::query_scalar(
            "SELECT s.source FROM classifier_source_heads h
            JOIN classifier_sources s ON s.id=h.source_id WHERE h.article_id=$1 AND h.sport=$2
            AND s.source->>'acquisition_policy'=$3 AND s.source->'acquisition_key'=$4::jsonb
            AND COALESCE(s.source->>'reader_version','')=$5",
        )
        .bind(item.entity_id)
        .bind(&item.sport)
        .bind(POLICY)
        .bind(&before.provenance["acquisition_key"])
        .bind(if web.reader_enabled() {
            crate::tools::reader::VERSION
        } else {
            ""
        })
        .fetch_optional(pool)
        .await?;
        if let Some(retained) = retained {
            let retained: Source = serde_json::from_value(retained)?;
            source.body = retained.body;
            for key in [
                "publisher_url",
                "publisher_domain",
                "reader_version",
                "source_document_id",
            ] {
                if let Some(value) = retained.provenance.get(key) {
                    source.provenance.insert(key.into(), value.clone());
                }
            }
            source
                .provenance
                .insert("acquisition_reused".into(), json!(true));
        }
    }
    let expired: bool = sqlx::query_scalar("SELECT COALESCE(a.published_at < now()-interval '48 hours',false)
        AND NOT EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=a.id AND h.sport=$2)
        FROM news_articles a WHERE a.id=$1")
        .bind(item.entity_id).bind(&item.sport).fetch_one(pool).await?;
    if expired {
        return Ok(Prepared::Skipped {
            reason: "expired",
            detail: "Never-acquired reporting is older than 48 hours".into(),
        });
    }
    if source.body.trim().is_empty() {
        let url = before.provenance["url"].as_str().expect("discovery URL");
        let scoped = web.scope(pool, &super::manifest::ACQUIRE_MANIFEST);
        let policy = FetchPolicy::default();
        let started = std::time::Instant::now();
        let fetched = match scoped.read_article(url, &policy).await {
            Err(error) => {
                let detail = format!("{error:#}");
                let delay = match error.downcast_ref::<BudgetedFetchError>() {
                    Some(BudgetedFetchError::UnsupportedBody { .. }) => {
                        return Ok(Prepared::Skipped {
                            reason: "unsupported_response",
                            detail,
                        })
                    }
                    Some(BudgetedFetchError::DomainSkipped { until_secs, .. }) => {
                        Duration::from_secs(*until_secs)
                    }
                    Some(BudgetedFetchError::Http { status: 429, .. }) => Duration::from_secs(300),
                    Some(BudgetedFetchError::Http {
                        status: 500..=599 | 408,
                        ..
                    }) => Duration::from_secs(30),
                    Some(BudgetedFetchError::Http { .. }) => {
                        return Ok(Prepared::Skipped {
                            reason: "http_rejected",
                            detail,
                        })
                    }
                    Some(BudgetedFetchError::Other(inner))
                        if inner.downcast_ref::<sqlx::Error>().is_some() =>
                    {
                        return Err(error)
                    }
                    Some(BudgetedFetchError::Other(inner))
                        if inner.downcast_ref::<reqwest::Error>().is_some() =>
                    {
                        Duration::from_secs(30)
                    }
                    Some(BudgetedFetchError::Other(_)) => {
                        return Ok(Prepared::Skipped {
                            reason: "unresolved_publisher",
                            detail,
                        })
                    }
                    None => return Err(error),
                };
                return retry(pool, item, before, "fetch_deferred", detail, delay).await;
            }
            Ok(fetched) => fetched,
        };
        source.provenance.insert(
            "fetch_elapsed_ms".into(),
            json!(started.elapsed().as_millis()),
        );
        if fetched.domain.as_deref() == Some("news.google.com") {
            return Ok(Prepared::Skipped {
                reason: "unresolved_publisher",
                detail: "Google wrapper is not publisher evidence".into(),
            });
        }
        source
            .provenance
            .insert("publisher_url".into(), json!(fetched.final_url));
        source
            .provenance
            .insert("publisher_domain".into(), json!(fetched.domain));
        source
            .provenance
            .insert("source_document_id".into(), json!(fetched.document_id));
        source
            .provenance
            .insert("fetch_cache_hit".into(), json!(fetched.from_cache));
        let started = std::time::Instant::now();
        let text = if web.reader_enabled() {
            source.provenance.insert(
                "reader_version".into(),
                json!(crate::tools::reader::VERSION),
            );
            Some(fetched.body)
        } else {
            fetch::extract_article_text_strict(&fetched.body)
        };
        let Some(text) = text else {
            return Ok(Prepared::Skipped {
                reason: "poor_extraction",
                detail: "No usable structured article body or article container".into(),
            });
        };
        source.provenance.insert(
            "extract_elapsed_ms".into(),
            json!(started.elapsed().as_millis()),
        );
        source.body = text;
    } else if source.body.trim_start().starts_with('<') {
        let Some(text) = fetch::extract_article_text_strict(&source.body) else {
            return Ok(Prepared::Skipped {
                reason: "poor_extraction",
                detail: "Retained HTML has no usable article body".into(),
            });
        };
        source.body = text;
    }
    source.body = source.body.replace('\0', "");
    if let Some(reason) = rejection(&source.body) {
        return Ok(Prepared::Skipped {
            reason,
            detail: format!(
                "{} bytes, {} words",
                source.body.len(),
                fetch::count_words(&source.body)
            ),
        });
    }
    source
        .provenance
        .insert("acquisition_policy".into(), json!(POLICY));
    Ok(Prepared::Ready(source))
}

fn rejection(body: &str) -> Option<&'static str> {
    let lower = body.to_ascii_lowercase();
    if fetch::article_unavailable(body) {
        Some("unavailable")
    } else if [
        "subscribe to continue reading",
        "enable javascript and cookies to continue",
        "verify you are human",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        Some("access_wall")
    } else if fetch::count_words(body) < fetch::ARTICLE_MIN_WORDS {
        Some("low_content")
    } else if body.len() > MAX_ARTICLE_BYTES {
        Some("oversized")
    } else {
        None
    }
}

pub(super) async fn record(
    publication: &mut ClaimPublication<'_>,
    item: &Item,
    before: &Source,
    status: &str,
    reason: &str,
    receipt: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO classifier_acquisitions(article_id,sport,acquisition_key,discovery_version,policy_version,status,reason,receipt)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(item.entity_id).bind(&item.sport).bind(before.provenance["acquisition_key"].as_str().expect("acquisition key"))
        .bind(item.input_version.as_deref().unwrap_or_default()).bind(POLICY).bind(status).bind(reason).bind(receipt)
        .execute(&mut **publication.transaction()).await?;
    Ok(())
}

pub(super) async fn finish(
    pool: &PgPool,
    item: &Item,
    before: &Source,
    prepared: Prepared,
    elapsed_ms: u128,
) -> Result<PluginOutcome> {
    let (status, reason, detail, delay) = match prepared {
        Prepared::Skipped { reason, detail } => ("skipped", reason, detail, None),
        Prepared::Retry {
            reason,
            detail,
            delay,
        } => ("retryable_error", reason, detail, Some(delay)),
        Prepared::Ready(_) => unreachable!(),
    };
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    super::adapter::fence_source(pool, item, before, &mut publication).await?;
    record(
        &mut publication,
        item,
        before,
        status,
        reason,
        json!({"detail":detail,"elapsed_ms":elapsed_ms,"url":before.provenance["url"]}),
    )
    .await?;
    if let Some(delay) = delay {
        // The persisted attempt spends the only retry, so repeated holds terminate.
        publication.commit_progress().await?;
        Ok(PluginOutcome::deferred(reason, delay))
    } else {
        // A terminal skip must not strand a character behind pending evidence from an older source.
        sqlx::query("UPDATE classifier_spectrum_deliveries d SET status='held',reason=$3,updated_at=now()
            FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.article_id=$1 AND x.sport=$2 AND d.status='pending'")
            .bind(item.entity_id).bind(&item.sport).bind(format!("acquisition_skipped:{reason}"))
            .execute(&mut **publication.transaction()).await?;
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_keeps_complete_text_and_rejects_unusable_bodies() {
        assert_eq!(
            rejection("Sorry, this blog is currently unavailable"),
            Some("unavailable")
        );
        assert_eq!(rejection("An injury update"), Some("low_content"));
        assert_eq!(rejection(&"word ".repeat(5000)), Some("oversized"));
        let complete = format!(
            "{} Later correction: the player is available.",
            "Publisher detail. ".repeat(60)
        );
        assert_eq!(rejection(&complete), None);
    }
}

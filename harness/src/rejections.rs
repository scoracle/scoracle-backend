//! Append every rejected generation before correction or queue retry can discard it.
mod pending;
pub use pending::start as start_flag_delivery;
#[cfg(test)]
pub(crate) async fn deliver_flags(pool: &sqlx::PgPool, archive: &Path) -> Result<()> {
    pending::deliver(pool, &archive.with_extension("watchdog-pending")).await
}

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{fs::OpenOptions, future::Future, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

tokio::task_local! { static CLAIM: Value; }
#[cfg(test)]
tokio::task_local! { pub(crate) static TEST_PATH: std::path::PathBuf; }

fn path() -> std::path::PathBuf {
    #[cfg(test)]
    if let Ok(path) = TEST_PATH.try_with(Clone::clone) {
        return path;
    }
    std::env::var_os("COGNITION_REJECTIONS_FILE")
        .map(Into::into)
        .unwrap_or_else(|| "logs/rejected-outputs.jsonl".into())
}

fn append(path: &Path, record: Option<&Value>) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    file.lock()?;
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    if let Some(record) = record {
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        let before = file.metadata()?.len();
        if let Err(error) = file.write_all(&line) {
            file.set_len(before)?;
            return Err(error.into());
        }
    }
    file.sync_data()?;
    Ok(())
}

pub fn verify() -> Result<()> {
    append(&path(), None).context("rejected-output archive must be writable before inference")
}

tokio::task_local! { static REVIEW_DB: sqlx::PgPool; }

pub fn is_character(stage: &str) -> bool {
    matches!(
        stage,
        "narratives" | "rating" | "vibe" | "transfers" | "momentum" | "sigil"
    )
}

pub(crate) fn character_claim() -> bool {
    CLAIM
        .try_with(|claim| claim["stage"].as_str().is_some_and(is_character))
        .unwrap_or(false)
}

pub async fn with_claim<F: Future>(
    pool: &sqlx::PgPool,
    item: &super::queue::work::Item,
    future: F,
) -> F::Output {
    CLAIM
        .scope(
            json!({"stage":item.stage.as_str(),"entity_type":item.entity_type,
        "entity_id":item.entity_id,"sport":item.sport,"input_version":item.input_version,
        "claim_token":item.claim_token,"job_attempt":item.attempts+1}),
            REVIEW_DB.scope(pool.clone(), future),
        )
        .await
}

pub async fn record(
    model: &str,
    prompt: &str,
    request: &Value,
    response: Option<&str>,
    raw_response: &str,
    error: &anyhow::Error,
    generation_attempt: usize,
) -> Result<()> {
    record_decision(
        model,
        prompt,
        request,
        response,
        raw_response,
        error,
        generation_attempt,
        "rejected",
    )
    .await
}

#[cfg(test)]
tokio::task_local! { pub(crate) static TEST_OBSERVE: bool; }

pub fn observe_content() -> bool {
    #[cfg(test)]
    if let Ok(observe) = TEST_OBSERVE.try_with(|value| *value) {
        return observe;
    }
    observation_active(
        std::env::var("CHARACTER_CONTENT_POLICY").ok().as_deref(),
        std::env::var("CHARACTER_CONTENT_OBSERVE_UNTIL")
            .ok()
            .as_deref(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(u64::MAX, |elapsed| elapsed.as_secs()),
    )
}

fn observation_active(policy: Option<&str>, until: Option<&str>, now: u64) -> bool {
    policy == Some("observe")
        && until.map_or(true, |deadline| {
            deadline.parse::<u64>().is_ok_and(|deadline| now < deadline)
        })
}

#[test]
fn observation_deadline_restores_strict_validation() {
    assert!(observation_active(Some("observe"), Some("100"), 99));
    assert!(!observation_active(Some("observe"), Some("100"), 100));
    assert!(!observation_active(Some("observe"), Some("100"), 101));
    assert!(!observation_active(Some("observe"), Some("invalid"), 99));
    assert!(!observation_active(None, Some("100"), 99));
    assert!(observation_active(Some("observe"), None, 99));
}

/// An observed validation failure is retained even when the candidate may publish.
#[allow(clippy::too_many_arguments)]
pub async fn record_decision(
    model: &str,
    prompt: &str,
    request: &Value,
    response: Option<&str>,
    raw_response: &str,
    error: &anyhow::Error,
    generation_attempt: usize,
    disposition: &str,
) -> Result<()> {
    let elapsed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    let utc = crate::util::utc_timestamp(elapsed.as_secs() as i64);
    let timestamp = format!(
        "{}.{:06}Z",
        utc.trim_end_matches('Z'),
        elapsed.subsec_micros()
    );
    let record = json!({"capture_version":1,"recorded_at":timestamp,
        "build_commit":super::buildinfo::COMMIT,"model":model,"disposition":disposition,
        "claim":CLAIM.try_with(Clone::clone).ok(),"generation_attempt":generation_attempt,
        "input_hash":crate::util::hash_components(prompt),
        "request_hash":crate::util::hash_components(&request.to_string()),
        "built_prompt":prompt,"request":request,"response":response,
        "raw_response_body":raw_response,"error":format!("{error:#}")});
    let published_flag = character_claim() && disposition == "flagged";
    if published_flag {
        let pool = REVIEW_DB.with(Clone::clone);
        let claim = CLAIM.with(Clone::clone);
        let flag = pending::Flag {
            stage: claim["stage"].as_str().unwrap().to_owned(),
            entity_type: claim["entity_type"].as_str().unwrap().to_owned(),
            entity_id: claim["entity_id"].as_i64().unwrap(),
            sport: claim["sport"].as_str().unwrap().to_owned(),
            claim_token: claim["claim_token"]
                .as_str()
                .context("missing character lease")?
                .to_owned(),
            model: model.to_owned(),
            reason: format!("{error:#}"),
            answer: response.map(str::to_owned),
            recorded_at: timestamp,
        };
        let flag = std::sync::Arc::new(flag);
        let queued = flag.clone();
        let directory = pending::directory();
        let queued = tokio::task::spawn_blocking(move || queued.queue(&directory)).await;
        if !matches!(queued, Ok(Ok(()))) {
            // The database is an independent fallback if local storage is unavailable.
            if !matches!(
                tokio::time::timeout(std::time::Duration::from_secs(2), flag.save(&pool)).await,
                Ok(Ok(()))
            ) {
                tracing::error!(stage=%flag.stage,claim=%flag.claim_token,reason=%flag.reason,
                    "parser flag delivery unavailable; publishing with worker-log diagnostic");
            }
        }
        let archive = path();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = append(&archive, Some(&record)) {
                tracing::warn!(%error,"secondary character archive unavailable");
            }
        });
        return Ok(());
    }
    let path = path();
    let result = tokio::task::spawn_blocking(move || append(&path, Some(&record))).await?;
    if let Err(error) = &result {
        tracing::error!(%error, "rejected-output archive failed; refusing another correction");
    }
    result.context("persist rejected model output")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_records_are_complete_private_and_append_only() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "rejections-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::thread::scope(|scope| {
            for i in 0..16 {
                let path = &path;
                scope.spawn(move || {
                    append(
                        path,
                        Some(&json!({"i":i,"response":"full\nUnicode — output"})),
                    )
                    .unwrap()
                });
            }
        });
        let records: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 16);
        assert!(records
            .iter()
            .all(|r| r["response"] == "full\nUnicode — output"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_file(&path).unwrap();
        assert!(append(Path::new("/dev/null/rejected.jsonl"), Some(&json!({}))).is_err());
    }
}

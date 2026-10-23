//! Append every rejected generation before correction or queue retry can discard it.
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

pub async fn with_claim<F: Future>(item: &super::queue::work::Item, future: F) -> F::Output {
    CLAIM
        .scope(
            json!({"stage":item.stage.as_str(),"entity_type":item.entity_type,
        "entity_id":item.entity_id,"sport":item.sport,"input_version":item.input_version,
        "claim_token":item.claim_token,"job_attempt":item.attempts+1}),
            future,
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
    let record = json!({"capture_version":1,"recorded_at":crate::util::utc_timestamp(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64),
        "build_commit":super::buildinfo::COMMIT,"model":model,
        "claim":CLAIM.try_with(Clone::clone).ok(),"generation_attempt":generation_attempt,
        "input_hash":crate::util::hash_components(prompt),
        "request_hash":crate::util::hash_components(&request.to_string()),
        "built_prompt":prompt,"request":request,"response":response,
        "raw_response_body":raw_response,"error":format!("{error:#}")});
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

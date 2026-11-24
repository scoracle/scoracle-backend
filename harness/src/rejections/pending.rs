//! Local delivery of parser flags. The product writer never waits for a watchdog run.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Deserialize, Serialize)]
pub(super) struct Flag {
    pub stage: String,
    pub entity_type: String,
    pub entity_id: i64,
    pub sport: String,
    pub claim_token: String,
    pub model: String,
    pub reason: String,
    pub answer: Option<String>,
    pub recorded_at: String,
}

impl Flag {
    pub(super) async fn save(&self, pool: &PgPool) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO character_output_flags(edition_date,stage,entity_type,entity_id,sport,claim_token,reason,model,answer,recorded_at)
            VALUES ((SELECT edition_date FROM coverage_editions WHERE frozen_at<=$9::text::timestamptz
                 AND (finished_at IS NULL OR finished_at>=$9::text::timestamptz) ORDER BY edition_date DESC LIMIT 1),
                $1,$2,$3,$4,$5::uuid,$6,$7,$8,$9::text::timestamptz)
            ON CONFLICT DO NOTHING")
            .bind(&self.stage).bind(&self.entity_type).bind(self.entity_id).bind(&self.sport)
            .bind(&self.claim_token).bind(self.reason.replace('\0', "\\u0000")).bind(&self.model)
            .bind(self.answer.as_ref().map(|text| text.replace('\0', "\\u0000")))
            .bind(&self.recorded_at).execute(pool).await?;
        Ok(())
    }

    pub(super) fn queue(&self, dir: &Path) -> Result<()> {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let key = crate::util::hash_components(&format!("{}:{}", self.claim_token, self.reason));
        let tmp = dir.join(format!(
            "{key}.{}.{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&tmp)?;
            file.write_all(&serde_json::to_vec(self)?)?;
            file.sync_all()?;
            fs::rename(&tmp, dir.join(format!("{key}.json")))?;
            fs::File::open(dir)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(tmp);
        }
        result
    }
}

pub(super) fn directory() -> PathBuf {
    super::path().with_extension("watchdog-pending")
}

pub(super) async fn deliver(pool: &PgPool, dir: &Path) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let bytes = match fs::read(&path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            result => result?,
        };
        let flag: Flag = match serde_json::from_slice(&bytes) {
            Ok(flag) => flag,
            Err(error) => {
                tracing::error!(%error,?path,"unreadable pending parser flag");
                continue;
            }
        };
        if let Err(error) = flag.save(pool).await {
            if error
                .as_database_error()
                .and_then(|db| db.code())
                .is_some_and(|code| code.starts_with("22") || code.starts_with("23"))
            {
                tracing::error!(%error,?path,"invalid parser flag retained for review");
                continue;
            }
            return Err(error.into());
        }
        if let Err(err) = fs::remove_file(&path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err.into());
            }
        }
    }
    Ok(())
}

pub fn start(pool: PgPool) {
    let dir = directory();
    tokio::spawn(async move {
        loop {
            if let Err(error) = deliver(&pool, &dir).await {
                tracing::warn!(%error,"parser flags retained for redelivery");
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

use super::*;
use crate::harness::{
    queue::work,
    skill::{Skill, SkillOutcome},
};
use anyhow::Result;
use sqlx::PgPool;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub(crate) struct Model {
    address: std::net::SocketAddr,
    pub requests: Arc<Mutex<Vec<Value>>>,
    pub fail: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Model {
    pub fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let fail = Arc::new(AtomicBool::new(false));
        let failed = fail.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&b[..n]);
                    if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let model = format!("LiquidAI/d1-3B-GGUF@sha256:{}", "0".repeat(64));
                let (status, body) = if headers.starts_with("GET /v1/models ") {
                    (
                        "200 OK",
                        serde_json::json!({"data":[{"id":model}]}).to_string(),
                    )
                } else {
                    assert!(headers.starts_with("POST /v1/systemone "));
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .unwrap()
                        .parse()
                        .unwrap();
                    while bytes.len() < header_end + length {
                        let mut b = [0; 4096];
                        let n = socket.read(&mut b).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&b[..n]);
                    }
                    let payload: Value =
                        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                    let state: Value =
                        serde_json::from_str(payload["state"].as_str().unwrap()).unwrap();
                    let headline = state["evidence"]["headline"].as_str().unwrap();
                    let rejected =
                        headline.contains("Unrelated") || state["target"]["name"] == "Maple United";
                    let answers=payload["questions"].as_object().unwrap().keys().map(|key|(key.clone(),serde_json::json!({"type":"noul","noul":if rejected || (key=="availability" && !headline.contains("availability fixture")) {0.1}else if key=="availability" {0.85}else{0.95}}))).collect::<serde_json::Map<_,_>>();
                    captured.lock().unwrap().push(payload);
                    (
                        if failed.swap(false, Ordering::Relaxed) {
                            "500 Internal Server Error"
                        } else {
                            "200 OK"
                        },
                        serde_json::json!({"model":model,"answers":answers}).to_string(),
                    )
                };
                write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Ok(Self {
            address,
            requests,
            fail,
            stop,
            thread: Some(thread),
        })
    }
    pub fn decision(&self) -> Result<decision::Decision> {
        decision::Decision::new(format!("http://{}", self.address))
    }
    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}

#[tokio::test]
async fn headline_replay_requires_explicit_opt_in() {
    let pool = PgPool::connect_lazy("postgres://localhost/classifier_test").unwrap();
    for setting in [None, Some("false"), Some(""), Some("TRUE"), Some("typo")] {
        assert!(adapter::headline_replay(&pool, setting).is_empty());
    }
    assert!(adapter::headline_replay(&pool, Some("true")).is_empty());
}

const SPORT: &str = "ZZ_HEADLINE";

include!("../../../tests/internal/classifier/tests.rs");

async fn seed(pool: &PgPool, id: i64, title: &str, body: &str) -> Result<()> {
    sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,full_text,published_at) VALUES($1,'https://example.invalid/'||$1,md5($1::text),$2,'Fixture Wire',$3,now())")
        .bind(id).bind(title).bind(body).execute(pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',910415,$2)").bind(id).bind(SPORT).execute(pool).await?;
    Ok(())
}
async fn take(
    pool: &PgPool,
    stage: crate::harness::queue::work::TaskKey,
    id: i64,
) -> Result<work::Item> {
    let mut selected = None;
    for item in work::claim(pool, stage, 100).await? {
        if item.entity_id == id && item.sport == SPORT {
            selected = Some(item);
        } else {
            work::release(pool, &item).await?;
        }
    }
    Ok(selected.expect("fixture claim missing"))
}
async fn admit(pool: &PgPool, handler: &adapter::ClassifierHandler, id: i64) -> Result<()> {
    sqlx::query("SELECT classifier_enqueue_admission($1,$2)")
        .bind(id)
        .bind(SPORT)
        .execute(pool)
        .await?;
    assert!(matches!(
        handler
            .execute(&take(pool, manifest::TASK, id).await?)
            .await?,
        SkillOutcome::Committed
    ));
    Ok(())
}

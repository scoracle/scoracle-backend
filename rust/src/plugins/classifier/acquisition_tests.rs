use super::*;
use crate::harness::{
    plugin::{PluginOutcome, StudioPlugin},
    queue::work::{self, Item},
    tools::WebBroker,
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

const SPORT: &str = "ZZ_ACQUISITION";

struct Publisher {
    url: String,
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Publisher {
    fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let url = format!("http://publisher.test:{}", address.port());
        listener.set_nonblocking(true)?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket.set_nonblocking(false).unwrap();
                let mut buffer = [0; 4096];
                let Ok(n) = socket.read(&mut buffer) else {
                    continue;
                };
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..n]);
                let path = request.split_whitespace().nth(1).unwrap().to_string();
                recorded.lock().unwrap().push(path.clone());
                let (status, body) = match path.as_str() {
                    "/https://publisher.example/reader" => ("200 OK", serde_json::json!({"code":200,"data":{
                        "url":"https://publisher.example/reader","title":"Cedar FC reporting",
                        "content":format!("{} Later correction: the player is available.","Complete publisher evidence. ".repeat(40))
                    }}).to_string()),
                    "/blocked" => ("403 Forbidden", "Access denied".into()),
                    "/busy" => ("503 Service Unavailable", "Try later".into()),
                    "/limited" => ("429 Too Many Requests", "Try later".into()),
                    "/binary" => ("200 OK", "Binary\0response".into()),
                    "/navigation" => ("200 OK", format!("<nav>{}</nav><div>{}</div>", "Menu ".repeat(100), "Related story ".repeat(100))),
                    _ => ("200 OK", format!("<article><p>{} Later correction: the player denied the move.</p></article>", "Cedar FC published reporting with complete evidence. ".repeat(30))),
                };
                write!(socket, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nRetry-After: 60\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Ok(Self {
            url,
            address,
            requests,
            stop,
            thread: Some(thread),
        })
    }

    fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| *request == path)
            .count()
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}

async fn seed(pool: &PgPool, id: i64, url: &str, body: Option<&str>, age_hours: i32) -> Result<()> {
    sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,full_text,published_at) VALUES($1,$2,md5($2),'Cedar FC news','Fixture Wire',$3,now()-make_interval(hours=>$4))")
        .bind(id).bind(url).bind(body).bind(age_hours).execute(pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) VALUES($1,'team',920001,$2,10)")
        .bind(id).bind(SPORT).execute(pool).await?;
    Ok(())
}

async fn claim(pool: &PgPool, id: i64) -> Result<Item> {
    sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
        .bind(id)
        .bind(SPORT)
        .execute(pool)
        .await?;
    Ok(work::claim_with_policy(
        pool,
        manifest::ACQUIRE_TASK,
        manifest::ACQUIRE_MANIFEST.claim_policy,
        1,
    )
    .await?
    .remove(0))
}

async fn reason(pool: &PgPool, id: i64) -> Result<String> {
    Ok(sqlx::query_scalar("SELECT reason FROM classifier_acquisitions WHERE article_id=$1 AND sport=$2 ORDER BY id DESC LIMIT 1")
        .bind(id).bind(SPORT).fetch_one(pool).await?)
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn acquisition_is_bounded_reusable_and_claim_fenced() -> Result<()> {
    let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM harvester_query_provenance WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id BETWEEN 920001 AND 920020")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Acquisition test',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query(
        "INSERT INTO teams(id,sport,name) VALUES(920001,$1,'Cedar FC') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    let publisher = Publisher::new()?;
    let broker = Arc::new(WebBroker::test_publisher(publisher.address)?);
    let handler = adapter::AcquireHandler::new(pool.clone(), broker);

    seed(&pool, 920001, &format!("{}/clean", publisher.url), None, 0).await?;
    let item = claim(&pool, 920001).await?;
    assert!(matches!(
        handler.execute(&item).await?,
        PluginOutcome::Committed
    ));
    let body: String = sqlx::query_scalar("SELECT source->>'body' FROM classifier_sources WHERE article_id=920001 ORDER BY id DESC LIMIT 1").fetch_one(&pool).await?;
    assert!(body.ends_with("the player denied the move."));
    let stages: Vec<String> = sqlx::query_scalar(
        "SELECT stage FROM pipeline_work WHERE sport=$1 AND entity_id=920001 ORDER BY stage",
    )
    .bind(SPORT)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        stages,
        ["classifier"],
        "acquisition only offers scoring work"
    );
    sqlx::query("DELETE FROM source_documents WHERE url=$1")
        .bind(format!("{}/clean", publisher.url))
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE harvester_query_provenance SET feed_rank=2 WHERE article_id=920001")
        .execute(&pool)
        .await?;
    let item = claim(&pool, 920001).await?;
    handler.execute(&item).await?;
    assert_eq!(
        publisher.count("/clean"),
        1,
        "metadata reconciliation must not fetch again, even without HTTP cache"
    );
    let reused: bool = sqlx::query_scalar("SELECT (source->>'acquisition_reused')::boolean FROM classifier_sources WHERE article_id=920001 ORDER BY id DESC LIMIT 1").fetch_one(&pool).await?;
    assert!(reused);

    for (id, path, expected) in [
        (920002, "/navigation", "poor_extraction"),
        (920003, "/blocked", "http_rejected"),
        (920013, "/binary", "unsupported_response"),
    ] {
        seed(&pool, id, &format!("{}{path}", publisher.url), None, 0).await?;
        let item = claim(&pool, id).await?;
        handler.execute(&item).await?;
        assert_eq!(reason(&pool, id).await?, expected);
        sqlx::query("UPDATE harvester_query_provenance SET feed_rank=3 WHERE article_id=$1")
            .bind(id)
            .execute(&pool)
            .await?;
        let queued: bool = sqlx::query_scalar("SELECT classifier_enqueue_acquisition($1,$2)")
            .bind(id)
            .bind(SPORT)
            .fetch_one(&pool)
            .await?;
        assert!(
            !queued,
            "rank changes cannot revive a skipped publisher input"
        );
        assert_eq!(publisher.count(path), 1);
    }
    let replayed: i64 = sqlx::query_scalar("SELECT classifier_replay_acquisition(1000)")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        replayed, 0,
        "backlog replay must not revive unchanged skips"
    );
    for (id, path, requests) in [(920004, "/busy", 2), (920005, "/limited", 1)] {
        seed(&pool, id, &format!("{}{path}", publisher.url), None, 0).await?;
        let item = claim(&pool, id).await?;
        let outcome = handler.execute(&item).await?;
        assert!(matches!(outcome, PluginOutcome::Deferred { .. }));
        // Simulate the host's deferral without waiting for wall-clock backoff.
        assert!(work::defer(&pool, &item, Duration::ZERO, "fixture retry").await?);
        sqlx::query("UPDATE harvester_query_provenance SET feed_rank=1 WHERE article_id=$1")
            .bind(id)
            .execute(&pool)
            .await?;
        let item = claim(&pool, id).await?;
        assert!(matches!(
            handler.execute(&item).await?,
            PluginOutcome::Committed
        ));
        assert_eq!(reason(&pool, id).await?, "retry_exhausted");
        assert_eq!(
            publisher.count(path),
            requests,
            "a rate-limit hold must not issue another HTTP request"
        );
    }
    // A separate broker keeps the preceding fixture's real rate-limit hold isolated.
    let handler = adapter::AcquireHandler::new(
        pool.clone(),
        Arc::new(WebBroker::test_publisher(publisher.address)?),
    );
    seed(
        &pool,
        920006,
        &format!("{}/expired", publisher.url),
        None,
        72,
    )
    .await?;
    let item = claim(&pool, 920006).await?;
    handler.execute(&item).await?;
    assert_eq!(reason(&pool, 920006).await?, "expired");
    assert_eq!(publisher.count("/expired"), 0);
    seed(
        &pool,
        920007,
        "https://example.invalid/oversized",
        Some(&"word ".repeat(5000)),
        0,
    )
    .await?;
    let item = claim(&pool, 920007).await?;
    handler.execute(&item).await?;
    assert_eq!(reason(&pool, 920007).await?, "oversized");

    // A changed source can recover; an unchanged skipped one cannot.
    let corrected = "Cedar FC supplied complete publisher evidence. ".repeat(30);
    sqlx::query("UPDATE news_articles SET full_text=$1 WHERE id=920002")
        .bind(&corrected)
        .execute(&pool)
        .await?;
    let item = claim(&pool, 920002).await?;
    handler.execute(&item).await?;
    assert_eq!(reason(&pool, 920002).await?, "article_body");
    assert_eq!(publisher.count("/navigation"), 1);

    seed(
        &pool,
        920008,
        "https://example.invalid/stale",
        Some("Too short"),
        0,
    )
    .await?;
    let stale = claim(&pool, 920008).await?;
    sqlx::query("UPDATE news_articles SET title='Changed input' WHERE id=920008")
        .execute(&pool)
        .await?;
    sqlx::query("SELECT classifier_enqueue_acquisition(920008,$1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    assert!(matches!(
        handler.execute(&stale).await?,
        PluginOutcome::Superseded
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM classifier_acquisitions WHERE article_id=920008")
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 0, "a stale claim cannot leave a skip receipt");
    let item = claim(&pool, 920008).await?;
    handler.execute(&item).await?;

    // A persisted URL mapping avoids Google entirely, including with a fresh fetcher.
    let wrapper = "https://news.google.com/rss/articles/acquisition-fixture";
    sqlx::query("INSERT INTO source_url_resolutions(wrapper_url,publisher_url) VALUES($1,$2) ON CONFLICT(wrapper_url) DO UPDATE SET publisher_url=EXCLUDED.publisher_url")
        .bind(wrapper).bind(format!("{}/mapped",publisher.url)).execute(&pool).await?;
    seed(&pool, 920009, wrapper, None, 0).await?;
    let item = claim(&pool, 920009).await?;
    handler.execute(&item).await?;
    assert_eq!(publisher.count("/mapped"), 1);
    assert_eq!(reason(&pool, 920009).await?, "article_body");

    seed(
        &pool,
        920010,
        &format!("{}/db-error", publisher.url),
        None,
        0,
    )
    .await?;
    sqlx::query("ALTER TABLE source_documents ADD CONSTRAINT acquisition_fixture_reject CHECK(url NOT LIKE '%/db-error')").execute(&pool).await?;
    let item = claim(&pool, 920010).await?;
    let result = handler.execute(&item).await;
    sqlx::query("ALTER TABLE source_documents DROP CONSTRAINT acquisition_fixture_reject")
        .execute(&pool)
        .await?;
    assert!(
        result.is_err(),
        "database failure must follow normal recovery, never skip evidence"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM classifier_acquisitions WHERE article_id=920010")
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 0);
    work::release(&pool, &item).await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;

    seed(
        &pool,
        920011,
        "https://example.invalid/old",
        Some("Short"),
        24,
    )
    .await?;
    seed(
        &pool,
        920012,
        "https://example.invalid/fresh",
        Some("Short"),
        0,
    )
    .await?;
    for id in [920011_i64, 920012] {
        sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
            .bind(id)
            .bind(SPORT)
            .execute(&pool)
            .await?;
    }
    let items = work::claim_with_policy(
        &pool,
        manifest::ACQUIRE_TASK,
        manifest::ACQUIRE_MANIFEST.claim_policy,
        1,
    )
    .await?;
    assert_eq!(
        items[0].entity_id, 920012,
        "fresh reporting must not wait behind older intake"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM classifier_spectra WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await?,
        0,
        "skips and acquisition never invent model scores"
    );
    // A URL reader replaces stale RSS text, then shares the exact retained read on reconciliation.
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM source_documents WHERE url='https://publisher.example/reader'")
        .execute(&pool)
        .await?;
    let reader = adapter::AcquireHandler::new(
        pool.clone(),
        Arc::new(WebBroker::test_reader(publisher.address)?),
    );
    seed(
        &pool,
        920019,
        "https://publisher.example/reader",
        Some("Stale RSS body."),
        0,
    )
    .await?;
    let item = claim(&pool, 920019).await?;
    reader.execute(&item).await?;
    let source: Value = sqlx::query_scalar(
        "SELECT source FROM classifier_sources WHERE article_id=920019 ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(source["reader_version"], crate::tools::reader::VERSION);
    assert!(source["body"]
        .as_str()
        .unwrap()
        .starts_with("Cedar FC reporting"));
    assert!(source["body"]
        .as_str()
        .unwrap()
        .ends_with("the player is available."));
    assert!(!source["body"].as_str().unwrap().contains("Stale RSS"));
    sqlx::query("UPDATE harvester_query_provenance SET feed_rank=3 WHERE article_id=920019")
        .execute(&pool)
        .await?;
    let item = claim(&pool, 920019).await?;
    reader.execute(&item).await?;
    assert_eq!(
        publisher.count("/https://publisher.example/reader"),
        1,
        "identical reads are retained across metadata reconciliation"
    );
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    Ok(())
}

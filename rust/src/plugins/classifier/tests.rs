use super::*;

#[tokio::test]
async fn acquisition_replay_requires_explicit_opt_in() {
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/classifier_test").unwrap();
    for setting in [None, Some("false"), Some(""), Some("TRUE"), Some("typo")] {
        assert!(adapter::acquisition_replay(&pool, setting).is_empty());
    }
    let operations = adapter::acquisition_replay(&pool, Some("true"));
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].name(), "classifier.acquisition-replay");
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn publisher_to_spectrum_to_character_is_complete_and_revision_bound() -> anyhow::Result<()> {
    use crate::harness::{dbtest, plugin::StudioPlugin, queue::work, tools::WebBroker};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
    };
    const SPORT: &str = "ZZ_KEV_PLUMBING";
    const ARTICLE: i64 = 910415;
    let pool = dbtest::pool("disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM harvester_query_provenance WHERE article_id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Kev fixture',2026) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query(
        "INSERT INTO teams(id,sport,name) VALUES(910415,$1,'Cedar FC') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'team',910415,'cedar fc','name')")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query("INSERT INTO persons(id,sport,full_name,kind,team_id) VALUES(910416,$1,'Morgan Vale','coach',910415) ON CONFLICT DO NOTHING")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'person',910416,'morgan vale','name')")
        .bind(SPORT).execute(&pool).await?;
    sqlx::query(
        "INSERT INTO teams(id,sport,name) VALUES(910417,$1,'Maple United') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO players(id,sport,name) VALUES(910418,$1,'Alex Reed') ON CONFLICT DO NOTHING",
    )
    .bind(SPORT)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'player',910418,'alex reed','name')")
        .bind(SPORT).execute(&pool).await?;
    let prose = format!(
        "Cedar FC reported an injury to Alex Reed before playing Maple United. Morgan Vale explained the update. {} A final correction says the player is available.",
        "The publisher supplied complete evidence. ".repeat(100)
    );
    let html = format!("<!doctype html><html><head><script>tracking()</script></head><body><nav>Buy now</nav><article><p>{prose}</p></article></body></html>");
    sqlx::query("INSERT INTO news_articles(id,url,title,source,full_text,published_at) VALUES($1,'https://example.invalid/kev','Cedar FC injury','Fixture Wire',$2,now())")
        .bind(ARTICLE).bind(html).execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',910415,$2)")
        .bind(ARTICLE).bind(SPORT).execute(&pool).await?;

    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',910417,$2)")
        .bind(ARTICLE).bind(SPORT).execute(&pool).await?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let (requests, received) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut request_number = 0;
        loop {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            if headers.starts_with("GET /shutdown ") {
                break;
            }
            if headers.starts_with("GET /v1/models ") {
                let body = serde_json::json!({"data":[{"id":format!("{}@sha256:{}",decision::MODEL_ID,"0".repeat(64))}]}).to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                continue;
            }
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
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            requests
                .send(
                    serde_json::from_slice::<Value>(&bytes[header_end..header_end + length])
                        .unwrap(),
                )
                .unwrap();
            let questions: Value = serde_json::from_str(include_str!(
                "../../../fixtures/classifier/classification-criteria-v1.json"
            ))
            .unwrap();
            let answers: serde_json::Map<String, Value> = questions["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), serde_json::json!({"type":"noul","noul":0.75})))
                .collect();
            let body = serde_json::json!({"model":format!("{}@sha256:{}", decision::MODEL_ID, "0".repeat(64)),"answers":answers}).to_string();
            let status = if request_number == 1 {
                "500 Internal Server Error"
            } else {
                "200 OK"
            };
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            request_number += 1;
        }
    });
    let decision = decision::Decision::new(format!("http://{address}/"))?;
    let acquire = adapter::AcquireHandler::new(pool.clone(), Arc::new(WebBroker::new(0)?));
    let mut source_ids = Vec::new();
    for revision in 0..3 {
        if revision == 1 {
            sqlx::query("UPDATE news_articles SET full_text=$2 WHERE id=$1")
                .bind(ARTICLE)
                .bind(format!("{prose} Additional publisher update."))
                .execute(&pool)
                .await?;
        }
        if revision == 2 {
            sqlx::query("UPDATE harvester_query_provenance SET feed_rank=7 WHERE article_id=$1 AND entity_id=910415")
                .bind(ARTICLE).execute(&pool).await?;
        }
        sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
            .bind(ARTICLE)
            .bind(SPORT)
            .execute(&pool)
            .await?;
        let item = work::claim(&pool, manifest::ACQUIRE_TASK, 100)
            .await?
            .into_iter()
            .find(|item| item.sport == SPORT)
            .unwrap();
        acquire.execute(&item).await?;
        let item = work::claim(&pool, manifest::TASK, 100)
            .await?
            .into_iter()
            .find(|item| item.sport == SPORT)
            .unwrap();
        source_ids.push(item.input_version.clone());
        if revision == 1 {
            let error = adapter::execute_spectrum(&pool, &item, &decision)
                .await
                .unwrap_err();
            assert!(format!("{error:#}").contains("500"));
            received.recv_timeout(std::time::Duration::from_secs(10))?;
        }
        adapter::execute_spectrum(&pool, &item, &decision).await?;
        let request = if revision == 2 {
            assert!(
                received.try_recv().is_err(),
                "RSS metadata changes must reuse the exact article measurement"
            );
            let receipt: String = sqlx::query_scalar("SELECT receipt::text FROM classifier_spectra WHERE source_id=$1 AND target->>'entity_type'='article'")
                .bind(item.input_version.as_deref().unwrap().parse::<i64>()?).fetch_one(&pool).await?;
            let receipt: Value = serde_json::from_str(&receipt)?;
            assert_eq!(receipt["cache_hit"], true);
            receipt["request"].clone()
        } else {
            received.recv_timeout(std::time::Duration::from_secs(10))?
        };
        assert_eq!(request["questions"].as_object().unwrap().len(), 4);
        assert_eq!(request["state"]["sport"], SPORT);
        assert!(
            request["state"].get("target").is_none(),
            "article inference must not depend on a target"
        );
        let sent = request["state"]["body"].as_str().unwrap();
        assert!(sent.contains("A final correction says the player is available."));
        assert!(!sent.contains('<') && !sent.contains("tracking()") && !sent.contains("Buy now"));
        let loaded = delivery::load_for_character(
            &pool,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            "team",
            910415,
            SPORT,
        )
        .await?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].context, sent);
        if revision == 0 {
            work::enqueue(
                &pool,
                &work::Item {
                    claim_token: None,
                    ..item.clone()
                },
            )
            .await?;
            let repeat = work::claim(&pool, manifest::TASK, 100)
                .await?
                .into_iter()
                .find(|i| i.sport == SPORT)
                .unwrap();
            adapter::execute_spectrum(&pool, &repeat, &decision).await?;
            assert!(
                received.try_recv().is_err(),
                "a committed article must not call Kev again for any target"
            );
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM classifier_spectra WHERE sport=$1")
                    .bind(SPORT)
                    .fetch_one(&pool)
                    .await?;
            assert_eq!(
                count, 3,
                "one article spectrum and two RSS-subject projections"
            );
            let player_deliveries: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND x.target->>'entity_type'='player'")
                .bind(SPORT).fetch_one(&pool).await?;
            assert_eq!(
                player_deliveries, 0,
                "incidental canonical mentions must not fan out character work"
            );
        }
    }
    let mut shutdown = std::net::TcpStream::connect(address)?;
    shutdown.write_all(b"GET /shutdown HTTP/1.1\r\nHost: localhost\r\n\r\n")?;
    server.join().unwrap();
    sqlx::query("UPDATE news_articles SET full_text='Transfer news. Sorry, this blog is currently unavailable. Back to Home.' WHERE id=$1")
        .bind(ARTICLE).execute(&pool).await?;
    sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
        .bind(ARTICLE)
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let unavailable = work::claim(&pool, manifest::ACQUIRE_TASK, 100)
        .await?
        .into_iter()
        .find(|i| i.sport == SPORT)
        .unwrap();
    let error = acquire.execute(&unavailable).await.unwrap_err();
    assert!(error.to_string().contains("no available article text"));
    // Restore the discovery bytes for the remaining publication-fence checks.
    sqlx::query("UPDATE news_articles SET full_text=$2 WHERE id=$1")
        .bind(ARTICLE)
        .bind(format!("{prose} Additional publisher update."))
        .execute(&pool)
        .await?;
    assert_ne!(source_ids[0], source_ids[1]);
    assert_ne!(source_ids[1], source_ids[2]);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectra WHERE sport=$1")
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        count, 9,
        "changed sources retain article measurements and projections"
    );
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT receipt::text,request_hash,clean_text,model FROM classifier_spectra WHERE sport=$1",
    )
    .bind(SPORT)
    .fetch_all(&pool)
    .await?;
    for row in rows {
        let receipt: Value = serde_json::from_str(row.get::<&str, _>("receipt"))?;
        assert_eq!(
            receipt["request"]["state"]["body"],
            row.get::<String, _>("clean_text")
        );
        assert_eq!(receipt["response"]["model"], row.get::<String, _>("model"));
        assert_eq!(
            hash(&receipt["request"].to_string()),
            row.get::<String, _>("request_hash")
        );
    }
    let errors: Vec<String> = sqlx::query_scalar(
        "SELECT receipt::text FROM classifier_measurements WHERE sport=$1 AND status='error'",
    )
    .bind(SPORT)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        errors.len(),
        1,
        "failed inference is retained without creating a spectrum"
    );
    let failure: Value = serde_json::from_str(&errors[0])?;
    assert!(failure["error"].as_str().unwrap().contains("500"));
    assert!(failure["raw_response"]
        .as_str()
        .unwrap()
        .contains("answers"));
    assert!(failure["request"]["state"]["body"]
        .as_str()
        .unwrap()
        .ends_with("Additional publisher update."));
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND d.status='pending'").bind(SPORT).fetch_one(&pool).await?;
    assert_eq!(
        pending, 8,
        "only the newest spectrum can dispatch each character"
    );
    let invalid = sqlx::query("INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
        SELECT source_id,article_id,sport,target,model||'-invalid',criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,'{}'::jsonb
        FROM classifier_spectra WHERE sport=$1 LIMIT 1")
        .bind(SPORT).execute(&pool).await.unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23514"),
        "the database must reject a new spectrum without its complete receipt"
    );
    sqlx::query("UPDATE classifier_source_heads SET source_id=$2 WHERE article_id=$1 AND sport=$3")
        .bind(ARTICLE)
        .bind(source_ids[0].as_deref().unwrap().parse::<i64>()?)
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let stale = delivery::load_for_character(
        &pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        "team",
        910415,
        SPORT,
    )
    .await
    .unwrap_err();
    assert!(
        stale.to_string().contains("source changed"),
        "a superseded acquired source must not reach a character"
    );
    sqlx::query("UPDATE classifier_source_heads SET source_id=$2 WHERE article_id=$1 AND sport=$3")
        .bind(ARTICLE)
        .bind(source_ids[2].as_deref().unwrap().parse::<i64>()?)
        .bind(SPORT)
        .execute(&pool)
        .await?;
    for (plugin, status) in [
        (
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            "used",
        ),
        (
            crate::plugins::insider::manifest::MANIFEST.id.as_str(),
            "abstained",
        ),
        (
            crate::plugins::scout::manifest::MANIFEST.id.as_str(),
            "held",
        ),
        (
            crate::plugins::influencer::manifest::MANIFEST.id.as_str(),
            "redundant",
        ),
    ] {
        let sources = delivery::load_for_character(&pool, plugin, "team", 910415, SPORT).await?;
        assert_eq!(sources.len(), 1);
        let ids: Vec<_> = sources.iter().map(|s| s.classification_id).collect();
        let mut tx = pool.begin().await?;
        delivery::validate_for_publication(&mut tx, plugin, "team", 910415, SPORT, &sources)
            .await?;
        delivery::settle(
            &mut tx,
            plugin,
            &ids,
            status,
            Some("fixture disposition"),
            serde_json::json!({"product_id":1}),
        )
        .await?;
        assert_eq!(
            delivery::undelivered_count(&mut tx, plugin, "team", 910415, SPORT).await?,
            0
        );
        tx.commit().await?;
        assert!(
            delivery::settle(
                &mut *pool.acquire().await?,
                plugin,
                &ids,
                status,
                None,
                serde_json::json!({})
            )
            .await
            .is_err(),
            "character acknowledgements must not consume an obligation twice"
        );
    }
    Ok(())
}

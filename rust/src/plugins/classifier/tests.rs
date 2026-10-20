use super::*;
use crate::harness::{
    plugin::{PluginOutcome, StudioPlugin},
    queue::work,
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
                    let rejected = payload["state"]["article"]["headline"]
                        .as_str()
                        .unwrap()
                        .contains("Unrelated")
                        || payload["state"]["subject_meta"]["identity"]["entity_id"] == 910417;
                    let answers=payload["questions"].as_object().unwrap().keys().map(|key|(key.clone(),serde_json::json!({"type":"noul","noul":if rejected {0.1}else{0.95}}))).collect::<serde_json::Map<_,_>>();
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
    assert_eq!(
        adapter::headline_replay(&pool, Some("true"))[0].name(),
        "classifier.admission-replay"
    );
}

const SPORT: &str = "ZZ_HEADLINE";
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
        PluginOutcome::Committed
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn headlines_archive_and_dispatch_without_publisher_io() -> Result<()> {
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
    sqlx::query("DELETE FROM news_articles WHERE id BETWEEN 910415 AND 910425")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Headline fixture',2026) ON CONFLICT DO NOTHING").bind(SPORT).execute(&pool).await?;
    for (id, name) in [(910415, "Cedar FC"), (910417, "Maple United")] {
        sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$2,$3) ON CONFLICT(id,sport) DO UPDATE SET name=EXCLUDED.name,city=NULL,venue_name=NULL").bind(id).bind(SPORT).bind(name).execute(&pool).await?;
        sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'team',$2,public.nrm($3),'name')").bind(SPORT).bind(id).bind(name).execute(&pool).await?;
    }
    let model = Model::new()?;
    let handler = adapter::ClassifierHandler::new(pool.clone(), model.decision()?);
    seed(
        &pool,
        910415,
        "Cedar FC appoints a coach",
        "Body must never be read or delivered.",
    )
    .await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(910415,'team',910417,$1)").bind(SPORT).execute(&pool).await?;
    admit(&pool, &handler, 910415).await?;
    assert_eq!(model.calls(), 2);
    let source: Value = sqlx::query_scalar("SELECT s.source FROM classifier_source_heads h JOIN classifier_sources s ON s.id=h.source_id WHERE h.article_id=910415 AND h.sport=$1").bind(SPORT).fetch_one(&pool).await?;
    assert_eq!(source["body"], "");
    assert_eq!(source["evidence_scope"], "headline");
    for request in model.requests.lock().unwrap().iter() {
        assert_eq!(request["questions"].as_object().unwrap().len(), 5);
        assert!(request["questions"].get("clickbait").is_some());
        assert!(request["state"]["article"].get("body").is_none());
    }
    let sources = delivery::load_for_character(
        &pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        "team",
        910415,
        SPORT,
    )
    .await?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].context, "Cedar FC appoints a coach");
    assert_eq!(
        sources[0].classifier_world.as_ref().unwrap()["spectrum"]["clickbait"],
        95.0
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.article_id=910415 AND d.status='pending'").fetch_one(&pool).await?,3,"only the admitted subject is delivered, including high clickbait");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pipeline_work WHERE stage='classifier_acquire' AND sport=$1"
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await?,
        0
    );
    let revision: String = sqlx::query_scalar("SELECT classifier_discovery_version(910415,$1)")
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
    sqlx::query("UPDATE news_articles SET full_text='A changed unseen article' WHERE id=910415")
        .execute(&pool)
        .await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT classifier_discovery_version(910415,$1)")
            .bind(SPORT)
            .fetch_one(&pool)
            .await?,
        revision
    );

    sqlx::query("UPDATE harvester_query_provenance SET feed_rank=7 WHERE article_id=910415")
        .execute(&pool)
        .await?;
    let queued: bool = sqlx::query_scalar("SELECT classifier_enqueue_admission(910415,$1)")
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
    assert!(!queued, "rank-only changes cannot reopen settled evidence");
    assert_eq!(model.calls(), 2);
    sqlx::query("UPDATE news_articles SET source='Cedar Bulletin' WHERE id=910415")
        .execute(&pool)
        .await?;
    admit(&pool, &handler, 910415).await?;
    assert_eq!(
        model.calls(),
        4,
        "publisher changes invalidate both requests"
    );
    assert_eq!(
        delivery::load_for_character(
            &pool,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            "team",
            910415,
            SPORT
        )
        .await?
        .len(),
        1,
        "old revisions are superseded"
    );

    seed(&pool, 910419, "Unrelated college tourism", "").await?;
    admit(&pool, &handler, 910419).await?;
    let rejected: i64 = sqlx::query_scalar(
        "SELECT id FROM classifier_spectra WHERE article_id=910419 ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    assert!(sqlx::query(
        "INSERT INTO classifier_spectrum_deliveries(spectrum_id,character) VALUES($1,'journalist')"
    )
    .bind(rejected)
    .execute(&pool)
    .await
    .is_err());

    seed(&pool, 910420, "Cedar FC update", "").await?;
    sqlx::query("SELECT classifier_enqueue_admission(910420,$1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let item = take(&pool, manifest::TASK, 910420).await?;
    model.fail.store(true, Ordering::Relaxed);
    assert!(handler.execute(&item).await.is_err());
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM classifier_measurements WHERE article_id=910420 AND status='error'").fetch_one(&pool).await?,1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM classifier_spectra WHERE article_id=910420"
        )
        .fetch_one(&pool)
        .await?,
        0,
        "a failure is not negative evidence"
    );
    work::release(&pool, &item).await?;
    admit(&pool, &handler, 910420).await?;

    seed(&pool, 910421, "Cedar FC update", "").await?;
    sqlx::query("SELECT classifier_enqueue_admission(910421,$1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let stale = take(&pool, manifest::TASK, 910421).await?;
    sqlx::query("UPDATE news_articles SET title='Cedar FC changed headline' WHERE id=910421")
        .execute(&pool)
        .await?;
    sqlx::query("SELECT classifier_enqueue_admission(910421,$1)")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    let calls = model.calls();
    assert!(matches!(
        handler.execute(&stale).await?,
        PluginOutcome::Superseded
    ));
    assert_eq!(model.calls(), calls, "revoked claims cannot call a model");

    for (id, days) in [(910423_i64, 3_i32), (910424, 10)] {
        seed(
            &pool,
            id,
            "Cedar FC earlier transfer report",
            "Unused old body",
        )
        .await?;
        sqlx::query("DELETE FROM harvester_query_provenance WHERE article_id=$1")
            .bind(id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO news_article_entities(article_id,entity_type,entity_id,sport) VALUES($1,'team',910415,$2)").bind(id).bind(SPORT).execute(&pool).await?;
        sqlx::query(
            "UPDATE news_articles SET published_at=now()-make_interval(days=>$2) WHERE id=$1",
        )
        .bind(id)
        .bind(days)
        .execute(&pool)
        .await?;
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT classifier_replay_admission(1000)")
            .fetch_one(&pool)
            .await?,
        2
    );
    for id in [910423_i64, 910424] {
        let old = take(&pool, manifest::TASK, id).await?;
        assert!(old
            .input_version
            .as_deref()
            .unwrap()
            .starts_with("headline-history:"));
        handler.execute(&old).await?;
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.article_id IN (910423,910424)").fetch_one(&pool).await?,0,"backscored history cannot flood the fresh edition");
    let subject = crate::tools::meta::EntityMeta {
        name: "Cedar FC".into(),
        entity_type: "team".into(),
        entity_id: 910415,
        sport: SPORT.into(),
    };
    let fresh = delivery::load_for_character(
        &pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        "team",
        910415,
        SPORT,
    )
    .await?;
    let memories =
        super::memories::load_on(&mut *pool.acquire().await?, &subject, &fresh, "journalist")
            .await?;
    assert!(memories.iter().any(|m| m.article_id == 910423));
    assert!(!memories.iter().any(|m| m.article_id == 910424));
    assert!(memories
        .iter()
        .all(|m| !fresh.iter().any(|f| f.article_id == m.article_id)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT classifier_replay_admission(1000)")
            .fetch_one(&pool)
            .await?,
        0,
        "backscoring is restart-safe"
    );
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn team_rss_extracts_known_roles_and_recovers_reverted_headlines() -> Result<()> {
    let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    let sport = "ZZ_SUBJECTS";
    sqlx::query("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=920001")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Headline subjects',2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(920001,$1,'Cedar Sample') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO players(id,sport,name) VALUES(920001,$1,'Alex Sample') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    let mut headline = "Cedar Sample and Alex Sample welcome".to_string();
    for (index, kind) in [
        "coach",
        "agent",
        "owner",
        "executive",
        "family",
        "official",
        "other",
    ]
    .iter()
    .enumerate()
    {
        let name = format!("Casey {kind}");
        let id = 920001 + index as i32;
        sqlx::query("INSERT INTO persons(id,sport,full_name,kind) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET sport=EXCLUDED.sport,full_name=EXCLUDED.full_name,kind=EXCLUDED.kind")
            .bind(id).bind(sport).bind(&name).bind(kind).execute(&pool).await?;
        // An unambiguous stored alias resolves to the same canonical subject.
        let surface = if *kind == "agent" {
            "Casey Rep".to_string()
        } else {
            name.clone()
        };
        headline.push_str(&format!(" {surface},"));
        sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('person',$1,$2,nrm($3),$4)")
            .bind(id).bind(sport).bind(surface).bind(if *kind == "agent" { "alias" } else { "name" }).execute(&pool).await?;
    }
    headline.push_str(" alongside Unknown Visitor");
    sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('player',920001,$1,nrm('Alex Sample'),'name')")
        .bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(920001,'https://example.invalid/subjects',md5('subjects'),$1,'Fixture Wire',now())")
        .bind(&headline).execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(920001,'team',920001,$1)").bind(sport).execute(&pool).await?;
    let model = Model::new()?;
    let handler = adapter::ClassifierHandler::new(pool.clone(), model.decision()?);
    async fn score(pool: &PgPool, handler: &adapter::ClassifierHandler, sport: &str) -> Result<()> {
        let queued: bool = sqlx::query_scalar("SELECT classifier_enqueue_admission(920001,$1)")
            .bind(sport)
            .fetch_one(pool)
            .await?;
        assert!(queued);
        let item = work::claim(pool, manifest::TASK, 1).await?.remove(0);
        assert_eq!(item.entity_id, 920001);
        assert!(matches!(
            handler.execute(&item).await?,
            PluginOutcome::Committed
        ));
        Ok(())
    }
    score(&pool, &handler, sport).await?;
    assert_eq!(
        model.calls(),
        9,
        "team, player and all seven known person roles are scored; unknowns are not created"
    );
    let old_head: i64 = sqlx::query_scalar(
        "SELECT source_id FROM classifier_source_heads WHERE article_id=920001 AND sport=$1",
    )
    .bind(sport)
    .fetch_one(&pool)
    .await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.source_id=$1 AND d.status='pending'").bind(old_head).fetch_one(&pool).await?;
    assert_eq!(pending, 27);
    for kind in ["team", "player", "person"] {
        let reports = delivery::load_for_character(
            &pool,
            crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
            kind,
            920001,
            sport,
        )
        .await?;
        assert_eq!(
            reports.len(),
            1,
            "the same numeric ID must stay separate across entity types"
        );
        assert_eq!(
            crate::tools::meta::lookup_entity_name(&pool, kind, 920001, sport)
                .await?
                .is_empty(),
            false
        );
    }
    sqlx::query("UPDATE news_articles SET title=title||' update' WHERE id=920001")
        .execute(&pool)
        .await?;
    score(&pool, &handler, sport).await?;
    assert_eq!(model.calls(), 18);
    sqlx::query("UPDATE news_articles SET title=$1 WHERE id=920001")
        .bind(&headline)
        .execute(&pool)
        .await?;
    score(&pool, &handler, sport).await?;
    assert_eq!(model.calls(), 18, "reverting reuses the archived decisions");
    let head: i64 = sqlx::query_scalar(
        "SELECT source_id FROM classifier_source_heads WHERE article_id=920001 AND sport=$1",
    )
    .bind(sport)
    .fetch_one(&pool)
    .await?;
    assert_eq!(head, old_head);
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.source_id=$1 AND d.status='pending'").bind(head).fetch_one(&pool).await?;
    assert_eq!(pending, 27, "reverted pending obligations must be restored");
    sqlx::query("UPDATE classifier_spectrum_deliveries d SET status='used' FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.source_id=$1").bind(head).execute(&pool).await?;
    sqlx::query("UPDATE harvester_query_provenance SET feed_rank=0 WHERE article_id=920001")
        .execute(&pool)
        .await?;
    let queued: bool = sqlx::query_scalar("SELECT classifier_enqueue_admission(920001,$1)")
        .bind(sport)
        .fetch_one(&pool)
        .await?;
    assert!(!queued, "rank changes never reopen used evidence");
    // A repaired canonical name index can be reconciled even when RSS no longer returns the URL.
    sqlx::query("DELETE FROM entity_name_surfaces WHERE entity_type='person' AND entity_id=920007 AND sport=$1").bind(sport).execute(&pool).await?;
    let replayed: i64 = sqlx::query_scalar("SELECT classifier_replay_admission(1)")
        .fetch_one(&pool)
        .await?;
    assert_eq!(replayed, 1, "replay repairs a stale current revision");
    let item = work::claim(&pool, manifest::TASK, 1).await?.remove(0);
    handler.execute(&item).await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.article_id=920001 AND d.status='pending'").fetch_one(&pool).await?;
    assert_eq!(
        pending, 0,
        "candidate-only revisions do not repeat settled subject evidence"
    );
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable migrated TEST_DATABASE_URL"]
async fn embedded_unknown_names_remain_unresolved_and_batch_checks_are_identical() -> Result<()> {
    use sqlx::Acquire;
    let pool = crate::harness::dbtest::pool("disposable identity boundary database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    let sport = "ZZ_IDENTITY_BOUNDARY";
    sqlx::query("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=920805")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,$1,2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(920801,$1,'Chelsea') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO players(id,sport,name,first_name,last_name) VALUES(920802,$1,'William','William','de Asevedo Furtado') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',920801,$1,'chelsea','name'),('player',920802,$1,'william','name')").bind(sport).execute(&pool).await?;
    let mut connection = pool.acquire().await?;
    let mut sources = Vec::new();
    for (title, expected) in [
        (
            "GPL: William Bediako joins Atlético Madrid from Berekum Chelsea",
            0,
        ),
        ("william bediako joins atletico from berekum chelsea", 0),
        ("Chelsea Women sign a forward", 0),
        ("Former Chelsea midfielder returns", 1),
        ("Chelsea deny talks for William", 2),
        ("William scores; Chelsea win", 2),
    ] {
        let mut source = Source {
            article_id: 1,
            body: String::new(),
            source: "Wire".into(),
            published_at: None,
            query_entities: vec![],
            provenance: std::collections::BTreeMap::from([(
                "title".into(),
                serde_json::json!(title),
            )]),
        };
        let found = super::identity::candidates(&mut connection, sport, &source).await?;
        assert_eq!(
            found.as_array().unwrap().len(),
            expected,
            "{title}: {found}"
        );
        source
            .provenance
            .insert("identity_candidates".into(), found);
        sources.push(source);
    }
    assert_eq!(
        super::identity::candidates_for(&mut connection, sport, &sources).await?,
        sources
            .iter()
            .map(|s| s.provenance["identity_candidates"].clone())
            .collect::<Vec<_>>()
    );
    let mut tx = connection.begin().await?;
    super::identity::validate_all(&mut tx, sport, &sources).await?;
    sources[0]
        .provenance
        .insert("identity_candidates".into(), serde_json::json!([]));
    let wrong = sources[4].provenance["identity_candidates"].clone();
    sources[1]
        .provenance
        .insert("identity_candidates".into(), wrong);
    assert!(super::identity::validate_all(&mut tx, sport, &sources)
        .await
        .is_err());
    tx.rollback().await?;
    drop(connection);
    sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(920805,'https://example.invalid/identity-boundary',md5('identity-boundary'),'GPL: William Bediako joins Atletico from Berekum Chelsea','Wire',now()-interval '3 days')").execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(920805,'team',920801,$1)").bind(sport).execute(&pool).await?;
    sqlx::query("SELECT classifier_enqueue_admission(920805,$1)")
        .bind(sport)
        .execute(&pool)
        .await?;
    let scorer = Model::new()?;
    let handler = adapter::ClassifierHandler::new(pool.clone(), scorer.decision()?);
    let mut selected = None;
    for item in work::claim(&pool, manifest::TASK, 100).await? {
        if item.entity_id == 920805 && item.sport == sport {
            selected = Some(item);
        } else {
            work::release(&pool, &item).await?;
        }
    }
    handler.execute(&selected.unwrap()).await?;
    let subject = crate::tools::meta::EntityMeta {
        name: "Chelsea".into(),
        entity_type: "team".into(),
        entity_id: 920801,
        sport: sport.into(),
    };
    for plugin in [
        crate::plugins::journalist::manifest::MANIFEST.id,
        crate::plugins::influencer::manifest::MANIFEST.id,
        crate::plugins::insider::manifest::MANIFEST.id,
    ] {
        let delivered =
            delivery::load_for_character(&pool, plugin.as_str(), "team", 920801, sport).await?;
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            delivered[0].classifier_world.as_ref().unwrap()["target_identity_resolved"],
            false,
            "query provenance and high model relevance must not certify target identity"
        );
    }
    let now = crate::plugins::influencer::now();
    let fresh = [crate::tools::source::SourceContext {
        classification_id: 0,
        article_id: 920899,
        headline: "Chelsea fresh reporting".into(),
        context: "Chelsea fresh reporting".into(),
        source: "Wire".into(),
        published_at_epoch: Some(now),
        classifier_world: None,
    }];
    assert!(
        super::memories::load_on(&mut *pool.acquire().await?, &subject, &fresh, "journalist")
            .await?
            .is_empty(),
        "another club's archived headline must not return as memory"
    );
    sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable migrated TEST_DATABASE_URL and SCORACLE_MEMORY_STUDY_BIN"]
async fn exact_token_overflow_drops_history_then_holds_each_complete_character_packet() -> Result<()>
{
    use crate::harness::model::{ContextExceeded, GenerateOptions, GenerateResult, Inference};
    struct Budget(std::sync::Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl Inference for Budget {
        async fn generate(
            &self,
            prompt: &str,
            _: &GenerateOptions,
        ) -> Result<(GenerateResult, Value)> {
            self.0.lock().unwrap().push(prompt.into());
            Err(ContextExceeded {
                input: 6310,
                reserved: 2100,
                context: 7900,
            }
            .into())
        }
        fn model(&self) -> &str {
            "budget-test"
        }
        fn request_body(&self, _: &str, _: &GenerateOptions) -> Value {
            serde_json::json!({})
        }
    }
    let pool = crate::harness::dbtest::pool("disposable admission database").await;
    plumbing_tests::setup_disposable(&pool).await?;
    let sport = "ZZ_TOKEN_ADMISSION";
    sqlx::query("UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id IN (920902,920903)")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,$1,2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES(920901,$1,'Budget Club') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
    sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',920901,$1,'budget club','name')").bind(sport).execute(&pool).await?;
    let scorer = Model::new()?;
    let classifier = adapter::ClassifierHandler::new(pool.clone(), scorer.decision()?);
    for (id, days, title) in [
        (920902_i64, 3, "Budget Club deny talks"),
        (920903, 0, "Budget Club celebrate a new transfer report"),
    ] {
        sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES($1,'https://example.invalid/'||$1,md5($1::text),$2,'Fixture Wire',now()-make_interval(days=>$3))").bind(id).bind(title).bind(days).execute(&pool).await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',920901,$2)").bind(id).bind(sport).execute(&pool).await?;
        sqlx::query("SELECT classifier_enqueue_admission($1,$2)")
            .bind(id)
            .bind(sport)
            .execute(&pool)
            .await?;
        if days > 0 {
            sqlx::query("UPDATE pipeline_work SET input_version='headline-history:'||input_version WHERE stage='classifier' AND entity_id=$1 AND sport=$2").bind(id).bind(sport).execute(&pool).await?;
        }
        let mut selected = None;
        for item in work::claim(&pool, manifest::TASK, 100).await? {
            if item.entity_id == id && item.sport == sport {
                selected = Some(item);
            } else {
                work::release(&pool, &item).await?;
            }
        }
        classifier.execute(&selected.unwrap()).await?;
    }
    for stage in [
        crate::plugins::journalist::manifest::TASK,
        crate::plugins::influencer::manifest::TASK,
        crate::plugins::insider::manifest::TASK,
    ] {
        let mut selected = None;
        for item in work::claim(&pool, stage, 100).await? {
            if item.entity_id == 920901 && item.sport == sport {
                selected = Some(item);
            } else {
                work::release(&pool, &item).await?;
            }
        }
        let item = selected.expect("daily character obligation");
        let backend = Budget(std::sync::Mutex::new(Vec::new()));
        let result = match stage.as_str() {
            "narratives" => {
                crate::plugins::journalist::execute_with_backend(&pool, &backend, 7900, &item)
                    .await?
            }
            "vibe" => {
                crate::plugins::influencer::execute_with_backend(&pool, &backend, 7900, &item)
                    .await?
            }
            _ => {
                crate::plugins::insider::execute_with_backend(&pool, &backend, 7900, &item).await?
            }
        };
        assert_eq!(result, PluginOutcome::Committed);
        let prompts = backend.0.lock().unwrap();
        assert_eq!(
            prompts.len(),
            2,
            "{} must attempt admission once with history and once without",
            stage
        );
        let first: Value = serde_json::from_str(&prompts[0])?;
        let second: Value = serde_json::from_str(&prompts[1])?;
        assert_eq!(first["memories"].as_array().unwrap().len(), 1);
        assert!(second["memories"].as_array().unwrap().is_empty());
        assert_eq!(
            first["fresh"], second["fresh"],
            "complete fresh evidence changed"
        );
    }
    let held:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND d.status='held'").bind(sport).fetch_one(&pool).await?;
    assert_eq!(held, 3);
    let failed:i64=sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage IN ('narratives','vibe','transfers')").bind(sport).fetch_one(&pool).await?;
    assert_eq!(
        failed, 0,
        "permanent token admission must not enter the retry queue"
    );
    sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
        .bind(sport)
        .execute(&pool)
        .await?;
    Ok(())
}

use super::*;
/// 7.13 (G1): the Editor hands graph its article ONLY on the packet rail. Under `RAIL=legacy`
/// The cutover blocker measured 2026-08-06: three Editor rows at attempts≥5, one per day, all
/// `persist article full_text <id>: invalid byte sequence for encoding "UTF8": 0x00`. A NUL in a
/// scraped body is not storable in a Postgres text column at all, so the write can only ever
/// fail — and §2 clause 4 needs the dead-letter count at 0 for seven consecutive days, which one
/// arrival per day resets forever. The body is sanitised where it enters the Editor, so the same
/// clean text is what gets hashed, prompted, sliced for candidate evidence, and persisted.
#[test]
fn a_body_carrying_nul_is_sanitised_before_it_can_reach_a_text_column() {
    let fetched = FetchedArticle {
        final_url: "https://example.com/a".to_string(),
        final_domain: Some("example.com".to_string()),
        text: "Vinicius \u{0}Junior is \u{0}staying at Real Madrid.\u{0}".to_string(),
    };
    assert!(
        fetched.text.contains('\0'),
        "the fixture must carry the byte under test"
    );

    let clean = sanitize_fetched(fetched);
    assert!(
        !clean.text.contains('\0'),
        "no NUL may survive into full_text"
    );
    assert_eq!(clean.text, "Vinicius Junior is staying at Real Madrid.");
    // The derivations that ride the same body must be computable on it.
    assert_eq!(count_words(&clean.text), 7);
    assert!(!content_hash(&clean.text).is_empty());
    // The URL columns are bound from the same struct and must survive untouched.
    assert_eq!(clean.final_url, "https://example.com/a");
    assert_eq!(clean.final_domain.as_deref(), Some("example.com"));
}

/// Sanitisation must be invisible to the 99.99% of bodies that carry no NUL: same bytes in, same
/// bytes out, so no prompt and no `content_hash` moves for an ordinary article.
#[test]
fn a_body_without_nul_passes_through_byte_identical() {
    let body = "Arsenal have agreed a fee.\n\nThe deal is not done.\t— sources";
    let clean = sanitize_fetched(FetchedArticle {
        final_url: "https://example.com/b".to_string(),
        final_domain: None,
        text: body.to_string(),
    });
    assert_eq!(clean.text, body);
    assert_eq!(content_hash(&clean.text), content_hash(body));
}

mod postgres_tests {
    use super::*;
    use crate::application::queue::work;
    use crate::studio::Parser;
    const SPORT: &str = "ZZ_EDITOR_FENCE";
    const ARTICLE: i64 = 9_400_001;
    const TEAM: i32 = 9_400_002;

    async fn setup() -> PgPool {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .connect(
                &std::env::var("TEST_DATABASE_URL")
                    .expect("isolated database through migration 261"),
            )
            .await
            .unwrap();
        // Tests run serially. Remove only this fixture's data; preserve unrelated suites.
        for sql in [
            "DELETE FROM pipeline_work WHERE sport = $1",
            "DELETE FROM data_fetch_ledger WHERE sport = $1",
            "DELETE FROM storylines WHERE sport = $1",
            "DELETE FROM entity_candidates WHERE sport = $1",
            "DELETE FROM fixtures WHERE sport = $1",
            "DELETE FROM teams WHERE sport = $1",
            "DELETE FROM entity_name_surfaces WHERE sport = $1",
        ] {
            sqlx::query(sql).bind(SPORT).execute(&pool).await.unwrap();
        }
        sqlx::query("DELETE FROM news_articles WHERE id = $1")
            .bind(ARTICLE)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1,'Editor test',2026) ON CONFLICT DO NOTHING").bind(SPORT).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO teams (id, sport, name) VALUES ($1,$2,'Editor Test Club')")
            .bind(TEAM)
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind) VALUES ('team',$1,$2,public.nrm('Editor Test Club'),'name')").bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO news_articles (id,url_hash,url,title,source) VALUES ($1,'editor-fence','https://example.test/editor','Editor story','Wire')").bind(ARTICLE).execute(&pool).await.unwrap();
        pool
    }
    fn pending(revision: &str) -> Item {
        Item {
            stage: crate::plugins::editor::manifest::TASK,
            entity_type: "article".into(),
            entity_id: ARTICLE,
            sport: SPORT.into(),
            input_version: Some(revision.into()),
            attempts: 0,
            claim_token: None,
        }
    }
    async fn claim(pool: &PgPool) -> Item {
        let mut claims = work::claim(pool, crate::plugins::editor::manifest::TASK, 1)
            .await
            .unwrap();
        assert_eq!(claims.len(), 1);
        claims.remove(0)
    }
    fn prepared(page_kind: &str) -> Prepared {
        let raw = json!({"source_language":"en", "page_kind":page_kind,
            "names":[{"name":"Editor Test Club","kind_hint":"club","descriptor":"football club"},
                     {"name":"New Test Player","kind_hint":"person","descriptor":"striker at the club"}],
            "entity_roles":[{"entity":"Editor Test Club","role":"subject"}],
            "story_type":"roster", "key_facts":["New Test Player joins Editor Test Club."],
            "evidence_blurb":"The club signed a striker."}).to_string();
        let read = crate::plugins::editor::cognition::EditorReadParser { hypothesis: &[] }
            .parse(&raw)
            .unwrap();
        let text = "New Test Player joins Editor Test Club.".to_string();
        Prepared::Read {
            article: EditorArticleRow {
                url: "https://example.test/editor".into(),
                source: "Wire".into(),
                title: "Editor story".into(),
                description: String::new(),
                duplicate_of: None,
            },
            body_hash: content_hash(&text),
            fetched: FetchedArticle {
                text,
                final_url: "https://example.test/editor".into(),
                final_domain: Some("example.test".into()),
            },
            extracted: Box::new(Extracted {
                value: read,
                raw_response: raw,
                model: "test-editor".into(),
                built_prompt: "prepared evidence".into(),
                request_body: json!({}),
                eval_count: 1,
                wall_ms: 1,
            }),
        }
    }
    async fn count(pool: &PgPool, table: &str, predicate: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE {predicate}"))
            .bind(ARTICLE)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn assert_unpublished(pool: &PgPool) {
        assert_eq!(count(pool, "editor_reads", "article_id=$1").await, 0);
        assert_eq!(count(pool, "data_fetch_ledger", "target_id=$1").await, 0);
        assert_eq!(
            count(pool, "news_article_entities", "article_id=$1").await,
            0
        );
        assert_eq!(count(pool, "candidate_mentions", "article_id=$1").await, 0);
        let text: Option<String> =
            sqlx::query_scalar("SELECT full_text FROM news_articles WHERE id=$1")
                .bind(ARTICLE)
                .fetch_one(pool)
                .await
                .unwrap();
        assert!(text.is_none());
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn read_links_nominations_story_and_graph_commit_once() {
        let pool = setup().await;
        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let item = claim(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &item, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(count(&pool,"editor_reads","article_id=$1 AND status='success' AND model_version='test-editor' AND contract_version='ep8'").await,1);
        assert_eq!(
            count(&pool, "news_article_entities", "article_id=$1").await,
            1
        );
        assert_eq!(count(&pool, "candidate_mentions", "article_id=$1").await, 1);
        assert_eq!(count(&pool, "storyline_articles", "article_id=$1").await, 1);
        assert_eq!(
            count(&pool, "pipeline_work", "entity_id=$1 AND stage='graph'").await,
            1
        );
        let investigations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='investigate_entity'",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(investigations, 1);
        assert_eq!(
            count(&pool, "pipeline_work", "entity_id=$1 AND stage='editor'").await,
            0
        );
        assert_eq!(
            commit_claimed(&pool, &item, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Superseded
        );
        assert_eq!(count(&pool, "data_fetch_ledger", "target_id=$1").await, 1);
        pool.close().await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn revision_and_reclaim_fence_every_editor_effect() {
        let pool = setup().await;
        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let stale = claim(&pool).await;
        work::enqueue(&pool, &pending("v2")).await.unwrap();
        assert_eq!(
            commit_claimed(&pool, &stale, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Superseded
        );
        assert_unpublished(&pool).await;
        let old = claim(&pool).await;
        work::release(&pool, &old).await.unwrap();
        let current = claim(&pool).await;
        assert_ne!(old.claim_token, current.claim_token);
        assert_eq!(
            commit_claimed(&pool, &old, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Superseded
        );
        assert_unpublished(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &current, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(
            commit_claimed(
                &pool,
                &stale,
                &Prepared::Terminal {
                    status: "fetch_failed",
                    fetched: None,
                    error: None
                }
            )
            .await
            .unwrap(),
            PluginOutcome::Superseded
        );
        assert_eq!(
            count(&pool, "editor_reads", "article_id=$1 AND status='success'").await,
            1
        );
        pool.close().await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn provenance_failure_rolls_back_all_effects_and_retry_recovers() {
        let pool = setup().await;
        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let item = claim(&pool).await;
        // Fail after read, candidates, story, links and graph have been written.
        sqlx::query("ALTER TABLE data_fetch_ledger ADD CONSTRAINT editor_test_failure CHECK (target_id <> 9400001)").execute(&pool).await.unwrap();
        let result = commit_claimed(&pool, &item, &prepared("article")).await;
        sqlx::query("ALTER TABLE data_fetch_ledger DROP CONSTRAINT editor_test_failure")
            .execute(&pool)
            .await
            .unwrap();
        assert!(result.is_err());
        assert_unpublished(&pool).await;
        assert_eq!(
            count(
                &pool,
                "pipeline_work",
                "entity_id=$1 AND stage='editor' AND status='running'"
            )
            .await,
            1
        );
        let effects: i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM storylines WHERE sport=$1) + (SELECT count(*) FROM entity_candidates WHERE sport=$1) + (SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage <> 'editor')").bind(SPORT).fetch_one(&pool).await.unwrap();
        assert_eq!(effects, 0);
        assert_eq!(
            commit_claimed(&pool, &item, &prepared("article"))
                .await
                .unwrap(),
            PluginOutcome::Committed
        );
        pool.close().await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn irrelevant_read_clears_links_and_unchanged_work_only_completes() {
        let pool = setup().await;
        sqlx::query("INSERT INTO news_article_entities (article_id,entity_type,entity_id,sport) VALUES ($1,'team',$2,$3)").bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
        work::enqueue(&pool, &pending("v1")).await.unwrap();
        let item = claim(&pool).await;
        let output = prepared("roundup");
        assert_eq!(
            commit_claimed(&pool, &item, &output).await.unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(
            count(
                &pool,
                "editor_reads",
                "article_id=$1 AND status='irrelevant'"
            )
            .await,
            1
        );
        assert_eq!(
            count(&pool, "news_article_entities", "article_id=$1").await,
            0
        );
        assert_eq!(count(&pool, "candidate_mentions", "article_id=$1").await, 0);
        assert_eq!(
            count(&pool, "pipeline_work", "entity_id=$1 AND stage='graph'").await,
            0
        );
        if let Prepared::Read { body_hash, .. } = &output {
            assert!(read_is_current(&pool, ARTICLE, body_hash).await.unwrap());
        }
        work::enqueue(&pool, &pending("v2")).await.unwrap();
        let item = claim(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &item, &Prepared::Unchanged)
                .await
                .unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(count(&pool, "data_fetch_ledger", "target_id=$1").await, 1);
        pool.close().await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn terminal_and_parser_markers_complete_with_honest_provenance() {
        let pool = setup().await;
        for (n, status) in [
            "duplicate",
            "blocked",
            "fetch_failed",
            "paywall",
            "empty_body",
        ]
        .iter()
        .enumerate()
        {
            work::enqueue(&pool, &pending(&format!("v{n}")))
                .await
                .unwrap();
            let item = claim(&pool).await;
            assert_eq!(
                commit_claimed(
                    &pool,
                    &item,
                    &Prepared::Terminal {
                        status,
                        fetched: None,
                        error: None
                    }
                )
                .await
                .unwrap(),
                PluginOutcome::Committed
            );
            let saved: (String, Option<String>, String) = sqlx::query_as(
                "SELECT status,model_version,parser_outcome FROM editor_reads WHERE article_id=$1",
            )
            .bind(ARTICLE)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(saved, (status.to_string(), None, "no_call".into()));
        }
        let mut output = prepared("article");
        if let Prepared::Read { extracted, .. } = &mut output {
            extracted.value = None;
        }
        work::enqueue(&pool, &pending("parser")).await.unwrap();
        let item = claim(&pool).await;
        assert_eq!(
            commit_claimed(&pool, &item, &output).await.unwrap(),
            PluginOutcome::Committed
        );
        assert_eq!(count(&pool,"editor_reads","article_id=$1 AND status='parse_failed' AND model_version='test-editor' AND parser_outcome='fail_closed' AND content_hash IS NOT NULL").await,1);
        assert_eq!(count(&pool, "pipeline_work", "entity_id=$1").await, 0);
        pool.close().await;
    }
    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn fixture_nomination_rolls_back_with_read_then_persists_without_retired_boxscore_work() {
        let pool = setup().await;
        sqlx::query("INSERT INTO teams (id,sport,name) VALUES ($1,$2,'Editor Away Club')")
            .bind(TEAM + 1)
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entity_name_surfaces (entity_type,entity_id,sport,norm,surface_kind) VALUES ('team',$1,$2,public.nrm('Editor Away Club'),'name')").bind(TEAM+1).bind(SPORT).execute(&pool).await.unwrap();
        let mut output = prepared("article");
        if let Prepared::Read { extracted, .. } = &mut output {
            extracted.value.as_mut().unwrap().result_line =
                "Editor Test Club 2-1 Editor Away Club".into();
        }
        work::enqueue(&pool, &pending("fixture")).await.unwrap();
        let item = claim(&pool).await;
        sqlx::query("ALTER TABLE data_fetch_ledger ADD CONSTRAINT editor_fixture_failure CHECK (target_id <> 9400001)").execute(&pool).await.unwrap();
        let result = commit_claimed(&pool, &item, &output).await;
        sqlx::query("ALTER TABLE data_fetch_ledger DROP CONSTRAINT editor_fixture_failure")
            .execute(&pool)
            .await
            .unwrap();
        assert!(result.is_err());
        assert_unpublished(&pool).await;
        let fixtures: i64 = sqlx::query_scalar("SELECT count(*) FROM fixtures WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fixtures, 0);
        assert_eq!(
            commit_claimed(&pool, &item, &output).await.unwrap(),
            PluginOutcome::Committed
        );
        let fixture: (i32,i32,i32,i32,bool) = sqlx::query_as("SELECT home_team_id,away_team_id,home_score,away_score,(meta->>'needs_verification')::boolean FROM fixtures WHERE sport=$1").bind(SPORT).fetch_one(&pool).await.unwrap();
        assert_eq!(fixture, (TEAM, TEAM + 1, 2, 1, true));
        let work: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='fixture_boxscore'",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(work, 0);
        pool.close().await;
    }
}

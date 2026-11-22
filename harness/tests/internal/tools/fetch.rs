#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn cached_publisher_excerpt_must_match_the_complete_document() -> Result<()> {
    let pool = publisher_test_pool().await?;
    let url = "https://example.invalid/kev-cache-integrity";
    sqlx::query("DELETE FROM source_documents WHERE url=$1")
        .bind(url)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO source_documents(url,content_hash,http_status,retained_excerpt) VALUES($1,$2,200,$3)")
        .bind(url).bind(content_hash("Complete article. Later correction.")).bind("Complete article.")
        .execute(&pool).await?;
    let fetcher = BudgetedFetcher::new()?;
    assert!(fetcher
        .cached(&pool, url, Duration::from_secs(60))
        .await?
        .is_none());
    sqlx::query("UPDATE source_documents SET retained_excerpt='Complete article. Later correction.' WHERE url=$1")
        .bind(url).execute(&pool).await?;
    assert_eq!(
        fetcher
            .cached(&pool, url, Duration::from_secs(60))
            .await?
            .unwrap()
            .body,
        "Complete article. Later correction."
    );
    Ok(())
}

#[tokio::test]
#[ignore = "local_real_model: requires disposable TEST_DATABASE_URL and TEST_PUBLISHER_URL"]
async fn live_google_news_fetch_retains_publisher_prose() -> Result<()> {
    let pool = publisher_test_pool().await?;
    let url = std::env::var("TEST_PUBLISHER_URL")?;
    assert!(
        google_news_article_id(&url).is_some(),
        "Google News wrapper required"
    );
    let fetched = BudgetedFetcher::new()?
        .fetch(&pool, &url, &FetchPolicy::default())
        .await
        .map_err(|error| anyhow!("{error}"))?;
    assert_ne!(fetched.domain.as_deref(), Some("news.google.com"));
    let prose = extract_article_text(&fetched.body);
    assert!(count_words(&prose) >= ARTICLE_MIN_WORDS);
    assert!(!prose.trim_start().starts_with('<'));
    Ok(())
}

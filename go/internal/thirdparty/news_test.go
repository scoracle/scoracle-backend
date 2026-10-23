package thirdparty

import (
	"context"
	"net/http"
	"os"
	"path/filepath"

	"github.com/jackc/pgx/v5/pgxpool"
	"strings"
	"testing"
	"time"
)

func TestRSSWhenTokenSpansSubDayAndDayWindows(t *testing.T) {
	for _, tc := range []struct {
		hours int
		want  string
	}{
		{1, "1h"},
		{12, "12h"},
		{49, "3d"},
		{1000, "42d"},
		{24, "1d"},
	} {
		if got := rssWhenToken(tc.hours); got != tc.want {
			t.Fatalf("rssWhenToken(%d) = %q, want %q", tc.hours, got, tc.want)
		}
	}
}

// buildRSSSearchQueries is now one lane per name we know the entity by — canonical name first,
// then every alias in DB order, each with the sport term, deduped on a normalized key. Nothing
// is scored, capped, or allow-listed (PLAN-one-rail 8.9). This test pins that contract; the
// eight tests it replaces asserted the alias SCORING that 8.9 deleted.
func TestBuildRSSSearchQueries_EveryNameGetsALane(t *testing.T) {
	got := buildRSSSearchQueries("Manchester United", "FOOTBALL",
		[]string{"MUN", "Man UTD", "Man United", "MUFC"})
	want := []string{
		`"Manchester United" soccer football`,
		`MUN soccer football`,
		`"Man UTD" soccer football`,
		`"Man United" soccer football`,
		`MUFC soccer football`,
	}
	if len(got) != len(want) {
		t.Fatalf("buildRSSSearchQueries = %#v, want %#v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("lane %d = %q, want %q", i, got[i], want[i])
		}
	}
}

// The short, ambiguous aliases the old allow-list existed to suppress now get a lane like any
// other name. That is the point: the sport term is what disambiguates them for Google, and the
// Editor rejects whatever still slips through.
func TestBuildRSSSearchQueries_KeepsShortAndRiskyAliases(t *testing.T) {
	got := buildRSSSearchQueries("OGC Nice", "FOOTBALL", []string{"Nice", "OGCN"})
	want := []string{`"OGC Nice" soccer football`, `Nice soccer football`, `OGCN soccer football`}
	for i := range want {
		if i >= len(got) || got[i] != want[i] {
			t.Fatalf("buildRSSSearchQueries = %#v, want %#v", got, want)
		}
	}
}

// Asking Google the same question twice buys the same page twice. Case and spacing differences
// are the same question.
func TestBuildRSSSearchQueries_DedupesEquivalentNames(t *testing.T) {
	got := buildRSSSearchQueries("Arsenal", "FOOTBALL", []string{"arsenal", "  Arsenal  ", ""})
	if len(got) != 1 || got[0] != "Arsenal soccer football" {
		t.Fatalf("buildRSSSearchQueries = %#v, want exactly one Arsenal lane", got)
	}
}

// The sport term is uniform now — NBA and NFL teams kept it all along; football teams did not,
// and that asymmetry was the per-term suffix branching 8.9 removed.
func TestBuildRSSSearchQueries_SportTermIsUniform(t *testing.T) {
	nba := buildRSSSearchQueries("Chicago Bulls", "NBA", nil)
	if len(nba) != 1 || nba[0] != `"Chicago Bulls" NBA basketball` {
		t.Fatalf("NBA lanes = %#v", nba)
	}
	football := buildRSSSearchQueries("Inter", "FOOTBALL", nil)
	if len(football) != 1 || football[0] != "Inter soccer football" {
		t.Fatalf("FOOTBALL lanes = %#v", football)
	}
}

func TestLimitRSSArticles_ZeroMeansUncapped(t *testing.T) {
	articles := []Article{{Title: "one"}, {Title: "two"}, {Title: "three"}}
	total, got := limitRSSArticles(articles, 0)
	if total != 3 || len(got) != 3 {
		t.Fatalf("limitRSSArticles limit 0 = total %d len %d, want 3/3", total, len(got))
	}
}

func TestLimitRSSArticles_PositiveLimitCapsReturnedOnly(t *testing.T) {
	articles := []Article{{Title: "one"}, {Title: "two"}, {Title: "three"}}
	total, got := limitRSSArticles(articles, 2)
	if total != 3 || len(got) != 2 {
		t.Fatalf("limitRSSArticles limit 2 = total %d len %d, want 3/2", total, len(got))
	}
}

func TestFilterArticlesByWindowDropsStaleUnknownAndFutureRSSItems(t *testing.T) {
	now := time.Date(2026, 7, 23, 18, 0, 0, 0, time.UTC)
	articles := []Article{
		{Title: "fresh", PublishedAt: now.Add(-11 * time.Hour).Format(time.RFC1123Z)},
		{Title: "boundary overlap", PublishedAt: now.Add(-12*time.Hour - 10*time.Minute).Format(time.RFC1123Z)},
		{Title: "stale", PublishedAt: now.Add(-12*time.Hour - 20*time.Minute).Format(time.RFC1123Z)},
		{Title: "unknown date", PublishedAt: "not a date"},
	}

	got := filterArticlesByWindow(articles, newsWindow{since: now.Add(-12 * time.Hour), through: now})
	if len(got) != 1 {
		t.Fatalf("recent articles = %d, want 1: %#v", len(got), got)
	}
	for _, a := range got {
		if a.Title == "stale" {
			t.Fatalf("stale article was kept: %#v", got)
		}
	}
}

func TestDeduplicateArticlesNormalizesGoogleNewsURL(t *testing.T) {
	articles := []Article{
		{Title: "one", URL: "https://news.google.com/rss/articles/CBMiExample?hl=en-US&gl=US&ceid=US:en"},
		{Title: "one", URL: "https://news.google.com/rss/articles/CBMiExample?hl=es-ES&gl=ES&ceid=ES:es"},
		{Title: "two", URL: "https://news.google.com/rss/articles/CBMiOther?hl=en-US&gl=US&ceid=US:en"},
	}

	got := deduplicateArticles(articles)
	if len(got) != 2 {
		t.Fatalf("dedupe len = %d, want 2: %#v", len(got), got)
	}
}

func TestDeduplicateArticlesCollapsesSameTitleAndSource(t *testing.T) {
	articles := []Article{
		{Title: "Manchester United agree deal", Source: "Example FC", URL: "https://news.google.com/rss/articles/one"},
		{Title: "Manchester United agree deal", Source: "Example FC", URL: "https://news.google.com/rss/articles/two"},
		{Title: "Manchester United agree deal", Source: "Other Outlet", URL: "https://news.google.com/rss/articles/three"},
	}

	got := deduplicateArticles(articles)
	if len(got) != 2 {
		t.Fatalf("dedupe len = %d, want 2: %#v", len(got), got)
	}
}

// TestRSSEditionsForFootballTeams pins the ENGLISH-ONLY contract. This test previously asserted the
// opposite — that es-ES/fr-FR/de-DE/it-IT/pt-PT/nl-NL were present — and that assertion was correct
// for its time. The editions were retired on measurement: they made the corpus only 24.1% English
// while the candle's embedder (`bge-small-en-v1.5`) and every downstream prompt are English-only.
// Restore them together with a model that reads them; until then a non-English locale appearing
// here should fail loudly rather than quietly re-flood the pipeline.
func TestRSSEditionsForFootballTeams(t *testing.T) {
	got := rssEditionsForEntity("team", "FOOTBALL")
	if len(got) == 0 {
		t.Fatal("football team editions is empty — football ingestion would stop entirely")
	}
	for _, e := range got {
		if !strings.HasPrefix(strings.ToLower(e.hl), "en-") {
			t.Fatalf("non-English edition %q is live, but nothing downstream can read it yet: %#v", e.hl, got)
		}
	}
	if got[0].hl != "en-GB" {
		t.Fatalf("football teams should lead with en-GB (British outlets cover all five leagues): %#v", got)
	}
}

// --- cleanRSSDescription -----------------------------------------------------

// Google News RSS descriptions arrive as anchor markup with the outlet name glued on by
// non-breaking spaces. Tag-stripping alone left the literal entities in the text, which then
// reached the model's prompt verbatim.
func TestCleanRSSDescription_StripsMarkupAndEntities(t *testing.T) {
	raw := `<a href="https://news.google.com/rss/articles/CBMi">Arsenal sign Greek winger Christos Tzolis</a>&nbsp;&nbsp;<font color="#6f6f6f">Sky Sports</font>`
	got := cleanRSSDescription(raw)
	want := "Arsenal sign Greek winger Christos Tzolis Sky Sports"
	if got != want {
		t.Errorf("cleanRSSDescription = %q, want %q", got, want)
	}
	if strings.Contains(got, "&nbsp;") || strings.Contains(got, "<") {
		t.Errorf("markup survived cleaning: %q", got)
	}
}

func TestCleanRSSDescription_DecodesAmpersandsAndCollapsesSpace(t *testing.T) {
	got := cleanRSSDescription("Brighton &amp; Hove Albion   &quot;done deal&quot;\n\nSource")
	want := `Brighton & Hove Albion "done deal" Source`
	if got != want {
		t.Errorf("cleanRSSDescription = %q, want %q", got, want)
	}
	if cleanRSSDescription("") != "" {
		t.Error("empty description should stay empty")
	}
}

// The -rss-limit cut must keep what Google ranked highest, not what published most recently.
// This used to sort by date, which meant the cap discarded well-ranked articles for being a few
// hours older — 2,235 of 9,694 on the 2026-07-26 sweep. Recency is not this product's axis.
func TestFeedRankSortKeepsGooglesBestUnderLimit(t *testing.T) {
	articles := []Article{
		{Title: "stale but Google's top hit", PublishedAt: "Mon, 20 Jul 2026 08:00:00 +0000", FeedRank: 0},
		{Title: "fresh also-ran", PublishedAt: "Tue, 21 Jul 2026 08:00:00 +0000", FeedRank: 7},
	}
	sortArticlesByFeedRank(articles)

	if articles[0].Title != "stale but Google's top hit" {
		t.Fatalf("rank 0 must lead regardless of date, got %q first", articles[0].Title)
	}
	if _, kept := limitRSSArticles(articles, 1); kept[0].FeedRank != 0 {
		t.Errorf("limit kept FeedRank %d, want Google's best (0)", kept[0].FeedRank)
	}
}

// FeedRank is per-query, so the primary query and an alias lane both produce a rank 0. A stable
// sort keeps them in the order the queries ran, which is the entity's own name before its aliases.
func TestFeedRankSortIsStableAcrossQueryLanes(t *testing.T) {
	articles := []Article{
		{Title: "primary query hit", FeedRank: 0},
		{Title: "alias lane hit", FeedRank: 0},
	}
	sortArticlesByFeedRank(articles)

	if articles[0].Title != "primary query hit" {
		t.Errorf("tie must resolve to query order, got %q first", articles[0].Title)
	}
}

// Real producer transaction plus the same migrations used by the Rust worker.
func TestClassifierRSSAcquisition(t *testing.T) {
	dbURL := os.Getenv("TEST_DATABASE_URL")
	if dbURL == "" {
		t.Skip("TEST_DATABASE_URL requires a disposable classifier_test database")
	}
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(pool.Close)
	var database string
	if err := pool.QueryRow(ctx, "SELECT current_database()").Scan(&database); err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(database, "classifier_test") {
		t.Fatal("use a disposable classifier_test database")
	}
	if _, err := pool.Exec(ctx, `
        CREATE TABLE IF NOT EXISTS schema_migrations(version text PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS teams(id integer,sport text,name text,PRIMARY KEY(id,sport));
        CREATE TABLE IF NOT EXISTS news_articles(id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
            url_hash text UNIQUE,url text NOT NULL,title text NOT NULL,description text,source text,
            published_at timestamptz,full_text text,duplicate_of bigint,feed_rank integer,raw jsonb);
        CREATE TABLE IF NOT EXISTS harvester_query_provenance(article_id bigint REFERENCES news_articles(id),
            entity_type text,entity_id integer,sport text,feed_rank integer,query_terms jsonb,last_seen_at timestamptz,
            PRIMARY KEY(article_id,entity_type,entity_id,sport));
        CREATE TABLE IF NOT EXISTS sports(id text PRIMARY KEY,display_name text,current_season integer);
        INSERT INTO sports(id,display_name,current_season) VALUES('INTAKE_TEST','Intake fixture',2026) ON CONFLICT DO NOTHING;
        INSERT INTO teams(id,sport,name) VALUES(1,'INTAKE_TEST','Test Club') ON CONFLICT DO NOTHING;
    `); err != nil {
		t.Fatal(err)
	}
	for _, migration := range []string{"102_pipeline_work.sql", "109_pipeline_work_article_stage.sql", "256_pipeline_work_claim_fencing.sql", "291_classifier_plumbing.sql", "292_classifier_acquisition_intake.sql", "309_continuous_coverage.sql"} {
		version := strings.TrimSuffix(migration, ".sql")
		var applied bool
		if err := pool.QueryRow(ctx, "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=$1)", version).Scan(&applied); err != nil {
			t.Fatal(err)
		}
		if applied {
			continue
		}
		body, err := os.ReadFile(filepath.Join("../../../sql/migrations", migration))
		if err != nil {
			t.Fatal(err)
		}
		if _, err := pool.Exec(ctx, string(body)); err != nil {
			t.Fatalf("%s: %v", migration, err)
		}
		if _, err := pool.Exec(ctx, "INSERT INTO schema_migrations(version) VALUES($1) ON CONFLICT DO NOTHING", version); err != nil {
			t.Fatal(err)
		}
	}
	t.Cleanup(func() {
		pool.Exec(ctx, "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL")
	})
	// The transition lock must win even if a fetch started while coverage was open.
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer tx.Rollback(ctx)
	if _, err := tx.Exec(ctx, "SELECT pg_advisory_xact_lock(1935896421,1)"); err != nil {
		t.Fatal(err)
	}
	blocked := make(chan error, 1)
	go func() {
		_, err := NewNewsService(pool, nil).persistArticles(ctx, "INTAKE_TEST", "team", 1,
			[]Article{{URL: "https://example.test/late-discovery", Title: "Must not persist"}}, newsWindow{}, false)
		blocked <- err
	}()
	select {
	case err := <-blocked:
		t.Fatalf("discovery crossed the held transition lock: %v", err)
	case <-time.After(100 * time.Millisecond):
	}
	if _, err := tx.Exec(ctx, "UPDATE coverage_schedule SET enabled=true,enrolled_at=now(),phase='draining',edition_date=current_date"); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-blocked:
		if err == nil || !strings.Contains(err.Error(), "coverage closed") {
			t.Fatalf("late discovery accepted: %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("discovery stayed blocked after transition")
	}
	var late int
	if err := pool.QueryRow(ctx, "SELECT count(*) FROM news_articles WHERE url='https://example.test/late-discovery'").Scan(&late); err != nil || late != 0 {
		t.Fatalf("late discovery persisted: %d %v", late, err)
	}
	if _, err := pool.Exec(ctx, "UPDATE coverage_schedule SET phase='characters'"); err != nil {
		t.Fatal(err)
	}
	if _, err := NewNewsService(pool, nil).persistArticles(ctx, "INTAKE_TEST", "team", 1, nil, newsWindow{}, false); err == nil {
		t.Fatal("character phase accepted discovery")
	}
	requests := 0
	closed := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) { requests++ })
	closed.pool = pool
	if _, _, err := closed.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, nil); err == nil || !strings.Contains(err.Error(), "collection is closed") {
		t.Fatalf("closed collection reported success: %v", err)
	}
	if requests != 0 {
		t.Fatalf("started %d RSS requests after closure", requests)
	}
	if _, err := pool.Exec(ctx, "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL"); err != nil {
		t.Fatal(err)
	}

	article := Article{URL: "https://example.test/classifier-intake", Title: "Test Club report", Source: "Example", FeedRank: 3}
	cleanup := func() {
		pool.Exec(ctx, "DELETE FROM pipeline_work WHERE sport='INTAKE_TEST'")
		pool.Exec(ctx, "DELETE FROM harvester_query_provenance WHERE sport='INTAKE_TEST'")
		pool.Exec(ctx, "DELETE FROM news_articles WHERE url=$1", article.URL)
	}
	cleanup()
	defer cleanup()
	// Legacy flags no longer gate retaining RSS discovery or acquisition enrollment.
	t.Setenv("HARVESTER_INGEST_ENABLED", "0")
	t.Setenv("HARVESTER_SHADOW_MODE", "1")
	service := NewNewsService(pool, nil)
	ingest := func(want int) {
		t.Helper()
		ids, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, []Article{article}, newsWindow{}, false)
		if err != nil || len(ids) != want {
			t.Fatalf("queued=%v error=%v, want %d", ids, err, want)
		}
	}
	ingest(1)
	ingest(0)
	var id int64
	if err := pool.QueryRow(ctx, "SELECT id FROM news_articles WHERE url=$1", article.URL).Scan(&id); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, "UPDATE pipeline_work SET status='running' WHERE sport='INTAKE_TEST'"); err != nil {
		t.Fatal(err)
	}
	ingest(0)
	var token, after string
	if err := pool.QueryRow(ctx, "SELECT claim_token::text FROM pipeline_work WHERE sport='INTAKE_TEST'").Scan(&token); err != nil {
		t.Fatal(err)
	}
	ingest(0)
	if err := pool.QueryRow(ctx, "SELECT claim_token::text FROM pipeline_work WHERE sport='INTAKE_TEST'").Scan(&after); err != nil || after != token {
		t.Fatalf("duplicate revoked claim: %v", err)
	}
	if _, err := pool.Exec(ctx, "UPDATE pipeline_work SET status='failed',attempts=7,last_error='unavailable',available_at=now()+interval '100 years' WHERE sport='INTAKE_TEST'"); err != nil {
		t.Fatal(err)
	}
	ingest(0)
	var attempts int
	if err := pool.QueryRow(ctx, "SELECT attempts FROM pipeline_work WHERE sport='INTAKE_TEST'").Scan(&attempts); err != nil || attempts != 7 {
		t.Fatalf("failure reset: %d %v", attempts, err)
	}
	article.Title += " corrected"
	ingest(1)
	var status, stage string
	if err := pool.QueryRow(ctx, "SELECT stage,status,attempts FROM pipeline_work WHERE sport='INTAKE_TEST'").Scan(&stage, &status, &attempts); err != nil || stage != "classifier" || status != "pending" || attempts != 0 {
		t.Fatalf("revision not reopened: %s %s %d %v", stage, status, attempts, err)
	}
	if _, err := pool.Exec(ctx, `WITH retained AS (INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source,discovery_version)
        VALUES($1,'INTAKE_TEST','test','test','{}',classifier_discovery_version($1,'INTAKE_TEST')) RETURNING id)
        INSERT INTO classifier_source_heads(article_id,sport,source_id) SELECT $1,'INTAKE_TEST',id FROM retained;
        `, id); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, "DELETE FROM pipeline_work WHERE sport='INTAKE_TEST'"); err != nil {
		t.Fatal(err)
	}
	ingest(0) // A completed current revision is not recreated.
	article.FeedRank = 1
	ingest(0) // Feed rank only affects queue order, never evidence identity.
	t.Run("reposts_and_persistent_fetch_progress", func(t *testing.T) {
		if _, err := pool.Exec(ctx, "DELETE FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'"); err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() {
			pool.Exec(ctx, "DELETE FROM pipeline_work WHERE sport='INTAKE_TEST'")
			pool.Exec(ctx, "DELETE FROM harvester_query_provenance WHERE sport='INTAKE_TEST'")
			pool.Exec(ctx, "DELETE FROM news_articles WHERE url LIKE 'https://example.test/repost-%'")
			pool.Exec(ctx, "DELETE FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'")
		})
		published := time.Now().Add(-30 * time.Minute).Truncate(time.Second)
		original := Article{URL: "https://example.test/repost-original", Title: "Test Club denies reaching a transfer agreement with Cedar FC", Source: "Example", PublishedAt: published.Format(time.RFC3339)}
		first, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, []Article{original}, newsWindow{}, false)
		if err != nil || len(first) != 1 {
			t.Fatalf("first discovery=%v error=%v", first, err)
		}
		copy := original
		copy.URL = "https://example.test/repost-copy"
		copy.Title = "TEST CLUB denies  reaching a transfer agreement with Cedar FC"
		copy.Source = "Other Outlet"
		copy.PublishedAt = published.Add(10 * time.Minute).Format(time.RFC3339)
		queued, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, []Article{copy}, newsWindow{}, false)
		if err != nil || len(queued) != 0 {
			t.Fatalf("repost queued=%v error=%v", queued, err)
		}
		var canonical int64
		if err := pool.QueryRow(ctx, "SELECT duplicate_of FROM news_articles WHERE url=$1", copy.URL).Scan(&canonical); err != nil || canonical != first[0] {
			t.Fatalf("repost canonical=%d error=%v", canonical, err)
		}
		original.PublishedAt = published.Add(15 * time.Minute).Format(time.RFC3339)
		if ids, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, []Article{original}, newsWindow{}, false); err != nil || len(ids) != 0 {
			t.Fatalf("timestamp-only repost queued=%v error=%v", ids, err)
		}
		var retained time.Time
		if err := pool.QueryRow(ctx, "SELECT published_at FROM news_articles WHERE id=$1", canonical).Scan(&retained); err != nil || !retained.Equal(published) {
			t.Fatalf("original reporting time changed: %v %v", retained, err)
		}
		copy.Title = "Test Club confirms reaching a transfer agreement with Cedar FC"
		if ids, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, []Article{copy}, newsWindow{}, false); err != nil || len(ids) != 1 || ids[0] == canonical {
			t.Fatalf("changed claim was suppressed: %v %v", ids, err)
		}
		through := time.Now().Add(-2 * time.Hour).Truncate(time.Microsecond)
		window := newsWindow{since: through.Add(-8 * time.Hour), through: through}
		rechecked := newsWindow{since: window.since, through: through.Add(time.Hour)}
		for _, step := range []struct {
			complete bool
			window   newsWindow
			want     time.Time
		}{
			{false, window, window.since},
			{true, window, window.since},
			{true, window, window.since},
			{false, rechecked, window.since},
			{true, rechecked, rechecked.through},
			{false, window, rechecked.through},
		} {
			if _, err := service.persistArticles(ctx, "INTAKE_TEST", "team", 1, nil, step.window, step.complete); err != nil {
				t.Fatal(err)
			}
			if err := pool.QueryRow(ctx, "SELECT fetched_through FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST' AND entity_id=1").Scan(&retained); err != nil || !retained.Equal(step.want) {
				t.Fatalf("fetch progress=%v want=%v error=%v", retained, step.want, err)
			}
		}
	})

	t.Run("fetch_resumes_hourly_then_catches_up_after_character_work", func(t *testing.T) {
		now := time.Now().Truncate(time.Second)
		cursor := now.Add(-45 * time.Minute)
		var queries []string
		fetcher := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) {
			q := r.URL.Query().Get("q")
			queries = append(queries, q)
			if strings.Contains(q, "Broken Alias") {
				w.WriteHeader(http.StatusServiceUnavailable)
				return
			}
			w.Header().Set("Content-Type", "application/xml")
			w.Write([]byte(rssFixture(
				rssFixtureItem{"Test Club confirms a fresh development", "Wire", "https://example.test/resume-fresh", now.Add(-10 * time.Minute), ""},
				rssFixtureItem{"Test Club announces an overnight development", "Wire", "https://example.test/resume-overnight", now.Add(-2 * time.Hour), ""},
			)))
		})
		fetcher.pool = pool
		t.Cleanup(func() {
			pool.Exec(ctx, "DELETE FROM pipeline_work WHERE sport='INTAKE_TEST'")
			pool.Exec(ctx, "DELETE FROM harvester_query_provenance WHERE sport='INTAKE_TEST'")
			pool.Exec(ctx, "DELETE FROM news_articles WHERE url LIKE 'https://example.test/resume-%'")
			pool.Exec(ctx, "DELETE FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'")
		})
		setCursor := func(at time.Time) {
			t.Helper()
			if _, err := pool.Exec(ctx, `INSERT INTO headline_fetch_watermarks(entity_type,entity_id,sport,fetched_through)
				VALUES('team',1,'INTAKE_TEST',$1) ON CONFLICT(entity_type,entity_id,sport) DO UPDATE SET fetched_through=EXCLUDED.fetched_through`, at); err != nil {
				t.Fatal(err)
			}
		}
		setCursor(cursor)
		ids, _, err := fetcher.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, nil)
		if err != nil || len(ids) != 1 || len(queries) != 1 || !strings.Contains(queries[0], "when:1h") {
			t.Fatalf("hourly fetch ids=%v queries=%v error=%v", ids, queries, err)
		}
		var through time.Time
		if err := pool.QueryRow(ctx, "SELECT fetched_through FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'").Scan(&through); err != nil || !through.After(cursor) {
			t.Fatalf("successful fetch did not advance progress: %v %v", through, err)
		}
		morningSince := now.Add(-9*time.Hour - 30*time.Minute)
		setCursor(morningSince)
		ids, _, err = fetcher.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, nil)
		if err != nil || len(ids) != 1 || !strings.Contains(queries[1], "when:10h") {
			t.Fatalf("morning catch-up ids=%v queries=%v error=%v", ids, queries, err)
		}
		if err := pool.QueryRow(ctx, "SELECT fetched_through FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'").Scan(&through); err != nil || !through.Equal(morningSince) {
			t.Fatalf("first catch-up advanced before recheck: %v %v", through, err)
		}
		// Simulate the next hourly boundary without a wall-clock sleep.
		if _, err := pool.Exec(ctx, "UPDATE headline_fetch_watermarks SET recheck_after=clock_timestamp()-interval '1 second' WHERE sport='INTAKE_TEST'"); err != nil {
			t.Fatal(err)
		}
		resumed := NewNewsService(pool, nil)
		resumed.rssBaseURL, resumed.httpClient = fetcher.rssBaseURL, fetcher.httpClient
		ids, _, err = resumed.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, nil)
		if err != nil || len(ids) != 0 || !strings.Contains(queries[2], "when:10h") {
			t.Fatalf("recheck repeated model work or changed the catch-up window: %v %v %v", ids, queries, err)
		}
		if err := pool.QueryRow(ctx, "SELECT fetched_through FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST' AND recheck_after IS NULL").Scan(&through); err != nil || !through.After(morningSince) {
			t.Fatalf("successful recheck did not advance progress: %v %v", through, err)
		}
		setCursor(cursor)
		_, funnel, err := fetcher.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, []string{"Broken Alias"})
		if err != nil || funnel.RSSErrors != 1 {
			t.Fatalf("partial fetch errors=%d error=%v", funnel.RSSErrors, err)
		}
		if err := pool.QueryRow(ctx, "SELECT fetched_through FROM headline_fetch_watermarks WHERE sport='INTAKE_TEST'").Scan(&through); err != nil || !through.Equal(cursor) {
			t.Fatalf("partial fetch skipped uncollected time: %v %v", through, err)
		}
	})

	failing := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/xml")
		w.Write([]byte(rssFixture(rssFixtureItem{"New report", "Example", "https://example.test/failed-intake", time.Now().Add(-time.Minute), "report"})))
	})
	failing.pool = pool
	// Force a database rejection after a successful RSS fetch.
	if _, err := pool.Exec(ctx, "ALTER TABLE news_articles ADD CONSTRAINT intake_test_rejection CHECK(url != 'https://example.test/failed-intake')"); err != nil {
		t.Fatal(err)
	}
	defer pool.Exec(ctx, "ALTER TABLE news_articles DROP CONSTRAINT intake_test_rejection")
	if _, _, err := failing.GetEntityNews(ctx, "team", 1, "Test Club", "INTAKE_TEST", 0, nil); err == nil || !strings.Contains(err.Error(), "persist RSS discovery") {
		t.Fatalf("persistence failure reported as success: %v", err)
	}
}

func TestRSSWindowCoversHourlyLateAndMorningFetches(t *testing.T) {
	through := time.Date(2026, 10, 23, 8, 0, 0, 0, time.UTC)
	for _, tc := range []struct {
		gap   time.Duration
		hours int
	}{{time.Hour, 1}, {time.Hour + time.Minute, 2}, {10 * time.Hour, 10}, {25 * time.Hour, 25}} {
		window := newsWindow{since: through.Add(-tc.gap), through: through}
		if got := window.hours(); got != tc.hours {
			t.Fatalf("gap=%v hours=%d want=%d", tc.gap, got, tc.hours)
		}
	}
	window := newsWindow{since: through.Add(-time.Hour), through: through}
	articles := []Article{{Title: "lower", PublishedAt: window.since.Format(time.RFC3339)}, {Title: "upper", PublishedAt: through.Format(time.RFC3339)}, {Title: "future", PublishedAt: through.Add(time.Second).Format(time.RFC3339)}}
	if got := filterArticlesByWindow(articles, window); len(got) != 2 {
		t.Fatalf("window boundaries=%v", got)
	}
}

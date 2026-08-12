package thirdparty

import (
	"strings"
	"testing"
	"time"
)

// ingestCronPeriodHours is the period of the hosting ingest cron
// (`0 2 * * *` in scripts/hosting/crontab.example). Keep the two in step.
const ingestCronPeriodHours = 24

// The lookback window is both the `when:` token sent to Google News and the
// cutoff in filterArticlesByLookback, so it bounds what a single sweep can see.
// Narrower than the cron's period means every run leaves a gap that no later run
// recovers -- news dropped twice over, with no error and nothing missing from
// pipeline_work to reveal it. Widening is safe (already-seen URLs dedupe);
// narrowing below the cron period is the direction that loses data silently.
func TestRSSLookbackCoversIngestCronPeriod(t *testing.T) {
	if rssLookbackHours < ingestCronPeriodHours {
		t.Fatalf(
			"rssLookbackHours = %d, but the ingest cron runs every %dh -- a window narrower "+
				"than the cron period silently drops the news in between",
			rssLookbackHours, ingestCronPeriodHours,
		)
	}
}

func TestRSSWhenTokenSpansSubDayAndDayWindows(t *testing.T) {
	for _, tc := range []struct {
		hours int
		want  string
	}{
		{12, "12h"},
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

func TestFilterArticlesByLookbackDropsStaleRSSItems(t *testing.T) {
	now := time.Date(2026, 7, 23, 18, 0, 0, 0, time.UTC)
	articles := []Article{
		{Title: "fresh", PublishedAt: now.Add(-11 * time.Hour).Format(time.RFC1123Z)},
		{Title: "boundary overlap", PublishedAt: now.Add(-12*time.Hour - 10*time.Minute).Format(time.RFC1123Z)},
		{Title: "stale", PublishedAt: now.Add(-12*time.Hour - 20*time.Minute).Format(time.RFC1123Z)},
		{Title: "unknown date", PublishedAt: "not a date"},
	}

	got := filterArticlesByLookback(articles, 12, now)
	if len(got) != 3 {
		t.Fatalf("recent articles = %d, want 3: %#v", len(got), got)
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

// --------------------------------------------------------------------------
// D-T21 — the per-entity daily read cap
// --------------------------------------------------------------------------

// The cap keeps the FRONT of the fresh list, because that list arrives in Google's result order
// and Google is the relevancy source (8.9). If this ever starts keeping some other subset, a
// ranking heuristic has grown back in the one place the rail spent 393 lines removing it from.
func TestCapFreshReadsKeepsGooglesOrder(t *testing.T) {
	fresh := []int64{10, 11, 12, 13, 14}

	kept, withheld := capFreshReads(fresh, 0, 3)

	if len(kept) != 3 || withheld != 2 {
		t.Fatalf("cap 3 over 5 fresh: got %d kept / %d withheld, want 3/2", len(kept), withheld)
	}
	for i, want := range []int64{10, 11, 12} {
		if kept[i] != want {
			t.Errorf("kept[%d] = %d, want %d — the cap must keep Google's top results", i, kept[i], want)
		}
	}
}

// A cap of 0 is NO CAP and is the default, so deploying the code changes nothing until the env
// knob is set. This is the test that protects the "deploy is inert" property.
func TestCapFreshReadsZeroMeansNoCap(t *testing.T) {
	fresh := []int64{1, 2, 3, 4, 5, 6, 7, 8}

	for _, cap := range []int{0, -1} {
		kept, withheld := capFreshReads(fresh, 99, cap)
		if len(kept) != len(fresh) || withheld != 0 {
			t.Errorf("cap %d: got %d kept / %d withheld, want all %d kept and 0 withheld",
				cap, len(kept), withheld, len(fresh))
		}
	}
}

// The allowance is spent across the DAY, not per sweep: an entity that already had its fill this
// morning gets nothing more tonight, and one that is over its allowance never goes negative.
func TestCapFreshReadsSpendsTheDailyAllowance(t *testing.T) {
	fresh := []int64{1, 2, 3, 4}

	cases := []struct {
		name         string
		already, cap int
		wantKept     int
		wantWithheld int
	}{
		{"room for all", 0, 10, 4, 0},
		{"partly spent", 8, 10, 2, 2},
		{"exactly spent", 10, 10, 0, 4},
		{"over the cap already", 15, 10, 0, 4},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			kept, withheld := capFreshReads(fresh, c.already, c.cap)
			if len(kept) != c.wantKept || withheld != c.wantWithheld {
				t.Errorf("already=%d cap=%d: got %d kept / %d withheld, want %d/%d",
					c.already, c.cap, len(kept), withheld, c.wantKept, c.wantWithheld)
			}
		})
	}
}

// The knob defaults to OFF and refuses nonsense rather than guessing, so a typo in the unit file
// cannot silently cap production at some arbitrary number.
func TestEditorReadsPerEntityDayDefaultsToNoCap(t *testing.T) {
	t.Setenv("EDITOR_MAX_READS_PER_ENTITY_DAY", "")
	if got := editorReadsPerEntityDay(); got != 0 {
		t.Errorf("unset = %d, want 0 (no cap)", got)
	}
	t.Setenv("EDITOR_MAX_READS_PER_ENTITY_DAY", "ten")
	if got := editorReadsPerEntityDay(); got != 0 {
		t.Errorf("unparseable = %d, want 0 (no cap)", got)
	}
	t.Setenv("EDITOR_MAX_READS_PER_ENTITY_DAY", "-3")
	if got := editorReadsPerEntityDay(); got != 0 {
		t.Errorf("negative = %d, want 0 (no cap)", got)
	}
	t.Setenv("EDITOR_MAX_READS_PER_ENTITY_DAY", "10")
	if got := editorReadsPerEntityDay(); got != 10 {
		t.Errorf("set = %d, want 10", got)
	}
}

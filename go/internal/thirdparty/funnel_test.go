package thirdparty

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// rssFixture builds a Google News-shaped RSS payload. Titles carry the " - Source"
// suffix the real feed uses, because fetchRSS splits on it to derive Source and
// the dedupe key is title+source.
func rssFixture(items ...rssFixtureItem) string {
	var b strings.Builder
	b.WriteString(`<?xml version="1.0"?><rss version="2.0"><channel>`)
	for _, it := range items {
		fmt.Fprintf(&b,
			`<item><title>%s - %s</title><link>%s</link><pubDate>%s</pubDate><description>%s</description></item>`,
			it.title, it.source, it.link, it.published.Format(time.RFC1123Z), it.description)
	}
	b.WriteString(`</channel></rss>`)
	return b.String()
}

type rssFixtureItem struct {
	title       string
	source      string
	link        string
	published   time.Time
	description string
}

// newFixtureService returns a NewsService pointed at a local RSS server. pool is
// nil, so GetEntityNews skips the write-through entirely and exercises only the
// fetch funnel.
func newFixtureService(t *testing.T, handler http.HandlerFunc) *NewsService {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	s := NewNewsService(nil, slog.New(slog.NewTextHandler(io.Discard, nil)))
	s.rssBaseURL = srv.URL
	return s
}

// TestFunnelAccountsForEveryDrop drives one team sweep whose fixture is built to
// lose an article at each stage, and checks both the individual counters and the
// balance invariant. This is the regression net for the ingest path: any future
// filter that drops articles without a counter fails Residual here rather than
// silently shrinking the corpus in production.
func TestFunnelAccountsForEveryDrop(t *testing.T) {
	now := time.Now()
	fresh := now.Add(-2 * time.Hour)
	stale := now.Add(-30 * time.Hour) // outside the 12h window + 15m slack

	// 6 items per response: 4 admitted, 1 stale (window), 1 duplicate of the first by
	// title+source (dedupe).
	//
	// "Markets rally after earnings" is the off-topic one. It used to be dropped here by
	// MatchesEntity and is now admitted on purpose: ingest no longer judges relevance, so
	// junk reaches the Reader and is rejected by something that read it. Keeping it in the
	// fixture is the point — it proves the article flows through rather than vanishing.
	body := rssFixture(
		rssFixtureItem{"Manchester United agree deal", "Example FC", "https://ex.test/a", fresh, "Club confirms the move."},
		rssFixtureItem{"Man United track midfielder", "Other Outlet", "https://ex.test/b", fresh, "Transfer latest."},
		rssFixtureItem{"Manchester United injury update", "Third Paper", "https://ex.test/c", fresh, "Squad news."},
		rssFixtureItem{"Manchester United win late", "Stale Paper", "https://ex.test/d", stale, "Old report."},
		rssFixtureItem{"Markets rally after earnings", "Business Wire", "https://ex.test/e", fresh, "No team here."},
		rssFixtureItem{"Manchester United agree deal", "Example FC", "https://ex.test/f", fresh, "Syndicated copy."},
	)

	var calls int
	s := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) {
		calls++
		w.Header().Set("Content-Type", "application/xml")
		_, _ = io.WriteString(w, body)
	})

	// limit 0 = uncapped, so the whole edition x query grid runs and nothing is
	// truncated. Manchester United is a football team: 8 editions.
	_, f, err := s.GetEntityNews(context.Background(), "team", 1,
		"Manchester United", "FOOTBALL", 0, []string{"Man United", "MUN"})
	if err != nil {
		t.Fatalf("GetEntityNews: %v", err)
	}

	if f.Entities != 1 {
		t.Errorf("Entities = %d, want 1", f.Entities)
	}
	if f.EditionsPlanned != len(footballTeamRSSEditions) || f.EditionsQueried != len(footballTeamRSSEditions) {
		t.Errorf("editions planned/queried = %d/%d, want %d/%d",
			f.EditionsPlanned, f.EditionsQueried, len(footballTeamRSSEditions), len(footballTeamRSSEditions))
	}
	// 8.9: every lane runs every sweep, so the plan and the run are the same number by
	// construction. A future cap that breaks the plan early owes this a counter again.
	if f.QueriesPlanned != f.QueriesRun {
		t.Errorf("queries planned %d != run %d", f.QueriesPlanned, f.QueriesRun)
	}
	if f.RSSCalls != calls || f.RSSCalls != f.QueriesRun {
		t.Errorf("RSSCalls = %d, server saw %d, queries run %d", f.RSSCalls, calls, f.QueriesRun)
	}
	if f.RSSItems != 6*f.RSSCalls {
		t.Errorf("RSSItems = %d, want %d (6 per call)", f.RSSItems, 6*f.RSSCalls)
	}
	if f.WindowDropped != f.RSSCalls {
		t.Errorf("WindowDropped = %d, want 1 per call (%d)", f.WindowDropped, f.RSSCalls)
	}
	// 5 survive the window per call, of which one is a title+source duplicate; every
	// call after the first re-delivers the same four survivors.
	if f.Matched != 4 {
		t.Errorf("Matched = %d, want 4 distinct articles", f.Matched)
	}
	if f.LimitTruncated != 0 {
		t.Errorf("LimitTruncated = %d, want 0 when uncapped", f.LimitTruncated)
	}
	// Every fixture description is non-empty, so the split is all-bearing. The
	// pair partitions Matched — not a drop stage, so Residual ignores it.
	if f.DescriptionBearing != f.Matched || f.DescriptionEmpty != 0 {
		t.Errorf("description split = %d bearing / %d empty, want %d / 0",
			f.DescriptionBearing, f.DescriptionEmpty, f.Matched)
	}
	if r := f.Residual(); r != 0 {
		t.Errorf("funnel does not balance: residual %d (%v)", r, f.LogAttrs())
	}
}

// TestFunnelSurvivesFetchErrors — an edition that errors must still be counted as
// attempted. Otherwise a systematically failing locale reads as "no news there".
func TestFunnelSurvivesFetchErrors(t *testing.T) {
	s := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "upstream sad", http.StatusBadGateway)
	})

	_, f, err := s.GetEntityNews(context.Background(), "team", 1,
		"Chicago Bulls", "NBA", 0, nil)
	if err != nil {
		t.Fatalf("GetEntityNews: %v", err)
	}
	if f.RSSCalls == 0 || f.RSSErrors != f.RSSCalls {
		t.Errorf("RSSCalls %d / RSSErrors %d, want every call counted as an error", f.RSSCalls, f.RSSErrors)
	}
	if f.RSSItems != 0 || f.Matched != 0 {
		t.Errorf("failed fetches produced items: RSSItems %d Matched %d", f.RSSItems, f.Matched)
	}
	if r := f.Residual(); r != 0 {
		t.Errorf("funnel does not balance: residual %d (%v)", r, f.LogAttrs())
	}
}

func TestFunnelAddRollsUpEveryField(t *testing.T) {
	a := Funnel{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15}
	var got Funnel
	got.Add(a)
	got.Add(a)

	// Compare against a doubled literal rather than field by field, so a Funnel
	// that grows a counter without a matching line in Add fails to compile here
	// instead of silently under-reporting in the rolled-up sweep totals.
	want := Funnel{2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30}
	if got != want {
		t.Errorf("Add rolled up to %+v, want %+v", got, want)
	}
}

// D-T21's cap is not a fetch-path drop — it acts inside persistArticles, downstream of Matched,
// and it discards no article. So it must NOT disturb the residual invariant, or a capped sweep
// would fire the "a drop was added without a counter" alarm on every run.
func TestReadsWithheldDoesNotBreakTheResidualInvariant(t *testing.T) {
	f := Funnel{
		RSSItems:       100,
		WindowDropped:  10,
		DedupCollapsed: 20,
		LimitTruncated: 5,
		Matched:        65,
		ReadsWithheld:  65, // the cap withheld every single read
	}

	if r := f.Residual(); r != 0 {
		t.Errorf("a fully-capped sweep must still balance: residual %d (%v)", r, f.LogAttrs())
	}
}

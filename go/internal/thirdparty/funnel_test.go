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

	// 6 items per response: 3 admitted, 1 stale (window), 1 off-topic (matcher),
	// 1 duplicate of the first by title+source (dedupe).
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
	_, _, f, err := s.GetEntityNews(context.Background(), "team", 1,
		"Manchester United", "FOOTBALL", "", 0, "", "", []string{"Man United", "MUN"})
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
	if f.EditionsSkipped != 0 || f.QueriesSkipped != 0 {
		t.Errorf("uncapped sweep skipped work: editions %d queries %d", f.EditionsSkipped, f.QueriesSkipped)
	}
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
	if f.MatchRejected != f.RSSCalls {
		t.Errorf("MatchRejected = %d, want 1 per call (%d)", f.MatchRejected, f.RSSCalls)
	}
	// 4 admitted per call, of which one is a title+source duplicate; every call
	// after the first re-delivers the same three survivors.
	if f.Matched != 3 {
		t.Errorf("Matched = %d, want 3 distinct articles", f.Matched)
	}
	if f.LimitTruncated != 0 {
		t.Errorf("LimitTruncated = %d, want 0 when uncapped", f.LimitTruncated)
	}
	if r := f.Residual(); r != 0 {
		t.Errorf("funnel does not balance: residual %d (%v)", r, f.LogAttrs())
	}
}

// TestFunnelCountsQueriesSkippedByLimit pins what the -rss-limit cap can and cannot cost now that
// football runs a SINGLE edition (en-GB). The edition-skipping failure this test was written for —
// a team filling its limit from en-US and never reaching the seven localized editions — is
// structurally gone: there are no other editions to lose. What remains is the query-level break,
// which still stops alias lanes past the first two. The counters must show that distinction, so a
// future edition addition makes editions_skipped meaningful again rather than silently.
func TestFunnelCountsQueriesSkippedByLimit(t *testing.T) {
	now := time.Now().Add(-1 * time.Hour)
	items := make([]rssFixtureItem, 0, 12)
	for i := 0; i < 12; i++ {
		items = append(items, rssFixtureItem{
			title:       fmt.Sprintf("Paris Saint Germain story %d", i),
			source:      fmt.Sprintf("Outlet %d", i),
			link:        fmt.Sprintf("https://ex.test/%d", i),
			published:   now,
			description: "Match report.",
		})
	}
	body := rssFixture(items...)

	editionsSeen := map[string]bool{}
	s := newFixtureService(t, func(w http.ResponseWriter, r *http.Request) {
		editionsSeen[r.URL.Query().Get("ceid")] = true
		w.Header().Set("Content-Type", "application/xml")
		_, _ = io.WriteString(w, body)
	})

	_, _, f, err := s.GetEntityNews(context.Background(), "team", 1,
		"Paris Saint Germain", "FOOTBALL", "", 12, "", "", []string{"PSG", "Paris Saint-Germain"})
	if err != nil {
		t.Fatalf("GetEntityNews: %v", err)
	}

	if f.EditionsQueried != len(footballTeamRSSEditions) {
		t.Fatalf("EditionsQueried = %d, want all %d", f.EditionsQueried, len(footballTeamRSSEditions))
	}
	if f.EditionsSkipped != 0 {
		t.Errorf("EditionsSkipped = %d, want 0 — a single edition cannot be skipped", f.EditionsSkipped)
	}
	if len(editionsSeen) != 1 || !editionsSeen["GB:en"] {
		t.Errorf("editions actually fetched = %v, want only GB:en", editionsSeen)
	}
	// PSG yields three safe lanes (primary + PSG + Paris Saint-Germain). runRSSQueryPastLimit
	// exempts only the first two, so exactly one lane is cut once the limit fills.
	if f.QueriesSkipped != 1 {
		t.Errorf("QueriesSkipped = %d, want 1 alias lane cut by the limit", f.QueriesSkipped)
	}
	if f.QueriesPlanned != f.QueriesRun+f.QueriesSkipped {
		t.Errorf("query plan does not balance: planned %d, run %d, skipped %d",
			f.QueriesPlanned, f.QueriesRun, f.QueriesSkipped)
	}
	if f.EditionsPlanned != f.EditionsQueried+f.EditionsSkipped {
		t.Errorf("edition plan does not balance: planned %d, queried %d, skipped %d",
			f.EditionsPlanned, f.EditionsQueried, f.EditionsSkipped)
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

	_, _, f, err := s.GetEntityNews(context.Background(), "team", 1,
		"Chicago Bulls", "NBA", "", 0, "", "", nil)
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

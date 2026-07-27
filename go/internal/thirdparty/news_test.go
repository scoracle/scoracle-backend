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
	if len(timeWindows) != 1 {
		t.Fatalf("timeWindows = %#v, want exactly one window", timeWindows)
	}
	if timeWindows[0] < ingestCronPeriodHours {
		t.Fatalf(
			"timeWindows = %#v, but the ingest cron runs every %dh -- a window narrower "+
				"than the cron period silently drops the news in between",
			timeWindows, ingestCronPeriodHours,
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

func TestNameInText_ExactMatch(t *testing.T) {
	if !nameInText("FC Bayern Munchen", "FC Bayern Munchen wins again", "", "", "", nil, "") {
		t.Error("expected exact match")
	}
}

func TestNameInText_CaseInsensitive(t *testing.T) {
	if !nameInText("FC Bayern Munchen", "fc bayern munchen wins again", "", "", "", nil, "") {
		t.Error("expected case-insensitive match")
	}
}

func TestNameInText_AliasMatch(t *testing.T) {
	aliases := []string{"Bayern Munich", "Bayern München", "FC Bayern", "Bayern"}

	if !nameInText("FC Bayern Munchen", "Bayern Munich transfer news", "", "", "", aliases, "") {
		t.Error("expected alias 'Bayern Munich' to match")
	}

	if !nameInText("FC Bayern Munchen", "Bayern München beats rival", "", "", "", aliases, "") {
		t.Error("expected alias 'Bayern München' to match")
	}

	if !nameInText("FC Bayern Munchen", "FC Bayern signs striker", "", "", "", aliases, "") {
		t.Error("expected alias 'FC Bayern' to match")
	}
}

func TestNameInText_ShortAliasRequiresSportContext(t *testing.T) {
	aliases := []string{"FCB"}

	// Short alias without sport context — should NOT match.
	if nameInText("FC Barcelona", "FCB releases new product", "", "", "", aliases, "") {
		t.Error("short alias without sport context should not match")
	}

	// Short alias WITH sport context in text — should match.
	if !nameInText("FC Barcelona", "FCB soccer football update", "", "", "", aliases, "soccer football") {
		t.Error("short alias with sport context should match")
	}
}

func TestMatchesEntity_TrustedShortTeamAlias(t *testing.T) {
	in := EntityMatchInput{
		EntityType: "team",
		Name:       "Paris Saint Germain",
		Aliases:    []string{"PSG"},
		Sport:      "FOOTBALL",
	}

	if !MatchesEntity("PSG close in on new midfielder", in) {
		t.Error("expected trusted short team alias PSG to match without explicit sport context")
	}

	untrusted := EntityMatchInput{
		EntityType: "team",
		Name:       "Manchester United",
		Aliases:    []string{"MUN"},
		Sport:      "FOOTBALL",
	}
	if MatchesEntity("MUN shares climb after earnings", untrusted) {
		t.Error("generic provider-style short code should not match without sport context")
	}
}

func TestNameInText_NoFalsePositive(t *testing.T) {
	aliases := []string{"Bayern Munich"}

	if nameInText("FC Bayern Munchen", "Real Madrid wins championship", "", "", "", aliases, "") {
		t.Error("should not match unrelated text")
	}
}

func TestNameInText_UsesEntityTermBoundaries(t *testing.T) {
	if nameInText("Inter", "The international break changes the schedule", "", "", "", nil, "") {
		t.Error("expected Inter not to match inside international")
	}
	if !nameInText("Inter", "Inter signs a new midfielder", "", "", "", nil, "") {
		t.Error("expected Inter whole-word match")
	}
}

func TestNameInText_AmbiguousFootballTeamNeedsContext(t *testing.T) {
	if nameInText("Nice", "Nice weather expected this weekend", "", "", "", nil, "soccer football") {
		t.Error("expected Nice to need football context")
	}
	if !nameInText("Nice", "Nice sign a new midfielder before the league opener", "", "", "", nil, "soccer football") {
		t.Error("expected Nice to match with football context")
	}
}

func TestNameInText_PlayerWithParts(t *testing.T) {
	if !nameInText("LeBron James", "LeBron and James dominate", "LeBron", "James", "Lakers", nil, "") {
		t.Error("expected first+last name match")
	}
}

func TestNameInText_PlayerPartWithTeam(t *testing.T) {
	if !nameInText("LeBron James", "James leads the Lakers", "LeBron", "James", "Lakers", nil, "") {
		t.Error("expected last name + team context match")
	}
}

func TestBestAliasQuery(t *testing.T) {
	tests := []struct {
		primary string
		aliases []string
		want    string
	}{
		{"FC Bayern Munchen", []string{"Bayern Munich", "FC Bayern", "BAY"}, "Bayern Munich"},
		{"FC Bayern Munchen", []string{"FCB"}, ""},       // too short
		{"Bayern Munich", []string{"bayern munich"}, ""}, // same as primary
		{"Arsenal", nil, ""}, // no aliases
		{"Arsenal", []string{"Arsenal FC", "The Gunners"}, "The Gunners"}, // longest wins
	}

	for _, tt := range tests {
		got := bestAliasQuery(tt.primary, tt.aliases)
		if got != tt.want {
			t.Errorf("bestAliasQuery(%q, %v) = %q, want %q", tt.primary, tt.aliases, got, tt.want)
		}
	}
}

func TestBuildRSSSearchQueries_TeamUsesAliasLanes(t *testing.T) {
	got := buildRSSSearchQueries(
		"team",
		"Manchester United",
		"FOOTBALL",
		[]string{"MUN", "Man UTD", "Man United", "MUFC"},
	)
	want := []string{`"Manchester United"`, `"Man United"`, `"Man UTD"`}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] || got[2] != want[2] {
		t.Fatalf("buildRSSSearchQueries team = %#v, want %#v", got, want)
	}
}

func TestBuildRSSSearchQueries_TeamAllowsTrustedShortAlias(t *testing.T) {
	got := buildRSSSearchQueries("team", "Paris Saint Germain", "FOOTBALL", []string{"PSG", "Paris Saint-Germain"})
	want := []string{`"Paris Saint Germain"`, `PSG`, `"Paris Saint-Germain"`}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] || got[2] != want[2] {
		t.Fatalf("buildRSSSearchQueries PSG = %#v, want %#v", got, want)
	}
}

func TestBuildRSSSearchQueries_TeamCapsSafeAliasLanes(t *testing.T) {
	got := buildRSSSearchQueries("team", "FC Bayern München", "FOOTBALL", []string{
		"Bayern Munich",
		"Bayern Munchen",
		"Bayern München",
		"Bayern",
		"FC Bayern",
		"FCB",
		"Bayern Monaco",
	})
	want := []string{`"FC Bayern München"`, `"FC Bayern"`, `"Bayern Munich"`}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] || got[2] != want[2] {
		t.Fatalf("buildRSSSearchQueries Bayern = %#v, want %#v", got, want)
	}
}

func TestBuildRSSSearchQueries_TeamAvoidsAliasBags(t *testing.T) {
	got := buildRSSSearchQueries("team", "Ajax", "FOOTBALL", []string{
		"Ajax Alt 01", "Ajax Alt 02", "Ajax Alt 03", "Ajax Alt 04", "Ajax Alt 05", "Ajax Alt 06",
		"Ajax Alt 07", "Ajax Alt 08", "Ajax Alt 09", "Ajax Alt 10", "Ajax Alt 11", "Ajax Alt 12",
	})
	want := []string{
		`Ajax`,
		`"Ajax Alt 01"`,
		`"Ajax Alt 02"`,
	}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] || got[2] != want[2] {
		t.Fatalf("buildRSSSearchQueries alias lanes = %#v, want %#v", got, want)
	}
	for _, q := range got {
		if strings.Contains(q, " OR ") {
			t.Fatalf("team RSS query should not contain OR alias bags: %#v", got)
		}
	}
}

func TestBuildRSSSearchQueries_NonFootballTeamKeepsSportContext(t *testing.T) {
	got := buildRSSSearchQueries("team", "Giants", "NFL", []string{"NYG"})
	want := []string{`Giants NFL football`, `NYG NFL football`}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("buildRSSSearchQueries NFL team = %#v, want %#v", got, want)
	}
}

func TestBuildRSSSearchQueries_RiskyFootballPrimaryUsesSaferAlias(t *testing.T) {
	got := buildRSSSearchQueries("team", "Inter", "FOOTBALL", []string{"Inter Milan", "Nerazzurri"})
	want := []string{`"Inter Milan"`}
	if len(got) != len(want) || got[0] != want[0] {
		t.Fatalf("buildRSSSearchQueries Inter = %#v, want %#v", got, want)
	}

	nice := buildRSSSearchQueries("team", "Nice", "FOOTBALL", []string{"OGC Nice", "Nice FC"})
	niceWant := []string{`"OGC Nice"`, `"Nice FC"`}
	if len(nice) != len(niceWant) || nice[0] != niceWant[0] || nice[1] != niceWant[1] {
		t.Fatalf("buildRSSSearchQueries Nice = %#v, want %#v", nice, niceWant)
	}
}

func TestBuildRSSSearchQueries_PlayerKeepsConservativeFallback(t *testing.T) {
	got := buildRSSSearchQueries("player", "FC Bayern Munchen", "FOOTBALL", []string{"Bayern Munich", "FC Bayern", "BAY"})
	want := []string{"FC Bayern Munchen soccer football", "Bayern Munich soccer football"}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("buildRSSSearchQueries player = %#v, want %#v", got, want)
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

func TestBuildSearchName(t *testing.T) {
	tests := []struct {
		full, first, last, want string
	}{
		{"Neymar da Silva Santos Junior", "Neymar", "Junior", "Neymar Junior"},
		{"LeBron James", "LeBron", "James", "LeBron James"},
		{"Robert Lewandowski", "Robert", "Lewandowski", "Robert Lewandowski"},
		{"Saquon Barkley Jr", "", "", "Saquon Jr"},
	}

	for _, tt := range tests {
		got := buildSearchName(tt.full, tt.first, tt.last)
		if got != tt.want {
			t.Errorf("buildSearchName(%q, %q, %q) = %q, want %q", tt.full, tt.first, tt.last, got, tt.want)
		}
	}
}

// --- FirstMatchPos / proximity gate -----------------------------------------

func TestFirstMatchPos_BasicAndAbsent(t *testing.T) {
	title := "Everton want Chelsea striker Liam Delap and Tottenham Hotspur midfielder Conor Gallagher"

	chelsea := EntityMatchInput{Name: "Chelsea", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, chelsea); got != 13 {
		t.Errorf("Chelsea pos = %d, want 13", got)
	}
	gallagher := EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, gallagher); got != 73 {
		t.Errorf("Gallagher pos = %d, want 73", got)
	}
	absent := EntityMatchInput{Name: "Mohamed Salah", FirstName: "Mohamed", LastName: "Salah", Sport: "FOOTBALL"}
	if got := FirstMatchPos(title, absent); got != -1 {
		t.Errorf("absent entity pos = %d, want -1", got)
	}
}

// The whole point: the proximity gate keeps the genuine pair and drops the
// roundup artifact in the same headline.
func TestFirstMatchPos_ProximityGate(t *testing.T) {
	title := "Everton want Chelsea striker Liam Delap and Tottenham Hotspur midfielder Conor Gallagher"
	const window = 50

	chelsea := FirstMatchPos(title, EntityMatchInput{Name: "Chelsea", Sport: "FOOTBALL"})
	delap := FirstMatchPos(title, EntityMatchInput{Name: "Liam Delap", FirstName: "Liam", LastName: "Delap", Sport: "FOOTBALL"})
	gallagher := FirstMatchPos(title, EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher", Sport: "FOOTBALL"})

	if abs(chelsea-delap) > window {
		t.Errorf("Chelsea↔Delap (%d) should be within window", abs(chelsea-delap))
	}
	if abs(chelsea-gallagher) <= window {
		t.Errorf("Chelsea↔Gallagher (%d) should be OUTSIDE window (spurious)", abs(chelsea-gallagher))
	}
}

// FirstMatchPos must agree with MatchesEntity on presence/absence.
func TestFirstMatchPos_AgreesWithMatchesEntity(t *testing.T) {
	cases := []struct {
		text string
		in   EntityMatchInput
	}{
		{"Bayern Munich transfer news", EntityMatchInput{Name: "FC Bayern Munchen", Aliases: []string{"Bayern Munich"}}},
		{"Some unrelated headline", EntityMatchInput{Name: "FC Bayern Munchen", Aliases: []string{"Bayern Munich"}}},
		{"Conor Gallagher to Atletico", EntityMatchInput{Name: "Conor Gallagher", FirstName: "Conor", LastName: "Gallagher"}},
	}
	for _, c := range cases {
		want := MatchesEntity(c.text, c.in)
		got := FirstMatchPos(c.text, c.in) >= 0
		if got != want {
			t.Errorf("FirstMatchPos>=0 = %v, MatchesEntity = %v for %q / %q", got, want, c.text, c.in.Name)
		}
	}
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
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

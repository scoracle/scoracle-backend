package thirdparty

// Funnel counts what one RSS ingest sweep admitted and, more importantly, what
// it discarded. Every stage below can silently drop an article, and none of
// those losses are recoverable from the database afterwards: only admitted
// articles are ever persisted. So a regex regression in MatchesEntity, a
// timezone slip in the lookback window, or an -rss-limit that stops the sweep
// before the localized editions run all look identical from SQL — fewer rows.
// This type is the difference between "ingestion dropped" and "ingestion
// dropped HERE".
//
// The counters obey one invariant, checked by Residual and asserted both in
// tests and at the end of every sweep:
//
//	RSSItems - WindowDropped - MatchRejected - DedupCollapsed - LimitTruncated == Matched
//
// Add a drop to the fetch path without a counter and Residual goes non-zero,
// which is the alarm that keeps this type honest as the path changes.
type Funnel struct {
	// Entities is the number of entity sweeps folded in — 1 per GetEntityNews
	// call, so a rolled-up Funnel says how many teams it covers.
	Entities int

	// Query plan — how much of the configured edition x query grid actually ran.
	// EditionsSkipped > 0 means the -rss-limit cap ended the sweep before the
	// localized editions were reached: runRSSQueryPastLimit only exempts edition
	// 0, so a team that fills its limit from en-US never sees en-gb/es-es/fr-fr/
	// de-de/it-it/pt-pt/nl-nl. That is invisible in every other signal we have.
	EditionsPlanned int
	EditionsQueried int
	EditionsSkipped int
	QueriesPlanned  int
	QueriesRun      int
	QueriesSkipped  int

	// Fetch.
	RSSCalls  int
	RSSErrors int
	RSSItems  int // items parsed out of the RSS payloads, before any filtering

	// Drops, in the order the fetch path applies them.
	WindowDropped  int // published before the lookback cutoff
	MatchRejected  int // MatchesEntity says the text does not mention the entity
	DedupCollapsed int // already seen this sweep, by URL or by title+source
	LimitTruncated int // beyond -rss-limit after the date sort

	// Matched is what reached persistArticles.
	Matched int

	// Description split of Matched — not a drop stage, so neither participates
	// in the Residual invariant. Phase 3's Editor fetches bodies; this pair says
	// up front how many arrivals carry a body-bearing RSS description versus an
	// empty one, i.e. how often the fetch will be the only source of text.
	// DescriptionBearing + DescriptionEmpty == Matched.
	DescriptionBearing int
	DescriptionEmpty   int
}

// Add folds another funnel into f. Written out field by field on purpose: a new
// counter that someone forgets to add here shows up as a rolled-up total that
// disagrees with the per-entity lines, which is a louder failure than reflection
// quietly doing the right thing for a field nobody meant to aggregate.
func (f *Funnel) Add(o Funnel) {
	f.Entities += o.Entities
	f.EditionsPlanned += o.EditionsPlanned
	f.EditionsQueried += o.EditionsQueried
	f.EditionsSkipped += o.EditionsSkipped
	f.QueriesPlanned += o.QueriesPlanned
	f.QueriesRun += o.QueriesRun
	f.QueriesSkipped += o.QueriesSkipped
	f.RSSCalls += o.RSSCalls
	f.RSSErrors += o.RSSErrors
	f.RSSItems += o.RSSItems
	f.WindowDropped += o.WindowDropped
	f.MatchRejected += o.MatchRejected
	f.DedupCollapsed += o.DedupCollapsed
	f.LimitTruncated += o.LimitTruncated
	f.Matched += o.Matched
	f.DescriptionBearing += o.DescriptionBearing
	f.DescriptionEmpty += o.DescriptionEmpty
}

// Residual is the number of articles lost to a stage the funnel does not count.
// It must be zero. A non-zero value means a drop was added to the fetch path
// without a counter — exactly the blindness this type exists to prevent.
func (f Funnel) Residual() int {
	return f.RSSItems - f.WindowDropped - f.MatchRejected - f.DedupCollapsed - f.LimitTruncated - f.Matched
}

// LogAttrs renders the funnel as slog key/value pairs so every emitter — per
// team, per sport, per run — uses one field vocabulary and the log stays
// greppable across granularities.
func (f Funnel) LogAttrs() []any {
	return []any{
		"entities", f.Entities,
		"editions_planned", f.EditionsPlanned,
		"editions_queried", f.EditionsQueried,
		"editions_skipped", f.EditionsSkipped,
		"queries_planned", f.QueriesPlanned,
		"queries_run", f.QueriesRun,
		"queries_skipped", f.QueriesSkipped,
		"rss_calls", f.RSSCalls,
		"rss_errors", f.RSSErrors,
		"rss_items", f.RSSItems,
		"window_dropped", f.WindowDropped,
		"match_rejected", f.MatchRejected,
		"dedup_collapsed", f.DedupCollapsed,
		"limit_truncated", f.LimitTruncated,
		"matched", f.Matched,
		"desc_bearing", f.DescriptionBearing,
		"desc_empty", f.DescriptionEmpty,
		"residual", f.Residual(),
	}
}

package thirdparty

// Funnel counts what one RSS ingest sweep admitted and, more importantly, what
// it discarded. Every stage below can silently drop an article, and none of
// those losses are recoverable from the database afterwards: only admitted
// articles are ever persisted. So a timezone slip in the lookback window or an
// -rss-limit that stops the sweep before the localized editions run both look
// identical from SQL — fewer rows. This type is the difference between
// "ingestion dropped" and "ingestion dropped HERE".
//
// The counters obey one invariant, checked by Residual and asserted both in
// tests and at the end of every sweep:
//
//	RSSItems - WindowDropped - DedupCollapsed - LimitTruncated == Matched
//
// Add a drop to the fetch path without a counter and Residual goes non-zero,
// which is the alarm that keeps this type honest as the path changes.
type Funnel struct {
	// Entities is the number of entity sweeps folded in — 1 per GetEntityNews
	// call, so a rolled-up Funnel says how many teams it covers.
	Entities int

	// Query plan — the edition x query grid. Planned == Queried/Run now: 8.9 deleted the
	// -rss-limit early break, so every name lane runs for every entity every sweep and the
	// cap applies to RESULTS instead. The Skipped counters went with the break that fed them;
	// re-add them with the mechanism if a cap ever returns to the plan.
	EditionsPlanned int
	EditionsQueried int
	QueriesPlanned  int
	QueriesRun      int

	// Fetch.
	RSSCalls  int
	RSSErrors int
	RSSItems  int // items parsed out of the RSS payloads, before any filtering

	// Drops, in the order the fetch path applies them. Relevance is NOT one of
	// them: ingest admits everything Google ranked and the Editor decides, having
	// read the body (PLAN-one-rail 8.8 deleted the MatchRejected counter with the
	// matcher it counted).
	WindowDropped  int // published before the lookback cutoff
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

	// ReadsWithheld is D-T21's cap biting: articles that WERE inserted but whose
	// Editor read was withheld because the entity had spent its daily allowance.
	//
	// Not a drop stage and NOT part of the Residual invariant — the cap acts inside
	// persistArticles, downstream of Matched, and it discards no article. The row is
	// written and keeps its provenance; only the read is withheld.
	//
	// It exists because without it the sweep log lies by omission. `fresh_articles`
	// counts the ids handed back for the Editor, so a fully-capped sweep prints
	// `fresh_articles=0` next to hundreds of genuinely new rows — the exact shape of
	// "a WARN that says continuing hides its own frequency". Read the two together:
	// fresh_articles is what will be READ, reads_withheld is what was STORED and skipped.
	ReadsWithheld int
}

// Add folds another funnel into f. Written out field by field on purpose: a new
// counter that someone forgets to add here shows up as a rolled-up total that
// disagrees with the per-entity lines, which is a louder failure than reflection
// quietly doing the right thing for a field nobody meant to aggregate.
func (f *Funnel) Add(o Funnel) {
	f.Entities += o.Entities
	f.EditionsPlanned += o.EditionsPlanned
	f.EditionsQueried += o.EditionsQueried
	f.QueriesPlanned += o.QueriesPlanned
	f.QueriesRun += o.QueriesRun
	f.RSSCalls += o.RSSCalls
	f.RSSErrors += o.RSSErrors
	f.RSSItems += o.RSSItems
	f.WindowDropped += o.WindowDropped
	f.DedupCollapsed += o.DedupCollapsed
	f.LimitTruncated += o.LimitTruncated
	f.Matched += o.Matched
	f.DescriptionBearing += o.DescriptionBearing
	f.DescriptionEmpty += o.DescriptionEmpty
	f.ReadsWithheld += o.ReadsWithheld
}

// Residual is the number of articles lost to a stage the funnel does not count.
// It must be zero. A non-zero value means a drop was added to the fetch path
// without a counter — exactly the blindness this type exists to prevent.
func (f Funnel) Residual() int {
	return f.RSSItems - f.WindowDropped - f.DedupCollapsed - f.LimitTruncated - f.Matched
}

// LogAttrs renders the funnel as slog key/value pairs so every emitter — per
// team, per sport, per run — uses one field vocabulary and the log stays
// greppable across granularities.
func (f Funnel) LogAttrs() []any {
	return []any{
		"entities", f.Entities,
		"editions_planned", f.EditionsPlanned,
		"editions_queried", f.EditionsQueried,
		"queries_planned", f.QueriesPlanned,
		"queries_run", f.QueriesRun,
		"rss_calls", f.RSSCalls,
		"rss_errors", f.RSSErrors,
		"rss_items", f.RSSItems,
		"window_dropped", f.WindowDropped,
		"dedup_collapsed", f.DedupCollapsed,
		"limit_truncated", f.LimitTruncated,
		"matched", f.Matched,
		"desc_bearing", f.DescriptionBearing,
		"desc_empty", f.DescriptionEmpty,
		"reads_withheld", f.ReadsWithheld,
		"residual", f.Residual(),
	}
}

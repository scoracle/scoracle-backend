// Package model holds the engine-independent analytical types both
// implementations of the analytics boundary produce. Keeping them in a leaf
// package lets the postgres and duckdb implementations share them without an
// import cycle.
package model

import "math"

// Trajectory window constants — the Rust Scout's TRAJECTORY_WINDOW_* values
// (rust/src/junctions/scout/mod.rs:955-957).
const (
	WindowPct = 0.10
	WindowMin = int64(3)
	WindowMax = int64(16)
)

// Trajectory is the Scout's recent-form result for one entity and season:
// where the overall rating z-score is and which way it is moving. It mirrors
// the RatingTrajectory produced by the Rust Scout
// (rust/src/junctions/scout/mod.rs load_rating_trajectory) so both engines
// can be compared field for field.
type Trajectory struct {
	// Key is the rising/falling/steady classification (±0.25 slope threshold).
	Key string
	// Label is the user-facing phrasing for Key.
	Label string
	// Reason is non-empty only for steady fallback paths
	// (sparse_recent_events | sparse_z_score_events).
	Reason string
	// EventsPlayed is the entity's count of scored events this season.
	EventsPlayed int64
	// WindowSize is the adaptive recent window:
	// clamp(round(events_played * 0.10), 3, 16).
	WindowSize int64
	// SampleSize is len(Series).
	SampleSize int
	// Slope is the raw OLS slope over the chronological window.
	Slope float64
	// Latest is the most recent window rating z, rounded to 1 decimal.
	Latest float64
	// Series holds the window rating z-scores most-recent-first, rounded to
	// 1 decimal.
	Series []float64
}

// WindowSize mirrors the Rust Scout's window computation:
// clamp(round(events_played * 0.10), 3, 16).
func WindowSize(eventsPlayed int64) int64 {
	v := int64(math.Round(float64(eventsPlayed) * WindowPct))
	if v < WindowMin {
		return WindowMin
	}
	if v > WindowMax {
		return WindowMax
	}
	return v
}

// round1 mirrors the Rust runtime's round1 (half away from zero, 1 decimal).
func round1(x float64) float64 {
	return math.Round(x*10.0) / 10.0
}

func roundedSeries(vals []float64) []float64 {
	out := make([]float64, len(vals))
	for i, v := range vals {
		out[i] = round1(v)
	}
	return out
}

func trajectoryKey(slope float64) string {
	switch {
	case slope > 0.25:
		return "rising"
	case slope < -0.25:
		return "falling"
	default:
		return "steady"
	}
}

func trajectoryLabel(key string) string {
	switch key {
	case "rising":
		return "overall scores trending up over recent games"
	case "falling":
		return "overall scores trending down over recent games"
	default:
		return "overall scores holding steady over recent games"
	}
}

// LinearSlope is a Go port of the Rust Scout's linear_slope
// (rust/src/junctions/scout/mod.rs:908): mean-centered OLS slope over
// [0..N-1], 0.0 for fewer than two values or a degenerate denominator. The
// Postgres engine uses it to reproduce the production path; the DuckDB engine
// uses SQL regr_slope, which is mathematically equivalent but not
// FP-accumulation identical — equivalence tests compare slopes within
// tolerance and classification labels exactly.
func LinearSlope(vals []float64) float64 {
	n := len(vals)
	if n < 2 {
		return 0.0
	}
	nF := float64(n)
	meanX := (nF - 1.0) / 2.0
	sum := 0.0
	for _, y := range vals {
		sum += y
	}
	meanY := sum / nF
	var num, den float64
	for i, y := range vals {
		dx := float64(i) - meanX
		num += dx * (y - meanY)
		den += dx * dx
	}
	if math.Abs(den) < 1e-9 {
		return 0.0
	}
	return num / den
}

// NewTrajectory assembles a Trajectory from shared raw inputs, applying the
// Scout's sparse-sample fallbacks and rounding. Both engine implementations
// use it so equivalence tests compare like for like.
//
// eventsPlayed counts scored events this season; seriesDesc holds the window
// rating z-scores most-recent-first (nil/short when the sample is too
// sparse); slope is the engine's OLS slope over the chronological window.
func NewTrajectory(eventsPlayed int64, seriesDesc []float64, slope float64) Trajectory {
	series := roundedSeries(seriesDesc)
	t := Trajectory{
		Key:          "steady",
		Label:        trajectoryLabel("steady"),
		EventsPlayed: eventsPlayed,
		WindowSize:   WindowSize(eventsPlayed),
		SampleSize:   len(series),
		Series:       series,
	}
	if eventsPlayed < WindowMin {
		t.Reason = "sparse_recent_events"
		return t
	}
	if len(seriesDesc) < int(WindowMin) {
		t.Reason = "sparse_z_score_events"
		return t
	}
	latest := 0.0
	if len(seriesDesc) > 0 {
		latest = seriesDesc[0]
	}
	key := trajectoryKey(slope)
	t.Key = key
	t.Label = trajectoryLabel(key)
	t.Slope = slope
	t.Latest = round1(latest)
	return t
}

// Approx reports |a-b| <= tol, used by engine equivalence gates.
func Approx(a, b float64, tol float64) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d <= tol
}

// BreakdownEntry is one measurement row of a rating bundle's breakdown,
// mirroring the migration-253 rating_breakdown JSONB shape:
// {label, measure, eligible, value, z, pct, in_comp, in_spec, sign, facet,
// scoped_pct{position, conference, division, league}}.
type BreakdownEntry struct {
	Label     string              `json:"label"`
	Measure   string              `json:"measure"`
	Eligible  bool                `json:"eligible"`
	Value     *float64            `json:"value"`
	Z         *float64            `json:"z"`
	Pct       *float64            `json:"pct"`
	InComp    bool                `json:"in_comp"`
	InSpec    bool                `json:"in_spec"`
	Sign      int                 `json:"sign"`
	Facet     string              `json:"facet"`
	ScopedPct map[string]*float64 `json:"scoped_pct"`
}

// BundleRow is one entity's rating-bundle result for a (sport, season,
// rate_mode) rebuild — the migration-253 _compute_rating_bundle contract.
// Nullable columns are nil where the engine produced NULL.
type BundleRow struct {
	PlayerID       int32               `json:"player_id"`
	LeagueID       int32               `json:"league_id"`
	Composite      *float64            `json:"composite"`
	CompositeRank  *float64            `json:"composite_rank"`
	CompositeScore *float64            `json:"composite_score"`
	Breakdown      []BreakdownEntry    `json:"breakdown"`
	ScopedRanks    map[string]*float64 `json:"scoped_ranks"`
	ScopedScores   map[string]*float64 `json:"scoped_scores"`
}

// EntityContextRow is one entity's season-grain cohort context: its rating
// arc (current vs prior season) against the season-over-season delta
// distribution of its league cohort. Derived knowledge — recomputable from
// player_stats/team_stats ratings; never authoritative. This is the shape
// stored in public.analytics_entity_context (migration 255) and consumed by
// memories.rs as a sourced record.
type EntityContextRow struct {
	Sport           string   `json:"sport"`
	EntityType      string   `json:"entity_type"`
	EntityID        int32    `json:"entity_id"`
	Season          int32    `json:"season"`
	LeagueID        int32    `json:"league_id"`
	Rating          float64  `json:"rating"`
	PriorSeason     *int32   `json:"prior_season"`
	PriorRating     *float64 `json:"prior_rating"`
	Delta           *float64 `json:"delta"`
	DeltaPctile     *float64 `json:"delta_pctile"`
	PeerCount       int32    `json:"peer_count"`
	PeerDeltaMedian *float64 `json:"peer_delta_median"`
	PeerDeltaP25    *float64 `json:"peer_delta_p25"`
	PeerDeltaP75    *float64 `json:"peer_delta_p75"`
}

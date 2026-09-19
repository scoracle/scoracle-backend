// Package duckdb is the experimental analytics engine implementation. It
// embeds DuckDB in-process via the official Go client and attaches the
// canonical Postgres database read-only through the postgres extension, so
// analytical queries run against the same data the production path serves.
// This package owns all DuckDB SQL; nothing else in the backend may reference
// the engine.
package duckdb

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
	_ "github.com/duckdb/duckdb-go/v2"
)

// Options configures the embedded engine.
type Options struct {
	// DatabaseURL is the canonical Postgres URL attached read-only.
	DatabaseURL string
	// Path is the DuckDB database file; empty means in-memory.
	Path string
	// MemoryLimit caps the engine's buffer-manager memory (e.g. "512MB").
	MemoryLimit string
}

// Analytics serves analytical context from the embedded DuckDB engine.
type Analytics struct {
	database *sql.DB
}

// Open initializes the engine, caps its resources, and attaches Postgres as
// the read-only upstream `pg`. The postgres extension is installed on first
// use (cached under ~/.duckdb afterwards).
func Open(ctx context.Context, opts Options) (*Analytics, error) {
	database, err := sql.Open("duckdb", opts.Path)
	if err != nil {
		return nil, fmt.Errorf("open duckdb: %w", err)
	}
	database.SetMaxOpenConns(1)
	database.SetConnMaxIdleTime(0)
	database.SetConnMaxLifetime(0)

	if opts.DatabaseURL == "" {
		// No upstream: in-memory engine only (unit tests, parity probes).
		return &Analytics{database: database}, nil
	}

	steps := []string{
		fmt.Sprintf("SET memory_limit='%s'", orDefault(opts.MemoryLimit, "512MB")),
		"SET threads=2",
		"INSTALL postgres",
		"LOAD postgres",
		fmt.Sprintf(attachQuery, opts.DatabaseURL),
	}
	for _, step := range steps {
		execCtx, cancel := context.WithTimeout(ctx, 5*time.Minute)
		_, err := database.ExecContext(execCtx, step)
		cancel()
		if err != nil {
			database.Close()
			return nil, fmt.Errorf("duckdb setup step %q: %w", step, err)
		}
	}
	return &Analytics{database: database}, nil
}

func orDefault(value, fallback string) string {
	if value == "" {
		return fallback
	}
	return value
}

// Close releases the engine and its Postgres attachment.
func (a *Analytics) Close(ctx context.Context) error {
	return a.database.Close()
}

func eventTable(entityType string) (table string, idCol string, err error) {
	switch entityType {
	case "player":
		return "event_box_scores", "player_id", nil
	case "team":
		return "event_team_stats", "team_id", nil
	default:
		return "", "", fmt.Errorf("unsupported trajectory entity type %q", entityType)
	}
}

// attachQuery mounts the canonical Postgres database as the read-only `pg`
// upstream (postgres extension).
const attachQuery = "ATTACH '%s' AS pg (TYPE POSTGRES, READ_ONLY)"

// bundleExpansion is pushed to Postgres verbatim through postgres_query('pg',
// ...). It is the migration-253 dp CTE (lasp + eff gate + measurement
// expansion) with the parameters inlined — the measurement-identity function
// rating_measurements stays Postgres-owned, so only the analytical core
// downstream is re-expressed in DuckDB. Placeholders: sport, season,
// rate_mode.
const bundleExpansion = `
WITH lasp AS (
    SELECT CASE WHEN %[1]s='FOOTBALL'
                THEN round(avg(NULLIF(stats->>'save_pct','')::numeric), 4) END AS asp
    FROM player_stats
    WHERE sport='FOOTBALL' AND season=%[2]d AND position='Goalkeeper'
      AND (stats->>'appearances')::numeric >= 15
),
eff AS MATERIALIZED (
    SELECT rt.stat_key,
           LEAST(rt.min_value, GREATEST(1, ceil(0.5 * COALESCE((
               SELECT MAX(NULLIF(ps2.stats->>rt.stat_key,'')::numeric)
               FROM player_stats ps2
               WHERE ps2.sport = %[1]s AND ps2.season = %[2]d), 0)))) AS min_value
    FROM public.rating_thresholds rt
    WHERE rt.sport = %[1]s
),
dp AS (
    SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
           tm.conference, tm.division,
           d.label, d.measure, d.value::float8 AS value, d.in_comp, d.in_spec, d.sign, d.facet,
           COALESCE((
               SELECT bool_and(COALESCE((ps.stats->>e.stat_key)::numeric, 0) >= e.min_value)
               FROM eff e
           ), FALSE) AS is_ranked
    FROM player_stats ps
    LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = %[1]s
    LEFT JOIN LATERAL (
        SELECT tts.stats->>'opp_possession_pct' AS opp
        FROM team_stats tts
        WHERE tts.team_id = ps.team_id AND tts.sport = %[1]s AND tts.season = %[2]d
        LIMIT 1
    ) topp ON %[1]s = 'FOOTBALL'
    CROSS JOIN lasp
    CROSS JOIN LATERAL public.rating_measurements(
        %[1]s,
        CASE WHEN %[1]s = 'FOOTBALL'
             THEN ps.stats || jsonb_strip_nulls(jsonb_build_object(
                      'team_opp_possession', topp.opp,
                      'league_avg_save_pct', lasp.asp))
             ELSE ps.stats END,
        %[3]s, ps.position) d
    WHERE ps.sport = %[1]s AND ps.season = %[2]d AND ps.stats <> '{}'::jsonb
)
SELECT * FROM dp`

// bundleQuery is the migration-253 analytical core (pop, z, comp, scored, bd,
// base, ranks, scoped CTEs) re-expressed in DuckSQL over the pushed-down dp
// rows. rating_score is inlined (067). Only the comp_flat composite is used,
// as in 253. scoped_pct nulls are stripped by the comparison layer, matching
// jsonb_strip_nulls on the Postgres side. Placeholders: %[1]s pushed
// expansion SQL, %[2]s sport literal.
const bundleQuery = `
WITH dp AS (
    SELECT * FROM postgres_query('pg', $sql$%[1]s$sql$)
),
pop AS (
    SELECT label, measure, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
    FROM dp WHERE is_ranked GROUP BY label, measure
),
z AS (
    SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
           d.label, d.measure, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
           CASE WHEN p.mean IS NOT NULL THEN COALESCE((d.value - p.mean) / p.sd, 0) END AS zr
    FROM dp d LEFT JOIN pop p USING (label, measure)
    WHERE d.value IS NOT NULL
),
comp AS (
    SELECT player_id, league_id, SUM(sign * zr) FILTER (WHERE in_comp) AS composite
    FROM z GROUP BY player_id, league_id
),
rk AS (
    SELECT DISTINCT player_id, league_id, is_ranked FROM dp
),
scored AS (
    SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,
           CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure) > min(sign*zr) OVER (PARTITION BY label, measure)
                THEN ROUND((percent_rank() OVER (PARTITION BY label, measure ORDER BY sign * zr ASC)) * 100, 1) END AS pct,
           CASE WHEN %[2]s IN ('NFL','FOOTBALL') AND position IS NOT NULL
                THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position) > min(sign*zr) OVER (PARTITION BY label, measure, position)
                     THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position ORDER BY sign*zr ASC)) * 100, 1) END END AS pct_position,
           CASE WHEN %[2]s IN ('NFL','NBA') AND position IS NOT NULL
                THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, conference) > min(sign*zr) OVER (PARTITION BY label, measure, position, conference)
                     THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, conference ORDER BY sign*zr ASC)) * 100, 1) END END AS pct_conference,
           CASE WHEN %[2]s='NFL' AND position IS NOT NULL
                THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, division) > min(sign*zr) OVER (PARTITION BY label, measure, position, division)
                     THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, division ORDER BY sign*zr ASC)) * 100, 1) END END AS pct_division,
           CASE WHEN %[2]s='FOOTBALL' AND position IS NOT NULL
                THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, league_id) > min(sign*zr) OVER (PARTITION BY label, measure, position, league_id)
                     THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, league_id ORDER BY sign*zr ASC)) * 100, 1) END END AS pct_league
    FROM z WHERE is_ranked AND zr IS NOT NULL
    UNION ALL
    SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,
           NULL, NULL, NULL, NULL, NULL
    FROM z WHERE NOT is_ranked OR zr IS NULL
),
bd AS (
    SELECT sub.player_id, sub.league_id,
           CAST(to_json(list(sub.e ORDER BY sub.label)) AS VARCHAR) AS breakdown
    FROM (
        SELECT s.player_id, s.league_id, s.label AS label, {
               'label': s.label, 'measure': s.measure, 'eligible': r.is_ranked,
               'value': s.value, 'z': round(s.zr, 4), 'pct': s.pct,
               'in_comp': s.in_comp, 'in_spec': s.in_spec, 'sign': s.sign, 'facet': s.facet,
               'scoped_pct': {'position': s.pct_position, 'conference': s.pct_conference,
                              'division': s.pct_division, 'league': s.pct_league}
           } AS e
        FROM scored s JOIN rk r USING (player_id, league_id)
    ) sub
    GROUP BY sub.player_id, sub.league_id
),
base AS (
    SELECT c.player_id, c.league_id,
           ROUND(c.composite, 4) AS composite,
           bd.breakdown, (rk.is_ranked AND c.composite IS NOT NULL) AS is_ranked
    FROM comp c
    JOIN bd USING (player_id, league_id)
    JOIN rk USING (player_id, league_id)
),
ranks AS (
    SELECT player_id, league_id, is_ranked,
           CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY composite ASC)) * 100, 1) END AS composite_rank,
           CASE WHEN is_ranked AND STDDEV_POP(composite) OVER (PARTITION BY is_ranked) IS NOT NULL
                     AND STDDEV_POP(composite) OVER (PARTITION BY is_ranked) <> 0
                THEN ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (composite - AVG(composite) OVER (PARTITION BY is_ranked))
                     / STDDEV_POP(composite) OVER (PARTITION BY is_ranked))), 1) END AS composite_score
    FROM base
),
dim AS (
    SELECT DISTINCT player_id, league_id, position, conference, division FROM dp
),
sc AS (
    SELECT b.player_id, b.league_id,
           CASE WHEN %[2]s IN ('NFL','FOOTBALL') THEN ROUND((percent_rank() OVER (PARTITION BY d.position ORDER BY b.composite ASC)) * 100, 1) END AS pos_pct,
           CASE WHEN %[2]s IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY d.position, d.conference ORDER BY b.composite ASC)) * 100, 1) END AS conf_pct,
           CASE WHEN %[2]s='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY d.position, d.division ORDER BY b.composite ASC)) * 100, 1) END AS div_pct,
           CASE WHEN %[2]s='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY d.position, d.league_id ORDER BY b.composite ASC)) * 100, 1) END AS league_pct,
           CASE WHEN %[2]s IN ('NFL','FOOTBALL') AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position) IS NOT NULL
                     AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position) <> 0
                THEN ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (b.composite - AVG(b.composite) OVER (PARTITION BY d.position))
                     / STDDEV_POP(b.composite) OVER (PARTITION BY d.position))), 1) END AS pos_score,
           CASE WHEN %[2]s IN ('NFL','NBA') AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.conference) IS NOT NULL
                     AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.conference) <> 0
                THEN ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (b.composite - AVG(b.composite) OVER (PARTITION BY d.position, d.conference))
                     / STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.conference))), 1) END AS conf_score,
           CASE WHEN %[2]s='NFL' AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.division) IS NOT NULL
                     AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.division) <> 0
                THEN ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (b.composite - AVG(b.composite) OVER (PARTITION BY d.position, d.division))
                     / STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.division))), 1) END AS div_score,
           CASE WHEN %[2]s='FOOTBALL' AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.league_id) IS NOT NULL
                     AND STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.league_id) <> 0
                THEN ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (b.composite - AVG(b.composite) OVER (PARTITION BY d.position, d.league_id))
                     / STDDEV_POP(b.composite) OVER (PARTITION BY d.position, d.league_id))), 1) END AS league_score
    FROM base b
    JOIN dim d USING (player_id, league_id)
    WHERE d.position IS NOT NULL AND b.is_ranked
)
SELECT b.player_id, b.league_id,
       CASE WHEN b.is_ranked THEN b.composite END AS composite,
       r.composite_rank, r.composite_score,
       b.breakdown,
       CAST(json_object('position', sc.pos_pct, 'conference', sc.conf_pct,
                   'division', sc.div_pct, 'league', sc.league_pct) AS VARCHAR) AS scoped_ranks,
       CAST(json_object('position', sc.pos_score, 'conference', sc.conf_score,
                   'division', sc.div_score, 'league', sc.league_score) AS VARCHAR) AS scoped_scores
FROM base b
JOIN ranks r USING (player_id, league_id)
LEFT JOIN sc USING (player_id, league_id)
`

// The whole Scout recent-form computation in one query: scored-event count,
// adaptive window (clamp(round(events*0.10),3,16)), the window series most
// recent first, and the OLS slope via regr_slope. Postgres is attached as pg.
//
// Ordering uses a deterministic (start_time, rating) tiebreaker matching the
// Postgres implementation; regr_slope is invariant under the affine shift of
// chrono_idx (global season indices with gaps), which is why no re-index is
// needed after the window filter.
const trajectoryQuery = `
WITH events AS (
    SELECT f.start_time AS start_time, e.rating::DOUBLE AS rating
    FROM pg.public.%[1]s e
    JOIN pg.public.fixtures f ON f.id = e.fixture_id
    WHERE e.%[2]s = $1 AND e.sport = $2 AND e.season = $3
      AND e.rating IS NOT NULL
),
ranked AS (
    SELECT start_time, rating,
           COUNT(*) OVER () AS events_played,
           ROW_NUMBER() OVER (ORDER BY start_time DESC, rating DESC) AS recent_idx,
           ROW_NUMBER() OVER (ORDER BY start_time ASC, rating DESC) AS chrono_idx
    FROM events
),
windowed AS (
    SELECT rating, chrono_idx, events_played
    FROM ranked
    WHERE events_played >= %[3]d
      AND recent_idx <= GREATEST(%[3]d, LEAST(%[4]d, CAST(ROUND(events_played * %[5]f) AS BIGINT)))
),
stats AS (
    SELECT COALESCE(MAX(events_played), 0) AS events_played, COUNT(*) AS sample_size
    FROM windowed
),
agg AS (
    SELECT COALESCE(regr_slope(rating, chrono_idx), 0.0) AS slope,
           COALESCE(CAST(to_json(list(rating ORDER BY chrono_idx DESC)) AS VARCHAR), '[]') AS series_json
    FROM windowed
)
SELECT s.events_played, s.sample_size, a.slope, a.series_json
FROM stats s, agg a
`

// RatingTrajectory computes the Scout's recent-form context in a single
// DuckDB query where the production path needs two Postgres round trips plus
// Rust-side OLS plumbing.
func (a *Analytics) RatingTrajectory(ctx context.Context, entityType string, entityID int32, sport string, season int32) (model.Trajectory, error) {
	table, idCol, err := eventTable(entityType)
	if err != nil {
		return model.Trajectory{}, err
	}

	var (
		eventsPlayed int64
		sampleSize   int64
		slope        float64
		seriesJSON   string
	)
	err = a.database.QueryRowContext(ctx, fmt.Sprintf(trajectoryQuery, table, idCol, model.WindowMin, model.WindowMax, model.WindowPct),
		entityID, sport, season).Scan(&eventsPlayed, &sampleSize, &slope, &seriesJSON)
	if err != nil {
		return model.Trajectory{}, fmt.Errorf("duckdb rating trajectory %s/%d: %w", entityType, entityID, err)
	}

	var series []float64
	if err := json.Unmarshal([]byte(seriesJSON), &series); err != nil {
		return model.Trajectory{}, fmt.Errorf("duckdb trajectory series %s/%d: %w", entityType, entityID, err)
	}
	return model.NewTrajectory(eventsPlayed, series, slope), nil
}

func parseBundleJSON(r *model.BundleRow, breakdown, scopedRanks, scopedScores sql.NullString) error {
	if breakdown.Valid && breakdown.String != "" && breakdown.String != "[]" {
		entries := []model.BreakdownEntry{}
		if err := json.Unmarshal([]byte(breakdown.String), &entries); err != nil {
			return fmt.Errorf("breakdown: %w", err)
		}
		r.Breakdown = entries
	}
	r.ScopedRanks = parseScopedJSON(scopedRanks)
	r.ScopedScores = parseScopedJSON(scopedScores)
	return nil
}

func parseScopedJSON(raw sql.NullString) map[string]*float64 {
	if !raw.Valid || raw.String == "" || raw.String == "{}" {
		return nil
	}
	scoped := map[string]*float64{}
	if err := json.Unmarshal([]byte(raw.String), &scoped); err != nil {
		return nil
	}
	if len(scoped) == 0 {
		return nil
	}
	return scoped
}

// RatingBundle recomputes the migration-253 rating bundle in DuckDB. The
// measurement expansion is pushed to Postgres verbatim (rating_measurements
// stays the single source of truth for measurement identity); everything from
// the cohort populations down (z-scores, composites, percent-rank breakdown,
// ranks, scoped ranks) runs in DuckDB.
func (a *Analytics) RatingBundle(ctx context.Context, sport string, season int32, rateMode string) ([]model.BundleRow, error) {
	if strings.ContainsAny(sport+rateMode, "'") {
		return nil, fmt.Errorf("invalid rating bundle parameter sport=%q rate_mode=%q", sport, rateMode)
	}
	pushed := fmt.Sprintf(bundleExpansion, sqlLiteral(sport), season, sqlLiteral(rateMode))
	query := fmt.Sprintf(bundleQuery, pushed, sqlLiteral(sport))

	rows, err := a.database.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("duckdb rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
	}
	defer rows.Close()

	out := []model.BundleRow{}
	for rows.Next() {
		var r model.BundleRow
		var breakdown, scopedRanks, scopedScores sql.NullString
		if err := rows.Scan(&r.PlayerID, &r.LeagueID, &r.Composite, &r.CompositeRank, &r.CompositeScore, &breakdown, &scopedRanks, &scopedScores); err != nil {
			return nil, fmt.Errorf("scan rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
		}
		if err := parseBundleJSON(&r, breakdown, scopedRanks, scopedScores); err != nil {
			return nil, fmt.Errorf("parse rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
		}
		out = append(out, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate rating bundle %s/%d/%s: %w", sport, season, rateMode, err)
	}
	return out, nil
}

// sqlLiteral inlines a validated parameter into pushed-down SQL
// (postgres_query takes a plain string; callers reject quote characters
// first).
func sqlLiteral(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "''") + "'"
}

// contextQuery computes the whole cohort's season-grain context in one
// statement: each entity's rating arc against the season-over-season delta
// distribution of its league cohort (inclusive of the entity itself, so
// delta_pctile is the rank among peers including self — peer_count likewise).
// Entities without a prior-season rating keep their row with nil deltas;
// entities without a current rating are absent (rating IS NOT NULL upstream).
// Placeholders: %[1]s sport literal, %[2]d season, %[3]s table,
// %[4]s id column.
const contextQuery = `
WITH events AS (
    SELECT COALESCE(league_id, 0) AS league_id, %[4]s AS entity_id, season,
           rating::DOUBLE AS rating
    FROM pg.public.%[3]s
    WHERE sport = %[1]s AND rating IS NOT NULL
),
cur AS (
    SELECT * FROM events WHERE season = %[2]d
),
prior AS (
    SELECT * FROM events WHERE season = %[2]d - 1
),
deltas AS (
    SELECT c.league_id, c.entity_id, c.season,
           p.season AS prior_season, c.rating, p.rating AS prior_rating,
           (c.rating - p.rating) AS delta
    FROM cur c
    LEFT JOIN prior p ON p.league_id = c.league_id AND p.entity_id = c.entity_id
),
peer AS (
    SELECT league_id, season,
           COUNT(*) AS peer_count,
           quantile_cont(delta, 0.5) AS med,
           quantile_cont(delta, 0.25) AS p25,
           quantile_cont(delta, 0.75) AS p75
    FROM deltas WHERE delta IS NOT NULL
    GROUP BY league_id, season
),
pct AS (
    SELECT league_id, season, entity_id,
           percent_rank() OVER (PARTITION BY league_id, season ORDER BY delta ASC) * 100 AS delta_pctile
    FROM deltas WHERE delta IS NOT NULL
)
SELECT d.league_id, d.entity_id, d.season, d.rating,
       d.prior_season, d.prior_rating, d.delta, p.delta_pctile,
       COALESCE(pr.peer_count, 0) AS peer_count, pr.med, pr.p25, pr.p75
FROM deltas d
LEFT JOIN pct p ON p.league_id = d.league_id AND p.season = d.season AND p.entity_id = d.entity_id
LEFT JOIN peer pr ON pr.league_id = d.league_id AND pr.season = d.season
`

// EntityContext produces the derived season-grain cohort context for a whole
// (sport, season) cohort — the shape the snapshot batch job writes back to
// Postgres (migration 255) for memories.rs to read as sourced records.
func (a *Analytics) EntityContext(ctx context.Context, sport string, season int32, entityType string) ([]model.EntityContextRow, error) {
	table, idCol, err := contextTable(entityType)
	if err != nil {
		return nil, err
	}
	if strings.ContainsAny(sport, "'") {
		return nil, fmt.Errorf("invalid entity context parameter sport=%q", sport)
	}
	query := fmt.Sprintf(contextQuery, sqlLiteral(sport), season, table, idCol)

	rows, err := a.database.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("duckdb entity context %s/%d/%s: %w", sport, season, entityType, err)
	}
	defer rows.Close()

	out := []model.EntityContextRow{}
	for rows.Next() {
		var r model.EntityContextRow
		r.Sport = sport
		r.EntityType = entityType
		r.Season = season
		if err := rows.Scan(&r.LeagueID, &r.EntityID, &r.Season, &r.Rating,
			&r.PriorSeason, &r.PriorRating, &r.Delta, &r.DeltaPctile,
			&r.PeerCount, &r.PeerDeltaMedian, &r.PeerDeltaP25, &r.PeerDeltaP75); err != nil {
			return nil, fmt.Errorf("scan entity context %s/%d/%s: %w", sport, season, entityType, err)
		}
		out = append(out, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate entity context %s/%d/%s: %w", sport, season, entityType, err)
	}
	return out, nil
}

func contextTable(entityType string) (table string, idCol string, err error) {
	switch entityType {
	case "player":
		return "player_stats", "player_id", nil
	case "team":
		return "team_stats", "team_id", nil
	default:
		return "", "", fmt.Errorf("unsupported entity context type %q", entityType)
	}
}

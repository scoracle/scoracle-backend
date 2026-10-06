package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
)

type MatchObservation struct {
	FixtureID int64    `json:"fixture_id"`
	PlayedAt  int64    `json:"played_at"`
	Value     *float64 `json:"value"`
}
type StatisticRequest struct {
	Kind         string             `json:"kind"`
	Version      string             `json:"version"`
	Metric       string             `json:"metric"`
	Unit         string             `json:"unit"`
	From         int64              `json:"from"`
	Split        int64              `json:"split"`
	Before       int64              `json:"before"`
	Observations []MatchObservation `json:"observations"`
}
type StatisticWindow struct {
	From     int64    `json:"from"`
	Before   int64    `json:"before"`
	Fixtures int      `json:"fixtures"`
	Measured int      `json:"measured"`
	Total    *float64 `json:"total"`
	PerMatch *float64 `json:"per_match"`
}
type StatisticFinding struct {
	Metric         string          `json:"metric"`
	Unit           string          `json:"unit"`
	Previous       StatisticWindow `json:"previous"`
	Current        StatisticWindow `json:"current"`
	PerMatchChange *float64        `json:"per_match_change"`
	PercentChange  *float64        `json:"percent_change"`
	FixtureIDs     []int64         `json:"fixture_ids"`
}

func (a *Analytics) StudyStatistic(ctx context.Context, req StatisticRequest) (StatisticFinding, error) {
	out := StatisticFinding{Metric: req.Metric, Unit: req.Unit, FixtureIDs: []int64{}, Previous: StatisticWindow{From: req.From, Before: req.Split}, Current: StatisticWindow{From: req.Split, Before: req.Before}}
	if req.Version != "match-statistic-v1" || req.From >= req.Split || req.Split >= req.Before || req.Metric == "" || req.Unit == "" || len(req.Observations) > 20000 {
		return out, fmt.Errorf("invalid statistic request")
	}
	seen := map[int64]bool{}
	for _, r := range req.Observations {
		if r.FixtureID <= 0 || seen[r.FixtureID] || (r.Value != nil && (math.IsNaN(*r.Value) || math.IsInf(*r.Value, 0))) {
			return out, fmt.Errorf("invalid statistic observation")
		}
		seen[r.FixtureID] = true
		if r.PlayedAt >= req.From && r.PlayedAt < req.Before {
			out.FixtureIDs = append(out.FixtureIDs, r.FixtureID)
		}
	}
	raw, err := json.Marshal(req.Observations)
	if err != nil {
		return out, err
	}
	_, err = a.database.ExecContext(ctx, `CREATE TEMP TABLE match_memory AS SELECT (value->>'fixture_id')::BIGINT fixture_id,(value->>'played_at')::BIGINT played_at,(value->>'value')::DOUBLE metric_value FROM json_each(?)`, string(raw))
	if err != nil {
		return out, err
	}
	rows, err := a.database.QueryContext(ctx, `SELECT played_at>=? current_window,count(*) fixtures,count(metric_value) measured,sum(metric_value),avg(metric_value) FROM match_memory WHERE played_at>=? AND played_at<? GROUP BY current_window`, req.Split, req.From, req.Before)
	if err != nil {
		return out, err
	}
	defer rows.Close()
	for rows.Next() {
		var current bool
		var window StatisticWindow
		if err = rows.Scan(&current, &window.Fixtures, &window.Measured, &window.Total, &window.PerMatch); err != nil {
			return out, err
		}
		if current {
			window.From = req.Split
			window.Before = req.Before
			out.Current = window
		} else {
			window.From = req.From
			window.Before = req.Split
			out.Previous = window
		}
	}
	if err = rows.Err(); err != nil {
		return out, err
	}
	// A partial stored sample remains visible, but does not establish a change.
	if out.Current.Fixtures > 0 && out.Previous.Fixtures > 0 && out.Current.Measured == out.Current.Fixtures && out.Previous.Measured == out.Previous.Fixtures {
		delta := *out.Current.PerMatch - *out.Previous.PerMatch
		out.PerMatchChange = &delta
		if *out.Previous.PerMatch != 0 {
			pct := 100 * delta / math.Abs(*out.Previous.PerMatch)
			out.PercentChange = &pct
		}
	}
	return out, nil
}

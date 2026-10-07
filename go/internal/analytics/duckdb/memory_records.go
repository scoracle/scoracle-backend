package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
)

// SourceOutcome is a stored attribution, not a generated publisher score.
type SourceOutcome struct {
	Publisher string `json:"publisher"`
	PlayerID  int64  `json:"player_id"`
	TeamID    int64  `json:"team_id"`
	Confirmed bool   `json:"confirmed"`
}
type SourceRecordsRequest struct {
	Kind         string          `json:"kind"`
	Version      string          `json:"version"`
	Observations []SourceOutcome `json:"observations"`
}
type SourceRecord struct {
	Publisher   string `json:"publisher"`
	Confirmed   int    `json:"confirmed"`
	Tracked     int    `json:"tracked"`
	Reliability int    `json:"reliability"`
}

// StudySourceRecords preserves the existing k=5 smoothing, computed over distinct
// publisher/player/team attributions. Reposts do not increase the sample size.
func (a *Analytics) StudySourceRecords(ctx context.Context, req SourceRecordsRequest) ([]SourceRecord, error) {
	if req.Version != "publisher-outcomes-v1" || len(req.Observations) > 20000 {
		return nil, fmt.Errorf("invalid publisher study request")
	}
	for _, r := range req.Observations {
		if r.Publisher == "" || r.PlayerID <= 0 || r.TeamID <= 0 {
			return nil, fmt.Errorf("invalid publisher observation")
		}
	}
	raw, err := json.Marshal(req.Observations)
	if err != nil {
		return nil, err
	}
	rows, err := a.database.QueryContext(ctx, `WITH observations AS (
        SELECT value->>'publisher' publisher, (value->>'player_id')::BIGINT player_id,
        (value->>'team_id')::BIGINT team_id, (value->>'confirmed')::BOOLEAN confirmed FROM json_each(?)
    ), pairs AS (
        SELECT publisher,player_id,team_id,bool_or(confirmed) confirmed FROM observations GROUP BY ALL
    ), counts AS (
        SELECT publisher,count(*) tracked,count(*) FILTER(WHERE confirmed) confirmed FROM pairs GROUP BY publisher
    ) SELECT publisher,confirmed,tracked,round(100.0*confirmed/(confirmed+5))::INTEGER reliability
      FROM counts ORDER BY reliability DESC,tracked DESC,publisher LIMIT 12`, string(raw))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []SourceRecord{}
	for rows.Next() {
		var r SourceRecord
		if err = rows.Scan(&r.Publisher, &r.Confirmed, &r.Tracked, &r.Reliability); err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

type ScoutRecordsRequest struct {
	Kind         string            `json:"kind"`
	Version      string            `json:"version"`
	EntityType   string            `json:"entity_type"`
	MaxReports   int               `json:"max_reports"`
	Personnel    []json.RawMessage `json:"personnel"`
	Availability []json.RawMessage `json:"availability"`
	Claims       []json.RawMessage `json:"claims"`
}
type ReportedMemory struct {
	Publisher        string `json:"publisher"`
	PublishedAt      string `json:"published_at"`
	ReportedHeadline string `json:"reported_headline"`
	Withdrawn        *bool  `json:"withdrawn,omitempty"`
	Disputed         *bool  `json:"disputed,omitempty"`
}

// StudyScoutRecords selects and deduplicates a bounded stored-record snapshot.
// Reconciliation dates are record dates; a withdrawal never establishes recovery.
// Literal publisher quotes remain claims; token overlap is not an established dispute.
func (a *Analytics) StudyScoutRecords(ctx context.Context, req ScoutRecordsRequest) ([]ReportedMemory, error) {
	if req.Version != "scout-records-v1" || (req.EntityType != "player" && req.EntityType != "team") || req.MaxReports < 1 || req.MaxReports > 20 || len(req.Personnel)+len(req.Availability)+len(req.Claims) > 20000 {
		return nil, fmt.Errorf("invalid Scout memory request")
	}
	raw, err := json.Marshal(req)
	if err != nil {
		return nil, err
	}
	_, err = a.database.ExecContext(ctx, `CREATE TEMP TABLE scout_memory_records AS
        SELECT 'availability' kind,value record FROM json_each(?,'$.availability')
        UNION ALL SELECT 'personnel',value FROM json_each(?,'$.personnel')
        UNION ALL SELECT 'claim',value FROM json_each(?,'$.claims')`, string(raw), string(raw), string(raw))
	if err != nil {
		return nil, err
	}
	var invalid int
	err = a.database.QueryRowContext(ctx, `SELECT count(*) FROM scout_memory_records WHERE
        CASE kind WHEN 'availability' THEN
            coalesce(record->>'kind','') NOT IN ('opened','returned','reverted') OR
            coalesce(record->>'event_kind','') NOT IN ('injury','suspension') OR
            coalesce(record->>'date_label','')='' OR coalesce(record->>'event_date_label','')='' OR
            coalesce(record->>'player_name','')=''
        WHEN 'personnel' THEN coalesce(record->>'kind','') NOT IN ('applied','reverted') OR
            coalesce(record->>'date_label','')='' OR coalesce(record->>'player_name','')=''
        ELSE coalesce(record->>'source','')='' OR coalesce(record->>'fact','')='' END`).Scan(&invalid)
	if err != nil {
		return nil, err
	}
	if invalid != 0 {
		return nil, fmt.Errorf("invalid Scout source record")
	}
	rows, err := a.database.QueryContext(ctx, `WITH labeled AS (
        SELECT *,CASE WHEN ?='player' THEN 'The player' ELSE record->>'player_name' END subject
        FROM scout_memory_records
    ), descriptions AS (
        SELECT 0 priority,'Adjudicated record' publisher,record->>'date_label' published_at,
        CASE record->>'kind'
          WHEN 'opened' THEN subject||' '||(record->>'event_kind')||' ('||(record->>'event_date_label')||') from '||coalesce(record->>'team_name','an unspecified club')||'; reported return '||coalesce(record->>'expected_return_label','not stated')
          WHEN 'returned' THEN subject||' availability recorded as resumed after '||(record->>'event_kind')||' dated '||(record->>'event_date_label')
          ELSE 'An earlier '||(record->>'event_kind')||'-report for '||subject||' dated '||(record->>'event_date_label')||' was withdrawn as incorrect; it is not evidence of a return' END headline,
        (record->>'kind')='reverted' withdrawn,false disputed FROM labeled WHERE kind='availability'
        UNION ALL
        SELECT 1,'Adjudicated record',record->>'date_label',
        CASE record->>'kind' WHEN 'reverted' THEN 'An earlier recorded move for '||subject||' to '||coalesce(record->>'new_team','an unspecified club')||' was reverted and is not in force'
        ELSE 'Recorded move for '||subject||' from '||coalesce(record->>'old_team','an unspecified club')||' to '||coalesce(record->>'new_team','an unspecified club') END,
        (record->>'kind')='reverted',false FROM labeled WHERE kind='personnel'
        UNION ALL
        SELECT 2,record->>'source',coalesce(strftime(to_timestamp((record->>'published_at')::BIGINT),'%Y-%m-%dT%H:%M:%SZ'),'unknown'),
        record->>'fact',false,false FROM labeled WHERE kind='claim'
    ), unique_records AS (
        SELECT DISTINCT * FROM descriptions
    ) SELECT publisher,published_at,headline,withdrawn,disputed FROM unique_records
      ORDER BY priority,published_at DESC,publisher,headline LIMIT ?`, req.EntityType, req.MaxReports)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []ReportedMemory{}
	for rows.Next() {
		var r ReportedMemory
		var withdrawn, disputed bool
		if err = rows.Scan(&r.Publisher, &r.PublishedAt, &r.ReportedHeadline, &withdrawn, &disputed); err != nil {
			return nil, err
		}
		if withdrawn {
			r.Withdrawn = &withdrawn
		}
		if disputed {
			r.Disputed = &disputed
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

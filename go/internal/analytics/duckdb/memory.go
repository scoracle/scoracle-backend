package duckdb

import (
	"context"
	"encoding/json"
	"fmt"
)

const MemoryVersion = "reporting-frequency-v1"

// MemoryRequest contains source observations prepared for one entity/window by
// the caller. Topic is a retrieval group, never an asserted real-world event.
type MemoryObservation struct {
	ArticleID   int64  `json:"article_id"`
	CanonicalID int64  `json:"canonical_id"`
	Topic       string `json:"topic"`
	Publisher   string `json:"publisher"`
	ReportedAt  int64  `json:"reported_at"`
	Headline    string `json:"headline"`
}
type MemoryRequest struct {
	Version      string              `json:"version"`
	From         int64               `json:"from"`
	Before       int64               `json:"before"`
	Limit        int                 `json:"limit"`
	PerTopic     int                 `json:"per_topic"`
	Observations []MemoryObservation `json:"observations"`
}
type PublisherCount struct {
	Publisher string `json:"publisher"`
	Articles  int    `json:"articles"`
}
type MemoryFinding struct {
	From           int64               `json:"from"`
	Before         int64               `json:"before"`
	Publishers     []PublisherCount    `json:"publishers"`
	Topic          string              `json:"topic"`
	ArticleCount   int                 `json:"article_count"`
	PublisherCount int                 `json:"publisher_count"`
	SourceIDs      []int64             `json:"source_ids"`
	Reports        []MemoryObservation `json:"reports"`
}

// StudyMemory computes frequency from distinct canonical articles. It excludes
// fresh/future/out-of-window input, collapses duplicate extractions and reposts,
// and ranks groups before choosing representative source observations.
func (a *Analytics) StudyMemory(ctx context.Context, req MemoryRequest) ([]MemoryFinding, error) {
	if req.Version != MemoryVersion || req.From >= req.Before || req.Limit < 1 || req.Limit > 20 || req.PerTopic < 1 || req.PerTopic > 3 || len(req.Observations) > 20000 {
		return nil, fmt.Errorf("invalid memory study request")
	}
	seen := map[int64]bool{}
	for _, r := range req.Observations {
		if r.ArticleID <= 0 || r.CanonicalID <= 0 || r.Topic == "" || r.Publisher == "" || r.Headline == "" || seen[r.ArticleID] {
			return nil, fmt.Errorf("invalid or duplicate memory observation")
		}
		seen[r.ArticleID] = true
	}
	raw, err := json.Marshal(req.Observations)
	if err != nil {
		return nil, err
	}
	// One private engine per request. Never shares a mutable table across callers.
	_, err = a.database.ExecContext(ctx, `CREATE TEMP TABLE memory_observations AS
 SELECT (value->>'article_id')::BIGINT article_id,(value->>'canonical_id')::BIGINT canonical_id,
 value->>'topic' topic,value->>'publisher' publisher,(value->>'reported_at')::BIGINT reported_at,
 value->>'headline' headline FROM json_each(?)`, string(raw))
	if err != nil {
		return nil, err
	}
	rows, err := a.database.QueryContext(ctx, `WITH eligible AS (
 SELECT *, row_number() OVER(PARTITION BY canonical_id ORDER BY reported_at,article_id) AS copy_rank
 FROM memory_observations WHERE reported_at>=? AND reported_at<?
 ), unique_reports AS (SELECT * FROM eligible WHERE copy_rank=1), groups AS (
 SELECT topic,count(*) article_count,count(DISTINCT lower(trim(publisher))) publisher_count,max(reported_at) latest,
 to_json(list(article_id ORDER BY article_id))::VARCHAR source_ids
 FROM unique_reports GROUP BY topic ORDER BY article_count DESC,publisher_count DESC,latest DESC,topic LIMIT ?
 ), publisher_counts AS (
 SELECT topic,lower(trim(publisher)) publisher,count(*) articles FROM unique_reports GROUP BY topic,lower(trim(publisher))
 ), publisher_lists AS (
 SELECT topic,to_json(list({'publisher':publisher,'articles':articles} ORDER BY articles DESC,publisher))::VARCHAR publishers
 FROM publisher_counts GROUP BY topic
 ), ranked AS (
 SELECT u.*,row_number() OVER(PARTITION BY u.topic ORDER BY u.reported_at DESC,u.article_id DESC) AS report_rank
 FROM unique_reports u JOIN groups g USING(topic)
 ) SELECT g.topic,g.article_count,g.publisher_count,g.source_ids,p.publishers,
 to_json(list({'article_id':r.article_id,'canonical_id':r.canonical_id,'topic':r.topic,'publisher':r.publisher,'reported_at':r.reported_at,'headline':r.headline} ORDER BY r.report_rank))::VARCHAR
 FROM groups g JOIN ranked r USING(topic) JOIN publisher_lists p USING(topic) WHERE r.report_rank<=?
 GROUP BY g.topic,g.article_count,g.publisher_count,g.latest,g.source_ids,p.publishers
 ORDER BY g.article_count DESC,g.publisher_count DESC,g.latest DESC,g.topic`, req.From, req.Before, req.Limit, req.PerTopic)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []MemoryFinding{}
	for rows.Next() {
		f := MemoryFinding{From: req.From, Before: req.Before}
		var ids, reports, publishers string
		if err = rows.Scan(&f.Topic, &f.ArticleCount, &f.PublisherCount, &ids, &publishers, &reports); err != nil {
			return nil, err
		}
		if err = json.Unmarshal([]byte(publishers), &f.Publishers); err != nil {
			return nil, err
		}
		if err = json.Unmarshal([]byte(ids), &f.SourceIDs); err != nil {
			return nil, err
		}
		if err = json.Unmarshal([]byte(reports), &f.Reports); err != nil {
			return nil, err
		}
		out = append(out, f)
	}
	return out, rows.Err()
}

// memory-study reads one bounded source snapshot from stdin and returns DuckDB
// findings. It has no database credentials, network attachment or write-back path.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"io"
	"os"
	"time"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func run() error {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	data, err := io.ReadAll(io.LimitReader(os.Stdin, 8*1024*1024+1))
	if err != nil {
		return err
	}
	if len(data) > 8*1024*1024 {
		return fmt.Errorf("memory snapshot exceeds byte budget")
	}
	var kind struct {
		Kind string `json:"kind"`
	}
	if err = json.Unmarshal(data, &kind); err != nil {
		return err
	}
	engine, err := duckdb.Open(ctx, duckdb.Options{MemoryLimit: "256MB"})
	if err != nil {
		return err
	}
	defer engine.Close(ctx)
	if kind.Kind == "statistic" {
		var req duckdb.StatisticRequest
		if err = json.Unmarshal(data, &req); err != nil {
			return err
		}
		result, err := engine.StudyStatistic(ctx, req)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(result)
	}
	if kind.Kind != "" && kind.Kind != "reporting" {
		return fmt.Errorf("unknown memory study kind")
	}
	var req duckdb.MemoryRequest
	if err = json.Unmarshal(data, &req); err != nil {
		return err
	}
	findings, err := engine.StudyMemory(ctx, req)
	if err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(findings)
}

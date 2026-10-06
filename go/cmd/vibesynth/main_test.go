package main

import (
	"context"
	"os"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
)

func TestHarvesterPlayerSelectionHasNoStorylineDependency(t *testing.T) {
	query := enumStaleSigilSQL(true)
	if strings.Contains(query, "storyline_entities") || strings.Contains(query, "JOIN storylines") {
		t.Fatal("Harvester Sigil selection still depends on storylines")
	}
	if !strings.Contains(query, "harvester_resolved_links") || !strings.Contains(query, "selected.tier='headliner'") {
		t.Fatal("Harvester Sigil selection lost its source-linked or headliner player route")
	}
	if !strings.Contains(enumStaleSigilSQL(false), "JOIN storylines") {
		t.Fatal("legacy selection changed before cutover")
	}
}

func TestHarvesterPlayerSelectionSQL(t *testing.T) {
	dsn := os.Getenv("HARVESTER_TEST_DATABASE_URL")
	if dsn == "" {
		t.Skip("set HARVESTER_TEST_DATABASE_URL to a disposable database with Harvester migrations")
	}
	pool, err := pgxpool.New(context.Background(), dsn)
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	for _, useHarvester := range []bool{false, true} {
		rows, err := pool.Query(context.Background(), enumStaleSigilSQL(useHarvester), "NBA", 2026)
		if err != nil {
			t.Fatalf("selection mode %v: %v", useHarvester, err)
		}
		rows.Close()
		if err := rows.Err(); err != nil {
			t.Fatalf("selection mode %v: %v", useHarvester, err)
		}
	}
}

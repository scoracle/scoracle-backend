// Package dataimport is the gap-driven stats importer — the data step of the
// daily sweep (pipeline -mode data).
//
// The shape (PLAN-weekly-fantasy-rail.md rev 2): free fantasy-sports feeds are
// the stats source; the feed itself detects the event. Each run refreshes
// schedules and rosters from fixed URLs, asks the database which finished
// fixtures have no stat rows yet (the gap query), fetches exactly those, and
// promotes them through finalize_fixture(). No detector, no daemon, no model
// work; a failed fetch is simply still in the gap tomorrow.
package dataimport

import (
	"compress/gzip"
	"context"
	"encoding/csv"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// fetchTimeout caps one file download. The largest file this package pulls is a
// season of weekly player stats (~15 MB of CSV); a minute is generous headroom,
// and the daily cadence means a timeout costs nothing but one day of freshness.
const fetchTimeout = 120 * time.Second

const userAgent = "scoracle-pipeline/1.0 (data import)"

var httpClient = &http.Client{Timeout: fetchTimeout}

// Table is one fetched CSV: a header index and the raw records. Column access
// goes through Get so a renamed or missing upstream column degrades to "" (and
// the numeric readers to 0) rather than a panic — nflverse adds columns freely
// and this package must not care.
type Table struct {
	idx  map[string]int
	rows [][]string
}

func (t *Table) Len() int { return len(t.rows) }

// Get returns the named column of row i, "" if the column does not exist.
func (t *Table) Get(i int, col string) string {
	j, ok := t.idx[col]
	if !ok || j >= len(t.rows[i]) {
		return ""
	}
	return t.rows[i][j]
}

// Num returns the named column as a float. "", "NA", and unparsable values are
// 0 — nflverse uses NA for not-applicable, which for counting stats is zero.
func (t *Table) Num(i int, col string) float64 {
	s := t.Get(i, col)
	if s == "" || s == "NA" {
		return 0
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 0
	}
	return v
}

// Has reports whether the column exists at all (distinct from being zero).
func (t *Table) Has(col string) bool {
	_, ok := t.idx[col]
	return ok
}

// fetchCSV downloads and parses one CSV. A 404 returns (nil, errNotPublished):
// nflverse creates each season's file only once the season has data, so before
// week 1 the current-season stats URL legitimately does not exist yet.
var errNotPublished = fmt.Errorf("not published yet (404)")

func fetchCSV(ctx context.Context, url string) (*Table, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("GET %s: %w", url, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("%s: %w", url, errNotPublished)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GET %s: status %d", url, resp.StatusCode)
	}

	var body io.Reader = resp.Body
	if strings.HasSuffix(url, ".gz") {
		gz, err := gzip.NewReader(resp.Body)
		if err != nil {
			return nil, fmt.Errorf("gunzip %s: %w", url, err)
		}
		defer gz.Close()
		body = gz
	}

	r := csv.NewReader(body)
	r.FieldsPerRecord = -1 // tolerate ragged rows; Get bounds-checks anyway
	header, err := r.Read()
	if err != nil {
		return nil, fmt.Errorf("read header %s: %w", url, err)
	}
	t := &Table{idx: make(map[string]int, len(header))}
	for i, h := range header {
		t.idx[strings.TrimSpace(h)] = i
	}
	for {
		rec, err := r.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", url, err)
		}
		t.rows = append(t.rows, rec)
	}
	return t, nil
}

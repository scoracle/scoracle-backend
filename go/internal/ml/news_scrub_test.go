package ml

import "testing"

// TestParseScrubRelevant locks the scrub verdict gate. applyVerdicts persists a
// whole article's scrub in ONE atomic UPDATE (over unnest'd arrays), so the
// verdict set this parser returns IS the unit that lands transactionally — a
// wrong parse would stamp the wrong vetted flags on every link at once. The
// contract a regression must not break:
//   - a clean object returns a 1-indexed set; out-of-range indices are dropped,
//     never panic and never widen the set;
//   - an EMPTY {"relevant": []} (or {}) is a SUCCESSFUL "none relevant" verdict
//     (ok=true, empty set) — the scrub analog of F-019's empty-narratives — not a
//     parse failure that would dead-letter a legitimately-irrelevant article;
//   - genuinely malformed output is ok=false, so ScrubArticle returns an error,
//     applyVerdicts never runs, and the article stays unscrubbed + retryable.
func TestParseScrubRelevant(t *testing.T) {
	cases := []struct {
		name    string
		raw     string
		n       int
		wantOK  bool
		wantSet map[int]bool
	}{
		{
			name:    "two relevant candidates",
			raw:     `{"relevant": [1, 3]}`,
			n:       3,
			wantOK:  true,
			wantSet: map[int]bool{1: true, 3: true},
		},
		{
			name:    "empty relevant array is a valid none-relevant verdict",
			raw:     `{"relevant": []}`,
			n:       3,
			wantOK:  true,
			wantSet: map[int]bool{},
		},
		{
			name:    "missing relevant key is a valid none-relevant verdict",
			raw:     `{}`,
			n:       3,
			wantOK:  true,
			wantSet: map[int]bool{},
		},
		{
			name:    "out-of-range indices are silently dropped",
			raw:     `{"relevant": [0, 1, 5, 2]}`,
			n:       3,
			wantOK:  true,
			wantSet: map[int]bool{1: true, 2: true}, // 0 and 5 are out of [1,3]
		},
		{
			name:    "object wrapped in prose is salvaged",
			raw:     "Sure, here you go: {\"relevant\": [2]} — done.",
			n:       3,
			wantOK:  true,
			wantSet: map[int]bool{2: true},
		},
		{
			name:   "prose with no object is a parse failure",
			raw:    `I cannot help with that.`,
			n:      3,
			wantOK: false,
		},
		{
			name:   "empty response is a parse failure",
			raw:    ``,
			n:      3,
			wantOK: false,
		},
		{
			name:   "malformed object is a parse failure",
			raw:    `{relevant: [1]}`,
			n:      3,
			wantOK: false,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			set, ok := parseScrubRelevant(tc.raw, tc.n)
			if ok != tc.wantOK {
				t.Fatalf("parseScrubRelevant ok = %v, want %v (raw=%q)", ok, tc.wantOK, tc.raw)
			}
			if !tc.wantOK {
				if set != nil {
					t.Fatalf("a failed parse must return a nil set, got %v", set)
				}
				return
			}
			if len(set) != len(tc.wantSet) {
				t.Fatalf("set size = %d, want %d (got %v, raw=%q)", len(set), len(tc.wantSet), set, tc.raw)
			}
			for idx := range tc.wantSet {
				if !set[idx] {
					t.Fatalf("expected candidate %d in relevant set, got %v (raw=%q)", idx, set, tc.raw)
				}
			}
		})
	}
}

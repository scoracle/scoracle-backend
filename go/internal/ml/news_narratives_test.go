package ml

import "testing"

// TestParseNarrativesEmptyArrayIsNoData locks FIRST-GPT-AUDIT Session 11 / F-019:
// a cleanly-closed narratives array — including the empty {"narratives": []} Gemma
// emits for a thin/vague corpus — is a SUCCESSFUL parse with zero narratives (a
// no-data outcome the caller turns into a marker), NOT a parse failure. A genuinely
// malformed/truncated response stays a failure so the work queue retries it rather
// than writing a spurious marker (generation_failed must not masquerade as no-data).
func TestParseNarrativesEmptyArrayIsNoData(t *testing.T) {
	cases := []struct {
		name      string
		raw       string
		wantOK    bool
		wantCount int
	}{
		{
			name:      "empty array is parseable no-data",
			raw:       `{"narratives": []}`,
			wantOK:    true,
			wantCount: 0,
		},
		{
			name:      "empty array with whitespace",
			raw:       `{"narratives": [ ]}`,
			wantOK:    true,
			wantCount: 0,
		},
		{
			name:      "one well-formed narrative",
			raw:       `{"narratives": [{"title":"A","body":"b","articles":[1]}]}`,
			wantOK:    true,
			wantCount: 1,
		},
		{
			name:      "two narratives",
			raw:       `{"narratives": [{"title":"A","body":"a","articles":[1]},{"title":"B","body":"b","articles":[2]}]}`,
			wantOK:    true,
			wantCount: 2,
		},
		{
			name:      "truncated tail salvages complete objects",
			raw:       `{"narratives": [{"title":"A","body":"a","articles":[1]},{"title":"B","bo`,
			wantOK:    true,
			wantCount: 1,
		},
		{
			name:      "no narratives key is a failure",
			raw:       `Sorry, I cannot help with that.`,
			wantOK:    false,
			wantCount: 0,
		},
		{
			name:      "key present but truncated before any object closes is a failure",
			raw:       `{"narratives": [{"title":"A","bo`,
			wantOK:    false,
			wantCount: 0,
		},
		{
			name:      "empty response is a failure",
			raw:       ``,
			wantOK:    false,
			wantCount: 0,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := parseNarratives(tc.raw)
			if ok != tc.wantOK {
				t.Fatalf("parseNarratives ok = %v, want %v (raw=%q)", ok, tc.wantOK, tc.raw)
			}
			if len(got.Narratives) != tc.wantCount {
				t.Fatalf("parseNarratives count = %d, want %d (raw=%q)", len(got.Narratives), tc.wantCount, tc.raw)
			}
		})
	}
}

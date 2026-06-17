package ml

import "testing"

func TestParseSentiment(t *testing.T) {
	cases := []struct {
		name    string
		raw     string
		want    int
		wantErr bool
	}{
		{"bare two-digit", "73", 73, false},
		{"bare three-digit clamps", "150", 100, false},
		{"with newline", "47\n", 47, false},
		{"trailing prose", "82 — the vibe is good", 82, false},
		{"leading prose", "The answer is 64.", 64, false},
		{"clamp high (>100)", "101", 100, false},
		{"clamp low (<1)", "0", 1, false},
		{"single-digit stays single-digit", "7", 7, false},
		{"multi-digit picks first", "85 out of 100", 85, false},
		{"no digits", "abc", 0, true},
		{"empty", "", 0, true},
		{"whitespace only", "   \n  ", 0, true},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := parseSentiment(tc.raw)
			if tc.wantErr {
				if err == nil {
					t.Fatalf("expected error, got %d", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tc.want {
				t.Fatalf("got %d, want %d", got, tc.want)
			}
		})
	}
}

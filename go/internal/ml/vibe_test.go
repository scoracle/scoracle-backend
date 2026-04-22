package ml

import "testing"

func TestParseSentiment(t *testing.T) {
	cases := []struct {
		name    string
		raw     string
		want    int
		wantErr bool
	}{
		{"bare digit", "7", 70, false},
		{"digit with newline", "7\n", 70, false},
		{"trailing prose", "8 — the vibe is good", 80, false},
		{"leading prose", "The answer is 8.", 80, false},
		{"clamp high", "11", 100, false},
		{"clamp low", "0", 10, false},
		{"multi-digit picks first", "7 out of 10", 70, false},
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

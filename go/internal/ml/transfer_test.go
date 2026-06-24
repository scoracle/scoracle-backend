package ml

import "testing"

// TestParseTransferVerdictFailClosed locks FIRST-GPT-AUDIT Session 10 / F-020:
// the transfer pipeline must FAIL CLOSED. parseTransferVerdict is the gate that
// decides whether Gemma's output is usable at all; analyzePair then routes a
// (verdict==nil OR is_rumor==nil) result to a fail-closed UNKNOWN row (is_rumor
// NULL, never served — reads require is_rumor IS TRUE). The two failure shapes a
// regression could reintroduce are:
//   - unparseable output treated as a (false) verdict instead of a parse failure;
//   - a verdict that never committed to is_rumor treated as a confident "cleared"
//     (is_rumor=false) instead of UNKNOWN.
//
// Both must keep the rumor OFF the served read path. This test asserts the parse
// contract the fail-closed routing depends on: ok=false for junk, and a non-nil
// IsRumor only when the model actually emitted the field.
func TestParseTransferVerdictFailClosed(t *testing.T) {
	cases := []struct {
		name        string
		raw         string
		wantOK      bool
		wantRumor   *bool // nil ⇒ is_rumor absent ⇒ fail-closed UNKNOWN
		description string
	}{
		{
			name:        "empty response is a parse failure",
			raw:         ``,
			wantOK:      false,
			description: "Gemma returned nothing → not a verdict → UNKNOWN/retry",
		},
		{
			name:        "prose with no JSON object is a parse failure",
			raw:         `Sorry, I cannot determine whether this is a transfer rumor.`,
			wantOK:      false,
			description: "a refusal must not be read as is_rumor=false",
		},
		{
			name:        "unterminated object is a parse failure",
			raw:         `{"is_rumor": tr`,
			wantOK:      false,
			description: "no closing brace → regex finds nothing → not a verdict",
		},
		{
			name:        "valid JSON but is_rumor omitted parses with nil is_rumor",
			raw:         `{"subject":"Player X","summary":"linked with a move"}`,
			wantOK:      true,
			wantRumor:   nil,
			description: "the CRUX of F-020: a missing is_rumor is NOT a cleared rumor — IsRumor stays nil so the caller fails closed",
		},
		{
			name:        "explicit is_rumor true is a served rumor",
			raw:         `{"is_rumor":true,"stage":"advanced_talks","confidence":0.8}`,
			wantOK:      true,
			wantRumor:   boolptr(true),
			description: "a confident rumor is the only thing that survives the read filter",
		},
		{
			name:        "explicit is_rumor false is a cleared row",
			raw:         `{"is_rumor":false,"subject":"roster note"}`,
			wantOK:      true,
			wantRumor:   boolptr(false),
			description: "a confident clear is recorded (and hidden by the read filter)",
		},
		{
			name:        "object wrapped in prose is salvaged",
			raw:         "Here is the verdict:\n{\"is_rumor\": true, \"stage\": \"here_we_go\"}\nDone.",
			wantOK:      true,
			wantRumor:   boolptr(true),
			description: "JSONMode notwithstanding, the regex defends against wrapping",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			v, ok := parseTransferVerdict(tc.raw)
			if ok != tc.wantOK {
				t.Fatalf("parseTransferVerdict ok = %v, want %v (%s; raw=%q)", ok, tc.wantOK, tc.description, tc.raw)
			}
			if !tc.wantOK {
				return // a failed parse routes to UNKNOWN regardless of struct contents
			}
			switch {
			case tc.wantRumor == nil && v.IsRumor != nil:
				t.Fatalf("IsRumor = %v, want nil (%s) — a non-nil here would let the caller treat an unspecified verdict as a real decision", *v.IsRumor, tc.description)
			case tc.wantRumor != nil && v.IsRumor == nil:
				t.Fatalf("IsRumor = nil, want %v (%s)", *tc.wantRumor, tc.description)
			case tc.wantRumor != nil && *v.IsRumor != *tc.wantRumor:
				t.Fatalf("IsRumor = %v, want %v (%s)", *v.IsRumor, *tc.wantRumor, tc.description)
			}
		})
	}
}

// TestNormStageDefaultsToSpeculation locks the rumor-shaping normalization: any
// stage the model returns that is not one of the four known stages collapses to
// the most conservative "speculation", and casing/spacing is normalized so
// "Advanced Talks" and "advanced_talks" are the same stage.
func TestNormStageDefaultsToSpeculation(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"speculation", "speculation"},
		{"concrete_interest", "concrete_interest"},
		{"advanced_talks", "advanced_talks"},
		{"here_we_go", "here_we_go"},
		{"Advanced Talks", "advanced_talks"}, // spaces + case normalized
		{"  HERE WE GO  ", "here_we_go"},
		{"done deal", "speculation"}, // unknown → conservative default
		{"", "speculation"},
	}
	for _, tc := range cases {
		if got := normStage(tc.in); got != tc.want {
			t.Fatalf("normStage(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

// TestClampConfBounds locks the confidence clamp to [0,1] so a model overshoot
// never persists an out-of-range confidence on a served rumor.
func TestClampConfBounds(t *testing.T) {
	cases := []struct {
		in, want float64
	}{
		{-0.5, 0}, {0, 0}, {0.42, 0.42}, {1, 1}, {1.5, 1},
	}
	for _, tc := range cases {
		if got := clampConf(tc.in); got != tc.want {
			t.Fatalf("clampConf(%v) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

func boolptr(b bool) *bool { return &b }

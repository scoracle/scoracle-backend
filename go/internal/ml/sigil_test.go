package ml

import "testing"

func intptr(i int) *int         { return &i }
func f64ptr(f float64) *float64 { return &f }

// sigilHash mirrors the production debounce key: build the input components from
// the three pillars, then hash them. This pair (buildSynthesisInputComponents +
// hashComponents) IS the event-driven convergence signal — Generate re-runs Gemma
// only when this hash differs from the entity-season's last stored input_hash.
func sigilHash(narr []synthNarrative, rating *synthRating, mom synthMomentum) string {
	return hashComponents(buildSynthesisInputComponents(narr, rating, mom))
}

// TestSynthesisHashStableAndOrderInvariant locks the debounce floor: identical
// grounding facts must hash identically (so an unchanged entity is NOT
// re-synthesized), and the hash must not depend on narrative ordering — titles
// are sorted before hashing, so the same set of narratives delivered in a
// different order is the SAME input. Without this, the work queue would churn
// Gemma calls every cycle.
func TestSynthesisHashStableAndOrderInvariant(t *testing.T) {
	rating := &synthRating{divinedPeak: "Scoring Forward", body: "ignored by hash", notability: 80}
	mom := synthMomentum{
		sentimentSlope:   0.7,
		compositeSlope:   -0.2,
		latestSentiment:  intptr(62),
		latestComposite:  f64ptr(71.3),
		latestVibePrompt: "quietly surging",
	}
	a := []synthNarrative{
		{title: "Trade buzz heats up", body: "b1", impact: 90},
		{title: "Career night", body: "b2", impact: 80},
	}
	// Same titles, reversed order, different bodies/impacts (neither is hashed).
	b := []synthNarrative{
		{title: "Career night", body: "DIFFERENT", impact: 1},
		{title: "Trade buzz heats up", body: "ALSO DIFFERENT", impact: 2},
	}

	h1 := sigilHash(a, rating, mom)
	h2 := sigilHash(b, rating, mom)
	if h1 == "" {
		t.Fatal("hash is empty — components failed to marshal")
	}
	if h1 != h2 {
		t.Fatalf("hash must be order-invariant and body-independent: %q != %q", h1, h2)
	}
	// Recomputing the identical input is deterministic.
	if again := sigilHash(a, rating, mom); again != h1 {
		t.Fatalf("hash is not deterministic: %q != %q", again, h1)
	}
}

// TestSynthesisHashConvergesOnEachPillar locks event-driven convergence: a real
// change in ANY of the three pillars (Rating, Vibe-narrative, Momentum) must
// change the hash so the entity re-converges. Each sub-case perturbs exactly one
// grounding fact and asserts a different hash from the baseline.
func TestSynthesisHashConvergesOnEachPillar(t *testing.T) {
	baseRating := &synthRating{divinedPeak: "Scoring Forward", notability: 80}
	baseMom := synthMomentum{
		latestSentiment:  intptr(62),
		latestComposite:  f64ptr(71.3),
		latestVibePrompt: "quietly surging",
	}
	baseNarr := []synthNarrative{{title: "Trade buzz", body: "b", impact: 50}}
	base := sigilHash(baseNarr, baseRating, baseMom)

	cases := []struct {
		name string
		narr []synthNarrative
		rate *synthRating
		mom  synthMomentum
	}{
		{
			name: "rating peak changes (stat identity pillar)",
			narr: baseNarr,
			rate: &synthRating{divinedPeak: "Playmaking Guard", notability: 80},
			mom:  baseMom,
		},
		{
			name: "rating notability changes",
			narr: baseNarr,
			rate: &synthRating{divinedPeak: "Scoring Forward", notability: 81},
			mom:  baseMom,
		},
		{
			name: "a new narrative appears (news pillar)",
			narr: []synthNarrative{{title: "Trade buzz", body: "b", impact: 50}, {title: "Injury scare", body: "c", impact: 70}},
			rate: baseRating,
			mom:  baseMom,
		},
		{
			name: "latest sentiment changes (momentum pillar)",
			narr: baseNarr,
			rate: baseRating,
			mom:  synthMomentum{latestSentiment: intptr(40), latestComposite: f64ptr(71.3), latestVibePrompt: "quietly surging"},
		},
		{
			name: "latest composite changes by a visible step (momentum pillar)",
			narr: baseNarr,
			rate: baseRating,
			mom:  synthMomentum{latestSentiment: intptr(62), latestComposite: f64ptr(73.9), latestVibePrompt: "quietly surging"},
		},
		{
			name: "vibe felt-read prose changes (Vibe end product)",
			narr: baseNarr,
			rate: baseRating,
			mom:  synthMomentum{latestSentiment: intptr(62), latestComposite: f64ptr(71.3), latestVibePrompt: "cooling off"},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := sigilHash(tc.narr, tc.rate, tc.mom); got == base {
				t.Fatalf("hash did not change for %q — this pillar change would NOT trigger re-synthesis", tc.name)
			}
		})
	}
}

// TestSynthesisHashDebouncesNoise locks the intentional stabilizers that stop
// convergence churn:
//   - latest_composite is round1'd, so sub-0.1 jitter does NOT reconverge;
//   - momentum SLOPES are display-only (prose), not part of the grounding facts,
//     so a slope-only change does NOT reconverge.
//
// A regression that folded raw composites or slopes into the hash would make the
// Sigil re-run Gemma on every cycle.
func TestSynthesisHashDebouncesNoise(t *testing.T) {
	rating := &synthRating{divinedPeak: "Scoring Forward", notability: 80}
	narr := []synthNarrative{{title: "Trade buzz", body: "b", impact: 50}}

	base := sigilHash(narr, rating, synthMomentum{
		sentimentSlope: 0.1, compositeSlope: 0.1,
		latestSentiment: intptr(62), latestComposite: f64ptr(71.32),
	})

	// Composite jitter under the 0.1 rounding floor → same hash.
	jitter := sigilHash(narr, rating, synthMomentum{
		sentimentSlope: 0.1, compositeSlope: 0.1,
		latestSentiment: intptr(62), latestComposite: f64ptr(71.34),
	})
	if jitter != base {
		t.Fatalf("sub-0.1 composite jitter changed the hash (round1 debounce broken): %q != %q", jitter, base)
	}

	// Slope-only change (latest values identical) → same hash.
	slopeOnly := sigilHash(narr, rating, synthMomentum{
		sentimentSlope: 9.9, compositeSlope: -9.9,
		latestSentiment: intptr(62), latestComposite: f64ptr(71.32),
	})
	if slopeOnly != base {
		t.Fatalf("slope-only change changed the hash (slopes must be display-only): %q != %q", slopeOnly, base)
	}
}

// TestSynthMomentumEmpty locks the no-pillars detection used to decide a marker
// vs a real synthesis. empty() is true ONLY when both the latest sentiment and
// latest composite are absent — a felt-read prompt alone is not enough signal to
// synthesize a Sigil.
func TestSynthMomentumEmpty(t *testing.T) {
	if !(synthMomentum{}).empty() {
		t.Fatal("zero momentum should be empty")
	}
	if !(synthMomentum{latestVibePrompt: "only prose"}).empty() {
		t.Fatal("a vibe prompt without sentiment/composite should still be empty")
	}
	if (synthMomentum{latestSentiment: intptr(50)}).empty() {
		t.Fatal("a latest sentiment makes momentum non-empty")
	}
	if (synthMomentum{latestComposite: f64ptr(50)}).empty() {
		t.Fatal("a latest composite makes momentum non-empty")
	}
}

// TestParseSynthesisResponse locks Gemma's two-line reply parsing, including the
// 1..100 score clamp and multi-line blurb absorption.
func TestParseSynthesisResponse(t *testing.T) {
	cases := []struct {
		name      string
		raw       string
		wantScore int
		wantBlurb string
	}{
		{
			name:      "clean two-line reply",
			raw:       "SCORE: 73\nBLURB: The forward is quietly surging.",
			wantScore: 73,
			wantBlurb: "The forward is quietly surging.",
		},
		{
			name:      "score clamps below 1",
			raw:       "SCORE: 0\nBLURB: In freefall.",
			wantScore: 1,
			wantBlurb: "In freefall.",
		},
		{
			name:      "score clamps above 100",
			raw:       "SCORE: 250\nBLURB: Dominant.",
			wantScore: 100,
			wantBlurb: "Dominant.",
		},
		{
			name:      "multi-line blurb is absorbed",
			raw:       "SCORE: 50\nBLURB: Steady.\nNothing dramatic to report.",
			wantScore: 50,
			wantBlurb: "Steady. Nothing dramatic to report.",
		},
		{
			name:      "leading/trailing whitespace tolerated",
			raw:       "\n  SCORE: 42  \n  BLURB: Holding pattern.  \n",
			wantScore: 42,
			wantBlurb: "Holding pattern.",
		},
		{
			name:      "garbage yields zero score and empty blurb",
			raw:       "I'm not sure how to score this.",
			wantScore: 0,
			wantBlurb: "",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			score, blurb := parseSynthesisResponse(tc.raw)
			if score != tc.wantScore {
				t.Fatalf("score = %d, want %d (raw=%q)", score, tc.wantScore, tc.raw)
			}
			if blurb != tc.wantBlurb {
				t.Fatalf("blurb = %q, want %q (raw=%q)", blurb, tc.wantBlurb, tc.raw)
			}
		})
	}
}

// TestLinearSlopeAndTrendDir locks the momentum direction signal feeding the
// synthesis prompt: slope sign tracks the series trend, a too-short series is
// flat, and trendDir maps slope magnitude to the labeled band.
func TestLinearSlopeAndTrendDir(t *testing.T) {
	if s := linearSlope([]float64{0, 1, 2, 3}); s != 1 {
		t.Fatalf("perfectly linear ramp slope = %v, want 1", s)
	}
	if s := linearSlope([]float64{3, 2, 1, 0}); s != -1 {
		t.Fatalf("descending ramp slope = %v, want -1", s)
	}
	if s := linearSlope([]float64{5, 5, 5}); s != 0 {
		t.Fatalf("flat series slope = %v, want 0", s)
	}
	if s := linearSlope([]float64{42}); s != 0 {
		t.Fatalf("single point slope = %v, want 0 (need >=2 points)", s)
	}

	dirs := []struct {
		slope float64
		want  string
	}{
		{2.0, "trending up strongly"},
		{0.5, "trending up"},
		{0.0, "steady"},
		{-0.5, "trending down"},
		{-2.0, "trending down strongly"},
	}
	for _, d := range dirs {
		if got := trendDir(d.slope); got != d.want {
			t.Fatalf("trendDir(%v) = %q, want %q", d.slope, got, d.want)
		}
	}
}

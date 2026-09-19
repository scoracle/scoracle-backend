package model

import "testing"

func TestRound1IsHalfAwayFromZero(t *testing.T) {
	cases := []struct {
		in   float64
		want float64
	}{
		{0.25, 0.3},
		{-0.25, -0.3},
		{0.24, 0.2},
		{1.96, 2.0},
		{-1.96, -2.0},
		{0, 0},
	}
	for _, c := range cases {
		if got := round1(c.in); got != c.want {
			t.Errorf("round1(%v) = %v, want %v", c.in, got, c.want)
		}
	}
}

func TestApprox(t *testing.T) {
	if !Approx(1.0, 1.0+1e-12, 1e-9) {
		t.Error("within tolerance reported false")
	}
	if Approx(1.0, 2.0, 0.5) {
		t.Error("beyond tolerance reported true")
	}
}

package main

import (
	"testing"
	"time"
)

func TestCoverageMonitorHourlyPollingAndImmediateCatchup(t *testing.T) {
	start := time.Date(2026, 10, 16, 12, 0, 0, 0, time.UTC)
	var monitor coverageMonitor
	for _, tc := range []struct {
		name  string
		after time.Duration
		open  bool
		due   bool
	}{
		{"disabled or character work", 0, false, false},
		{"startup catch-up", time.Minute, true, true},
		{"no duplicate at next tick", time.Minute + 15*time.Second, true, false},
		{"before hourly boundary", time.Hour, true, false},
		{"hourly boundary", time.Hour + time.Minute, true, true},
		{"draining or clock error", time.Hour + 2*time.Minute, false, false},
		{"character time", 2 * time.Hour, false, false},
		{"reopen catch-up", 2*time.Hour + time.Minute, true, true},
		{"catch-up does not repeat", 2*time.Hour + 2*time.Minute, true, false},
	} {
		if got := monitor.poll(tc.open, start.Add(tc.after)); got != tc.due {
			t.Errorf("%s: sweep=%v, want %v", tc.name, got, tc.due)
		}
	}
	var restarted coverageMonitor
	if !restarted.poll(true, start.Add(3*time.Hour)) {
		t.Fatal("a restart during coverage must catch up immediately")
	}
}

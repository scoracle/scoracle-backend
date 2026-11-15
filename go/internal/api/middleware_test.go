package api

import (
	"testing"
	"time"
)

func TestLimiterEvictsIdleVisitorsWithoutResettingActiveLimits(t *testing.T) {
	l := newIPLimiter(2, time.Minute)
	active := l.getLimiter("active")
	if !active.Allow() || active.Allow() {
		t.Fatal("expected exhausted active bucket")
	}
	l.getLimiter("idle")
	old := l.limiters["idle"]
	old.lastSeen = time.Now().Add(-l.idleTTL - time.Second)
	l.limiters["idle"] = old
	l.nextSweep = time.Time{}
	if l.getLimiter("active") != active || active.Allow() {
		t.Fatal("active bucket reset")
	}
	if _, ok := l.limiters["idle"]; ok {
		t.Fatal("idle visitor retained")
	}
	if !l.getLimiter("idle").Allow() {
		t.Fatal("returning visitor should get a full bucket")
	}
}

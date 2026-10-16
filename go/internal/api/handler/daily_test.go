package handler

import (
	"net/http/httptest"
	"testing"
)

func TestDailyReportingDate(t *testing.T) {
	for _, test := range []struct {
		query string
		ok    bool
		date  any
	}{
		{"", true, nil}, {"?date=2026-03-08", true, "2026-03-08"},
		{"?date=2026-02-30", false, nil}, {"?date=03-08-2026", false, nil},
		{"?week=1", false, nil}, {"?week=", false, nil}, {"?year=2026", false, nil},
		{"?scope=current_week", false, nil}, {"?scope=last_month", false, nil},
	} {
		w := httptest.NewRecorder()
		r := httptest.NewRequest("GET", "/"+test.query, nil)
		date, ok := optionalReportingDate(w, r)
		if ok != test.ok || date != test.date || (!ok && w.Code != 400) {
			t.Fatalf("%s: %v %v %d", test.query, date, ok, w.Code)
		}
	}
}

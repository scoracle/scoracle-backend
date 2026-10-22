package cache

import (
	"testing"
	"time"
)

func TestPublicationBoundaryChangesETagEvenForIdenticalJSON(t *testing.T) {
	for _, enabled := range []bool{false, true} {
		c := New(enabled)
		first := c.Set("card:2026-10-22:1", []byte(`{"body":null}`), time.Minute)
		next := c.Set("card:2026-10-23:2", []byte(`{"body":null}`), time.Minute)
		if first == next {
			t.Fatal("edition switch reused an ETag")
		}
		if enabled {
			_, etag, ok := c.Get("card:2026-10-22:1")
			if !ok || etag != first {
				t.Fatal("cache crossed edition boundary")
			}
		}
	}
}

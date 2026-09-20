package snapshot

import (
	"fmt"
	"math"
	"reflect"

	"github.com/albapepper/scoracle-data/internal/analytics/model"
)

// Tolerance is fixed before observation: absolute 1e-9 for unrounded
// percentile interpolation only. Membership, NULLs, ratings, deltas, counts,
// order and prior-season selection are exact. There is no persisted rounding
// in cohort-context-v1; this does not relax card/ranking thresholds.
const Tolerance = 1e-9

func Compare(want, got []model.EntityContextRow) error {
	if len(want) != len(got) {
		return fmt.Errorf("membership count: postgres=%d duckdb=%d", len(want), len(got))
	}
	for i, w := range want {
		g := got[i]
		wp := []*float64{w.DeltaPctile, w.PeerDeltaMedian, w.PeerDeltaP25, w.PeerDeltaP75}
		gp := []*float64{g.DeltaPctile, g.PeerDeltaMedian, g.PeerDeltaP25, g.PeerDeltaP75}
		for j, a := range wp {
			b := gp[j]
			if (a == nil) != (b == nil) {
				return fmt.Errorf("row %d numeric %d missingness", i, j)
			}
			if a != nil && (math.IsNaN(*a) || math.IsNaN(*b) || math.IsInf(*a, 0) || math.IsInf(*b, 0) || math.Abs(*a-*b) > Tolerance) {
				return fmt.Errorf("row %d numeric %d differs", i, j)
			}
		}
		w.DeltaPctile = nil
		w.PeerDeltaMedian = nil
		w.PeerDeltaP25 = nil
		w.PeerDeltaP75 = nil
		g.DeltaPctile = nil
		g.PeerDeltaMedian = nil
		g.PeerDeltaP25 = nil
		g.PeerDeltaP75 = nil
		if !reflect.DeepEqual(w, g) {
			return fmt.Errorf("row %d exact fields differ: postgres=%+v duckdb=%+v", i, w, g)
		}
	}
	return nil
}

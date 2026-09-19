// articulator-compose exports serving-identical slices from an extracted product
// bundle. Training and evaluation can use the Go composer before a deployment.
package main

import (
	"encoding/json"
	"fmt"
	"github.com/albapepper/scoracle-data/internal/articulator"
	"os"
)

func main() {
	var bundle map[string]json.RawMessage
	if err := json.NewDecoder(os.Stdin).Decode(&bundle); err != nil {
		fail(err)
	}
	inputs := articulator.Inputs{
		Meta: bundle["meta"], Rating: bundle["rating"], Momentum: bundle["momentum"],
		MomentumSummary: bundle["momentum_summary"], Results: bundle["results"],
		News: bundle["news"], Vibe: bundle["vibe"], Transfers: bundle["transfers"],
	}
	for _, kind := range []string{"p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"} {
		slice, err := articulator.Compose(kind, inputs)
		if err != nil {
			fail(err)
		}
		row := struct {
			Kind     string  `json:"kind"`
			Data     string  `json:"data"`
			Followup *string `json:"followup_data,omitempty"`
		}{kind, slice.Data, slice.FollowupData}
		if err := json.NewEncoder(os.Stdout).Encode(row); err != nil {
			fail(err)
		}
	}
}

func fail(err error) { fmt.Fprintln(os.Stderr, err); os.Exit(1) }

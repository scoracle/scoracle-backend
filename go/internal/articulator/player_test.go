package articulator

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestPlayerSlices(t *testing.T) {
	inputs := fixtureInputs()
	inputs.Meta = []byte(`{"name":"A Player","sport":"nba","entity_type":"player","position":"G","nationality":"USA","team":{"name":"A Team"}}`)
	for _, kind := range []string{"p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"} {
		t.Run(kind, func(t *testing.T) {
			slice, err := Compose(kind, inputs)
			if err != nil {
				t.Fatal(err)
			}
			if slice.EntityName != "A Player" {
				t.Fatalf("wrong identity: %s", slice.EntityName)
			}
			var data map[string]json.RawMessage
			if err := json.Unmarshal([]byte(slice.Data), &data); err != nil {
				t.Fatal(err)
			}
			if kind == "p1" {
				if !strings.Contains(string(data["meta"]), `"entity_type":"player"`) ||
					!strings.Contains(string(data["meta"]), `"team":"A Team"`) {
					t.Fatalf("player identity missing: %s", slice.Data)
				}
				if strings.Contains(string(data["meta"]), "venue") {
					t.Fatal("team metadata leaked")
				}
			}
			if kind == "p4" {
				if _, ok := data["record"]; ok {
					t.Fatal("player inherited a team's record")
				}
				if got := string(data["performances"]); got != `[{"date":"2026-08-30","score":89.7}]` {
					t.Fatalf("unexpected individual performances: %s", got)
				}
			}
		})
	}
}

func TestPlayerResultsMissingIsHonestNull(t *testing.T) {
	inputs := Inputs{Meta: []byte(`{"name":"A Player","sport":"football","entity_type":"player"}`)}
	slice, err := Compose("p4", inputs)
	if err != nil {
		t.Fatal(err)
	}
	if slice.Data != `{"name":"A Player","performances":null}` {
		t.Fatal(slice.Data)
	}
}

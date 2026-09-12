package articulator

import (
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func fixtureInputs() Inputs {
	return Inputs{
		Meta: []byte(`{
			"name":"Chelsea","sport":"football","short_code":"CHE",
			"country":"England","city":"London","venue":"Stamford Bridge",
			"conference":null,"division":null,"tier":"headliner"
		}`),
		Rating: []byte(`{
			"rating":{"season":2026,"rating_score":56.3,"rating_rank":64.7,
				"rating_breakdown":[
					{"label":"Cards","facet":"defense","pct":82.4,"value":1},
					{"label":"Clean Sheets","facet":"defense","pct":0.0,"value":0},
					{"label":"Goals Against","facet":"defense","pct":23.5,"value":3},
					{"label":"Tackling","facet":"defense","pct":5.9,"value":20},
					{"label":"xG Against","facet":"defense","pct":82.4,"value":1.46},
					{"label":"Goals For","facet":"offense","pct":64.7,"value":4}
				]},
			"commentary":{"body":"Strong cards. The attack has room to improve.",
				"rating_trajectory":"steady","rating_trajectory_label":null}
		}`),
		Momentum: []byte(`{
			"window":{"games_used":3},
			"entity_recent_avgs":{"a":20.004,"b":5.0,"c":10.0,"d":12.0,"e":18.0},
			"entity_season_avgs":{"a":10.0,"b":10.0,"c":10.0,"d":10.0,"e":10.0},
			"peer_season_avgs":{"a":11.0,"b":9.0,"c":10.0,"d":8.0,"e":12.0},
			"peer_cohort_size":17,"entity_season_score_avg":56.2,
			"peer_season_score_avg":50.9,"entity_season_score_rank":76.5,
			"entity_event_scores":[
				{"start_time":"2026-08-30T15:00:20-04:00","composite_score":89.7},
				{"start_time":"2026-08-20T15:00:20-04:00","composite_score":null}
			],
			"entity_season_sentiment_series":[
				{"date":"2026-09-10","sentiment_avg":62,"snapshot_count":4},
				{"date":"2026-09-11","sentiment_avg":67,"snapshot_count":5}
			]
		}`),
		MomentumSummary: []byte(`{
			"summary":{"direction":"rising","headline":"Chelsea is rising steadily.",
				"body":"The form is rising on a modest sample. The mood agrees."}
		}`),
		Results: []byte(`{"results":[
			{"start_time":"2026-09-09T18:35:51-04:00","home_away":"home","team_score":2,"opponent_score":1,"result":"W","composite_score":null,"opponent":{"name":"Leeds & United"}},
			{"start_time":"2026-09-06T13:59:00-04:00","home_away":"home","team_score":3,"opponent_score":0,"result":"W","composite_score":90.0,"opponent":{"name":"Arsenal"}},
			{"start_time":"2026-09-05T07:43:12-04:00","home_away":"away","team_score":1,"opponent_score":1,"result":"D","composite_score":50.5,"opponent":{"name":"Villa"}},
			{"start_time":"2026-08-30T15:00:20-04:00","home_away":"home","team_score":4,"opponent_score":3,"result":"W","composite_score":89.7,"opponent":{"name":"Brighton"}},
			{"start_time":"2026-08-27T16:31:07-04:00","home_away":"home","team_score":0,"opponent_score":2,"result":"L","composite_score":null,"opponent":{"name":"Luton"}}
		]}`),
		News: []byte(`{
			"scope":{"label":"Current week"},"card_score":88,
			"narratives":[{"headline":"Chelsea comeback","body":"Chelsea won a wild cup tie.","trajectory":"heating_up"}]
		}`),
		Vibe: []byte(`{
			"current":{"heat":82,"headline":null,"body":"Quiet confidence is building. The room feels settled."},
			"window_days":7,"snapshots":[{},{}]
		}`),
		Transfers: []byte(`{
			"wire_read":"Chelsea is pursuing one midfielder. The wire remains steady.",
			"card_score":68,"transfers":[
				{"name":"Manu Koné","direction":"incoming","stage":"concrete_interest","trajectory_label":"Developing story...","source_count":13},
				{"name":"Liam Delap","direction":"outgoing","stage":"speculation","trajectory_label":null,"source_count":3}
			]
		}`),
	}
}

func TestComposeAllKindsMatchTrainingShape(t *testing.T) {
	inputs := fixtureInputs()
	expected := map[string]string{
		"p1": `{"meta":{"name":"Chelsea","sport":"football","short_code":"CHE","country":"England","city":"London","venue":"Stamford Bridge","tier":"headliner"},"rating":{"rating_score":56.3,"rating_rank":64.7,"strongest":{"label":"Cards","pct":82.4,"value":1},"weakest":{"label":"Clean Sheets","pct":0.0,"value":0}},"narratives":[{"headline":"Chelsea comeback","body":"Chelsea won a wild cup tie.","trajectory":"heating_up"}],"sentiment":82}`,
		"p2": `{"name":"Chelsea","rating":{"season":2026,"rating_score":56.3,"rating_rank":64.7,"strengths":[{"label":"Cards","facet":"defense","pct":82.4,"value":1},{"label":"xG Against","facet":"defense","pct":82.4,"value":1.46},{"label":"Goals For","facet":"offense","pct":64.7,"value":4},{"label":"Goals Against","facet":"defense","pct":23.5,"value":3}],"weaknesses":[{"label":"Goals For","facet":"offense","pct":64.7,"value":4},{"label":"Goals Against","facet":"defense","pct":23.5,"value":3},{"label":"Tackling","facet":"defense","pct":5.9,"value":20},{"label":"Clean Sheets","facet":"defense","pct":0.0,"value":0}],"brief":"Strong cards. The attack has room to improve."}}`,
		"p3": `{"name":"Chelsea","momentum":{"games_used":3,"recent_avgs":{"a":20.0,"e":18.0,"b":5.0,"d":12.0},"season_avgs":{"a":10.0,"e":10.0,"b":10.0,"d":10.0},"peer_season_avgs":{"a":11.0,"e":12.0,"b":9.0,"d":8.0},"peer_cohort_size":17,"season_score_avg":56.2,"peer_season_score_avg":50.9,"season_score_rank":76.5,"event_scores":[{"date":"2026-08-30","score":89.7}],"sentiment_series":[{"date":"2026-09-10","sentiment_avg":62,"snapshot_count":4},{"date":"2026-09-11","sentiment_avg":67,"snapshot_count":5}],"analyst":{"direction":"rising","headline":"Chelsea is rising steadily.","read":"The form is rising on a modest sample. The mood agrees."}}}`,
		"p4": `{"name":"Chelsea","record":{"played":5,"wins":3,"losses":1,"form":"WWDWL","scored":10,"conceded":7,"draws":1,"points":10},"results":[{"date":"2026-09-09","home_away":"home","team_score":2,"opponent_score":1,"result":"W","composite_score":null,"opponent":"Leeds & United"},{"date":"2026-09-06","home_away":"home","team_score":3,"opponent_score":0,"result":"W","composite_score":90.0,"opponent":"Arsenal"},{"date":"2026-09-05","home_away":"away","team_score":1,"opponent_score":1,"result":"D","composite_score":50.5,"opponent":"Villa"},{"date":"2026-08-30","home_away":"home","team_score":4,"opponent_score":3,"result":"W","composite_score":89.7,"opponent":"Brighton"},{"date":"2026-08-27","home_away":"home","team_score":0,"opponent_score":2,"result":"L","composite_score":null,"opponent":"Luton"}]}`,
		"p5": `{"name":"Chelsea","news":{"scope":"Current week","card_score":88,"narratives":[{"headline":"Chelsea comeback","body":"Chelsea won a wild cup tie.","trajectory":"heating_up"}]}}`,
		"p6": `{"meta":{"name":"Chelsea","sport":"football"},"rating":{"rating_score":56.3,"rating_rank":64.7},"narratives":[{"headline":"Chelsea comeback","body":"Chelsea won a wild cup tie.","trajectory":"heating_up"}],"sentiment":82}`,
		"p7": `{"name":"Chelsea","vibe":{"heat":82,"body":"Quiet confidence is building. The room feels settled."}}`,
		"p8": `{"name":"Chelsea","transfers":{"wire_read":"Chelsea is pursuing one midfielder. The wire remains steady.","card_score":68,"calls":[{"name":"Manu Koné","direction":"incoming","stage":"concrete_interest","trajectory_label":"Developing story...","source_count":13},{"name":"Liam Delap","direction":"outgoing","stage":"speculation","source_count":3}]}}`,
	}

	for kind, want := range expected {
		t.Run(kind, func(t *testing.T) {
			got, err := Compose(kind, inputs)
			if err != nil {
				t.Fatalf("Compose(%s) error = %v", kind, err)
			}
			if got.Data != want {
				t.Fatalf("Compose(%s) data:\n got: %s\nwant: %s", kind, got.Data, want)
			}
		})
	}

	p6, err := Compose("p6", inputs)
	if err != nil {
		t.Fatal(err)
	}
	if p6.FollowupData == nil {
		t.Fatal("p6 followup_data is nil")
	}
	wantFollowup := `{"weaknesses":[{"label":"Goals For","pct":64.7,"value":4},{"label":"Goals Against","pct":23.5,"value":3},{"label":"Tackling","pct":5.9,"value":20},{"label":"Clean Sheets","pct":0.0,"value":0}]}`
	if *p6.FollowupData != wantFollowup {
		t.Fatalf("p6 followup_data:\n got: %s\nwant: %s", *p6.FollowupData, wantFollowup)
	}
}

func TestComposeRejectsUnknownKind(t *testing.T) {
	_, err := Compose("p9", fixtureInputs())
	if !errors.Is(err, ErrInvalidKind) {
		t.Fatalf("Compose(p9) error = %v, want ErrInvalidKind", err)
	}
}

func TestPeerProseCutsOnSentenceBoundary(t *testing.T) {
	text := "First sentence. " + string(make([]byte, 590)) + " trailing fragment"
	got := peerProse(&text)
	if got == nil || *got != "First sentence." {
		t.Fatalf("peerProse() = %v, want first complete sentence", got)
	}
}

// This optional cross-repo test is the highest-fidelity composer check: the Go
// port consumes the frozen raw API bundle while the Python source of truth
// consumes its corresponding slim bundle. Set SCORACLE_ARTICULATOR_ROOT to run
// it locally; ordinary backend CI remains self-contained.
func TestPythonSnapshotParity(t *testing.T) {
	root := os.Getenv("SCORACLE_ARTICULATOR_ROOT")
	if root == "" {
		t.Skip("SCORACLE_ARTICULATOR_ROOT is not set")
	}

	python := `
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
sys.path.insert(0, str(root / "eval"))
import build_prompts
out = {}
for path in sorted((root / "data/slim/teams").glob("*.json")):
    bundle = json.loads(path.read_text())
    slices = {}
    for shape in build_prompts.prompts_for(bundle, 0):
        row = {"data": json.dumps(shape[2], ensure_ascii=False, separators=(",", ":"))}
        if len(shape) > 3:
            row["followup_data"] = json.dumps(shape[4], ensure_ascii=False, separators=(",", ":"))
        slices[shape[0]] = row
    out[path.stem] = slices
print(json.dumps(out, ensure_ascii=False))
`
	cmd := exec.Command("python3", "-c", python, root)
	wantBytes, err := cmd.Output()
	if err != nil {
		t.Fatalf("Python composer failed: %v", err)
	}
	type expectedSlice struct {
		Data         string  `json:"data"`
		FollowupData *string `json:"followup_data"`
	}
	var expected map[string]map[string]expectedSlice
	if err := json.Unmarshal(wantBytes, &expected); err != nil {
		t.Fatal(err)
	}

	rawPaths, err := filepath.Glob(filepath.Join(root, "data/raw/teams/*.json"))
	if err != nil {
		t.Fatal(err)
	}
	if len(rawPaths) != len(expected) {
		t.Fatalf("raw/slim bundle count differs: raw=%d slim=%d", len(rawPaths), len(expected))
	}
	comparisons := 0
	for _, rawPath := range rawPaths {
		stem := filepath.Base(rawPath[:len(rawPath)-len(filepath.Ext(rawPath))])
		wantByKind, ok := expected[stem]
		if !ok {
			t.Fatalf("raw bundle %s has no corresponding slim bundle", stem)
		}
		rawBytes, err := os.ReadFile(rawPath)
		if err != nil {
			t.Fatal(err)
		}
		var raw map[string]json.RawMessage
		if err := json.Unmarshal(rawBytes, &raw); err != nil {
			t.Fatal(err)
		}
		inputs := Inputs{
			Meta: raw["meta"], Rating: raw["rating"], Momentum: raw["momentum"],
			MomentumSummary: raw["momentum_summary"], Results: raw["results"],
			News: raw["news"], Vibe: raw["vibe"], Transfers: raw["transfers"],
		}
		for _, kind := range []string{"p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"} {
			comparisons++
			t.Run(stem+"/"+kind, func(t *testing.T) {
				got, err := Compose(kind, inputs)
				if err != nil {
					t.Fatal(err)
				}
				want := wantByKind[kind]
				if got.Data != want.Data {
					t.Fatalf("DATA differs\n got: %s\nwant: %s", got.Data, want.Data)
				}
				if (got.FollowupData == nil) != (want.FollowupData == nil) {
					t.Fatalf("followup presence differs: got %v want %v", got.FollowupData, want.FollowupData)
				}
				if got.FollowupData != nil && *got.FollowupData != *want.FollowupData {
					t.Fatalf("followup differs\n got: %s\nwant: %s", *got.FollowupData, *want.FollowupData)
				}
			})
		}
	}
	if want := len(expected) * 8; comparisons != want {
		t.Fatalf("parity comparisons = %d, want %d", comparisons, want)
	}
}

# The Articulator slice endpoint — spec (2026-08-29)

The iOS chat app runs the fused/quantized Articulator (granite-4.0-h-1b + v4
LoRA, 6-bit MLX) on-device. The model was trained on DATA blocks composed by
`scoracle-articulator/eval/build_prompts.py` over bundles slimmed by
`extractor/slim_teams.py`. **Train and inference distributions must be
identical** — the standing doctrine of the whole project — so the API serves
the composed slice and the phone never re-implements slicing.

## Route

    GET /api/v1/{sport}/{entityType:player|team}/{id}/articulator/{kind:p1|p2|p3|p4|p5|p6|p7|p8}

Response:

    {
      "kind": "p3",
      "entity": {"sport": "football", "entity_type": "team", "entity_id": 18, "name": "Chelsea"},
      "data": "{\"name\":\"Chelsea\",\"momentum\":{...}}",     // the EXACT compact JSON string
      "followup_data": "{...}"                                  // p6 only; absent otherwise
    }

`data` is a STRING, not an object — pre-serialized server-side so the bytes
the model reads are decided in exactly one place. The client renders the user
turn as `question + "\n\n" + "DATA: " + data`, matching the corpus format.

## Serialization contract (what "identical" means)

- Compact JSON: separators `,` and `:`, no spaces — Python's
  `json.dumps(obj, ensure_ascii=False, separators=(",", ":"))`.
- **Key order matches the Python composition's insertion order.** In Go this
  means typed structs (fields marshal in declaration order), never
  `map[string]any`. Field order below is normative, copied from
  `build_prompts.py` / `slim_teams.py`.
- Numbers pass through from the DB/JSONB as their original literals wherever
  possible (`json.RawMessage`), so `57.68` never becomes `57.680000000000001`.
- Omitted-when-absent fields (Python's conditional key adds) become
  `omitempty` — but note Python omits on `None`/empty, not on zero: `0` and
  `0.0` are REAL values that must serialize. Use pointer fields.
- Teams only for now (the corpus is teams; players are a later phase).

## The eight kinds (source of truth: `build_prompts.py:prompts_for`)

All slices start from the slimmed card views (`slim_teams.py`), then trim
(`build_prompts.py`). The Go implementation composes both stages from the same
DB statements the per-card endpoints already use.

- **p1 profile** — `{"meta": {...}, **profile_slice}`: meta sans
  conference/division; rating {rating_score, rating_rank, strongest, weakest};
  momentum {trajectory_label}; narratives[:3] (headline/body/trajectory/
  freshness); sentiment (vibe heat → sparkline fallback).
- **p2 rating** — `{"name", "rating": rating_slice}`: season, rating_score,
  rating_rank, strengths[:4] / weaknesses[-4:] (label/facet/pct/value, sorted
  by pct desc), rating_trajectory(+label) only when label non-null, `brief` =
  commentary body sentence-trimmed to ≤600 bytes. **No brief_headline — the
  field is dropped from the corpus entirely (55% fabrication, 2026-08-26).**
- **p3 momentum** — `{"name", "momentum": momentum_slice}`: games_used;
  recent/season/peer_season avgs trimmed to the SAME 4 keys by `moved_most`
  (largest |recent−season|/|season|, skip zero baselines and keys absent from
  peer, ties alphabetical); peer_cohort_size; season_score_avg;
  peer_season_score_avg; season_score_rank; event_scores (≤16, date+score);
  sentiment_series (last 14, only when non-empty); rating_trajectory(+label)
  when labeled; analyst {direction, headline, read≤600B} when present.
- **p4 results** — `{"name", "record", "results"}`: record precomputed
  (played/wins/losses/form/scored/conceded; draws only when >0 AND sport !=
  nba; points = 3W+D football only); results[:5] (date/home_away/team_score/
  opponent_score/result/composite_score/opponent).
- **p5 news** — `{"name", "news"}`: scope label, card_score, narratives[:3]
  (headline/body/trajectory/freshness when present).
- **p6 follow-up** — turn 1 `{"meta": {name, sport}, **profile_slice(condensed)}`
  (condensed: narratives[:1], no strongest/weakest); turn 2 (`followup_data`)
  `{"weaknesses": [...]}` — the rating weaknesses as label/pct/value.
- **p7 mood** — `{"name", "vibe"}`: sentiment/heat/headline (when non-null),
  body ≤600B. NOTE the wire serves `heat`; the slice carries it as-is (the
  slim layer reads `heat` since 2026-08-26).
- **p8 wire** — `{"name", "transfers"}`: wire_read ≤600B, card_score,
  calls[:4] (name/direction/stage/trajectory_label/source_count when present).

`peer_prose` (the 600-byte cap): whitespace-collapse, then cut at the last
`. `/`? `/`! ` inside 600 bytes; NO fragment — if no sentence boundary fits,
the field is omitted entirely.

Empty-card behavior: p5/p7/p8 SERVE the slice even when the card is empty
(`{"name": ..., "news": null}` etc.) — quiet cards are the honest state and
the corpus deliberately trains on them (emit-when-empty).

## The parity gate (non-optional, the L2 discipline)

`scoracle-articulator/eval/parity_slices.py` (to be written alongside):
for every team in `data/slim/teams/` × all 8 kinds, compose the slice with
the Python code and fetch the Go endpoint; compare the STRINGS byte-for-byte.
The endpoint does not ship until 204 × 8 = 1,632 comparisons pass (modulo
teams whose live cards moved between extract and check — re-extract those and
re-run; the gate reports mismatching PATHS, not just counts).

## Non-goals (this phase)

- No question generation server-side — questions are the user's.
- No player slices, no board/hierarchy questions.
- No LLM calls server-side; the model runs on-device. (A server-side
  generation fallback for old phones is a later decision.)

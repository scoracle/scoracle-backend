// Package articulator composes the exact DATA strings consumed by the
// on-device Scoracle Articulator.
//
// The Python training source of truth lives in scoracle-articulator's
// extractor/slim_teams.py and eval/build_prompts.py. This package deliberately
// mirrors their ordering, omission, rounding, and prose-cap rules. JSON object
// order is therefore part of this API contract, not cosmetic formatting.
package articulator

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

const peerProseCap = 600

var ErrInvalidKind = errors.New("articulator kind must be p1 through p8")

// Inputs are the precomputed product payloads already served by the public API.
// Only the payloads needed for the requested kind must be present.
type Inputs struct {
	Meta            []byte
	Rating          []byte
	Momentum        []byte
	MomentumSummary []byte
	Results         []byte
	News            []byte
	Vibe            []byte
	Transfers       []byte
}

type Slice struct {
	EntityName   string
	Data         string
	FollowupData *string
}

type objectField struct {
	key   string
	value any
}

// orderedObject exists because encoding/json sorts map keys. The Articulator's
// training JSON used Python insertion order, so the production string must too.
type orderedObject []objectField

func field(key string, value any) objectField { return objectField{key, value} }

func (o orderedObject) MarshalJSON() ([]byte, error) {
	var out bytes.Buffer
	out.WriteByte('{')
	for i, item := range o {
		if i > 0 {
			out.WriteByte(',')
		}
		key, err := marshalCompact(item.key)
		if err != nil {
			return nil, err
		}
		value, err := marshalCompact(item.value)
		if err != nil {
			return nil, fmt.Errorf("marshal %s: %w", item.key, err)
		}
		out.Write(key)
		out.WriteByte(':')
		out.Write(value)
	}
	out.WriteByte('}')
	return out.Bytes(), nil
}

func marshalCompact(value any) ([]byte, error) {
	var out bytes.Buffer
	enc := json.NewEncoder(&out)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(value); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(out.Bytes(), []byte("\n")), nil
}

// pythonNumber preserves whether JSON parsed a number as int or float, then
// emits the shortest spelling Python's json.dumps uses (not PostgreSQL's
// numeric scale and not Go's integer-looking spelling for 100.0).
type pythonNumber struct {
	value   float64
	integer bool
	rawInt  string
}

func (n *pythonNumber) UnmarshalJSON(data []byte) error {
	s := string(data)
	if s == "null" {
		return errors.New("number cannot be null")
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return err
	}
	n.value = v
	n.integer = !strings.ContainsAny(s, ".eE")
	if n.integer {
		n.rawInt = s
	}
	return nil
}

func (n pythonNumber) MarshalJSON() ([]byte, error) {
	if n.integer {
		return []byte(n.rawInt), nil
	}
	s := strconv.FormatFloat(n.value, 'g', -1, 64)
	if !strings.ContainsAny(s, ".eE") {
		s += ".0"
	}
	return []byte(s), nil
}

func roundedNumber(n pythonNumber) pythonNumber {
	// Python preserves an int when round(int, 2) is called. That matters for
	// season totals such as 5366, while true averages like 289.0 remain floats.
	if n.integer {
		return n
	}
	// Formatting to two places mirrors round(value, 2), including binary-float
	// boundary behavior more closely than multiplying and math.RoundToEven.
	s := strconv.FormatFloat(n.value, 'f', 2, 64)
	v, _ := strconv.ParseFloat(s, 64)
	return pythonNumber{value: v}
}

type metaProduct struct {
	Name      string  `json:"name"`
	Sport     string  `json:"sport"`
	ShortCode *string `json:"short_code"`
	Country   *string `json:"country"`
	City      *string `json:"city"`
	Venue     *string `json:"venue"`
	Tier      *string `json:"tier"`
}

type ratingFacet struct {
	Label *string       `json:"label"`
	Facet *string       `json:"facet"`
	Pct   *pythonNumber `json:"pct"`
	Value *pythonNumber `json:"value"`
}

type ratingProduct struct {
	Rating *struct {
		Season    *pythonNumber `json:"season"`
		Score     *pythonNumber `json:"rating_score"`
		Rank      *pythonNumber `json:"rating_rank"`
		Breakdown []ratingFacet `json:"rating_breakdown"`
	} `json:"rating"`
	Commentary *struct {
		Body            *string `json:"body"`
		Trajectory      *string `json:"rating_trajectory"`
		TrajectoryLabel *string `json:"rating_trajectory_label"`
	} `json:"commentary"`
}

type momentumProduct struct {
	Window *struct {
		GamesUsed *pythonNumber `json:"games_used"`
	} `json:"window"`
	Recent             map[string]json.RawMessage `json:"entity_recent_avgs"`
	Season             map[string]json.RawMessage `json:"entity_season_avgs"`
	Peer               map[string]json.RawMessage `json:"peer_season_avgs"`
	PeerCohortSize     *pythonNumber              `json:"peer_cohort_size"`
	SeasonScoreAvg     *pythonNumber              `json:"entity_season_score_avg"`
	PeerSeasonScoreAvg *pythonNumber              `json:"peer_season_score_avg"`
	SeasonScoreRank    *pythonNumber              `json:"entity_season_score_rank"`
	EventScores        []momentumEvent            `json:"entity_event_scores"`
	SentimentSeries    []sentimentPoint           `json:"entity_season_sentiment_series"`
}

type momentumEvent struct {
	StartTime      *string       `json:"start_time"`
	CompositeScore *pythonNumber `json:"composite_score"`
}

type sentimentPoint struct {
	Date          *string       `json:"date"`
	SentimentAvg  *pythonNumber `json:"sentiment_avg"`
	SnapshotCount *pythonNumber `json:"snapshot_count"`
}

type momentumSummaryProduct struct {
	Summary *struct {
		Direction *string `json:"direction"`
		Headline  *string `json:"headline"`
		Body      *string `json:"body"`
	} `json:"summary"`
}

type resultsProduct struct {
	Results []resultRow `json:"results"`
}

type resultRow struct {
	StartTime      *string       `json:"start_time"`
	HomeAway       *string       `json:"home_away"`
	TeamScore      *pythonNumber `json:"team_score"`
	OpponentScore  *pythonNumber `json:"opponent_score"`
	Result         *string       `json:"result"`
	CompositeScore *pythonNumber `json:"composite_score"`
	Opponent       *struct {
		Name *string `json:"name"`
	} `json:"opponent"`
}

type newsProduct struct {
	Scope *struct {
		Label *string `json:"label"`
	} `json:"scope"`
	CardScore  *pythonNumber     `json:"card_score"`
	Narratives []json.RawMessage `json:"narratives"`
}

type vibeProduct struct {
	Current    json.RawMessage   `json:"current"`
	Snapshots  []json.RawMessage `json:"snapshots"`
	WindowDays *pythonNumber     `json:"window_days"`
}

type transfersProduct struct {
	WireRead  *string           `json:"wire_read"`
	CardScore *pythonNumber     `json:"card_score"`
	Transfers []json.RawMessage `json:"transfers"`
}

type slimRating struct {
	season          *pythonNumber
	score           *pythonNumber
	rank            *pythonNumber
	strengths       []orderedObject
	weaknesses      []orderedObject
	brief           *string
	trajectory      *string
	trajectoryLabel *string
}

type slimBundle struct {
	meta       metaProduct
	rating     *slimRating
	momentum   *momentumProduct
	summary    *momentumSummaryProduct
	results    []orderedObject
	news       any
	narratives []orderedObject
	vibe       any
	vibeMood   *pythonNumber
	transfers  any
}

func decode(data []byte, target any) error {
	if len(data) == 0 {
		return errors.New("required product payload is absent")
	}
	if err := json.Unmarshal(data, target); err != nil {
		return err
	}
	return nil
}

func Compose(kind string, inputs Inputs) (Slice, error) {
	if len(kind) != 2 || kind[0] != 'p' || kind[1] < '1' || kind[1] > '8' {
		return Slice{}, ErrInvalidKind
	}

	var bundle slimBundle
	if err := decode(inputs.Meta, &bundle.meta); err != nil {
		return Slice{}, fmt.Errorf("decode meta: %w", err)
	}
	if bundle.meta.Name == "" {
		return Slice{}, errors.New("meta payload has no team name")
	}

	if strings.Contains("p1p2p3p6", kind) {
		var product ratingProduct
		if err := decode(inputs.Rating, &product); err != nil {
			return Slice{}, fmt.Errorf("decode rating: %w", err)
		}
		bundle.rating = slimRatingProduct(product)
	}
	if strings.Contains("p1p3p6", kind) {
		if !isJSONNull(inputs.Momentum) {
			var product momentumProduct
			if err := decode(inputs.Momentum, &product); err != nil {
				return Slice{}, fmt.Errorf("decode momentum: %w", err)
			}
			bundle.momentum = &product
		}
	}
	if kind == "p3" {
		var product momentumSummaryProduct
		if err := decode(inputs.MomentumSummary, &product); err != nil {
			return Slice{}, fmt.Errorf("decode momentum summary: %w", err)
		}
		bundle.summary = &product
	}
	if kind == "p4" {
		var product resultsProduct
		if err := decode(inputs.Results, &product); err != nil {
			return Slice{}, fmt.Errorf("decode results: %w", err)
		}
		bundle.results = slimResults(product.Results)
	}
	if strings.Contains("p1p5p6", kind) {
		if !isJSONNull(inputs.News) {
			var product newsProduct
			if err := decode(inputs.News, &product); err != nil {
				return Slice{}, fmt.Errorf("decode news: %w", err)
			}
			news, narratives, err := slimNews(product)
			if err != nil {
				return Slice{}, fmt.Errorf("slim news: %w", err)
			}
			bundle.news, bundle.narratives = news, narratives
		}
	}
	if strings.Contains("p1p6p7", kind) {
		var product vibeProduct
		if err := decode(inputs.Vibe, &product); err != nil {
			return Slice{}, fmt.Errorf("decode vibe: %w", err)
		}
		vibe, mood, err := slimVibe(product)
		if err != nil {
			return Slice{}, fmt.Errorf("slim vibe: %w", err)
		}
		bundle.vibe, bundle.vibeMood = vibe, mood
	}
	if kind == "p8" {
		var product transfersProduct
		if err := decode(inputs.Transfers, &product); err != nil {
			return Slice{}, fmt.Errorf("decode transfers: %w", err)
		}
		transfers, err := slimTransfers(product)
		if err != nil {
			return Slice{}, fmt.Errorf("slim transfers: %w", err)
		}
		bundle.transfers = transfers
	}

	data, followup, err := composeKind(kind, bundle)
	if err != nil {
		return Slice{}, err
	}
	encoded, err := marshalCompact(data)
	if err != nil {
		return Slice{}, err
	}
	result := Slice{EntityName: bundle.meta.Name, Data: string(encoded)}
	if followup != nil {
		encodedFollowup, err := marshalCompact(followup)
		if err != nil {
			return Slice{}, err
		}
		text := string(encodedFollowup)
		result.FollowupData = &text
	}
	return result, nil
}

func slimRatingProduct(product ratingProduct) *slimRating {
	if product.Rating == nil {
		return nil
	}
	facets := append([]ratingFacet(nil), product.Rating.Breakdown...)
	sort.SliceStable(facets, func(i, j int) bool {
		if facets[i].Pct == nil {
			return false
		}
		if facets[j].Pct == nil {
			return true
		}
		return facets[i].Pct.value > facets[j].Pct.value
	})
	converted := make([]orderedObject, 0, len(facets))
	for _, f := range facets {
		converted = append(converted, orderedObject{
			field("label", f.Label), field("facet", f.Facet),
			field("pct", f.Pct), field("value", f.Value),
		})
	}
	strengthEnd := min(4, len(converted))
	weakStart := max(0, len(converted)-4)
	slim := &slimRating{
		season: product.Rating.Season, score: product.Rating.Score,
		rank:      product.Rating.Rank,
		strengths: converted[:strengthEnd], weaknesses: converted[weakStart:],
	}
	if product.Commentary != nil {
		slim.brief = product.Commentary.Body
		if nonempty(product.Commentary.TrajectoryLabel) {
			slim.trajectory = product.Commentary.Trajectory
			slim.trajectoryLabel = product.Commentary.TrajectoryLabel
		}
	}
	return slim
}

func slimResults(rows []resultRow) []orderedObject {
	if len(rows) > 5 {
		rows = rows[:5]
	}
	out := make([]orderedObject, 0, len(rows))
	for _, row := range rows {
		date := ""
		if row.StartTime != nil {
			date = *row.StartTime
			if len(date) > 10 {
				date = date[:10]
			}
		}
		var opponent *string
		if row.Opponent != nil {
			opponent = row.Opponent.Name
		}
		out = append(out, orderedObject{
			field("date", date), field("home_away", row.HomeAway),
			field("team_score", row.TeamScore), field("opponent_score", row.OpponentScore),
			field("result", row.Result), field("composite_score", row.CompositeScore),
			field("opponent", opponent),
		})
	}
	return out
}

func slimNews(product newsProduct) (any, []orderedObject, error) {
	narratives := make([]orderedObject, 0, min(3, len(product.Narratives)))
	for _, raw := range product.Narratives[:min(3, len(product.Narratives))] {
		var source map[string]json.RawMessage
		if err := json.Unmarshal(raw, &source); err != nil {
			return nil, nil, err
		}
		var narrative orderedObject
		for _, key := range []string{"headline", "body", "trajectory", "freshness"} {
			if value, ok := source[key]; ok {
				decoded, err := decodeNullableString(value)
				if err != nil {
					return nil, nil, fmt.Errorf("%s: %w", key, err)
				}
				narrative = append(narrative, field(key, decoded))
			}
		}
		narratives = append(narratives, narrative)
	}
	var scope *string
	if product.Scope != nil {
		scope = product.Scope.Label
	}
	news := orderedObject{
		field("scope", scope), field("card_score", product.CardScore),
		field("narratives", narratives),
	}
	return news, narratives, nil
}

func slimVibe(product vibeProduct) (any, *pythonNumber, error) {
	if len(product.Current) == 0 || bytes.Equal(bytes.TrimSpace(product.Current), []byte("null")) {
		return nil, nil, nil
	}
	var current map[string]json.RawMessage
	if err := json.Unmarshal(product.Current, &current); err != nil {
		return nil, nil, err
	}
	var out orderedObject
	var mood *pythonNumber
	for _, key := range []string{"sentiment", "heat", "headline", "body"} {
		raw, ok := current[key]
		if !ok || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
			continue
		}
		var value any
		var err error
		if key == "sentiment" || key == "heat" {
			var n pythonNumber
			err = json.Unmarshal(raw, &n)
			value = &n
			if mood == nil {
				mood = &n
			}
		} else {
			value, err = decodeNullableString(raw)
		}
		if err != nil {
			return nil, nil, fmt.Errorf("%s: %w", key, err)
		}
		out = append(out, field(key, value))
	}
	if product.WindowDays != nil && product.WindowDays.value != 0 {
		out = append(out, field("window_days", product.WindowDays))
	}
	if len(product.Snapshots) > 0 {
		out = append(out, field("snapshot_count", len(product.Snapshots)))
	}
	if len(out) == 0 {
		return nil, nil, nil
	}
	return out, mood, nil
}

func slimTransfers(product transfersProduct) (any, error) {
	var out orderedObject
	if nonempty(product.WireRead) {
		out = append(out, field("wire_read", product.WireRead))
	}
	if product.CardScore != nil {
		out = append(out, field("card_score", product.CardScore))
	}
	calls := make([]orderedObject, 0, min(4, len(product.Transfers)))
	for _, raw := range product.Transfers[:min(4, len(product.Transfers))] {
		var source map[string]json.RawMessage
		if err := json.Unmarshal(raw, &source); err != nil {
			return nil, err
		}
		var call orderedObject
		for _, key := range []string{"name", "direction", "stage", "trajectory_label", "source_count"} {
			rawValue, ok := source[key]
			if !ok || bytes.Equal(bytes.TrimSpace(rawValue), []byte("null")) {
				continue
			}
			if key == "source_count" {
				var n pythonNumber
				if err := json.Unmarshal(rawValue, &n); err != nil {
					return nil, err
				}
				call = append(call, field(key, &n))
			} else {
				value, err := decodeNullableString(rawValue)
				if err != nil {
					return nil, err
				}
				call = append(call, field(key, value))
			}
		}
		calls = append(calls, call)
	}
	if len(calls) > 0 {
		out = append(out, field("calls", calls))
	}
	if len(out) == 0 {
		return nil, nil
	}
	return out, nil
}

func composeKind(kind string, bundle slimBundle) (orderedObject, any, error) {
	name := bundle.meta.Name
	switch kind {
	case "p1":
		profile := profileFields(bundle, false)
		return append(orderedObject{field("meta", metaObject(bundle.meta))}, profile...), nil, nil
	case "p2":
		return orderedObject{field("name", name), field("rating", ratingSlice(bundle.rating))}, nil, nil
	case "p3":
		return orderedObject{field("name", name), field("momentum", momentumSlice(bundle))}, nil, nil
	case "p4":
		return orderedObject{
			field("name", name), field("record", summarizeResults(bundle.results, bundle.meta.Sport)),
			field("results", bundle.results),
		}, nil, nil
	case "p5":
		return orderedObject{field("name", name), field("news", bundle.news)}, nil, nil
	case "p6":
		meta := orderedObject{field("name", name), field("sport", bundle.meta.Sport)}
		data := append(orderedObject{field("meta", meta)}, profileFields(bundle, true)...)
		weaknesses := make([]orderedObject, 0)
		if bundle.rating != nil {
			weaknesses = make([]orderedObject, 0, len(bundle.rating.weaknesses))
			for _, facet := range bundle.rating.weaknesses {
				weaknesses = append(weaknesses, orderedObject{
					facet[0], facet[2], facet[3],
				})
			}
		}
		return data, orderedObject{field("weaknesses", weaknesses)}, nil
	case "p7":
		return orderedObject{field("name", name), field("vibe", vibeSlice(bundle.vibe))}, nil, nil
	case "p8":
		return orderedObject{field("name", name), field("transfers", transferSlice(bundle.transfers))}, nil, nil
	default:
		return nil, nil, ErrInvalidKind
	}
}

func metaObject(meta metaProduct) orderedObject {
	return orderedObject{
		field("name", meta.Name), field("sport", meta.Sport),
		field("short_code", meta.ShortCode), field("country", meta.Country),
		field("city", meta.City), field("venue", meta.Venue), field("tier", meta.Tier),
	}
}

func profileFields(bundle slimBundle, condensed bool) orderedObject {
	var out orderedObject
	if bundle.rating != nil {
		rating := orderedObject{}
		if bundle.rating.score != nil {
			rating = append(rating, field("rating_score", bundle.rating.score))
		}
		if bundle.rating.rank != nil {
			rating = append(rating, field("rating_rank", bundle.rating.rank))
		}
		if !condensed {
			rating = append(rating,
				field("strongest", extremeFacet(bundle.rating.strengths, true)),
				field("weakest", extremeFacet(bundle.rating.weaknesses, false)))
		}
		if hasNonNilValue(rating) {
			out = append(out, field("rating", rating))
		}
		if nonempty(bundle.rating.trajectoryLabel) {
			out = append(out, field("momentum", orderedObject{
				field("trajectory_label", bundle.rating.trajectoryLabel),
			}))
		}
	}
	narratives := bundle.narratives
	if condensed && len(narratives) > 1 {
		narratives = narratives[:1]
	}
	if len(narratives) > 0 {
		out = append(out, field("narratives", narratives))
	}
	mood := bundle.vibeMood
	if mood == nil && bundle.momentum != nil && len(bundle.momentum.SentimentSeries) > 0 {
		mood = bundle.momentum.SentimentSeries[len(bundle.momentum.SentimentSeries)-1].SentimentAvg
	}
	if mood != nil {
		out = append(out, field("sentiment", mood))
	}
	return out
}

func extremeFacet(facets []orderedObject, strongest bool) any {
	if len(facets) == 0 {
		return nil
	}
	best := facets[0]
	bestValue := facetPct(best)
	for _, candidate := range facets[1:] {
		value := facetPct(candidate)
		if (strongest && value > bestValue) || (!strongest && value < bestValue) {
			best, bestValue = candidate, value
		}
	}
	return orderedObject{best[0], best[2], best[3]}
}

func facetPct(facet orderedObject) float64 {
	if len(facet) > 2 {
		if n, ok := facet[2].value.(*pythonNumber); ok && n != nil {
			return n.value
		}
	}
	return 0
}

func ratingSlice(rating *slimRating) any {
	if rating == nil {
		return nil
	}
	out := orderedObject{
		field("season", rating.season), field("rating_score", rating.score),
		field("rating_rank", rating.rank), field("strengths", rating.strengths),
		field("weaknesses", rating.weaknesses),
	}
	if nonempty(rating.trajectoryLabel) {
		out = append(out, field("rating_trajectory", rating.trajectory),
			field("rating_trajectory_label", rating.trajectoryLabel))
	}
	if text := peerProse(rating.brief); text != nil {
		out = append(out, field("brief", text))
	}
	return out
}

func momentumSlice(bundle slimBundle) any {
	if bundle.momentum == nil {
		return nil
	}
	m := bundle.momentum
	recent := roundedMap(m.Recent)
	season := roundedMap(m.Season)
	peer := roundedMap(m.Peer)
	keys := movedMost(recent, season, peer)

	var gamesUsed *pythonNumber
	if m.Window != nil {
		gamesUsed = m.Window.GamesUsed
	}
	events := make([]orderedObject, 0, min(16, len(m.EventScores)))
	for _, event := range m.EventScores[:min(16, len(m.EventScores))] {
		if event.CompositeScore == nil {
			continue
		}
		date := ""
		if event.StartTime != nil {
			date = *event.StartTime
			if len(date) > 10 {
				date = date[:10]
			}
		}
		events = append(events, orderedObject{
			field("date", date), field("score", event.CompositeScore),
		})
	}
	out := orderedObject{
		field("games_used", gamesUsed), field("recent_avgs", pickNumbers(recent, keys)),
		field("season_avgs", pickNumbers(season, keys)),
		field("peer_season_avgs", pickNumbers(peer, keys)),
		field("peer_cohort_size", m.PeerCohortSize),
		field("season_score_avg", m.SeasonScoreAvg),
		field("peer_season_score_avg", m.PeerSeasonScoreAvg),
		field("season_score_rank", m.SeasonScoreRank), field("event_scores", events),
	}
	if len(m.SentimentSeries) > 0 {
		series := m.SentimentSeries
		if len(series) > 14 {
			series = series[len(series)-14:]
		}
		points := make([]orderedObject, 0, len(series))
		for _, point := range series {
			points = append(points, orderedObject{
				field("date", point.Date), field("sentiment_avg", point.SentimentAvg),
				field("snapshot_count", point.SnapshotCount),
			})
		}
		out = append(out, field("sentiment_series", points))
	}
	if bundle.rating != nil && nonempty(bundle.rating.trajectoryLabel) {
		out = append(out, field("rating_trajectory", bundle.rating.trajectory),
			field("rating_trajectory_label", bundle.rating.trajectoryLabel))
	}
	if bundle.summary != nil && bundle.summary.Summary != nil {
		s := bundle.summary.Summary
		read := peerProse(s.Body)
		if read != nil || nonempty(s.Direction) {
			var analyst orderedObject
			if nonempty(s.Direction) {
				analyst = append(analyst, field("direction", s.Direction))
			}
			if nonempty(s.Headline) {
				analyst = append(analyst, field("headline", s.Headline))
			}
			if read != nil {
				analyst = append(analyst, field("read", read))
			}
			out = append(out, field("analyst", analyst))
		}
	}
	return out
}

func vibeSlice(value any) any {
	card, ok := value.(orderedObject)
	if !ok {
		return nil
	}
	lookup := objectLookup(card)
	var out orderedObject
	for _, key := range []string{"sentiment", "heat", "headline"} {
		if value, ok := lookup[key]; ok && value != nil {
			out = append(out, field(key, value))
		}
	}
	if body, ok := lookup["body"].(*string); ok {
		if text := peerProse(body); text != nil {
			out = append(out, field("body", text))
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func transferSlice(value any) any {
	card, ok := value.(orderedObject)
	if !ok {
		return nil
	}
	lookup := objectLookup(card)
	var out orderedObject
	if read, ok := lookup["wire_read"].(*string); ok {
		if text := peerProse(read); text != nil {
			out = append(out, field("wire_read", text))
		}
	}
	for _, key := range []string{"card_score", "calls"} {
		if value, ok := lookup[key]; ok && value != nil {
			out = append(out, field(key, value))
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func summarizeResults(results []orderedObject, sport string) any {
	if len(results) == 0 {
		return nil
	}
	tally := map[string]int{"W": 0, "D": 0, "L": 0}
	form := strings.Builder{}
	scored := numberSum{}
	conceded := numberSum{}
	for _, result := range results {
		values := objectLookup(result)
		letter := "?"
		if value, ok := values["result"].(*string); ok && value != nil {
			letter = *value
			if _, known := tally[letter]; known {
				tally[letter]++
			}
		}
		form.WriteString(letter)
		if n, ok := values["team_score"].(*pythonNumber); ok {
			scored.add(n)
		}
		if n, ok := values["opponent_score"].(*pythonNumber); ok {
			conceded.add(n)
		}
	}
	out := orderedObject{
		field("played", len(results)), field("wins", tally["W"]),
		field("losses", tally["L"]), field("form", form.String()),
		field("scored", scored.number()), field("conceded", conceded.number()),
	}
	if sport != "nba" && tally["D"] > 0 {
		out = append(out, field("draws", tally["D"]))
	}
	if sport == "football" {
		out = append(out, field("points", 3*tally["W"]+tally["D"]))
	}
	return out
}

type numberSum struct {
	value   float64
	isFloat bool
}

func (s *numberSum) add(n *pythonNumber) {
	if n == nil {
		return
	}
	s.value += n.value
	s.isFloat = s.isFloat || !n.integer
}

func (s numberSum) number() any {
	if !s.isFloat {
		return int64(s.value)
	}
	return pythonNumber{value: s.value}
}

func roundedMap(raw map[string]json.RawMessage) map[string]pythonNumber {
	out := make(map[string]pythonNumber, len(raw))
	for key, value := range raw {
		var n pythonNumber
		if err := json.Unmarshal(value, &n); err == nil {
			out[key] = roundedNumber(n)
		}
	}
	return out
}

func movedMost(recent, season, peer map[string]pythonNumber) []string {
	type movement struct {
		score float64
		key   string
	}
	var scored []movement
	for key, r := range recent {
		s, ok := season[key]
		if !ok || s.value == 0 {
			continue
		}
		if _, ok := peer[key]; !ok {
			continue
		}
		delta := r.value - s.value
		if delta < 0 {
			delta = -delta
		}
		base := s.value
		if base < 0 {
			base = -base
		}
		scored = append(scored, movement{-delta / base, key})
	}
	sort.Slice(scored, func(i, j int) bool {
		if scored[i].score == scored[j].score {
			return scored[i].key < scored[j].key
		}
		return scored[i].score < scored[j].score
	})
	if len(scored) > 4 {
		scored = scored[:4]
	}
	keys := make([]string, len(scored))
	for i, item := range scored {
		keys[i] = item.key
	}
	return keys
}

func pickNumbers(values map[string]pythonNumber, keys []string) orderedObject {
	out := make(orderedObject, 0, len(keys))
	for _, key := range keys {
		if value, ok := values[key]; ok {
			v := value
			out = append(out, field(key, v))
		}
	}
	return out
}

func peerProse(text *string) *string {
	if !nonempty(text) {
		return nil
	}
	collapsed := strings.Join(strings.Fields(*text), " ")
	runes := []rune(collapsed)
	if len(runes) <= peerProseCap {
		return &collapsed
	}
	cut := string(runes[:peerProseCap])
	end := max(strings.LastIndex(cut, ". "),
		strings.LastIndex(cut, "? "), strings.LastIndex(cut, "! "))
	if end <= 0 {
		return nil
	}
	trimmed := cut[:end+1]
	return &trimmed
}

func decodeNullableString(raw json.RawMessage) (*string, error) {
	if bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return nil, nil
	}
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return nil, err
	}
	return &value, nil
}

func isJSONNull(raw []byte) bool {
	return len(raw) == 0 || bytes.Equal(bytes.TrimSpace(raw), []byte("null"))
}

func objectLookup(object orderedObject) map[string]any {
	out := make(map[string]any, len(object))
	for _, item := range object {
		out[item.key] = item.value
	}
	return out
}

func nonempty(value *string) bool { return value != nil && *value != "" }

func hasNonNilValue(object orderedObject) bool {
	for _, item := range object {
		if item.value != nil {
			return true
		}
	}
	return false
}

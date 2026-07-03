package transferidentity

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

type Application struct {
	ID                        int64           `json:"id"`
	Sport                     string          `json:"sport"`
	PlayerID                  int             `json:"player_id"`
	PlayerName                string          `json:"player_name,omitempty"`
	OldTeamID                 *int            `json:"old_team_id,omitempty"`
	OldTeamName               string          `json:"old_team_name,omitempty"`
	OldLeagueID               *int            `json:"old_league_id,omitempty"`
	NewTeamID                 int             `json:"new_team_id"`
	NewTeamName               string          `json:"new_team_name,omitempty"`
	NewLeagueID               *int            `json:"new_league_id,omitempty"`
	SourceRumorID             *int64          `json:"source_rumor_id,omitempty"`
	SourceSynthesisID         *int64          `json:"source_synthesis_id,omitempty"`
	DeterministicHeat         int16           `json:"deterministic_heat"`
	DeterministicConfidence   float64         `json:"deterministic_confidence"`
	ThresholdConfig           json.RawMessage `json:"threshold_config"`
	Adjudication              json.RawMessage `json:"adjudication"`
	AdjudicationRaw           string          `json:"adjudication_raw,omitempty"`
	AdjudicationModelVersion  string          `json:"adjudication_model_version,omitempty"`
	AdjudicationPromptVersion string          `json:"adjudication_prompt_version,omitempty"`
	Decision                  string          `json:"decision"`
	EventType                 string          `json:"event_type,omitempty"`
	AdjudicationConfidence    *float64        `json:"adjudication_confidence,omitempty"`
	Status                    string          `json:"status"`
	Reason                    string          `json:"reason,omitempty"`
	Evidence                  json.RawMessage `json:"evidence"`
	OverrideID                *int64          `json:"override_id,omitempty"`
	AppliedAt                 *time.Time      `json:"applied_at,omitempty"`
	RevertedAt                *time.Time      `json:"reverted_at,omitempty"`
	RevertedBy                string          `json:"reverted_by,omitempty"`
	RevertReason              string          `json:"revert_reason,omitempty"`
	CreatedAt                 time.Time       `json:"created_at"`
}

type Filters struct {
	Sport         string
	PlayerID      *int
	Status        string
	SourceRumorID *int64
	Limit         int
}

type CurrentIdentity struct {
	Sport           string     `json:"sport"`
	PlayerID        int        `json:"player_id"`
	TeamID          *int       `json:"team_id,omitempty"`
	LeagueID        *int       `json:"league_id,omitempty"`
	Source          string     `json:"source,omitempty"`
	SourceUpdatedAt *time.Time `json:"source_updated_at,omitempty"`
}

type AutofillVersion struct {
	Sport         string    `json:"sport"`
	Version       int64     `json:"version"`
	GeneratedAt   time.Time `json:"generated_at"`
	TotalEntities int       `json:"total_entities"`
	Status        string    `json:"status"`
	Reason        string    `json:"reason,omitempty"`
}

type RevertResult struct {
	ApplicationID int64            `json:"application_id"`
	OverrideID    *int64           `json:"override_id,omitempty"`
	Status        string           `json:"status"`
	Reason        string           `json:"reason"`
	Before        CurrentIdentity  `json:"before"`
	After         CurrentIdentity  `json:"after"`
	VersionBefore AutofillVersion  `json:"version_before"`
	VersionAfter  AutofillVersion  `json:"version_after"`
	Refresh       *AutofillVersion `json:"refresh,omitempty"`
	Application   Application      `json:"application"`
}

func List(ctx context.Context, pool *pgxpool.Pool, f Filters) ([]Application, error) {
	limit := f.Limit
	if limit <= 0 || limit > 500 {
		limit = 50
	}

	where := []string{"TRUE"}
	args := []any{}
	nextArg := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	if f.Sport != "" {
		where = append(where, "a.sport = "+nextArg(strings.ToUpper(f.Sport)))
	}
	if f.PlayerID != nil {
		where = append(where, "a.player_id = "+nextArg(*f.PlayerID))
	}
	if f.Status != "" {
		where = append(where, "a.status = "+nextArg(f.Status))
	}
	if f.SourceRumorID != nil {
		where = append(where, "a.source_rumor_id = "+nextArg(*f.SourceRumorID))
	}
	args = append(args, limit)

	sql := baseApplicationQuery() + `
		WHERE ` + strings.Join(where, " AND ") + `
		ORDER BY a.created_at DESC
		LIMIT $` + fmt.Sprint(len(args))

	rows, err := pool.Query(ctx, sql, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []Application
	for rows.Next() {
		app, err := scanApplication(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, app)
	}
	return out, rows.Err()
}

func Get(ctx context.Context, pool *pgxpool.Pool, id int64) (Application, error) {
	row := pool.QueryRow(ctx, baseApplicationQuery()+` WHERE a.id = $1`, id)
	return scanApplication(row)
}

func Revert(ctx context.Context, pool *pgxpool.Pool, applicationID int64, revertedBy, reason string) (RevertResult, error) {
	app, err := Get(ctx, pool, applicationID)
	if err != nil {
		return RevertResult{}, err
	}
	before, err := GetCurrentIdentity(ctx, pool, app.Sport, app.PlayerID)
	if err != nil {
		return RevertResult{}, err
	}
	versionBefore, err := GetAutofillVersion(ctx, pool, app.Sport)
	if err != nil {
		return RevertResult{}, err
	}

	var result RevertResult
	var overrideID *int64
	if err := pool.QueryRow(ctx, `
		SELECT application_id, override_id, status, reason
		FROM public.revert_applied_transfer_identity($1, $2, $3)`,
		applicationID, revertedBy, reason,
	).Scan(&result.ApplicationID, &overrideID, &result.Status, &result.Reason); err != nil {
		return RevertResult{}, err
	}
	result.OverrideID = overrideID
	result.Before = before
	result.VersionBefore = versionBefore

	if result.Status == "reverted" {
		refresh, err := RefreshSportAutofillConcurrent(ctx, pool, app.Sport, "revert_applied_transfer_identity")
		if err != nil {
			return RevertResult{}, err
		}
		result.Refresh = &refresh
	}

	after, err := GetCurrentIdentity(ctx, pool, app.Sport, app.PlayerID)
	if err != nil {
		return RevertResult{}, err
	}
	versionAfter, err := GetAutofillVersion(ctx, pool, app.Sport)
	if err != nil {
		return RevertResult{}, err
	}
	updated, err := Get(ctx, pool, applicationID)
	if err != nil {
		return RevertResult{}, err
	}
	result.After = after
	result.VersionAfter = versionAfter
	result.Application = updated
	return result, nil
}

func GetCurrentIdentity(ctx context.Context, pool *pgxpool.Pool, sport string, playerID int) (CurrentIdentity, error) {
	var out CurrentIdentity
	err := pool.QueryRow(ctx, `
		SELECT sport, player_id, team_id, league_id, COALESCE(source, ''), source_updated_at
		FROM public.player_current_identity
		WHERE sport = $1 AND player_id = $2`,
		strings.ToUpper(sport), playerID,
	).Scan(&out.Sport, &out.PlayerID, &out.TeamID, &out.LeagueID, &out.Source, &out.SourceUpdatedAt)
	return out, err
}

func GetAutofillVersion(ctx context.Context, pool *pgxpool.Pool, sport string) (AutofillVersion, error) {
	var out AutofillVersion
	err := pool.QueryRow(ctx, `
		SELECT sport, version, generated_at, total_entities, status, COALESCE(reason, '')
		FROM public.sport_autofill_versions
		WHERE sport = $1`,
		strings.ToUpper(sport),
	).Scan(&out.Sport, &out.Version, &out.GeneratedAt, &out.TotalEntities, &out.Status, &out.Reason)
	return out, err
}

func RefreshSportAutofillConcurrent(ctx context.Context, pool *pgxpool.Pool, sport, reason string) (AutofillVersion, error) {
	sport = strings.ToUpper(sport)
	view, err := autofillView(sport)
	if err != nil {
		return AutofillVersion{}, err
	}

	if _, err := pool.Exec(ctx, `SELECT public.request_sport_autofill_refresh($1, $2)`, sport, reason); err != nil {
		return AutofillVersion{}, err
	}
	if _, err := pool.Exec(ctx, "REFRESH MATERIALIZED VIEW CONCURRENTLY "+view); err != nil {
		_, _ = pool.Exec(context.Background(), `SELECT public.fail_sport_autofill_refresh($1, $2)`, sport, err.Error())
		return AutofillVersion{}, err
	}

	var total int
	if err := pool.QueryRow(ctx, "SELECT COUNT(*)::int FROM "+view).Scan(&total); err != nil {
		_, _ = pool.Exec(context.Background(), `SELECT public.fail_sport_autofill_refresh($1, $2)`, sport, err.Error())
		return AutofillVersion{}, err
	}

	var out AutofillVersion
	err = pool.QueryRow(ctx, `
		SELECT sport, version, generated_at, total_entities, status, COALESCE(reason, '')
		FROM public.complete_sport_autofill_refresh($1, $2, $3)`,
		sport, total, reason,
	).Scan(&out.Sport, &out.Version, &out.GeneratedAt, &out.TotalEntities, &out.Status, &out.Reason)
	return out, err
}

func autofillView(sport string) (string, error) {
	switch strings.ToUpper(sport) {
	case "NBA":
		return "nba.autofill_entities", nil
	case "NFL":
		return "nfl.autofill_entities", nil
	case "FOOTBALL":
		return "football.autofill_entities", nil
	default:
		return "", fmt.Errorf("unsupported sport %q", sport)
	}
}

func baseApplicationQuery() string {
	return `
		SELECT
			a.id, a.sport, a.player_id, COALESCE(p.name, ''),
			a.old_team_id, COALESCE(old_team.name, ''), a.old_league_id,
			a.new_team_id, COALESCE(new_team.name, ''), a.new_league_id,
			a.source_rumor_id, a.source_synthesis_id,
			a.deterministic_heat, a.deterministic_confidence::float8,
			a.threshold_config::text, a.adjudication::text, COALESCE(a.adjudication_raw, ''),
			COALESCE(a.adjudication_model_version, ''), COALESCE(a.adjudication_prompt_version, ''),
			a.decision, COALESCE(a.event_type, ''), a.adjudication_confidence::float8,
			a.status, COALESCE(a.reason, ''), a.evidence::text, a.override_id,
			a.applied_at, a.reverted_at, COALESCE(a.reverted_by, ''), COALESCE(a.revert_reason, ''),
			a.created_at
		FROM public.transfer_identity_applications a
		LEFT JOIN public.players p ON p.id = a.player_id AND p.sport = a.sport
		LEFT JOIN public.teams old_team ON old_team.id = a.old_team_id AND old_team.sport = a.sport
		LEFT JOIN public.teams new_team ON new_team.id = a.new_team_id AND new_team.sport = a.sport`
}

type applicationScanner interface {
	Scan(dest ...any) error
}

func scanApplication(row applicationScanner) (Application, error) {
	var app Application
	var thresholdConfig, adjudication, evidence string
	err := row.Scan(
		&app.ID, &app.Sport, &app.PlayerID, &app.PlayerName,
		&app.OldTeamID, &app.OldTeamName, &app.OldLeagueID,
		&app.NewTeamID, &app.NewTeamName, &app.NewLeagueID,
		&app.SourceRumorID, &app.SourceSynthesisID,
		&app.DeterministicHeat, &app.DeterministicConfidence,
		&thresholdConfig, &adjudication, &app.AdjudicationRaw,
		&app.AdjudicationModelVersion, &app.AdjudicationPromptVersion,
		&app.Decision, &app.EventType, &app.AdjudicationConfidence,
		&app.Status, &app.Reason, &evidence, &app.OverrideID,
		&app.AppliedAt, &app.RevertedAt, &app.RevertedBy, &app.RevertReason,
		&app.CreatedAt,
	)
	if err != nil {
		return Application{}, err
	}
	app.ThresholdConfig = json.RawMessage(thresholdConfig)
	app.Adjudication = json.RawMessage(adjudication)
	app.Evidence = json.RawMessage(evidence)
	return app, nil
}

var _ applicationScanner = pgx.Row(nil)

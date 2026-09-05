package dataimport

import (
	"context"
	"fmt"
	"regexp"
	"strings"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// querier is the subset of pgx satisfied by both *pgxpool.Pool and pgx.Tx, so
// identity binds can run standalone (roster phase) or inside a fixture's
// promotion transaction (a call-up first seen in a stat line).
type querier interface {
	Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error)
	Query(ctx context.Context, sql string, args ...any) (pgx.Rows, error)
	QueryRow(ctx context.Context, sql string, args ...any) pgx.Row
}

// Resolver maps one namespace's external handles onto Scoracle entity ids via
// entity_external_ids — the Investigator's table, not a new mechanism (mig 233
// only gave it a conflict target). All known bindings load once per run; binds
// created during the run update the in-memory maps in place.
//
// Teams are never created — a league's teams already exist, and an unmatched
// abbreviation is a funnel event for a human, not a new row. Players ARE
// created (roster arrivals, call-ups): entity existence comes from data. Entity
// STORIES stay news-driven — there are no enqueue triggers on players or
// team_rosters (verified at mig 233), so creation here can never wake a voice.
type Resolver struct {
	ns    string
	sport string

	teams    map[string]int
	players  map[string]int
	fixtures map[string]int
}

// upsertIdentitySQL's conflict target must match mig 233's partial unique index
// verbatim, predicate included.
const upsertIdentitySQL = `
	INSERT INTO entity_external_ids (entity_type, entity_id, sport, namespace, external_id)
	VALUES ($1, $2, $3, $4, $5)
	ON CONFLICT (namespace, entity_type, external_id)
	WHERE namespace IN ('nflverse', 'nba', 'fpl')
	DO NOTHING`

func NewResolver(ctx context.Context, pool *pgxpool.Pool, namespace, sport string) (*Resolver, error) {
	r := &Resolver{
		ns:       namespace,
		sport:    sport,
		teams:    map[string]int{},
		players:  map[string]int{},
		fixtures: map[string]int{},
	}
	rows, err := pool.Query(ctx, `
		SELECT entity_type, external_id, entity_id
		FROM entity_external_ids
		WHERE namespace = $1 AND sport = $2`, namespace, sport)
	if err != nil {
		return nil, fmt.Errorf("load %s identity map: %w", namespace, err)
	}
	defer rows.Close()
	for rows.Next() {
		var typ, ext string
		var id int
		if err := rows.Scan(&typ, &ext, &id); err != nil {
			return nil, err
		}
		switch typ {
		case "team":
			r.teams[ext] = id
		case "player":
			r.players[ext] = id
		case "fixture":
			r.fixtures[ext] = id
		}
	}
	return r, rows.Err()
}

func (r *Resolver) bind(ctx context.Context, q querier, entityType, ext string, id int) error {
	if _, err := q.Exec(ctx, upsertIdentitySQL, entityType, id, r.sport, r.ns, ext); err != nil {
		return fmt.Errorf("bind %s %s/%s -> %d: %w", r.ns, entityType, ext, id, err)
	}
	return nil
}

// Team resolves a source team handle (nflverse: the abbreviation). Unknown
// handles try short_code, then search_aliases, then exact name; a unique hit is
// bound permanently, anything else is an error the caller counts and skips.
func (r *Resolver) Team(ctx context.Context, q querier, ext string) (int, error) {
	if id, ok := r.teams[ext]; ok {
		return id, nil
	}
	var id int
	err := q.QueryRow(ctx, `
		SELECT id FROM teams
		WHERE sport = $1
		  AND (short_code = $2 OR $2 = ANY(search_aliases) OR name = $2)`,
		r.sport, ext).Scan(&id)
	if err == pgx.ErrNoRows {
		return 0, fmt.Errorf("team %q: no match", ext)
	}
	if err != nil {
		// A multi-row result surfaces here too (Scan on >1 row succeeds with the
		// first; pgx QueryRow returns the first row silently — so guard below).
		return 0, fmt.Errorf("team %q: %w", ext, err)
	}
	// Uniqueness guard: the abbreviation must identify exactly one team.
	var n int
	if err := q.QueryRow(ctx, `
		SELECT COUNT(*) FROM teams
		WHERE sport = $1
		  AND (short_code = $2 OR $2 = ANY(search_aliases) OR name = $2)`,
		r.sport, ext).Scan(&n); err != nil {
		return 0, err
	}
	if n != 1 {
		return 0, fmt.Errorf("team %q: %d candidates", ext, n)
	}
	if err := r.bind(ctx, q, "team", ext, id); err != nil {
		return 0, err
	}
	r.teams[ext] = id
	return id, nil
}

// suffixRe strips generational suffixes for name matching: seeder-era rows and
// nflverse disagree on "Jr."/"II" often enough that raw equality would mint
// duplicate players for established names — the one failure mode worse than an
// unmatched row.
var suffixRe = regexp.MustCompile(`(?i)\s+(jr\.?|sr\.?|ii|iii|iv|v)$`)

func normName(s string) string {
	s = suffixRe.ReplaceAllString(strings.TrimSpace(s), "")
	s = strings.ReplaceAll(s, ".", "")
	s = strings.ReplaceAll(s, "'", "")
	return strings.ToLower(s)
}

// normNameSQL is the same normalization in SQL, applied to the players side.
const normNameSQL = `lower(replace(replace(regexp_replace(%s, '\s+(jr\.?|sr\.?|ii|iii|iv|v)$', '', 'i'), '.', ''), '''', ''))`

// Player resolves an external player id, matching by normalized name (narrowed
// by team when the name alone is shared — the league has two Josh Allens) and
// creating the player when nothing matches. Returns (id, created, err); an
// ambiguous name that team context cannot split returns an error — binding the
// wrong human is worse than skipping one player's rows for a run.
func (r *Resolver) Player(ctx context.Context, q querier, ext, fullName, position string, teamID int) (int, bool, error) {
	if id, ok := r.players[ext]; ok {
		return id, false, nil
	}
	norm := normName(fullName)
	nameExpr := fmt.Sprintf(normNameSQL, "name")

	rows, err := q.Query(ctx, `
		SELECT id, COALESCE(team_id, 0) FROM players
		WHERE sport = $1 AND `+nameExpr+` = $2`, r.sport, norm)
	if err != nil {
		return 0, false, fmt.Errorf("player %q match: %w", fullName, err)
	}
	type cand struct{ id, teamID int }
	var cands []cand
	for rows.Next() {
		var c cand
		if err := rows.Scan(&c.id, &c.teamID); err != nil {
			rows.Close()
			return 0, false, err
		}
		cands = append(cands, c)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, false, err
	}

	// Exclude candidates already bound to a DIFFERENT external id: two source
	// players sharing a normalized name must not collapse onto one row.
	if len(cands) > 0 {
		filtered := cands[:0]
		for _, c := range cands {
			var taken bool
			if err := q.QueryRow(ctx, `
				SELECT EXISTS (
					SELECT 1 FROM entity_external_ids
					WHERE namespace = $1 AND entity_type = 'player'
					  AND entity_id = $2 AND external_id <> $3)`,
				r.ns, c.id, ext).Scan(&taken); err != nil {
				return 0, false, err
			}
			if !taken {
				filtered = append(filtered, c)
			}
		}
		cands = filtered
	}

	var id int
	switch {
	case len(cands) == 1:
		id = cands[0].id
	case len(cands) > 1 && teamID != 0:
		matched := 0
		for _, c := range cands {
			if c.teamID == teamID {
				id, matched = c.id, matched+1
			}
		}
		if matched != 1 {
			return 0, false, fmt.Errorf("player %q: %d candidates, team narrows to %d", fullName, len(cands), matched)
		}
	case len(cands) > 1:
		return 0, false, fmt.Errorf("player %q: %d candidates, no team context", fullName, len(cands))
	default:
		// No match: create. players.id mints from its sequence as of mig 233.
		first, last := splitName(fullName)
		err := q.QueryRow(ctx, `
			INSERT INTO players (sport, name, first_name, last_name, team_id, league_id, meta)
			VALUES ($1, $2, $3, $4, NULLIF($5, 0), 0,
			        jsonb_build_object('position_abbreviation', $6::text, 'created_by', 'dataimport'))
			RETURNING id`,
			r.sport, strings.TrimSpace(fullName), first, last, teamID, position).Scan(&id)
		if err != nil {
			return 0, false, fmt.Errorf("create player %q: %w", fullName, err)
		}
		if err := r.bind(ctx, q, "player", ext, id); err != nil {
			return 0, false, err
		}
		r.players[ext] = id
		return id, true, nil
	}

	if err := r.bind(ctx, q, "player", ext, id); err != nil {
		return 0, false, err
	}
	r.players[ext] = id
	return id, false, nil
}

// Fixture returns the bound fixture id for a source game key, 0 if unbound.
func (r *Resolver) Fixture(ext string) int { return r.fixtures[ext] }

// BindFixture records a schedule row's game key against a fixtures row.
func (r *Resolver) BindFixture(ctx context.Context, q querier, ext string, fixtureID int) error {
	if err := r.bind(ctx, q, "fixture", ext, fixtureID); err != nil {
		return err
	}
	r.fixtures[ext] = fixtureID
	return nil
}

func splitName(full string) (first, last string) {
	parts := strings.Fields(strings.TrimSpace(full))
	if len(parts) == 0 {
		return "", ""
	}
	if len(parts) == 1 {
		return parts[0], ""
	}
	return parts[0], strings.Join(parts[1:], " ")
}

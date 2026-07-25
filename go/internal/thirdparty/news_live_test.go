package thirdparty

import (
	"context"
	"database/sql"
	"errors"
	"math"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"
)

func TestLiveRSSBroadTeamFlow(t *testing.T) {
	if os.Getenv("SCORACLE_LIVE_RSS_SMOKE") != "1" {
		t.Skip("set SCORACLE_LIVE_RSS_SMOKE=1 to hit live DB + Google RSS")
	}
	loadLiveSmokeEnv()
	dbURL := os.Getenv("DATABASE_PRIVATE_URL")
	if dbURL == "" {
		dbURL = os.Getenv("DATABASE_URL")
	}
	if dbURL == "" {
		t.Fatal("DATABASE_PRIVATE_URL / DATABASE_URL not set")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 4*time.Minute)
	defer cancel()
	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect db: %v", err)
	}
	defer pool.Close()

	news := NewNewsService(pool, nil)
	dryRun := os.Getenv("SCORACLE_LIVE_RSS_DRY_RUN") == "1"
	if dryRun {
		news = NewNewsService(nil, nil)
	}
	var sawArticles bool
	for _, requested := range liveSmokeTeams() {
		team, err := loadLiveSmokeTeam(ctx, pool, requested)
		if err != nil {
			t.Fatalf("load team %q: %v", requested, err)
		}
		if team.id == 0 {
			t.Logf("team %q not found; trying next", requested)
			continue
		}

		result, fresh, funnel, err := news.GetEntityNews(ctx, "team", team.id, team.name, team.sport, "", liveSmokeRSSLimit(), "", "", team.aliases)
		if err != nil {
			t.Fatalf("GetEntityNews %s: %v", team.name, err)
		}
		articles, ok := result["articles"].([]Article)
		if !ok {
			t.Fatalf("GetEntityNews %s returned unexpected articles payload: %#v", team.name, result["articles"])
		}
		t.Logf("team=%s sport=%s aliases=%d rss_articles=%d affected_article_ids=%d funnel=%v", team.name, team.sport, len(team.aliases), len(articles), len(fresh), funnel.LogAttrs())
		if r := funnel.Residual(); r != 0 {
			t.Errorf("funnel does not balance for %s: residual %d (%v)", team.name, r, funnel.LogAttrs())
		}
		if len(articles) > 0 {
			sawArticles = true
		}
		if dryRun {
			if len(articles) > 0 {
				return
			}
			continue
		}
		if len(fresh) == 0 {
			continue
		}

		rows, err := loadFreshTeamLinks(ctx, pool, fresh, team)
		if err != nil {
			t.Fatalf("load fresh links for %s: %v", team.name, err)
		}
		var primaryCandidates int
		for _, row := range rows {
			if math.Abs(row.confidence-broadTeamPrimaryConfidence) > 0.0001 {
				t.Logf("article=%d requested-team link confidence=%.2f treated as pre-existing/secondary vetted=%s scrubbed=%v work_status=%s", row.articleID, row.confidence, nullBool(row.vetted), row.scrubbedAt.Valid, nullString(row.workStatus))
				continue
			}
			primaryCandidates++
			if !row.scrubbedAt.Valid && !row.workStatus.Valid {
				t.Fatalf("article %d was neither scrubbed nor queued/running in pipeline_work", row.articleID)
			}
			t.Logf("broad primary candidate article=%d confidence=%.2f vetted=%s scrubbed=%v work_status=%s", row.articleID, row.confidence, nullBool(row.vetted), row.scrubbedAt.Valid, nullString(row.workStatus))
		}
		if primaryCandidates == 0 {
			t.Logf("no fresh broad primary candidate rows for %s; trying next", team.name)
			continue
		}
		return
	}
	if sawArticles {
		t.Fatal("live RSS returned articles, but no fresh team links were created; rerun with SCORACLE_LIVE_RSS_TEAMS set to a less-recently-ingested team")
	}
	t.Fatal("live RSS returned no articles for smoke teams")
}

type liveSmokeTeam struct {
	id      int
	sport   string
	name    string
	aliases []string
}

// liveSmokeRSSLimit mirrors the pipeline's -rss-limit so the smoke can be run at
// the production cap and at 0 (uncapped) and the two funnels compared. That
// comparison is the only direct measurement of what the cap costs: the cap stops
// the sweep before the localized editions run, and a smaller corpus looks
// identical to those editions simply having no news.
//
//	SCORACLE_LIVE_RSS_SMOKE=1 SCORACLE_LIVE_RSS_DRY_RUN=1 SCORACLE_LIVE_RSS_LIMIT=12 go test ...
func liveSmokeRSSLimit() int {
	raw := strings.TrimSpace(os.Getenv("SCORACLE_LIVE_RSS_LIMIT"))
	if raw == "" {
		return 0
	}
	n, err := strconv.Atoi(raw)
	if err != nil || n < 0 {
		return 0
	}
	return n
}

func liveSmokeTeams() []string {
	raw := strings.TrimSpace(os.Getenv("SCORACLE_LIVE_RSS_TEAMS"))
	if raw == "" {
		raw = "Paris Saint-Germain,Manchester United,Arsenal,Real Madrid,Barcelona"
	}
	var out []string
	for _, part := range strings.Split(raw, ",") {
		if s := strings.TrimSpace(part); s != "" {
			out = append(out, s)
		}
	}
	return out
}

func loadLiveSmokeTeam(ctx context.Context, pool *pgxpool.Pool, requested string) (liveSmokeTeam, error) {
	const q = `
		SELECT id, sport, name, COALESCE(short_code, ''), COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams
		WHERE sport = 'FOOTBALL' AND lower(name) = lower($1)
		ORDER BY id
		LIMIT 1
	`
	const fuzzy = `
		SELECT id, sport, name, COALESCE(short_code, ''), COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams
		WHERE sport = 'FOOTBALL' AND name ILIKE '%' || $1 || '%'
		ORDER BY length(name), id
		LIMIT 1
	`
	team, err := scanLiveSmokeTeam(pool.QueryRow(ctx, q, requested))
	if err == nil || !errors.Is(err, pgx.ErrNoRows) {
		return team, err
	}
	team, err = scanLiveSmokeTeam(pool.QueryRow(ctx, fuzzy, requested))
	if errors.Is(err, pgx.ErrNoRows) {
		return liveSmokeTeam{}, nil
	}
	return team, err
}

func scanLiveSmokeTeam(row pgx.Row) (liveSmokeTeam, error) {
	var team liveSmokeTeam
	var shortCode string
	if err := row.Scan(&team.id, &team.sport, &team.name, &shortCode, &team.aliases); err != nil {
		return liveSmokeTeam{}, err
	}
	if shortCode != "" {
		team.aliases = append(team.aliases, shortCode)
	}
	return team, nil
}

type freshTeamLink struct {
	articleID  int64
	confidence float64
	vetted     sql.NullBool
	scrubbedAt sql.NullTime
	workStatus sql.NullString
}

func loadFreshTeamLinks(ctx context.Context, pool *pgxpool.Pool, articleIDs []int64, team liveSmokeTeam) ([]freshTeamLink, error) {
	rows, err := pool.Query(ctx, `
		SELECT nae.article_id,
		       nae.match_confidence::float8,
		       nae.vetted,
		       nae.scrubbed_at,
		       pw.status
		FROM news_article_entities nae
		LEFT JOIN pipeline_work pw
		  ON pw.stage = 'scrub'
		 AND pw.entity_type = 'article'
		 AND pw.entity_id = nae.article_id::int
		 AND pw.sport = nae.sport
		WHERE nae.article_id = ANY($1)
		  AND nae.entity_type = 'team'
		  AND nae.entity_id = $2
		  AND nae.sport = $3
		ORDER BY nae.article_id
	`, articleIDs, team.id, team.sport)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []freshTeamLink
	for rows.Next() {
		var row freshTeamLink
		if err := rows.Scan(&row.articleID, &row.confidence, &row.vetted, &row.scrubbedAt, &row.workStatus); err != nil {
			return nil, err
		}
		out = append(out, row)
	}
	return out, rows.Err()
}

func nullBool(v sql.NullBool) string {
	if !v.Valid {
		return "NULL"
	}
	if v.Bool {
		return "TRUE"
	}
	return "FALSE"
}

func nullString(v sql.NullString) string {
	if !v.Valid {
		return "NULL"
	}
	return v.String
}

func loadLiveSmokeEnv() {
	for _, path := range []string{
		".env.local", "../.env.local", "../../.env.local", "../../../.env.local",
		".env", "../.env", "../../.env", "../../../.env",
	} {
		_ = godotenv.Load(path)
	}
}

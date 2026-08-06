// Package thirdparty provides clients for third-party APIs.
package thirdparty

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"encoding/xml"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"
	"unicode"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/work"
)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const (
	newsMinArticles = 3
	newsRSSTimeout  = 15 * time.Second
	// Allow a small overlap around the cron boundary so a slightly late run does
	// not miss items published just before the lookback boundary.
	newsLookbackSlack = 15 * time.Minute

	// Keep Google from being handed broad team alias bags. The primary query is
	// always searched; only the safest aliases get their own explicit lanes.
	newsMaxTeamAliasRSSQueries       = 2
	newsMinTeamRSSQueriesBeforeLimit = 2
)

// RSS query lookback windows (hours). This MUST cover the ingest cron's period or
// the sweep is blind between runs: the window is both the `when:` token handed to
// Google News and the client-side `filterArticlesByLookback` cutoff, so anything
// older than it is dropped twice over and never reaches news_articles at all.
//
// 24h to match the daily 02:00 ingest (2026-07-26). The pairing has moved together
// every time: 6h window with the six-hour cron, 12h when that went twelve-hourly.
// A day's news now arrives in one sweep, and per-entity volume is bounded by
// `-rss-limit` (one Google page) rather than by how often the cron happens to fire.
var timeWindows = []int{24}

// Sport-specific search term suffixes for RSS.
var sportTerms = map[string]string{
	"NBA":      "NBA basketball",
	"NFL":      "NFL football",
	"FOOTBALL": "soccer football",
}

type rssEdition struct {
	name string
	hl   string
	gl   string
	ceid string
}

var defaultRSSEditions = []rssEdition{
	{name: "en-us", hl: "en-US", gl: "US", ceid: "US:en"},
}

// footballTeamRSSEditions is ENGLISH-ONLY, deliberately, until the cognition layer can read other
// languages first-hand.
//
// The localized editions (es-es, fr-fr, de-de, it-it, pt-pt, nl-nl) were live from 2026-07-23 and
// measurably worked — they made the corpus only 24.1% English, with 45.3% definitively non-English.
// The problem is that nothing downstream was ready for that:
//
//   - the candle's embedder is `bge-small-en-v1.5`, an ENGLISH-ONLY model, so it scored three
//     quarters of the corpus on text its weights never saw;
//   - the Article Reader was left translating AND summarizing in one 7B call on 76% of articles;
//   - every prompt, guard list, and stopword downstream is English.
//
// Restore the localized editions when a model that reads them natively is in place (Scott's call:
// at the 30B tier). Until then, ingesting text we cannot read is buying GPU load and noise, not
// coverage. Re-adding them is this one slice — the edition machinery is untouched and still tested.
// en-GB alone. We cover five leagues — Premier League (England), La Liga (Spain), Serie A (Italy),
// Ligue 1 (France), Bundesliga (Germany) — so by the "only fetch countries whose leagues we cover"
// rule the correct edition set is en-GB + es/it/fr/de. The four localized ones are PARKED, not
// rejected, until a model that reads them exists; en-GB is kept because British outlets cover all
// five leagues in English, and en-US was dropped because it duplicates that coverage more thinly.
//
// The named consequence, so nobody rediscovers it as a surprise: four of our five leagues are now
// covered only by English-language reporting of them. That is a deliberate accuracy-over-breadth
// trade while the pipeline is GPU-bound.
var footballTeamRSSEditions = []rssEdition{
	{name: "en-gb", hl: "en-GB", gl: "GB", ceid: "GB:en"},
}

var trustedShortTeamAliases = map[string]bool{
	"ASM":  true,
	"ASSE": true,
	"B04":  true,
	"BMG":  true,
	"BVB":  true,
	"HSV":  true,
	"LOSC": true,
	"OL":   true,
	"OM":   true,
	"PSG":  true,
	"RBL":  true,
	"S04":  true,
	"SGE":  true,
}

const broadTeamPrimaryConfidence = 0.95

// ---------------------------------------------------------------------------
// Article — normalized article shape
// ---------------------------------------------------------------------------

// Article is a normalized news article.
type Article struct {
	Title       string  `json:"title"`
	Description string  `json:"description"`
	URL         string  `json:"url"`
	Source      string  `json:"source"`
	PublishedAt string  `json:"published_at"`
	ImageURL    *string `json:"image_url"`
	Author      *string `json:"author,omitempty"`

	// FeedRank is this article's 0-based position in the RSS payload Google returned — its own
	// ranking of what matters most for the query. Captured at parse time, before any reordering
	// downstream can destroy it.
	//
	// This is the primary quality signal in the ingest path, and it is free. It decides which
	// articles survive `-rss-limit` here, and which are worth a model call once the reading
	// budget binds later. Both cuts rank by it, so "top N" means the same thing at every stage.
	FeedRank int `json:"feed_rank"`

	// Query provenance — which (query, lane, edition, window) request surfaced this
	// article. The query IS the hypothesis: every article is here because we asked
	// Google a specific question, and until now that question was discarded at fetch
	// time. persistArticles writes these into news_articles.raw on first insert.
	//
	// Unexported on purpose: provenance is for the corpus, not the API response, and
	// unexported fields stay out of the JSON marshal. Dedup keeps the first occurrence,
	// so an article seen by several lanes carries the lane that found it first (the
	// sweep runs primary before aliases — first-writer wins is also best-writer wins).
	queryTerm    string // literal search term as sent, minus the when: token
	queryLane    string // "primary" or "alias<N>" per buildRSSSearchQueries order
	queryEdition string // ceid of the edition that returned it
	queryWindow  string // when: token, e.g. "24h"
}

// ---------------------------------------------------------------------------
// NewsService — Google News RSS client
// ---------------------------------------------------------------------------

// entityPoolTTL controls how often the cross-entity match pool is refreshed
// from the DB. The pool is populated by player/team upserts via the seeder,
// so staleness of up to an hour is harmless.
const entityPoolTTL = 1 * time.Hour

// cachedEntity is one row in the in-memory entity pool used for cross-entity
// matching at write-through time. Built from teams + players rows.
type cachedEntity struct {
	entityType string // 'player' | 'team'
	entityID   int
	sport      string
	match      EntityMatchInput
}

type entityPool struct {
	entities  []cachedEntity
	refreshed time.Time
}

// defaultRSSBaseURL is the Google News RSS search endpoint. Held in a field
// rather than inlined so the funnel accounting can be exercised against a local
// test server instead of the live endpoint.
const defaultRSSBaseURL = "https://news.google.com/rss/search"

// NewsService fetches entity news from Google News RSS.
//
// When constructed with a non-nil pool, every matched article is also
// persisted to news_articles + news_article_entities as a write-through.
// That populates the long-term corpus consumed by the cognition layer.
type NewsService struct {
	httpClient *http.Client
	rssBaseURL string
	pool       *pgxpool.Pool
	logger     *slog.Logger

	entityMu    sync.RWMutex
	entityPools map[string]*entityPool // keyed by sport, lowercased
}

// NewNewsService creates a news service. If pool is non-nil, fetched
// articles are written to news_articles / news_article_entities on each
// successful entity match. If logger is nil, a default no-op logger is used.
func NewNewsService(pool *pgxpool.Pool, logger *slog.Logger) *NewsService {
	if logger == nil {
		logger = slog.Default()
	}
	return &NewsService{
		httpClient: &http.Client{
			Timeout: newsRSSTimeout,
		},
		rssBaseURL:  defaultRSSBaseURL,
		pool:        pool,
		logger:      logger,
		entityPools: make(map[string]*entityPool),
	}
}

// Status returns service configuration status.
func (s *NewsService) Status() map[string]interface{} {
	return map[string]interface{}{
		"rss_available":  true,
		"primary_source": "google_news_rss",
	}
}

// railIsPacket reports whether this process is running on the packet rail (PLAN-one-rail §2).
//
// Read per call rather than cached at boot, deliberately: Go's half of the flip is two skipped
// writes, and the cost of a getenv is nothing next to an RSS fetch. The Rust side parses RAIL once
// at boot and logs it — there the value rides the Harness through a whole drain, so a mid-drain
// change would be genuinely incoherent. Here it cannot be.
//
// Default legacy: anything other than exactly "packet" (case-insensitive, trimmed) leaves ingest
// behaving as it always has. An unset or misspelled RAIL never silently retires the old rail.
func railIsPacket() bool {
	return strings.EqualFold(strings.TrimSpace(os.Getenv("RAIL")), "packet")
}

// GetEntityNews fetches news for an entity via Google News RSS.
// entityType ("player"|"team") and entityID drive the write-through —
// matched articles are linked back to the requested entity in
// news_article_entities. Pass entityType="" / entityID=0 to skip write-through.
//
// The second return is the article IDs that gained a fresh link on this call.
// It is informational — persistArticles already vets primaries and enqueues
// scrub work in its own transaction. It is nil when write-through is skipped
// or the persist failed.
//
// The third return is the fetch funnel for this entity: how much of the query
// grid ran and what each filter stage discarded. It is returned even on error,
// because a sweep that times out mid-fetch still did (and dropped) real work.
func (s *NewsService) GetEntityNews(
	ctx context.Context,
	entityType string,
	entityID int,
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, []int64, Funnel, error) {
	result, matched, funnel, err := s.fetchFromRSS(ctx, entityType, entityName, sport, team, limit, firstName, lastName, aliases)
	if err != nil {
		return nil, nil, funnel, err
	}

	// Write-through: persist the matched articles and link them to this entity
	// (persistArticles also vets primaries and enqueues scrub work in-txn).
	// Non-fatal — a failed persist shouldn't break the response. affected is the
	// set of articles that gained a NEW link this call; it stays nil on the
	// serving path / on persist failure.
	var affected []int64
	if s.pool != nil && entityType != "" && entityID > 0 && len(matched) > 0 {
		ids, perr := s.persistArticles(ctx, sport, entityType, entityID, matched)
		if perr != nil {
			s.logger.Warn("persist failed", "sport", sport, "entity_type", entityType, "entity_id", entityID, "error", perr)
		} else {
			affected = ids
		}
	}

	return result, affected, funnel, nil
}

// persistArticles upserts articles by URL hash and links them to the primary
// requested entity plus any other teams/players mentioned in the title (cross-
// entity linking). The secondary pass catches relational patterns such as
// "Warriors trade talks for Durant", linking both Warriors and Durant even if
// only Durant was the queried entity.
//
// Runs in a single transaction, and the transaction also opens the pipe ends:
// fresh primary links are auto-vetted (firing the derive-enqueue trigger) and
// articles with fresh secondary links are enqueued for the Rust scrub stage, so
// new material flows on commit rather than on the maintenance sweep's cadence.
//
// Errors are returned to the caller but don't break the response path — the
// caller logs and moves on. The returned slice is the article IDs that gained a
// NEW entity link (a fresh primary or secondary link, RowsAffected>0); it is
// informational — scrub queueing is handled here. An article re-seen with only
// pre-existing links is omitted.
func (s *NewsService) persistArticles(
	ctx context.Context,
	sport, primaryEntityType string,
	primaryEntityID int,
	articles []Article,
) ([]int64, error) {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback(ctx)

	sportUpper := strings.ToUpper(sport)
	pool, err := s.getEntityPool(ctx, sportUpper)
	if err != nil {
		s.logger.Warn("entity pool load failed", "sport", sportUpper, "error", err)
		pool = nil
	}

	affected := make(map[int64]bool) // fresh primary OR secondary link — the informational return
	needScrub := make(map[int64]bool) // fresh secondary link → needs the model's verdict
	needEditor := make(map[int64]bool) // fresh ARTICLE ROW → the greenfield Editor reads it once

	// The primary entity's own match input — needed to record its title position
	// for the co-mention proximity gate. Broader team RSS now lets Google cast
	// the net while Rust scrub decides relevance, so some primary links will not
	// have an exact title position.
	var primaryMatch *EntityMatchInput
	for i := range pool {
		if pool[i].entityType == primaryEntityType && pool[i].entityID == primaryEntityID {
			m := pool[i].match
			primaryMatch = &m
			break
		}
	}

	for _, a := range articles {
		if a.URL == "" || a.Title == "" {
			continue
		}
		hash := sha256Hex(a.URL)

		var publishedAt *time.Time
		if ts := parseArticleDate(a.PublishedAt); !ts.IsZero() {
			publishedAt = &ts
		}

		// Query provenance (Phase 2, PLAN-one-rail): the question we asked Google,
		// written on INSERT ONLY — the conflict branch never touches raw, so the
		// first sweep that lands an article owns its provenance and re-seen
		// articles keep theirs. query_team_id records which team's sweep asked;
		// the sweep is teams-only by doctrine, so the guard is belt-and-braces
		// for any non-team caller of GetEntityNews.
		rawProv := []byte(`{}`)
		if a.queryTerm != "" {
			prov := map[string]any{
				"q":       a.queryTerm,
				"lane":    a.queryLane,
				"edition": a.queryEdition,
				"window":  a.queryWindow,
			}
			if isTeamEntity(primaryEntityType) {
				prov["query_team_id"] = primaryEntityID
			}
			if b, jerr := json.Marshal(prov); jerr == nil {
				rawProv = b
			}
		}

		// (xmax = 0) distinguishes a fresh INSERT from a conflict-update in the same
		// statement — the standard idiom, used here so the editor enqueue below fires
		// once per article's first sighting rather than once per sweep that re-sees it.
		var articleID int64
		var inserted bool
		err := tx.QueryRow(ctx, `
			INSERT INTO news_articles (url_hash, url, source, title, description, published_at, feed_rank, raw)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			ON CONFLICT (url_hash) DO UPDATE SET
			    title       = EXCLUDED.title,
			    description = EXCLUDED.description,
			    source      = COALESCE(EXCLUDED.source, news_articles.source),
			    published_at = COALESCE(EXCLUDED.published_at, news_articles.published_at),
			    feed_rank   = LEAST(COALESCE(news_articles.feed_rank, EXCLUDED.feed_rank), EXCLUDED.feed_rank)
			RETURNING id, (xmax = 0)
		`, hash, a.URL, nullIfEmpty(a.Source), a.Title, nullIfEmpty(a.Description), publishedAt, a.FeedRank, rawProv).Scan(&articleID, &inserted)
		if err != nil {
			return nil, fmt.Errorf("upsert article: %w", err)
		}
		if inserted {
			needEditor[articleID] = true
		}

		// Primary link — the entity that was queried. EVERY RSS search is a broad
		// candidate generator now, players included, so every primary link lands
		// below confidence 1.0 and is proposed to the scrub gate rather than
		// asserted here.
		//
		// Players used to auto-vet at 1.0 on the grounds that they were "exact
		// matches" — true only because filterRSSArticles had already guaranteed the
		// name appeared in the text. That filter is gone, so the premise is gone
		// with it: auto-vetting now would stamp vetted=TRUE on whatever Google
		// happened to return, with nothing having read it. One path for both entity
		// kinds, and the judge is downstream.
		primaryPos := -1
		if primaryMatch != nil {
			primaryPos = FirstMatchPos(a.Title, *primaryMatch)
		}
		primaryConfidence := broadTeamPrimaryConfidence
		ct, err := tx.Exec(ctx, `
			INSERT INTO news_article_entities (article_id, entity_type, entity_id, sport, match_confidence, title_pos)
			VALUES ($1, $2, $3, $4, $5, $6)
			ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
		`, articleID, primaryEntityType, primaryEntityID, sportUpper, primaryConfidence, posOrNil(primaryPos))
		if err != nil {
			return nil, fmt.Errorf("link primary entity: %w", err)
		}
		if ct.RowsAffected() > 0 {
			affected[articleID] = true
			needScrub[articleID] = true
		}

		// Secondary links — scan title + description against the cached entity pool
		// to pick up co-mentioned teams/players, recording WHERE each matched
		// in the title when available so the proximity gate can drop far-apart
		// co-mentions (roundup artifacts). Description-only matches get NULL
		// title_pos and are routed through scrub.
		// ON CONFLICT DO NOTHING means re-matching the primary entity is a cheap no-op.
		//
		// PLAN-one-rail 8.4: this whole loop is the regex link guesser the packet rail
		// replaces. Under RAIL=packet the Editor reads the article and writes the links it
		// actually resolved (8.5), so guessing from a substring match would bury those real
		// links under noise nothing adjudicates any more — scrub, the gate that used to judge
		// them, is off. Deletion is Phase 9; skipping is enough for flip day.
		matchText := articleMatchText(a)
		secondaryPool := pool
		if railIsPacket() {
			secondaryPool = nil
		}
		for i := range secondaryPool {
			e := &pool[i]
			// Skip the primary — we already linked it above.
			if e.entityType == primaryEntityType && e.entityID == primaryEntityID {
				continue
			}
			pos := FirstMatchPos(a.Title, e.match)
			if pos < 0 && !MatchesEntity(matchText, e.match) {
				continue
			}
			ct, err := tx.Exec(ctx, `
				INSERT INTO news_article_entities (article_id, entity_type, entity_id, sport, match_confidence, title_pos)
				VALUES ($1, $2, $3, $4, $5, $6)
				ON CONFLICT (article_id, entity_type, entity_id, sport) DO NOTHING
			`, articleID, e.entityType, e.entityID, sportUpper, 0.8, posOrNil(pos))
			if err != nil {
				return nil, fmt.Errorf("link secondary entity: %w", err)
			}
			if ct.RowsAffected() > 0 {
				affected[articleID] = true
				needScrub[articleID] = true
			}
		}
	}

	// Open the pipe ends: vet and enqueue at ingest, inside this transaction, so
	// a new article reaches the cognition stages on commit (mig-150 NOTIFY)
	// instead of waiting for the maintenance sweep's next tick. The sweep stays
	// on as the backstop for rows this path misses (pre-change backlog, repair).
	//
	// The auto-vet pass that used to live here is gone. It stamped vetted=TRUE on
	// confidence-1.0 player links without a model ever seeing them, which was only
	// defensible while filterRSSArticles guaranteed the entity was named in the text.
	// Nothing is vetted at ingest now; every link is proposed and scrub decides.
	//
	// Worth knowing if vetting is ever reinstated here: the mig-103
	// enqueue_derive_on_vetted trigger watches UPDATE OF vetted, so it must be an
	// UPDATE and never vetted=TRUE on the INSERT, or the derive never fires.

	// Every link needs the Rust ScrubHandler's verdict —
	// enqueue one article-keyed scrub item each. Duplicate enqueues collapse on
	// the pipeline_work PK without yanking a live lease.
	//
	// PLAN-one-rail 8.4: not on the packet rail. `scrub` is one of the two stages the flip
	// deletes (§2), and enqueueing work for a stage no worker claims would pile up pending rows
	// forever — a queue that only grows is indistinguishable from a wedged one when you go
	// looking for the next real problem.
	if railIsPacket() {
		needScrub = nil
	}
	for id := range needScrub {
		if err := work.Enqueue(ctx, tx, work.Item{
			Stage:      work.StageScrub,
			EntityType: "article",
			EntityID:   int(id),
			Sport:      sportUpper,
		}); err != nil {
			return nil, err
		}
	}

	// The greenfield Editor reads EVERY new article once (PLAN-one-rail 3.5) — same tx,
	// same ON CONFLICT discipline as scrub above. Keyed on fresh INSERTs rather than fresh
	// links: the Editor's unit is the article, and a re-seen URL keeps its read. The stage
	// drains only where COGNITION_STAGES includes 'editor' (archbox).
	for id := range needEditor {
		if err := work.Enqueue(ctx, tx, work.Item{
			Stage:      work.StageEditor,
			EntityType: "article",
			EntityID:   int(id),
			Sport:      sportUpper,
		}); err != nil {
			return nil, err
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return nil, err
	}

	ids := make([]int64, 0, len(affected))
	for id := range affected {
		ids = append(ids, id)
	}
	return ids, nil
}

// getEntityPool returns (and refreshes on staleness) the cached list of
// players + teams for a sport. The pool is small enough (<3.5k per sport
// post-purge) that scanning the whole list per article is fine.
func (s *NewsService) getEntityPool(ctx context.Context, sport string) ([]cachedEntity, error) {
	s.entityMu.RLock()
	p, ok := s.entityPools[sport]
	s.entityMu.RUnlock()

	if ok && time.Since(p.refreshed) < entityPoolTTL {
		return p.entities, nil
	}

	entities, err := s.loadEntityPool(ctx, sport)
	if err != nil {
		// If we have a stale cache, keep using it rather than failing.
		if ok {
			return p.entities, nil
		}
		return nil, err
	}

	s.entityMu.Lock()
	s.entityPools[sport] = &entityPool{entities: entities, refreshed: time.Now()}
	s.entityMu.Unlock()
	return entities, nil
}

func (s *NewsService) loadEntityPool(ctx context.Context, sport string) ([]cachedEntity, error) {
	// Teams
	teamRows, err := s.pool.Query(ctx, `
		SELECT id, name, COALESCE(short_code, ''), COALESCE(search_aliases, ARRAY[]::text[])
		FROM teams WHERE sport = $1
	`, sport)
	if err != nil {
		return nil, fmt.Errorf("load teams: %w", err)
	}
	var out []cachedEntity
	for teamRows.Next() {
		var id int
		var name, shortCode string
		var aliases []string
		if err := teamRows.Scan(&id, &name, &shortCode, &aliases); err != nil {
			teamRows.Close()
			return nil, err
		}
		al := aliases
		if shortCode != "" {
			al = append(al, shortCode)
		}
		out = append(out, cachedEntity{
			entityType: "team",
			entityID:   id,
			sport:      sport,
			match: EntityMatchInput{
				EntityType: "team",
				Name:       name,
				Aliases:    al,
				Sport:      sport,
			},
		})
	}
	teamRows.Close()

	// Players
	playerRows, err := s.pool.Query(ctx, `
		SELECT id, name, COALESCE(first_name, ''), COALESCE(last_name, ''),
		       COALESCE(search_aliases, ARRAY[]::text[])
		FROM players WHERE sport = $1
	`, sport)
	if err != nil {
		return nil, fmt.Errorf("load players: %w", err)
	}
	for playerRows.Next() {
		var id int
		var name, first, last string
		var aliases []string
		if err := playerRows.Scan(&id, &name, &first, &last, &aliases); err != nil {
			playerRows.Close()
			return nil, err
		}
		out = append(out, cachedEntity{
			entityType: "player",
			entityID:   id,
			sport:      sport,
			match: EntityMatchInput{
				EntityType: "player",
				Name:       name,
				FirstName:  first,
				LastName:   last,
				Aliases:    aliases,
				Sport:      sport,
			},
		})
	}
	playerRows.Close()

	s.logger.Info("loaded entity pool", "sport", sport, "count", len(out))
	return out, nil
}

func sha256Hex(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:])
}

func nullIfEmpty(s string) interface{} {
	if s == "" {
		return nil
	}
	return s
}

// posOrNil maps a title-match index to a SMALLINT value or NULL (no match / out
// of range). NULL is the "unknown" sentinel the proximity gate treats as lenient.
func posOrNil(p int) interface{} {
	if p < 0 {
		return nil
	}
	if p > 32767 {
		return int16(32767)
	}
	return int16(p)
}

// BackfillTitlePositions recomputes news_article_entities.title_pos for existing
// rows of a sport from the stored article title, using the same matcher as the
// write path. One-shot maintenance for the co-mention proximity gate (migration
// 033); idempotent. Rows whose entity is no longer in the pool (purged) keep a
// NULL title_pos, which the gate treats leniently. Returns rows updated.
func (s *NewsService) BackfillTitlePositions(ctx context.Context, sport string) (int, error) {
	sportUpper := strings.ToUpper(sport)
	pool, err := s.getEntityPool(ctx, sportUpper)
	if err != nil {
		return 0, err
	}
	type poolKey struct {
		etype string
		id    int
	}
	idx := make(map[poolKey]EntityMatchInput, len(pool))
	for i := range pool {
		idx[poolKey{pool[i].entityType, pool[i].entityID}] = pool[i].match
	}

	rows, err := s.pool.Query(ctx, `
		SELECT nae.article_id, nae.entity_type, nae.entity_id, a.title
		FROM news_article_entities nae
		JOIN news_articles a ON a.id = nae.article_id
		WHERE nae.sport = $1
	`, sportUpper)
	if err != nil {
		return 0, err
	}
	type update struct {
		articleID int64
		etype     string
		eid       int
		pos       interface{}
	}
	var batch []update
	for rows.Next() {
		var articleID int64
		var etype, title string
		var eid int
		if err := rows.Scan(&articleID, &etype, &eid, &title); err != nil {
			rows.Close()
			return 0, err
		}
		mi, ok := idx[poolKey{etype, eid}]
		if !ok {
			continue
		}
		batch = append(batch, update{articleID, etype, eid, posOrNil(FirstMatchPos(title, mi))})
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, err
	}

	// One transaction per sport — far faster than autocommit for ~tens of
	// thousands of single-row updates.
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return 0, err
	}
	defer tx.Rollback(ctx)
	updated := 0
	for _, u := range batch {
		ct, err := tx.Exec(ctx, `
			UPDATE news_article_entities SET title_pos = $1
			WHERE article_id = $2 AND entity_type = $3 AND entity_id = $4 AND sport = $5
		`, u.pos, u.articleID, u.etype, u.eid, sportUpper)
		if err != nil {
			return updated, err
		}
		updated += int(ct.RowsAffected())
	}
	if err := tx.Commit(ctx); err != nil {
		return updated, err
	}
	return updated, nil
}

func parseArticleDate(s string) time.Time {
	if s == "" {
		return time.Time{}
	}
	for _, f := range []string{
		time.RFC1123Z,
		time.RFC1123,
		time.RFC3339,
		"2006-01-02T15:04:05Z",
		"2006-01-02T15:04:05-07:00",
	} {
		if t, err := time.Parse(f, strings.TrimSpace(s)); err == nil {
			return t
		}
	}
	return time.Time{}
}

// ---------------------------------------------------------------------------
// RSS implementation
// ---------------------------------------------------------------------------

// rssResponse is the minimal XML structure for Google News RSS.
type rssResponse struct {
	XMLName xml.Name  `xml:"rss"`
	Items   []rssItem `xml:"channel>item"`
}

type rssItem struct {
	Title       string `xml:"title"`
	Link        string `xml:"link"`
	PubDate     string `xml:"pubDate"`
	Description string `xml:"description"`
}

// fetchRSS runs one (query, window, edition) request and returns the items that
// survive the lookback filter. It records the raw item count and the window drop
// into f — the window is the first place an article can vanish, and until now
// the only trace of it was a smaller number downstream.
func (s *NewsService) fetchRSS(ctx context.Context, query string, hoursBack int, edition rssEdition, f *Funnel) ([]Article, error) {
	u, err := url.Parse(s.rssBaseURL)
	if err != nil {
		return nil, err
	}
	q := u.Query()
	q.Set("q", fmt.Sprintf("%s when:%s", query, rssWhenToken(hoursBack)))
	q.Set("hl", edition.hl)
	q.Set("gl", edition.gl)
	q.Set("ceid", edition.ceid)
	u.RawQuery = q.Encode()

	req, err := http.NewRequestWithContext(ctx, "GET", u.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (compatible; ScoracleBot/1.0)")
	req.Header.Set("Accept", "application/rss+xml, application/xml, text/xml")

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("RSS fetch error: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("RSS HTTP %d", resp.StatusCode)
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("RSS read error: %w", err)
	}

	var rss rssResponse
	if err := xml.Unmarshal(body, &rss); err != nil {
		return nil, fmt.Errorf("RSS parse error: %w", err)
	}

	articles := make([]Article, 0, len(rss.Items))

	for i, item := range rss.Items {
		title := item.Title
		source := "Google News"

		// Extract source from "Title - Source" format.
		if idx := strings.LastIndex(title, " - "); idx != -1 {
			source = strings.TrimSpace(title[idx+3:])
			title = strings.TrimSpace(title[:idx])
		}

		desc := cleanRSSDescription(item.Description)
		if len(desc) > 300 {
			desc = desc[:300] + "..."
		}

		articles = append(articles, Article{
			Title:       title,
			Description: desc,
			URL:         item.Link,
			Source:      source,
			PublishedAt: item.PubDate,
			ImageURL:    nil,
			// Google's own ordering, captured before anything re-sorts. Position 0 is the
			// result Google ranked first for this query — the cheapest quality signal in the
			// pipeline and the only one that costs nothing to produce.
			FeedRank: i,
		})
	}

	kept := filterArticlesByLookback(articles, hoursBack, time.Now())
	f.RSSItems += len(articles)
	f.WindowDropped += len(articles) - len(kept)
	return kept, nil
}

func (s *NewsService) fetchFromRSS(
	ctx context.Context,
	entityType string,
	entityName, sport, team string,
	limit int,
	firstName, lastName string,
	aliases []string,
) (map[string]interface{}, []Article, Funnel, error) {
	searchName := buildSearchName(entityName, firstName, lastName)
	searchQueries := buildRSSSearchQueries(entityType, searchName, sport, aliases)
	editions := rssEditionsForEntity(entityType, sport)

	var allArticles []Article
	f := Funnel{
		Entities:        1,
		EditionsPlanned: len(editions),
		QueriesPlanned:  len(editions) * len(searchQueries),
	}

	for editionIdx, edition := range editions {
		if limit > 0 && len(allArticles) >= limit && !runRSSQueryPastLimit(entityType, editionIdx, 0) {
			// The limit ended the sweep here: this edition and every one after it
			// go unqueried. Count the whole remaining grid so the cost of the cap
			// is legible instead of showing up only as missing coverage.
			f.EditionsSkipped += len(editions) - editionIdx
			f.QueriesSkipped += (len(editions) - editionIdx) * len(searchQueries)
			break
		}
		f.EditionsQueried++
		for queryIdx, query := range searchQueries {
			if limit > 0 && len(allArticles) >= limit && !runRSSQueryPastLimit(entityType, editionIdx, queryIdx) {
				f.QueriesSkipped += len(searchQueries) - queryIdx
				break
			}
			f.QueriesRun++
			for _, hours := range timeWindows {
				if err := ctx.Err(); err != nil {
					return nil, nil, f, err
				}
				f.RSSCalls++
				articles, err := s.fetchRSS(ctx, query, hours, edition, &f)
				if err != nil {
					f.RSSErrors++
					s.logger.Warn("RSS fetch error", "edition", edition.name, "window_hours", hours, "error", err)
					continue
				}

				// Stamp query provenance here, the only place the lane index is
				// known. deduplicateArticles below keeps the first occurrence, so
				// the stamp survives cross-lane collisions with the earliest lane.
				lane := "primary"
				if queryIdx > 0 {
					lane = fmt.Sprintf("alias%d", queryIdx)
				}
				for i := range articles {
					articles[i].queryTerm = query
					articles[i].queryLane = lane
					articles[i].queryEdition = edition.ceid
					articles[i].queryWindow = rssWhenToken(hours)
				}

				// No relevance filter here any more. This used to run MatchesEntity over
				// title+description and drop whatever did not name the entity — 4,859 of
				// 9,694 articles on the 2026-07-26 sweep, half of everything Google
				// returned, and fifteen clubs (Nice, Spezia, Leganés, Huesca …) admitted
				// nothing at all because their names are short or ambiguous.
				//
				// It was re-deriving something the query already established: we asked
				// Google for this entity, so the link IS the query. Relevance is now
				// decided once, downstream, by something that reads the body instead of
				// pattern-matching a headline. MatchRejected stays in the funnel and
				// stays at zero -- the invariant still has to balance, and a filter
				// returning here would need somewhere to report itself.
				beforeDedup := len(allArticles) + len(articles)
				allArticles = deduplicateArticles(append(allArticles, articles...))
				f.DedupCollapsed += beforeDedup - len(allArticles)

				if limit > 0 && len(allArticles) >= limit {
					break
				}
				if len(allArticles) >= newsMinArticles {
					break
				}
				if err := sleepContext(ctx, 100*time.Millisecond); err != nil {
					return nil, nil, f, err
				}
			}
		}
	}

	// Cut by Google's ranking, not by recency. This sort decides WHICH articles survive
	// `-rss-limit`, and sorting by date here meant the cap kept the newest rather than the
	// best: on the 2026-07-26 sweep that discarded 2,235 of 9,694 articles on publish time
	// alone, while the read budget downstream was separately ranking by `feed_rank` — two
	// different notions of "top N" in one path.
	//
	// Ranking relevance is the one thing Google is unambiguously better at than anything
	// here, and FeedRank is that judgment for free. Recency is not this product's axis; the
	// entity's story is, and a well-ranked piece from this morning beats a thin one from
	// twenty minutes ago. Stable, so that at equal rank the primary query still outranks the
	// alias lanes that ran after it.
	sortArticlesByFeedRank(allArticles)
	totalResults, allArticles := limitRSSArticles(allArticles, limit)
	f.LimitTruncated = totalResults - len(allArticles)
	f.Matched = len(allArticles)
	for _, a := range allArticles {
		if a.Description != "" {
			f.DescriptionBearing++
		} else {
			f.DescriptionEmpty++
		}
	}

	return map[string]interface{}{
		"query":    entityName,
		"sport":    sport,
		"articles": allArticles,
		"provider": "google_news_rss",
		"meta": map[string]interface{}{
			"total_results": totalResults,
			"returned":      len(allArticles),
			"source":        "google_news_rss",
		},
	}, allArticles, f, nil
}

func buildRSSSearchQueries(entityType, primarySearch, sport string, aliases []string) []string {
	if isTeamEntity(entityType) {
		return buildTeamRSSSearchQueries(primarySearch, sport, aliases)
	}

	sportSuffix := rssSportSuffix(sport)
	searchQueries := []string{primarySearch + sportSuffix}
	if best := bestAliasQuery(primarySearch, aliases); best != "" {
		searchQueries = append(searchQueries, best+sportSuffix)
	}
	return searchQueries
}


func buildTeamRSSSearchQueries(primarySearch, sport string, aliases []string) []string {
	primaryTerm := teamRSSPrimaryQueryTerm(primarySearch, sport, aliases)
	if strings.TrimSpace(primaryTerm) == "" {
		return nil
	}
	queries := []string{formatRSSQueryTerm(primaryTerm) + teamRSSSuffixForTerm(sport, primaryTerm)}
	for _, alias := range teamRSSAliasQueryTerms(primarySearch, primaryTerm, sport, aliases) {
		queries = append(queries, formatRSSQueryTerm(alias)+teamRSSSuffixForTerm(sport, alias))
	}
	return queries
}

func runRSSQueryPastLimit(entityType string, editionIdx, queryIdx int) bool {
	return isTeamEntity(entityType) && editionIdx == 0 && queryIdx < newsMinTeamRSSQueriesBeforeLimit
}

func rssSportSuffix(sport string) string {
	if sport == "" {
		return ""
	}
	if term, ok := sportTerms[strings.ToUpper(sport)]; ok {
		return " " + term
	}
	return ""
}

func teamRSSSportSuffix(sport string) string {
	// Football team queries carry no sport suffix. NOTE the original reason for this is now
	// GONE: the suffix was dropped because it suppressed non-English sources across the
	// localized editions, and those editions were retired (see footballTeamRSSEditions).
	// Left as-is deliberately rather than silently restored — " soccer football" is two extra
	// required terms on every query, so re-adding it trades noise for recall, and that trade
	// needs the A4 funnel to measure it rather than a guess. Risky solo terms (nice, inter,
	// como, …) already get the suffix via teamRSSSuffixForTerm regardless.
	// Keep sport suffixes for NBA/NFL teams, where many team names are common English words.
	if strings.ToUpper(strings.TrimSpace(sport)) == "FOOTBALL" {
		return ""
	}
	return rssSportSuffix(sport)
}

func teamRSSSuffixForTerm(sport, term string) string {
	if strings.ToUpper(strings.TrimSpace(sport)) == "FOOTBALL" && riskyFootballSoloQueryTerm(term) {
		return rssSportSuffix(sport)
	}
	return teamRSSSportSuffix(sport)
}

func rssEditionsForEntity(entityType, sport string) []rssEdition {
	if isTeamEntity(entityType) && strings.ToUpper(strings.TrimSpace(sport)) == "FOOTBALL" {
		return footballTeamRSSEditions
	}
	return defaultRSSEditions
}

type teamRSSAliasCandidate struct {
	term  string
	score int
	order int
}

func teamRSSPrimaryQueryTerm(primarySearch, sport string, aliases []string) string {
	primary := strings.TrimSpace(primarySearch)
	if primary == "" || strings.ToUpper(strings.TrimSpace(sport)) != "FOOTBALL" || !riskyFootballSoloQueryTerm(primary) {
		return primary
	}

	for _, alias := range aliases {
		alias = strings.TrimSpace(alias)
		if alias == "" || normalizedAliasKey(alias) == normalizedAliasKey(primary) {
			continue
		}
		if ok, _ := safeTeamRSSAliasQuery(primary, alias, true); ok && len(entityTermTokens(alias)) >= 2 {
			return alias
		}
	}

	return primary
}

func teamRSSAliasQueryTerms(primarySearch, primaryQueryTerm, sport string, aliases []string) []string {
	football := strings.ToUpper(strings.TrimSpace(sport)) == "FOOTBALL"
	seen := map[string]bool{
		normalizedAliasKey(primarySearch):    true,
		normalizedAliasKey(primaryQueryTerm): true,
	}
	candidates := make([]teamRSSAliasCandidate, 0, len(aliases))

	for i, alias := range aliases {
		alias = strings.TrimSpace(alias)
		if alias == "" {
			continue
		}
		key := normalizedAliasKey(alias)
		if seen[key] {
			continue
		}
		seen[key] = true
		ok, score := safeTeamRSSAliasQuery(primarySearch, alias, football)
		if !ok {
			continue
		}
		candidates = append(candidates, teamRSSAliasCandidate{term: alias, score: score, order: i})
	}

	sort.SliceStable(candidates, func(i, j int) bool {
		if candidates[i].score != candidates[j].score {
			return candidates[i].score > candidates[j].score
		}
		return candidates[i].order < candidates[j].order
	})

	limit := newsMaxTeamAliasRSSQueries
	if len(candidates) < limit {
		limit = len(candidates)
	}
	terms := make([]string, 0, limit)
	for i := 0; i < limit; i++ {
		terms = append(terms, candidates[i].term)
	}
	return terms
}

func safeTeamRSSAliasQuery(primarySearch, alias string, football bool) (bool, int) {
	alias = strings.TrimSpace(alias)
	if alias == "" {
		return false, 0
	}
	key := normalizedAliasKey(alias)

	if !football {
		if shortRSSAlias(alias) {
			return true, 70
		}
		return true, 80
	}

	if riskyFootballSoloQueryKey(key) {
		return false, 0
	}
	if strings.HasPrefix(key, "the ") {
		return false, 0
	}
	if shortRSSAlias(alias) {
		if trustedShortTeamAlias(alias) {
			return true, 120
		}
		return false, 0
	}
	if trustedFootballAliasQuery(key) {
		return true, 110
	}

	tokens := entityTermTokens(alias)
	if len(tokens) < 2 && !sharesMeaningfulEntityToken(primarySearch, alias) {
		return false, 0
	}

	score := 60
	if len(tokens) >= 2 {
		score += 20
	}
	if sharesMeaningfulEntityToken(primarySearch, alias) {
		score += 25
	}
	if containsClubDesignator(tokens) {
		score += 10
	}
	return true, score
}

func trustedShortTeamAlias(alias string) bool {
	return trustedShortTeamAliases[strings.ToUpper(strings.TrimSpace(alias))]
}

func trustedFootballAliasQuery(key string) bool {
	switch key {
	case "barca", "barça", "spurs", "juve":
		return true
	default:
		return false
	}
}

func riskyFootballSoloQueryTerm(term string) bool {
	return riskyFootballSoloQueryKey(normalizedAliasKey(term))
}

func riskyFootballSoloQueryKey(key string) bool {
	switch key {
	case "athletic",
		"celtic",
		"city",
		"club",
		"como",
		"dynamo",
		"inter",
		"lens",
		"nice",
		"racing",
		"rangers",
		"real",
		"rovers",
		"sporting",
		"united",
		"union",
		"victory",
		"wanderers":
		return true
	default:
		return false
	}
}

func normalizedAliasKey(s string) string {
	return strings.ToLower(strings.Join(strings.Fields(s), " "))
}

func shortRSSAlias(s string) bool {
	n := 0
	for _, r := range s {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			n++
		}
	}
	return n > 0 && n < 4
}

func formatRSSQueryTerm(term string) string {
	term = strings.TrimSpace(strings.ReplaceAll(term, `"`, ""))
	if strings.ContainsAny(term, " \t-") {
		return `"` + term + `"`
	}
	return term
}

func entityTermTokens(s string) []string {
	return strings.FieldsFunc(strings.ToLower(s), func(r rune) bool {
		return !unicode.IsLetter(r) && !unicode.IsDigit(r)
	})
}

func sharesMeaningfulEntityToken(a, b string) bool {
	aTokens := make(map[string]bool)
	for _, tok := range entityTermTokens(a) {
		if meaningfulEntityToken(tok) {
			aTokens[tok] = true
		}
	}
	for _, tok := range entityTermTokens(b) {
		if meaningfulEntityToken(tok) && aTokens[tok] {
			return true
		}
	}
	return false
}

func meaningfulEntityToken(tok string) bool {
	switch tok {
	case "", "a", "ac", "afc", "as", "at", "ca", "cd", "cf", "club", "de", "el", "fc", "la", "los", "of", "rc", "sc", "the", "u":
		return false
	default:
		return len(tok) >= 4
	}
}

func containsClubDesignator(tokens []string) bool {
	for _, tok := range tokens {
		switch tok {
		case "ac", "afc", "as", "ca", "cd", "cf", "fc", "rc", "sc", "utd":
			return true
		}
	}
	return false
}

func rssWhenToken(hoursBack int) string {
	// Google News RSS accepts when:Nh / Nd / Ny. Use hours directly when
	// under a day so the six-hour cron searches only the matching lookback.
	switch {
	case hoursBack <= 0:
		return "1d"
	case hoursBack < 24:
		return fmt.Sprintf("%dh", hoursBack)
	case hoursBack <= 168:
		return fmt.Sprintf("%dd", (hoursBack+23)/24)
	default:
		return "30d"
	}
}

func filterArticlesByLookback(articles []Article, hoursBack int, now time.Time) []Article {
	if hoursBack <= 0 || now.IsZero() {
		return articles
	}
	cutoff := now.Add(-time.Duration(hoursBack)*time.Hour - newsLookbackSlack)
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		published := parseArticleDate(a.PublishedAt)
		if published.IsZero() || !published.Before(cutoff) {
			out = append(out, a)
		}
	}
	return out
}

var (
	rssHTMLTagRe    = regexp.MustCompile(`<[^>]+>`)
	rssEntityRe     = regexp.MustCompile(`&(nbsp|amp|quot|apos|lt|gt|#3[49]|#160);`)
	rssWhitespaceRe = regexp.MustCompile(`\s+`)
)

// cleanRSSDescription renders a Google News RSS <description> as plain text. It strips the anchor
// markup, decodes the handful of entities Google actually emits, and collapses whitespace.
//
// This is data hygiene, not interpretation. The raw field is
// `<a href="...">Headline</a>&nbsp;&nbsp;<font>Source</font>`, so tag-stripping alone left literal
// "&nbsp;&nbsp;" glued between the headline and the outlet name — which then reached the model's
// prompt verbatim and was embedded as if it were prose. 99.7% of descriptions are the title
// repeated plus the source, so what this mostly does is make that redundancy legible to the
// consumers that now have to decide whether the field adds anything.
func cleanRSSDescription(raw string) string {
	s := rssHTMLTagRe.ReplaceAllString(raw, " ")
	s = rssEntityRe.ReplaceAllStringFunc(s, func(e string) string {
		switch e {
		case "&amp;":
			return "&"
		case "&quot;", "&#34;":
			return `"`
		case "&apos;", "&#39;":
			return "'"
		case "&lt;":
			return "<"
		case "&gt;":
			return ">"
		default: // &nbsp; / &#160;
			return " "
		}
	})
	return strings.TrimSpace(rssWhitespaceRe.ReplaceAllString(s, " "))
}

func articleMatchText(a Article) string {
	if a.Description == "" {
		return a.Title
	}
	return a.Title + " " + a.Description
}

func limitRSSArticles(articles []Article, limit int) (int, []Article) {
	total := len(articles)
	if limit > 0 && len(articles) > limit {
		return total, articles[:limit]
	}
	return total, articles
}

func sleepContext(ctx context.Context, d time.Duration) error {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

// bestAliasQuery returns the best alias to use as a fallback search query.
// Prefers the longest alias that differs from the primary search name.
func bestAliasQuery(primarySearch string, aliases []string) string {
	primaryLower := strings.ToLower(primarySearch)
	best := ""
	for _, a := range aliases {
		// Skip short aliases (abbreviations) — they're too broad for search queries.
		if len(a) < 4 {
			continue
		}
		if strings.ToLower(a) == primaryLower {
			continue
		}
		if len(a) > len(best) {
			best = a
		}
	}
	return best
}

// ---------------------------------------------------------------------------
// Helpers — name matching, dedup, sort (ported from Python)
// ---------------------------------------------------------------------------

// buildSearchName shortens very long names (e.g. Brazilian players).
func buildSearchName(fullName, firstName, lastName string) string {
	parts := strings.Fields(fullName)

	// Long names (4+ parts): use first + last.
	if len(parts) >= 4 && firstName != "" && lastName != "" {
		return firstName + " " + lastName
	}

	// Names ending in Jr/Junior/II/III: use first + suffix.
	if len(parts) >= 3 {
		suffix := strings.ToLower(parts[len(parts)-1])
		if suffix == "jr" || suffix == "jr." || suffix == "junior" || suffix == "ii" || suffix == "iii" {
			return parts[0] + " " + parts[len(parts)-1]
		}
	}

	return fullName
}

// deduplicateArticles removes duplicate articles by normalized source key.
func deduplicateArticles(articles []Article) []Article {
	seen := make(map[string]bool)
	out := make([]Article, 0, len(articles))
	for _, a := range articles {
		keys := articleDedupKeys(a)
		if len(keys) == 0 {
			out = append(out, a)
			continue
		}
		if hasSeenArticleKey(seen, keys) {
			continue
		}
		for _, key := range keys {
			seen[key] = true
		}
		out = append(out, a)
	}
	return out
}

func articleDedupKeys(a Article) []string {
	keys := make([]string, 0, 2)
	if raw := strings.TrimSpace(a.URL); raw != "" {
		u, err := url.Parse(raw)
		if err == nil && strings.EqualFold(u.Hostname(), "news.google.com") {
			u.RawQuery = ""
			u.Fragment = ""
			keys = append(keys, "url:"+strings.ToLower(u.String()))
		} else {
			keys = append(keys, "url:"+raw)
		}
	}
	title := strings.Join(strings.Fields(a.Title), " ")
	source := strings.Join(strings.Fields(a.Source), " ")
	key := strings.TrimSpace(title + " " + source)
	if key != "" {
		keys = append(keys, "title_source:"+strings.ToLower(key))
	}
	return keys
}

func hasSeenArticleKey(seen map[string]bool, keys []string) bool {
	for _, key := range keys {
		if seen[key] {
			return true
		}
	}
	return false
}

// sortArticlesByFeedRank orders articles by the position Google gave them, best first.
//
// Stable on purpose: FeedRank is per-query, so the primary query's rank-3 and an alias
// lane's rank-3 collide. Ties then resolve to the order the queries actually ran, which
// puts the entity's own name ahead of its aliases.
func sortArticlesByFeedRank(articles []Article) {
	sort.SliceStable(articles, func(i, j int) bool {
		return articles[i].FeedRank < articles[j].FeedRank
	})
}


# Stories Page Surface — the storylines get read

**Date:** 2026-08-12

## The concept, and what vetting found

Scott's pitch: "the Editor compiles the stories of the day, the voices tell each
entity's part — we're already tracking the players in the story, why not surface
the whole story?" Vetting found the tracking **already built**: `storylines`
(mig 200) are the durable cross-day story objects, `storyline_entities` carries
each participant's role + lifespan (D5), `packets` (mig 202) are the compiled
snapshots — ~7,600+ in prod. The only missing piece was the surface, and that
absence was a recorded decision (PLAN-one-rail Appendix B, D-6: "parked until
Scott picks a surface").

Scott picked it: **Stories is the 4th AppTray page** (home, search, stories,
leaderboard).

## Decision trail (each step simplified the last)

1. ~~D-6 front page~~ (`front_pages` table + day-close Editor pick call) —
   superseded: Stories is a tray page, not a "front page of the day".
2. ~~Deterministic recency/volume ordering~~ — rejected: "Real Madrid and the
   Cowboys will always dominate."
3. ~~Newsroom vote~~ (all 4 narrative voices ballot a closed numbered list,
   Borda merge, `story_rankings` table + `story_rank` stage) — designed in
   full, then set aside as over-engineered. **It remains the recorded upgrade
   path if SQL heat feels flat.**
4. **Shipped: "the characters decide," minimally.** The voices already scored
   every entity — the Journalist's `card_score` IS "how much story is here,"
   the Influencer's sentiment IS emotional heat. A storyline inherits its
   cast's banked scores; ranking is one commented ORDER BY.

## What shipped (Go-only — no migration, no Rust, no model calls)

- `GET /api/v1/{sport}/stories` — open storylines (≤14d quiet) ranked by cast
  heat: `max(card_score)` over subject-role cast (fallback: whole active cast),
  vibe sentiment tie-break. `?status=resolved|dormant` = archive by recency;
  `?limit=` default 50 cap 200. The heat formula lives in ONE commented spot in
  the `story_list` statement — it's a taste knob.
- `GET /api/v1/{sport}/story/{id}` — the storyline whole: cast with roles AND
  lifespans (departed members stay — the part has its own lifespan), packet
  headline history (the append-only supersedes chain IS the evolving story),
  one full latest packet (claims/quotes/facts/register), 20 attached articles
  with mig-217 provenance, and voice-product endpoint pointers per active
  player/team cast member. 404 on unknown/wrong-sport id.

Files: `go/internal/db/db.go` (`story_list`, `story_archive`, `story_page`),
`go/internal/api/handler/data.go` (`GetStories`, `GetStory`),
`go/internal/api/server.go` (routes), `ENDPOINTS.md`. These are the **first
readers of the storylines/packets tables** — the one-rail archive becomes a
product.

## Verification (plumbing over benchmarks)

No Go toolchain on the Mac; everything ran on archbox in a scratch tree
(cleaned up after): gofmt/build/vet/test clean, `validate-stmts` registered
every statement against the live prod schema, and the statements were executed
against prod data through the exact handler call path (`pool.QueryRow` by
statement name). Football list came back heat-ordered (Villa/Watkins transfer
saga + the Garnacho-dig storyline lead at 99); storyline 8125's page served the
full shape; bogus id → no rows → 404. Resolved archive is `[]` — prod simply
has no resolved storylines yet.

## Open notes

- **Payload size on mega-storylines**: storyline 8125 serves ~90KB (65 cast,
  150 claims, 63 voice-product entries). Works, but caps on `voice_products`
  and possibly claims are the likely first tune once the frontend consumes it.
- **Heat ties at 99 are common** (card_score ceiling); vibe tie-break carries.
- **Deferred, recorded**: Phase 2 = story-scoped voice takes (Journalist,
  Insider, Influencer, Analyst) under the story page's `takes` key — "their
  side without the tarot-card limits"; D-9 rivalry taxonomy (Cowboys–Eagles
  class team-pair perennials don't clear ATTACH_THRESHOLD and would go dormant
  every 14 quiet days); the newsroom-vote ranking if SQL heat disappoints.
- Frontend: the AppTray button + the two views live in scoracle-frontend.

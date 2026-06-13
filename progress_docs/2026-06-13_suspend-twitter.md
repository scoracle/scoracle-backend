# 2026-06-13 — Suspend the live Twitter/X integration

## Goal
Twitter's live X-API fetch was the platform's only multi-second upstream (~11s cold when
an entity-feed request triggered a stale sport-feed refresh). With the product value moving
to Gemma news summaries (see vault `wiki/Plan - News to Gemma Summaries.md`), suspend the
live integration cleanly — keep the code, flip it off by default.

## What Was Done
Added a `TWITTER_ENABLED` master switch (mirrors the existing `TRANSFER_ENABLED` pattern),
**defaulting to `false`** so deploying suspends it with no prod env edit; re-enabling is an
explicit `TWITTER_ENABLED=true` opt-in.

- `config.go`: new `Config.TwitterEnabled` from `envBool("TWITTER_ENABLED", false)`.
- `thirdparty/twitter.go`: `TwitterService` gains an `enabled` field (via `NewTwitterService`).
  - `HasBearerToken()` → `enabled && bearerToken != ""` (drives the handlers' 503 guard).
  - `IsConfigured(sport)` → returns false when `!enabled`. This is the key gate: the
    entity-feed handler (`GetEntityTweets`) only triggers the opportunistic
    `GetSportFeed` **live fetch** when `IsConfigured` is true — so suspending skips the one
    multi-second upstream. Entity/sport feeds then return cached-or-empty fast.
- `handler.go` + `cmd/api/main.go`: pass `cfg.TwitterEnabled` into `NewTwitterService`.
- `.env`: documented `TWITTER_ENABLED=false`.

Vibes are unaffected: `ml/vibe.go` reads tweets cache-only + news and only skips Gemma when
*both* are empty — with news present it keeps generating sentiment; it just stops ingesting
fresh tweets.

## Files Changed
- `go/internal/config/config.go`, `go/internal/thirdparty/twitter.go`,
  `go/internal/api/handler/handler.go`, `go/cmd/api/main.go`, `.env`

## Verification
- `go build ./...` clean; `go vet` clean; `go test ./internal/thirdparty/... ./internal/config/...` pass.
- Logic trace: `TWITTER_ENABLED=false` → `IsConfigured` false → `GetEntityTweets` skips the
  live `GetSportFeed` refresh → the ~11s upstream is never hit; the frontend's
  `getTwitterFeed` already `.catch()`es to an empty feed.

## Result
The live X integration is suspended behind a flag (code retained). Deploying this build
removes the ~11s latency from the news/twitter path. Re-enable later with
`TWITTER_ENABLED=true` once/if X is worth resuming.

## Deploy
Build + `systemctl --user restart scoracle-api` (validate prepared statements first). No
migration. No prod `.env.local` edit needed (default-off).

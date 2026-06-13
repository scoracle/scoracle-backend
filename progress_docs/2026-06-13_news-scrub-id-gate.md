# 2026-06-13 — Gemma news scrub / ID-gate (Stage 1)

## Goal
Build + verify the Gemma "scrub" — the precision pass that vets the fuzzy matcher's
(deliberately liberal, high-recall) entity links: confirm each linked entity is GENUINELY
the article's subject and disambiguate same-name people via their identity card. Stage 1 of
the vault plan; the move that lets us open the news net while killing false positives.

## What Was Done
- `internal/ml/news_scrub.go` — `NewsScrubber.ScrubArticle(articleID, sport, dryRun)`: loads
  the article + every entity currently linked to it (with the identity card — name ·
  nationality · canonical current club · latest position, same disambiguators transfers
  use), asks Gemma which candidates the article is genuinely about (reusing the transfer
  subject-resolver principle: current club is the tie-breaker), returns a per-candidate
  verdict, and (only when `!dryRun`) deletes the dropped links.
  - **Primary link preserved:** the `confidence = 1.0` link (the entity the article was
    fetched for, returned by Google RSS for that query) is deterministically relevant and
    never droppable; Gemma only vets the secondary fuzzy guesses.
- `cmd/newsscrub/main.go` — dry-run/apply CLI (by team's candidate-rich articles, or a
  single article id).

## Verification (live dry-run, no writes) — Chelsea, last 7 days
- **Romano→Roma false positive killed:** article "…as Romano confirms agreement" had a
  fuzzy link to **Roma** (the club, from the journalist *Romano*) and to **Portu** (incidental).
  Gemma **dropped both**, kept Chelsea. Exactly the goofy name-matching noise, gone.
- "Pre-**son**" → dropped **Son**; kept genuine subjects (Cucurella, Barcelona, Tyrique
  George, Everton, Juventus, a coach's former clubs).
- Primary-link bug found + fixed: it first dropped Chelsea on the Cucurella article (the
  primary link); now preserved.
- `go build ./...` + `go vet` clean.

## Result
The scrub disambiguates and de-noises the link table cleanly on the live corpus. Verified
dry-run only — nothing written. Additive (does not change ingestion yet).

## Next
- Wire the scrub into ingestion (scrub newly-persisted articles), and point news + transfers
  at the vetted links; retire the fuzzy-only secondary matching + the `033` proximity gate
  together. Loosen the fuzzy pre-filter (open the gates) once the scrub is the precision pass.
- Decide apply semantics: delete dropped links (current) vs a `vetted` flag.

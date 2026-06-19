# Hygiene — remove stray `go/api` binary + gitignore guard

**Date:** 2026-06-18  ·  Backend, repo hygiene (no deploy).

## Goal
A 32 MB stray binary sat untracked at `go/api` (from a `go build ./cmd/api` run outside `go/bin/`).
`.gitignore` covers `bin/` and `go/bin/` but not a loose `go/api`, so a `git add .` would have committed
32 MB of binary.

## What Was Done
- `rm go/api` (rebuildable artifact; binaries belong in `go/bin/`).
- `.gitignore`: added `/go/api`, `/go/vibesynth`, `/go/statcommentary` guards under the Go section so
  stray `go build ./cmd/<x>` outputs are never trackable.

## Files Changed
`.gitignore` (+ removed untracked `go/api`).

## Verification
`git status` clean apart from the pre-existing auto-generated `seed/...SOURCES.txt` egg-info churn.

## Result
No more 32 MB stray binary; future stray builds of the three CLI binaries are ignored.

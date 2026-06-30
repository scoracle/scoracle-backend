# 2026-06-30 - Retire CLAUDE.md

## Goal

Retire `CLAUDE.md` now that `README.md` is the active session guide.

## What Changed

- Moved durable backend development guidance into `docs/DEVELOPMENT.md`.
- Linked the development rules from `README.md`.
- Removed the obsolete `CLAUDE.md` entry point.

## Files Changed

- `README.md`
- `RUNBOOK.md`
- `docs/DEVELOPMENT.md`
- `CLAUDE.md`
- `progress_docs/2026-06-30_retire-claude-md.md`

## Verification

- Harvested endpoint, implementation-boundary, migration, style, and key-file guidance before deletion.
- Confirmed operational details remain in `RUNBOOK.md` and route contracts remain in `ENDPOINTS.md`.
- Documentation-only change; no Go, Rust, Python, or database verification required.

## Result

Future backend sessions start from `README.md`, with local development rules in `docs/DEVELOPMENT.md`.

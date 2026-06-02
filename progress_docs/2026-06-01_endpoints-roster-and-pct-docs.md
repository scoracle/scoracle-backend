# 2026-06-01 — ENDPOINTS.md: roster endpoint + starline pct fields

## Goal

Document tonight's two rating-engine endpoint changes in `ENDPOINTS.md`, and make
the roster⇄leaderboard **shared row shape** explicit so future devs pick up on the
double usage of the board pattern.

## What Was Done

- New **`GET /{sport}/team/{id}/roster`** section, placed directly after
  `/leaderboard` so the relationship is adjacent. Params + response shape (a real
  SGA roster row) + a callout: roster is the leaderboard's **player board narrowed
  to one team** and re-sorted by the Composite+Specialist sum — the **same rating
  row**, which is why one frontend list component (`RatingList`) renders both. The
  callout notes the recipe generalizes (any board-over-a-slice = same row,
  different `WHERE` + `ORDER BY`).
- Bumped the rating-engine intro from "two dedicated endpoints" to **three**.
- Added the per-event **`rating_composite_pct` / `rating_specialist_pct`**
  (migration 029) to the starline `events[]` example + a paragraph on the 0–100
  percentile and why it exists (shared 0–100 axis with the vibe series).

## Files Changed

```
ENDPOINTS.md
```

## Verification

- Roster + pct sections present (`grep`); code fences balanced (28, even).
- Examples use real API values (SGA roster row, Wemby starline pct 99.4 / 96.2).

## Result

`ENDPOINTS.md` now covers all three rating-engine endpoints and surfaces the
board⇄roster shared shape as the reusable pattern it is.

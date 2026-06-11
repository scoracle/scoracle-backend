# 062 — Football players: Blocks + Clearances → display

**Date:** 2026-06-10

## Goal

Blocks and Clearances reward reactive, last-ditch volume, not defender quality —
the same finding that pulled them from the team composite in 060. Move them off the
football *player* composite too.

## Why (data)

- **Perverse vs outcome:** among PL defenders, blocks/clearances correlate POSITIVELY
  with the team's goals conceded (blocks +0.24, clearances +0.19) — high volume means
  you got bombarded, not that you're elite. Tackles/interceptions are neutral (~0.06–0.09).
- **Not noise, but misleading:** they're the *most* repeatable defensive stats (yoy 0.66
  / 0.83), so a high-volume blocker/clearer posts extreme z-scores that dominate the
  composite (a deep-block CB leapt rank 33→99 on one season's spike).
- **PAdj can't fix it:** the volume is individual (~5% explained by team possession), and
  most defenders sit at average possession, so possession-adjustment is a no-op. And we
  don't de-weight or cap.

So, consistent with the team treatment (display-only since 060), move them to the player
display tier.

## What was done

- `rating_datapoints` (FOOTBALL player branch only): `Clearances` and `Blocks` flipped
  in_comp/in_spec TRUE→FALSE (display-only). Tackles, Interceptions, Duels, etc. unchanged.
  Recompute football player ratings. NBA/NFL untouched. No API restart (not a prepared
  statement); the frontend z-pizza reflects it automatically.

## Files changed

- `sql/migrations/062_football_player_blocks_clearances.sql` (new; migration-canonical)

## Verification

Local clone: 062 applies clean — gate confirms 0 Blocks/Clearances in the composite,
both still present as display (7112 rows). The block/clearance merchants fall to mid-pack
(Burnley CB 88→57, Fulham 88→70, Everton 99→83); the top deep-block CB drops 99.4→**91.8**
— a strong, ball-involved season minus the junk, as intended.

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK) → `migrate.sh` apply. No API restart, no cf:deploy.

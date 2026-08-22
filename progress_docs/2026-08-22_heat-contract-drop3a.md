# The heat contract, drop 3a — one number key everywhere, and the stories page gets its recap

**Date:** 2026-08-22 (merged `f729072`, PR #5; deployed same day)
**Plan:** `planning_docs/PLAN-heat-contract.md` (drop 3b remains)

## Goal

Scott's calls, closing the loop the headline/body contract opened:

1. **The Editor stays voiceless** — it compiles and ranks, never writes prose we'd have
   to tune. The stories page's prose comes from the **Journalist**, who already writes
   storyline-linked chapters (`news_summaries.storyline_id`, mig 219). The recap is a
   join, not a generation.
2. **Heat is relative — stack rank by BEST FOR PRODUCT.** `heat` is each product's own
   number on its native scale, one uniform serving key; the per-board rank expression
   (a code-owned taste parameter) absorbs the product semantics. No normalization.
3. Confirmed rank expressions for 3b: vibes `ABS(sentiment−50) DESC` (a 3-meltdown at
   47 beats a 90-euphoria at 40 — charge, not valence), momentum `ABS(score) DESC`
   (biggest movers either way). Recap teller: latest chapter regardless of teller.

## What Changed (all additive — nothing served lost a key or reordered)

- **Every board row carries `heat`** mirroring its existing number: vibes/sigil
  (=score), news (=impact), momentum boards (=1-dp slope), rating research board
  (=the scope's sort metric). `transfers_leaderboard` already served `heat` natively.
- **Every profile voice card carries `heat`**: /news per-narrative (=impact),
  /transfers card (=insider score), /sigil current (=score), /momentum summary
  (=score), /rating top-level (=season composite).
- **/stories + /story/{id} serve `recap`** — the latest storyline-linked Journalist
  chapter `{headline, body, teller, generated_at}`, lateral over the LIMITed rows via
  `idx_news_summaries_storyline`; null when no chapter yet. Plus packet `routing_tags`.
- **The `/vibe` endpoint restored** (SOS, concurrent session — commits `487e601`,
  `15302d9`): the Influencer's card had been reachable only inside /momentum since the
  O14 rename. Serve-latest `current` `{headline, body, heat=sentiment}` + a 7-day
  prose-carrying snapshot feed; never-scored serves `current: null` with a 200. The
  registration line lost in commit interleaving was restored (`e3399c0`). ENDPOINTS.md
  documents the card.

## Verification

- Audit vs prod before building: recap covers 59/60 of heat-ranked top-20 rows across
  sports (33–48% overall — the gap is cold-tail storylines); avg chapter is 16h newer
  than its packet. Vibes reorder preview: 14–18 of each top-20 survive the 3b flip.
- Gate ladder: archbox scratch gofmt/vet/build/test → validate-stmts vs live prod
  schema (branch tip verified standalone after each interleaved commit) → EXECUTE
  smokes of all changed statements + entity_vibe against prod data through `db.New`
  (key assertions: heat mirrors its twin; recap 20/20 on the served football page;
  never-scored vibe → null current) → CI green ×3 → merge → `release.sh` @ `f729072`
  → live API smoke (stories recap+tags, board heat, /vibe card, profile heat) → both
  daemons active, zero errors/panics post-deploy, /health/db healthy.
- `story_list` with the recap lateral: 122ms cold / 55ms warm at limit 50 (~98KB).

## Watch items

- **Sigil board refill is trough-gated (~1wk), by design** — zero crowns since 08-20
  20:03; 3,681 ready sigil rows can't claim a shared slot while upstream backlogs are
  deep (DAG drain, sigil terminal). Not a stall; don't restart anything. Lever if the
  board matters sooner: reserve sigil a drain slot, or halve nightly caps (churn plan).
- **Recap attribution quality:** the live top football story (Bruno/Newcastle, 7649)
  serves a Bordeaux-relegation chapter told by Marseille — mode-of-cited-articles can
  attach an off-topic chapter. Rule is working as specified; watch whether mismatched
  recaps are common enough to want a cast-membership filter on the teller.
- Momentum guard rejections ran heavy today (641 "READ carries ASCII digits" of 805
  failed) and vibe carried 255 fails since 08-21 (hook_max_words, foreign-script) —
  fail-closed guards doing their job, but the reject rate is worth a tuning look.

## Next: drop 3b (the coordinated break — frontend/iOS first)

Old number keys drop from payloads (`score`, board `impact`-as-score, `card_score` as
a serving key); rank expressions land (vibes charge, momentum ABS); re-confirm no
client reads a removed key. ENDPOINTS.md stays authoritative.

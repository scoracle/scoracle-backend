# Plan — the heat contract (drop 3): one number per voice, ranked best-for-product

Opened 2026-08-22, out of the headline/body contract session (drops 1+2,
`progress_docs/2026-08-22_headline-body-contract.md`). Scope is **serving-layer only** —
no migration, no Rust, no new generation. Status: **ACTIVE — all taste cells confirmed by Scott
2026-08-22; audit underway.**

---

## Where this came from

Scott's calls, 2026-08-22:

1. **The Editor stays voiceless.** It compiles and it ranks; it never writes prose we'd
   have to tune, because an Editor voice poisons everything downstream of the rail. The
   stories page's prose comes from the **Journalist**, who already writes it.
2. **Heat is relative — stack rank by BEST FOR PRODUCT.** For some products higher is
   best, for some the middle is. The transform is a per-board taste parameter and it
   lives in code (SQL), never in a model.
3. (2) also answers normalization: there isn't any. Scales stay voice-native; the rank
   expression absorbs the product semantics.

## What exists today (audited against schema + db.go, 2026-08-22)

- **Stories serve zero model prose.** `packets.headline` is the best member *article's*
  title (lowest feed_rank) — a source headline, nobody's voice. The Editor's only model
  output is the per-article read (ep1: structured fields + evidence_blurb). Nothing to
  untune. ✅ already matches call (1).
- **The Journalist already writes storyline-linked prose.** `news_summaries.storyline_id`
  (mig 219) is derived deterministically (mode of the cited articles' storylines) and
  indexed: `idx_news_summaries_storyline (storyline_id, generated_at DESC) WHERE
  storyline_id IS NOT NULL`. The recap is a **read**, not a generation.
- **Every voice already stores headline + body + one product number.** Drop 1+2 shipped
  the headline/body halves; the numbers still serve under a per-voice key zoo
  (`impact`, `card_score`, `sentiment`, `score`, `notability`, rating).
- The Chelsea/Enzo mechanic (profile shows every narrative, board surfaces the hottest)
  is already live on /news via per-narrative `impact`.

## The contract

**Stories list** (`/stories`): `headline` (packet, code-compiled — unchanged) ·
`recap` (latest non-marker Journalist chapter for the storyline: `narrative_title` +
`body`; `null` when no chapter yet — frontend decides) · tags (`story_types`,
`register`, `routing_tags` (new), `cast` — all served, frontend picks) · `heat`
(existing editor-native formula, unchanged). `/story/{id}` gains the same `recap`.

**Boards**: every row serves `{headline, heat, rank}` + identity/trajectory metadata.
`heat` = the voice-native number, native scale. `rank` = position under that board's
**rank expression** — the best-for-product transform, a commented taste parameter in
the statement and only there (stories-heat precedent). `ORDER BY` = the rank expression,
so the served order IS the product order even when heat itself isn't monotonic in it.

**Profiles**: every voice card serves `{headline, body, heat}` + existing metadata
(omen, direction, trajectory markers stay).

## Per-board heat + rank expressions — CONFIRMED by Scott 2026-08-22

| Board | `heat` served (native scale) | Rank expression (proposal) | Change? |
|---|---|---|---|
| news | `impact` (0–100, deterministic) | `impact DESC` | none — rename only |
| transfers | pair rumor score (1–99) | `score DESC` | none — rename only |
| rating | composite rating | `rating DESC` | none — rename only |
| sigil | `score` (1–100) | `score DESC` | none — rename only |
| **vibes** | `sentiment` (1–100) | **`ABS(sentiment−50) DESC`** — charge, not valence: a 3-meltdown (47) outranks a 90-euphoria (40). Scott: "highest emotional score wins." | **reorder** |
| **momentum** | `score` (−5..+5) | **`ABS(score) DESC`** — biggest movers at the top, either direction | **reorder** |
| stories | editor-native formula | unchanged | none |

Profile card heat: /news narrative → `impact` · /transfers pair → pair score, wire-read
card → `insider_scores.score` · /sigil → `score` · /momentum → `score` · /rating →
rating. `card_score` (Journalist) retreats to audit + prompt memory — the
`insider_scores.read` precedent, in reverse.

## Drops

- **Drop 3a — additive, cannot break clients:** /stories + /story gain `recap` and
  `routing_tags`; boards and profiles gain the `heat` key alongside existing keys.
- **Drop 3b — the coordinated break** (frontend/iOS ready first): old number keys
  (`score`, `impact`, `sentiment`, `card_score` as serving keys) drop from payloads;
  rank expressions land (vibes + momentum reorder); ENDPOINTS.md rewritten.

## Audit results (run against prod 2026-08-22)

- [x] **Recap coverage:** 33–48% of open storylines overall (FOOTBALL 637/1362, NBA
      127/384, NFL 211/436) — but the VISIBLE page is covered: 59/60 of heat-ranked
      top-20 rows across sports carry a chapter (134/150 of top-50). The thin tail is
      cold storylines; null recaps there are honest. Avg chapter is 16h NEWER than the
      latest packet; only 56/975 lag it by >48h.
- [x] Teller choice: **latest chapter by `generated_at` regardless of teller** (Scott:
      simplest is often best). Teller mix for the record: 414/975 latest tellers are
      subject-role; 258 aren't cast members at all (mode-of-articles attribution).
- [x] **Statement inventory** (3b's break surface): `vibes_leaderboard`·`sigil_leaderboard`
      ·`narratives_leaderboard` serve `score`; `trending_*` serve `score`+`slope`;
      `transfers_leaderboard` already serves `heat` natively; `leaderboard` serves
      `rating`/`fantasy_points`; profiles serve `impact` (news), `card_score`
      (news+transfers), `score` (sigil current, momentum summary), `rating` (rating).
- [x] **Vibes reorder diff:** 14–18 of each sport's old top-20 survive the
      ABS(sentiment−50) flip (904/306/983 board pop; 143/49/212 rows sit below 40 —
      the meltdowns the current valence board buries). Momentum: ~⅓ of scored
      summaries are negative — fallers will genuinely surface.
- [x] 3b removes no key in 3a; client-read confirmation re-runs before the 3b break.

## Drop 3a shipped state (this session)

Additive keys live in every changed statement; gates run: gofmt/vet/build/test clean
on the archbox scratch tree, validate-stmts OK vs live prod schema, EXECUTE smokes of
all 13 changed statements against prod data through db.New — ALL GREEN (sigil board
executes but is empty in prod: zero or11-headline crowns exist yet, see watch item).
story_list with the recap lateral: 122ms cold / ~55ms warm at limit 50 (~98KB payload).

## ⚠️ Watch item found during gating — sigil junction starved

Zero crowns generated since 08-20 20:03 (all or10): 3,681 sigil work rows are ready
(`available_at <= NOW()`) with ZERO claims. Cause: the drain tops up stages in
registration (DAG) order and sigil is LAST in the shared archbox slot group; with deep
narratives/vibe/momentum/transfers backlogs since ~08-21, earlier stages fill every
shared slot on every pass. The sigil board cannot refill with or11 crowns until the
pillar backlog drains (~1wk trough forecast) — or the drain reserves sigil a slot.
Separate signal: momentum guard rejections are heavy today (641 "READ carries ASCII
digits" among 805 failed) and vibe has 255 failed since 08-21 — worth a look.

## Gate ladder (unchanged)

Local suites / archbox scratch-tree Go gates → CI (go + validate-stmts vs snapshot ·
shell · lineage) → validate-stmts vs live prod schema → EXECUTE smokes of every changed
statement through the real prepared-statement layer → merge on green → `release.sh` →
live API smoke. No migration ⇒ no dump/restore drill needed; rollback is
`git checkout <last-good> && release.sh`.

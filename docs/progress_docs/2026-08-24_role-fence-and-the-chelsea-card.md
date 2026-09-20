# The role fence: the Chelsea card was never a fabrication

**Session of 2026-08-24 (second session).** The live Chelsea vibe card (`vibe_scores` 60602)
carried Tottenham's £75m Savinho/Marmoush saga, a De Zerbi quote, and Mansfield's nickname.
The handoff brief called it fabrication. It was not.

---

## 1. The diagnosis: the model invented nothing

The cognition ledger keeps `built_prompt`. Ledger row 275124 is card 60602's exact prompt,
and **every "invented" item is in it verbatim**:

- The £75m Savinho deal, Marmoush's medicals, the £300m spree, the De Zerbi "kill us" quote —
  all in the first STORY packet block ("Tottenham close in on double transfer as medicals
  booked", storyline 18470).
- "The Stags" — in the PREVIOUS VIBE continuity anchor (the prior card was itself
  contaminated, so the echo compounds).

The brief's grep over the five `input_news_ids` articles found nothing because packet claims
are **not** covered by `input_news_ids` — that column carries only the narratives' source
articles. The packet block's claims come from other articles entirely. Bug 1 *is* Bug 2:
role-blind packet reads, exactly the ban-loses-to-the-input law.

Why did Chelsea read Tottenham's storyline at all? Enzo Fernandez appears in it in passing
(City want him to replace Rodri), Chelsea joined `storyline_entities` with a blank role, and
`load_packets_for_entity` filtered on `left_at IS NULL` alone. The Editor's verdict was
persisted and consumed by nothing.

## 2. The role policy (decided, per the brief's open question)

- **`subject` → in. Everything else → out** (`opponent`, `passing_mention`, blank, `absent`).
  Every voice reads through `load_packets_for_entity`, so the fence is there, once.
- `opponent` is out deliberately: Go queries news one ranked query per team, so a team's real
  coverage always arrives on its own lane with the team as subject. The opponent-role blocks
  measured on the live card were cross-team noise (the Scally/Gladbach mess).
- Blank (12,264 active rows) is out by the standing law: fail open to silence, never to a
  guess. Cost measured before shipping: Chelsea keeps 13+ subject storylines, so the card
  stays populated; some legit blanks (the Joao Pedro contract story) go quiet until they heal.
- `absent` (532 rows) now **never attaches** — the Editor explicitly said the entity is not in
  the text; a participant edge would fan work out to a story that never mentioned it.
- The role upsert takes the **strongest role over time** (was: first non-null wins, forever).
  A story that becomes about an entity upgrades it to subject, and the blank backlog heals
  forward with new coverage — no backfill, no regeneration, consistent with the no-regen rule.

Shipped as PR #12 (`role-fence-subject-only-packet-reads`, a38e4e3), deployed to archbox from
the branch 2026-08-24 ~18:00 ET. **PR #12 is open, awaiting Scott's merge** — the merge itself
was permission-blocked for the agent; a routine main release after merging re-converges.

## 3. Verified against the live card

- Post-filter, Chelsea's packet read returns the Liverpool/Chelsea valuation story and the
  Fernandez/Alonso saga; storyline 18470 (Tottenham, blank role) no longer loads.
- Forced one narratives regeneration for team 18: the four new narratives are all genuinely
  Chelsea (valuation war, Fernández deadline, Napoli's Guiu/Lang squeeze, Stamford Bridge /
  Champions League). Zero Tottenham.
- Forced vibe regeneration (manual queue row): card 61362 reads Chelsea's actual stories.
  Residue: one "kill us"/De Zerbi clause and one "Stags" mention, inherited from the PREVIOUS
  VIBE anchor (prompt-only continuity). Cleared 61362's `input_hash` (F2's NULL-hash-runs
  escape) and regenerated once more so the prior itself is clean.

## 4. Discovered on the way: one-waker means narratives does NOT wake vibe

The Journalist's handler deliberately does not enqueue the Influencer (7.6/E3, the mig-197
churn rationale) — she wakes only from mig 206's packet fan-out on `charged` packets. So a
manual narratives regen does NOT cascade to vibe; a manual vibe row is needed too. The sigil
chain does fire (sigil regenerated itself at 18:01, one guard retry).

**Follow-up worth weighing:** the mig 206/225/231 fan-out trigger is still role-blind — a
Tottenham packet still wakes Chelsea's vibe/narratives rows. With the read fence the wake
debounces before the GPU, so the cost is a claim + two queries, not a generation. A migration
filtering the trigger's participant fan-out to `role = 'subject'` would cut the noise; not
shipped today to keep the deployed diff minimal.

## 5. The 2-of-6 tabs mystery: stale frontend deploy

The backend served all six pillars all along (the brief proved it). The frontend repo's last
three commits — the dynamic presence-dealt deck, the Vibe card's own endpoint, SolidStart 2 —
were pushed to GitHub but **never deployed to Cloudflare**. Scott confirmed and authorized;
deployed via `npm run cf:deploy` (version 46b20ccf). The live Chelsea page now SSRs all six
tabs. Committed the stray `package-lock.json` sync (06c87f1).

## 6. THE CARD CONTRACT (same session, second act — Scott's brief before merging)

Scott: every direct-to-consumer character is score + headline + body as a CORE structure —
score top-middle (established), headline = the 140-char tweet hook, body = the voice under
fit-on-card. The Journalist and Insider were the two seats without an entity-level headline
(mig 226 called their per-item titles "their own headlines"; overruled — those are body
furniture). Shipped in the same PR:

- **Mig 232**: `news_summaries.headline` (generation-level, card_score pattern, markers
  included) + `insider_scores.headline`. Additive, no backfill.
- **Journalist n21 NOT bumped** (v23 precedent — no fleet regen; heals with movement).
  **Insider is4→is5 bumped** (is4 precedent — self-backfills one wrap per live-wire entity).
  Both hooks parse best-effort and settle through `guards::settle_title`.
- **THE BODY TRAIL** (Scott: "save only the bodies — that way we can better tell the
  developing story… having these narratives as reference will be a huge enrichment factor",
  the Palmer/Sanchez Fulham example): both seats' prior-read memory now leads with dated
  prose — the Insider's recent read bodies (280B each), the Journalist's front-page hooks plus
  his previous filing's top-3 storylines — and the score TRAIL collapses to the single latest
  anchor (momentum-s19). Extending the same treatment to the Analyst/Scout/Oracle memory
  cards is the natural follow-up.
- **Serving repairs found on the way**: the deck-score ring reads `card_score` on /news and
  /transfers — drop 3b had retired the former and the latter was serving `heat`, so BOTH
  cards' scores were broken against the deployed frontend; and /news items served
  `headline`/`heat` where the frontend `Narrative` type reads `narrative_title`/`impact`, so
  narrative titles rendered blank. Both payloads now serve both key sets.
- **The contract is written down** as THE CARD CONTRACT table in `junctions/mod.rs` — where
  each seat's triple lives.
- Verified live: Chelsea's first is5 wrap headline — "Chelsea's striker hunt stalls: Delap
  tests availability, but no deals materialize beyond lingering Adarabioyo track."

## 7. THE WEEK ARCHIVE (same session, third act — "I think I've got this unlocked")

The rail gets a clock. Scott's convention, decisions confirmed in-session (merged timeline /
card-shows-that-generation / Jan-1 week blocks):

- **`GET /{sport}/{type}/{id}/headlines?year=&week=`** — the card contract's index: every
  seat's (score, headline, body) entries for one week (week 1 = Jan 1–7), merged newest-first.
  The Journalist's entries carry storylines as `items`; the Scout's archive score is NULL (his
  number is season state). One statement (`entity_headlines`), UNION over the six product
  tables, headline-bearing generations only — the archive reaches exactly as far as the
  mig 226/232 rollout.
- **Frontend**: a Week dropdown on the conditions line, every card, default "Today". A week
  replaces the deck with the timeline (day-grouped rows: seat · score · time · headline);
  tapping deals that generation as a full archive card (score top-middle, hook, body) with a
  back step. `?week=YYYY-N` in the URL; one fetch powers both levels. New:
  `WeekArchive.tsx/css`, `lib/utils/week.ts`, `lib/data/headlines.server.ts`.
- Verified live: Chelsea week 34 serves 31 entries across all six seats.
- **THE DECK-OF-CARDS CORRECTION** (Scott, same evening: "We CANNOT lose the cards, that's
  our single most important brand pillar... the deck reflects the week selected"). The first
  cut rendered the week as a flat merged timeline replacing the deck — wrong surface. Rebuilt:
  a selected week deals the SAME deck (panes, pile, rail, arrows, swipe untouched); each
  seat's card face lists ITS headlines for the week (day · time · score · hook, newest
  first), and tapping one turns the face into that day's card — score top-middle, hook, full
  body — with a back step. Cards are dealt by archive presence (the Scout filed no headline
  in week 34, so his card isn't dealt), and all panes share ONE /headlines fetch. The merged
  cross-seat ordering now exists only in the API payload; if a "whole week at a glance" read
  is ever wanted, it should be a seventh CARD, never a page.
- Note: the old `newsScope` bucket dropdown still drives the LIVE News/Transfers cards and is
  hidden in week mode; retiring it in favour of the week axis is a natural follow-up.

## 8. State at close (2026-08-25 ~00:15Z)

- **PR #12 MERGED** (main `bea6532`) — the role fence, the card contract + mig 232, the
  body-trail memory, and the week-archive endpoint all landed in one branch. Production
  released from main same night; API + cognition healthy at `bea65327ed91`. Feature branch
  deleted everywhere; both checkouts clean on main.
- Frontend on main at `fd0f954`, deployed to Cloudflare (`9fe4991f`) with the deck-faithful
  week mode.
- Follow-ups carried forward: role-fence the mig 206/225/231 fan-out trigger (waste-only);
  the relational memory card's non-subject leak (mig 163, DB-side); body-trail memory for
  the Analyst/Scout/Oracle seats; retire `newsScope`; a name-grounding guard for the 3B's
  occasional invented name; frontend dependabot (1 high).

## 9. Still open / not this session

- **Relational memory contamination.** The memory card (`narrative_context_for_entity`,
  mig 163) still lists non-subject storylines ("this entity's part: passing_mention") and
  cross-entity priors (Brighton stories, on Chelsea's card). It is labeled continuity-not-
  evidence, but it is another door for the same leak. Same fence, different read — needs a
  migration since the SQL lives DB-side.
- **Prior-echo decay.** Any card generated pre-fence can echo once more through PREVIOUS
  VIBE. One manual decay pass was run for Chelsea only; the fleet heals as it regenerates.
- Storyline 18488's newest packet headline is a Real Madrid/Lille title on a Chelsea-subject
  storyline — storyline assembly drift, out of scope here.
- GitHub reports 7 dependabot vulnerabilities (2 high) on scoracle-frontend.

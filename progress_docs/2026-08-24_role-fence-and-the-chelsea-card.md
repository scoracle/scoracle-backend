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

## 6. Still open / not this session

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

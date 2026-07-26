# Handoff — finish the junction pass, then reorganize `rust/src`

Repo `/home/sheneveld/scoracle/scoracle-backend`, branch `main`, tree clean, **~16 commits
unpushed** (wiki has 1). Fetch/pull both repos first; parallel sessions push to origin.
**This Archbox IS production.** Prod actions need Scott's named approval.

Read `scoracle-wiki/progress_docs/2026-07-25_relevance-root-cause-and-teardown.md` — the plan of
record, plus the evening ADDENDUM which supersedes several of its phases. Do not re-derive the
diagnosis; it is measured and settled.

## State — all deployed and running

The news rail was rebuilt today: Google casts the net → Go records facts → **The Reader judges** →
The Journalist builds memory. The scrub GPU relevance gate, `resolve.rs`, `bin/relevance_bands.rs`
and all BGE/candle use outside `narratives`/`threads` are **deleted**. Novelty is pure
`token_jaccard >= 0.90`. Headline passthrough, Google `feed_rank` (mig 194) and a per-entity read
budget are live. The Reader runs `gemma3:4b`.

## Task 1 — six junctions still own their prompts inline

`src/prompts/reader.rs` is the **reference implementation**; copy its shape exactly.

| junction | stage file | prompt version | builder |
|---|---|---|---|
| The Journalist | `narratives.rs` (2,569) | `n13` | `build_narratives_prompt:613` |
| The Oracle | `sigil.rs` (2,152) | `or5` | `build_crown_prompt:971` |
| The Insider | `transfer.rs` (3,291) | `t11` + `is1` + identity-adjudication-v2 | `build_transfer_prompt:747`, `build_insider_score_prompt:2205` |
| The Influencer | `vibe.rs` (1,255) | `v14` | `build_sentiment_prompt:469` |
| The Analyst | `momentum.rs` (875) | `momentum-s7` | `build_momentum_prompt:311` |
| The Scout | `rating.rs` (2,463) | `s14` | `build_stat_prompt:785` |

`graph.rs` (`g3`) is typed extraction, not a character — migrate it, but do not invent a persona.
`judge.rs` is eval tooling, not a live junction; leave it.

Per junction: move the `*_PROMPT_VERSION` const and the builder (plus any prompt-only helpers) into
`src/prompts/<character>.rs`; `pub use` them from the stage module so no call site changes; make the
types the builder touches `pub(crate)`. Write a character-grade header stating **seat, contract
version, what it reads, what it feeds, and its authority** — and check the existing header for
claims that stopped being true today (several still describe the Candle and the scrub gate).
Update the table in `src/prompts/mod.rs` as each lands. `cargo test --lib` must stay at 230+ green.

## Task 2 — then reorganize

The flat `src/` is idiomatic Rust; the problem is 30 modules with no sense of role and four files
over 2,000 lines. Once prompts are extracted, group the junctions (`reader`, `narratives`, `sigil`,
`transfer`, `vibe`, `momentum`, `rating`, `graph`) under `src/junctions/`, leaving infrastructure
(`harness`, `route`, `ollama`, `work`, `worker`, `stage`, `db`, `config`, `ledger`, `embed`) and
primitives (`novelty`, `threads`, `trajectory`, `bucket`, `corpus`, `util`) at the root. Mechanical,
but do it as its own commit, after the prompts move, so a bisect can tell the two apart.

## Open items, in priority order

1. **Watch The Reader's `irrelevant` rate — and disambiguate it.** gemma3:4b sat at **0.0% across
   its first 24 readings** against mistral's **14.4%** baseline. At n=24 that is p≈2.4% under the
   baseline rate, so it is no longer dismissible as small-sample noise. It is now the *sole*
   relevance judge; if it never rejects, it is doing half its job.

   **But there is a strong confound, and it is the more likely explanation.** The 14.4% baseline was
   measured on a FIFO queue of everything; gemma reads only top-ranked articles under the new budget,
   and Google's top hits are genuinely more often about the entity. A lower rejection rate is what a
   working ranking system *should* produce. The two are distinguishable only with a rank-matched
   comparison, which was impossible on 07-25 because `feed_rank` had just started populating.

   With a day of data, compare like with like — if the rate stays ~0% even on poorly-ranked
   articles, the judge is the problem; if it rises as rank worsens, the ranking is working:
   ```sql
   SELECT CASE WHEN a.feed_rank IS NULL THEN 'unranked'
               WHEN a.feed_rank < 3 THEN 'top3' ELSE 'rest' END AS band,
          r.model_version, count(*),
          round(100.0*count(*) FILTER (WHERE r.status='irrelevant')
                /NULLIF(count(*) FILTER (WHERE r.status IN ('success','irrelevant')),0),1) AS irrelevant_pct
     FROM news_article_readings r JOIN news_articles a ON a.id=r.article_id
    WHERE r.updated_at > NOW()-INTERVAL '24 hours' GROUP BY 1,2 ORDER BY 1,2;
   ```
   A cheaper direct check that needs no rank data: hand-read 20 of gemma's `success` verdicts and
   look for articles that plainly are not about the entity.
   ```sql
   SELECT t.name AS entity, left(a.title,70) AS title, left(r.evidence_blurb,90) AS blurb
     FROM news_article_readings r
     JOIN news_articles a ON a.id = r.article_id
     JOIN news_article_entities nae ON nae.article_id = a.id AND nae.vetted IS TRUE
     JOIN teams t ON t.id = nae.entity_id AND t.sport = nae.sport   -- sport! see Traps below
    WHERE r.model_version = 'gemma3:4b' AND r.status = 'success'
    ORDER BY r.updated_at DESC LIMIT 20;
   ```
2. **Then raise the read budget to `COGNITION_ARTICLE_READ_TOP_K=8`** in `.env.local` + restart.
   Measured: K=4 → 701 reads/day (16.4% of ingest), K=8 → 1,058/day (24.7%) — Scott's 25% target.
   gemma sustains ~3,150/day (131.4/hr vs mistral 53.7/hr, 2.45x), so K=8 is affordable.
3. **Phase 2.3 — delete the panic guards** (`-rss-limit`, `short_code` solo lanes,
   `newsMaxTeamAliasRSSQueries`, risky-solo-term lists), then re-run
   `./scripts/ops/news_ingest_funnel.sh`. The funnel shows `-rss-limit` discarding **3,401 of 5,267**
   articles per sweep *after* they were fetched, matched and deduped. Do this last — it raises volume.
   Also make truncation rank-aware or it keeps discarding Google's top hits by construction
   (it truncates *after* `sortArticlesByDate`).
4. **Phase 3 of the plan — thread identity to The Journalist.** Build the `narrative_threads` merge
   path first (4,457 singletons), add `continues_thread` to the output contract, fix E7
   (`threads.rs:131` has `FOR UPDATE` with no `ORDER BY`). This is what finally deletes
   `Harness.embedder` — `narratives` clustering and `threads.rs` centroid are its last two consumers.
5. `topic_heat_embeddings` is orphaned — nothing reads or writes it. Drop in a later migration.
6. `examples/transfer_t10_fixtures.rs` does not compile (pre-existing, unrelated).

## Operational

- Env: `set -a && source .env.local && set +a`. `.env.local` is **gitignored** — the gemma route
  lives only on this box, with its rationale and rollback in a comment beside it.
- Deploy is atomic rename, never `cp` (ETXTBSY), never `pkill`:
  `cargo build --bin scoracle-cognition && cp target/debug/scoracle-cognition bin/.new && chmod 700 bin/.new && mv -f bin/.new bin/scoracle-cognition`
  The systemd **user** path unit auto-restarts. Go binary is `go/bin/pipeline`, same pattern.
- `COGNITION_STAGES` in `.env.local` **overrides** the systemd unit. Units are `systemctl --user`.
- `sql/schema/schema.sql` is a snapshot after mig 183; migs 184–194 live only in their files.

## Traps that cost real time today

- **`teams.id` is per-sport, not globally unique** — 204 rows, 157 distinct ids. Any join from
  `news_article_entities.entity_id` to `teams.id` **must** include `sport`, or ~47 teams are scored
  against another club's name. This silently turned a 100% result into 70%.
- **A stage with no model call must not be paced like one.** `StageHandler::rotation_batch()`
  exists because scrub, once model-free, was still draining one item per multi-minute rotation —
  a 7,165-item backlog would have taken weeks.
- **Verify a plan's claims against the code before implementing.** Phase 1.1 of the plan of record
  had already shipped hours before the plan was written; implementing it would have been a no-op.

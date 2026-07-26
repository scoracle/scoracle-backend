# Handoff — the junction pass is DONE; open items below

Repo `/home/sheneveld/scoracle/scoracle-backend`, branch `main`, tree clean.
Fetch/pull both repos first; parallel sessions push to origin.
**This Archbox IS production.** Prod actions need Scott's named approval.

> **Tasks 1 and 2 completed 2026-07-25** (commits `d965459`, `3beb75c`, `a277c9a`, `d4b55d1`).
> See "What landed" below before reading the rest — several sections of this doc describe the
> old layout. The binary in `bin/` is UNCHANGED; the refactor is not deployed.

Read `scoracle-wiki/progress_docs/2026-07-25_relevance-root-cause-and-teardown.md` — the plan of
record, plus the evening ADDENDUM which supersedes several of its phases. Do not re-derive the
diagnosis; it is measured and settled.

## State — all deployed and running

The news rail was rebuilt today: Google casts the net → Go records facts → **The Reader judges** →
The Journalist builds memory. The scrub GPU relevance gate, `resolve.rs`, `bin/relevance_bands.rs`
and all BGE/candle use outside `narratives`/`threads` are **deleted**. Novelty is pure
`token_jaccard >= 0.90`. Headline passthrough, Google `feed_rank` (mig 194) and a per-entity read
budget are live. The Reader runs `gemma3:4b`.

## What landed — Tasks 1 and 2, 2026-07-25

Every model-calling seat is one directory under `rust/src/junctions/`, named for its **character**,
holding exactly three files: `mod.rs` (stage machinery), `prompt.rs` (system prompt +
`*_PROMPT_VERSION` + format schema + builder, nothing else), `tests.rs`.

| junction | was | now | contract |
|---|---|---|---|
| The Reader | `reader.rs` | `junctions/reader/` | `ar3` |
| The Journalist | `narratives.rs` | `junctions/journalist/` | `n13` |
| The Oracle | `sigil.rs` | `junctions/oracle/` | `or5` |
| The Insider | `transfer.rs` | `junctions/insider/` | `t11` + `is1` + identity-adjudication-v2 |
| The Influencer | `vibe.rs` | `junctions/influencer/` | `v14` |
| The Analyst | `momentum.rs` | `junctions/analyst/` | `momentum-s7` |
| The Scout | `rating.rs` | `junctions/scout/` | `s14` |
| *(not a character)* | `graph.rs` | `junctions/graph/` | `g3` |

`src/prompts/` was **deleted** — one home per junction, not two. This deviates from the plan above,
which wanted a parallel `src/prompts/` tree; Scott chose one-dir-per-junction, so the already-shipped
`prompts/reader.rs` folded into `junctions/reader/prompt.rs`. `judge.rs` is eval tooling, not a live
junction, and was left alone.

Also done: 2,982 lines of test module moved out of the stage files into `tests.rs` (no file in
`src/junctions/` exceeds 2,000 lines now, down from four); six stale header claims corrected
(`route(SynthesisLogic)` → `OracleLogic`, four wrong `route(EmotionalNews)` claims, prompt versions
in prose reading n5/s9/t5). Verified byte-identical prompt bodies by diffing every line against the
parent commit — the only differences are `use` statements and five deliberate visibility widenings.
`cargo test --lib` stayed at **230 passed** through all four commits; zero build warnings.

**Traps for the next session.** Queue-stage identifiers were deliberately NOT renamed —
`pipeline_work` rows and `COGNITION_STAGES` still say `narratives`, `sigil`, `vibe`, `transfers`,
`peak`, `momentum`, because those name rows in a table, not seats. And `examples/graph_probe.rs`
joins `transfer_t10_fixtures.rs` as pre-existing-broken: it references `Harness.resolve`, deleted
with `resolve.rs` in the teardown.

**Not deployed.** `bin/scoracle-cognition` is still the pre-refactor binary; prod runs unchanged.

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
5. **Ollama is thrashing the GPU — measured 07-25, undecided.** **702 `llama runner started`
   events in 6 hours** (~1 reload/min). `mistral:7b` (5.1 GB, every character) plus `gemma3:4b`
   (~3.3 GB, The Reader) is ~8.4 GB against the 1070 Ti's 8192 MiB, and `OLLAMA_KEEP_ALIVE=30m`
   has both trying to stay resident, so an evict-and-reload fires on nearly every alternation
   between The Reader and a character stage. Note this interacts with item 2: raising `TOP_K`
   raises Reader volume, which raises the alternation rate.

   Scott asked about `OLLAMA_FLASH_ATTENTION=1` + `OLLAMA_KV_CACHE_TYPE=q8_0`. q8_0 KV
   **requires** FA (hard dependency), and this card is **compute capability 6.1 (Pascal)** — the
   bad case: GP104 runs fp16 at 1/64 of fp32 and llama.cpp's tensor-core FA kernels need cc ≥ 7.5,
   so it falls back to vec kernels. Expect flat-to-slower tok/s. The win is *headroom* (~240 MiB
   off mistral's KV at 4096 ctx, less on gemma3's sliding-window attention) — roughly half the
   ~400 MiB gap, so it is a coin flip alone. `OLLAMA_MAX_LOADED_MODELS=1` or a shorter keep-alive
   on the Reader route attacks the thrash directly. Judge by the reload count, never tok/s:
   `journalctl -u ollama --since "1 hour ago" | grep -c "llama runner started"`.

   **Trap:** ollama is a **system** unit (`/etc/systemd/system/ollama.service`, `User=ollama`),
   NOT `systemctl --user` like the scoracle units. Restarting it drops every loaded model.
6. `topic_heat_embeddings` is orphaned — nothing reads or writes it. Drop in a later migration.
7. `examples/transfer_t10_fixtures.rs` and `examples/graph_probe.rs` do not compile (pre-existing).

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

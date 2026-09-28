#!/usr/bin/env python3
"""Read-only Archbox export and same-machine memory query comparison.

Experimental retrieval probe, not a production memory selector. Requires duckdb
and psycopg[binary]. The source connection is repeatable-read/read-only; benchmark
Postgres uses session-local TEMP tables only. No credentials enter artifacts.
"""
import argparse
import hashlib
import json
import math
import platform
import statistics
import time
from pathlib import Path

import duckdb
import psycopg
from psycopg import sql


# Fixed projections keep production JSON, raw pages and generated prose out of
# this probe. Source titles/descriptions are retrieval candidates, not approved
# reporting for the articulation model.
TABLES = {
    "events": ("id bigint, sport text, subject_type text, subject_id bigint, predicate text, object_type text, object_id bigint, confidence text, article_id bigint, reported_at bigint, publisher text, origin text, prompt_version text",
        "SELECT id,sport,subject_type,subject_id,predicate,object_type,object_id,confidence,article_id,floor(extract(epoch FROM event_date))::bigint,source,origin,prompt_version FROM public.narrative_events ORDER BY id"),
    "links": ("sport text, link_type text, subject_type text, subject_id bigint, object_type text, object_id bigint, strength integer, confidence text, event_count integer, article_count integer, distinct_sources integer, first_seen bigint, last_seen bigint",
        "SELECT sport,link_type,subject_type,subject_id,object_type,object_id,strength,confidence,event_count,article_count,distinct_sources,floor(extract(epoch FROM first_seen_at))::bigint,floor(extract(epoch FROM last_event_at))::bigint FROM public.narrative_links ORDER BY sport,link_type,subject_type,subject_id,object_type,object_id"),
    "stories": ("id bigint, sport text, title text, status text, first_seen bigint, last_seen bigint",
        "SELECT id,sport,title,status,floor(extract(epoch FROM first_seen_at))::bigint,floor(extract(epoch FROM last_seen_at))::bigint FROM public.storylines ORDER BY id"),
    "parts": ("storyline_id bigint, sport text, entity_type text, entity_id bigint, role text, authority text, entry_count integer, last_seen bigint, left_at bigint",
        "SELECT storyline_id,sport,entity_type,entity_id,role,authority,entry_count,floor(extract(epoch FROM last_seen_at))::bigint,floor(extract(epoch FROM left_at))::bigint FROM public.storyline_entities ORDER BY storyline_id,sport,entity_type,entity_id"),
    "story_articles": ("storyline_id bigint, article_id bigint",
        "SELECT storyline_id,article_id FROM public.storyline_articles ORDER BY storyline_id,article_id"),
    "articles": ("id bigint, publisher text, published_at bigint, title text, description text",
        "SELECT id,source,floor(extract(epoch FROM published_at))::bigint,title,description FROM public.news_articles a WHERE EXISTS(SELECT 1 FROM public.narrative_events e WHERE e.article_id=a.id) OR EXISTS(SELECT 1 FROM public.storyline_articles s WHERE s.article_id=a.id) ORDER BY id"),
    "names": ("sport text, entity_type text, entity_id bigint, name text",
        "SELECT * FROM (SELECT sport,'team'::text AS entity_type,id AS entity_id,name FROM public.teams UNION ALL SELECT sport,'player',id,name FROM public.players UNION ALL SELECT sport,'person',id,full_name FROM public.persons) n ORDER BY sport,entity_type,entity_id"),
}

INDEXES = [
    "CREATE INDEX ON events(sport,subject_type,subject_id,reported_at DESC)",
    "CREATE INDEX ON events(sport,object_type,object_id,reported_at DESC)",
    "CREATE INDEX ON events(article_id)",
    "CREATE UNIQUE INDEX ON articles(id)",
    "CREATE UNIQUE INDEX ON stories(id)",
    "CREATE INDEX ON parts(sport,entity_type,entity_id,last_seen DESC)",
    "CREATE INDEX ON story_articles(storyline_id,article_id)",
    "CREATE UNIQUE INDEX ON names(sport,entity_type,entity_id)",
]


def export(dsn, directory):
    directory.mkdir(parents=True, exist_ok=False)
    started = time.perf_counter()
    manifest = {"tables": {}, "projection": "journalist-memory-probe-v1"}
    with psycopg.connect(dsn, options="-c default_transaction_read_only=on -c statement_timeout=30000") as conn:
        conn.execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        conn.execute("SET LOCAL TIME ZONE 'UTC'")
        stamp = conn.execute("SELECT pg_current_snapshot()::text, floor(extract(epoch FROM transaction_timestamp()))::bigint, version(), current_setting('transaction_read_only')").fetchone()
        manifest.update(mvcc_snapshot=stamp[0], as_of=stamp[1], source_version=stamp[2], source_read_only=stamp[3])
        for name, (_, query) in TABLES.items():
            path = directory / f"{name}.csv"
            with path.open("wb") as out, conn.cursor().copy(f"COPY ({query} LIMIT 1000001) TO STDOUT WITH (FORMAT CSV, HEADER TRUE)") as copy:
                for block in copy:
                    out.write(block)
            manifest["tables"][name] = {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    manifest["export_seconds"] = time.perf_counter() - started
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def queries(as_of):
    # Same SQL in both engines, fixed clock, stable tie-breaks and explicit null
    # ordering. Confidence/predicate remain extraction metadata, never facts.
    earliest = as_of - 90 * 86400
    return {
        "fleet_reporting_change": f"""
WITH endpoints AS (
 SELECT sport,subject_type AS entity_type,subject_id AS entity_id,article_id,reported_at,publisher
 FROM events WHERE origin='extraction' AND reported_at>={as_of - 14 * 86400} AND reported_at<{as_of}
 UNION ALL
 SELECT sport,object_type,object_id,article_id,reported_at,publisher
 FROM events WHERE origin='extraction' AND reported_at>={as_of - 14 * 86400} AND reported_at<{as_of}
 AND object_id IS NOT NULL AND (object_type<>subject_type OR object_id<>subject_id)
), study AS (
 SELECT sport,entity_type,entity_id,
 count(DISTINCT article_id) FILTER(WHERE reported_at>={as_of - 7 * 86400}) AS current_articles,
 count(DISTINCT article_id) FILTER(WHERE reported_at<{as_of - 7 * 86400}) AS previous_articles,
 count(DISTINCT publisher) FILTER(WHERE reported_at>={as_of - 7 * 86400}) AS current_publishers,
 count(DISTINCT publisher) FILTER(WHERE reported_at<{as_of - 7 * 86400}) AS previous_publishers,
 min(reported_at) AS first_report,max(reported_at) AS last_report,
 array_agg(DISTINCT article_id ORDER BY article_id) AS article_ids
 FROM endpoints GROUP BY sport,entity_type,entity_id
)
SELECT s.sport,s.entity_type,s.entity_id,n.name,s.current_articles,s.previous_articles,
 s.current_publishers,s.previous_publishers,s.first_report,s.last_report,s.article_ids
FROM study s JOIN names n USING(sport,entity_type,entity_id)
ORDER BY s.sport,s.entity_type,s.entity_id""",
        "entity_event_history": f"""
SELECT e.id,e.predicate,e.confidence,e.reported_at,e.publisher,a.title,a.description
FROM events e JOIN articles a ON a.id=e.article_id
WHERE e.sport='FOOTBALL' AND e.origin='extraction'
 AND ((e.subject_type='team' AND e.subject_id=18) OR (e.object_type='team' AND e.object_id=18))
 AND e.reported_at BETWEEN {earliest} AND {as_of}
ORDER BY e.reported_at DESC,e.id DESC LIMIT 8""",
        "entity_story_history": f"""
WITH selected AS (
 SELECT p.storyline_id,p.role,p.authority,p.entry_count,p.last_seen
 FROM parts p WHERE p.sport='FOOTBALL' AND p.entity_type='team' AND p.entity_id=18
 AND p.left_at IS NULL AND p.last_seen<={as_of}
 ORDER BY p.last_seen DESC,p.storyline_id DESC LIMIT 8
), reports AS (
 SELECT s.id,s.title,s.status,p.role,p.authority,p.entry_count,a.id AS article_id,
 a.published_at,a.publisher,a.title AS source_title,
 row_number() OVER(PARTITION BY s.id ORDER BY a.published_at DESC,a.id DESC) AS rn
 FROM selected p JOIN stories s ON s.id=p.storyline_id
 JOIN story_articles sa ON sa.storyline_id=s.id JOIN articles a ON a.id=sa.article_id
 WHERE a.published_at<={as_of}
)
SELECT id,title,status,role,authority,entry_count,article_id,published_at,publisher,source_title
FROM reports WHERE rn<=2 ORDER BY id,rn""",
        "fleet_event_memory": f"""
WITH endpoints AS (
 SELECT sport,subject_type AS entity_type,subject_id AS entity_id,id AS event_id,predicate,article_id,reported_at,publisher
 FROM events WHERE origin='extraction' AND reported_at BETWEEN {earliest} AND {as_of}
 UNION ALL
 SELECT sport,object_type,object_id,id,predicate,article_id,reported_at,publisher
 FROM events WHERE origin='extraction' AND reported_at BETWEEN {earliest} AND {as_of}
 AND object_id IS NOT NULL AND (object_type<>subject_type OR object_id<>subject_id)
), coverage AS (
 SELECT sport,entity_type,entity_id,predicate,count(DISTINCT article_id) AS articles,
 count(DISTINCT publisher) AS publishers,min(reported_at) AS first_report,max(reported_at) AS last_report
 FROM endpoints GROUP BY sport,entity_type,entity_id,predicate
), latest AS (
 SELECT *,row_number() OVER(PARTITION BY sport,entity_type,entity_id,predicate ORDER BY reported_at DESC,event_id DESC) AS rn
 FROM endpoints
), ranked AS (
 SELECT c.*,l.article_id,l.event_id,
 row_number() OVER(PARTITION BY c.sport,c.entity_type,c.entity_id ORDER BY c.last_report DESC,c.predicate) AS priority
 FROM coverage c JOIN latest l USING(sport,entity_type,entity_id,predicate) WHERE l.rn=1
)
SELECT r.sport,r.entity_type,r.entity_id,n.name,r.predicate,r.articles,r.publishers,r.first_report,r.last_report,r.event_id,r.article_id,a.publisher,a.title
FROM ranked r JOIN names n USING(sport,entity_type,entity_id) JOIN articles a ON a.id=r.article_id
WHERE r.priority<=3 ORDER BY r.sport,r.entity_type,r.entity_id,r.priority""",
    }


def canonical(rows):
    return json.dumps(rows, ensure_ascii=False, separators=(",", ":"))


def measure(fn, rounds):
    samples = []
    rows = None
    for _ in range(rounds):
        start = time.perf_counter()
        current = fn()
        samples.append((time.perf_counter() - start) * 1000)
        if rows is not None and rows != current:
            raise RuntimeError("unstable result within frozen snapshot")
        rows = current
    return rows, {"median_ms": statistics.median(samples), "p95_ms": sorted(samples)[math.ceil(.95 * len(samples)) - 1], "samples_ms": samples}


def benchmark(dsn, directory, output, rounds):
    manifest = json.loads((directory / "manifest.json").read_text())
    if manifest["projection"] != "journalist-memory-probe-v1":
        raise ValueError("unsupported snapshot projection")
    for name, table in manifest["tables"].items():
        if hashlib.sha256((directory / f"{name}.csv").read_bytes()).hexdigest() != table["sha256"]:
            raise ValueError(f"snapshot changed: {name}")
    duck = duckdb.connect(config={"threads": 2, "memory_limit": "512MB", "max_temp_directory_size": "256MB"})
    result = {"snapshot": manifest, "host": platform.platform(), "duckdb_version": duckdb.__version__, "rounds": rounds, "queries": {}, "timing_scope": "persistent local connections, execute plus fetch; no SSH, export or table load in query timings"}
    with psycopg.connect(dsn, autocommit=True, prepare_threshold=None) as pg:
        # Only pg_temp is writable in this script. No permanent DDL/DML.
        pg.execute("SET search_path TO pg_temp")
        pg.execute("SET client_encoding='UTF8'")
        pg.execute("SET statement_timeout='30s'")
        pg.execute("SET max_parallel_workers_per_gather=2")
        pg.execute("SET work_mem='64MB'")
        result["postgres_version"] = pg.execute("SELECT version()").fetchone()[0]
        start = time.perf_counter()
        for name, (schema, _) in TABLES.items():
            path = directory / f"{name}.csv"
            duck.execute(f"CREATE TEMP TABLE {name} ({schema})")
            # PostgreSQL distinguishes quoted empty strings from unquoted NULL.
            duck.execute(f"COPY {name} FROM '{str(path).replace(chr(39), chr(39)*2)}' (FORMAT CSV, HEADER TRUE, ALLOW_QUOTED_NULLS FALSE)")
            count = duck.execute(f"SELECT count(*) FROM {name}").fetchone()[0]
            if count > 1000000:
                raise ValueError(f"snapshot row cap exceeded: {name}")
            result["snapshot"]["tables"][name]["rows"] = count
        result["duckdb_load_seconds"] = time.perf_counter() - start
        start = time.perf_counter()
        for name, (schema, _) in TABLES.items():
            pg.execute(f"CREATE TEMP TABLE {name} ({schema})")
            with (directory / f"{name}.csv").open("rb") as inp, pg.cursor().copy(sql.SQL("COPY {} FROM STDIN WITH (FORMAT CSV, HEADER TRUE)").format(sql.Identifier(name))) as copy:
                while block := inp.read(1024 * 1024):
                    copy.write(block)
        for statement in INDEXES:
            pg.execute(statement)
        for name in TABLES:
            pg.execute(f"ANALYZE {name}")
            if pg.execute(f"SELECT count(*) FROM {name}").fetchone()[0] != result["snapshot"]["tables"][name]["rows"]:
                raise RuntimeError(f"row count mismatch: {name}")
        result["postgres_load_index_analyze_seconds"] = time.perf_counter() - start
        for name, query in queries(manifest["as_of"]).items():
            pg_fn = lambda: pg.execute(query).fetchall()
            duck_fn = lambda: duck.execute(query).fetchall()
            pg_first, pg_cold = measure(pg_fn, 1)
            duck_first, duck_cold = measure(duck_fn, 1)
            if pg_first != duck_first:
                first_difference = next(((p, d) for p, d in zip(pg_first, duck_first) if p != d), None)
                raise RuntimeError(f"engine result mismatch: {name}; lengths {len(pg_first)}/{len(duck_first)}; first difference {first_difference!r}")
            # Alternate engines each round to reduce execution-order bias.
            p_times, d_times = [], []
            for round_no in range(rounds):
                for label, fn, dest in (("pg", pg_fn, p_times), ("duck", duck_fn, d_times)) if round_no % 2 == 0 else (("duck", duck_fn, d_times), ("pg", pg_fn, p_times)):
                    rows, timing = measure(fn, 1)
                    if rows != pg_first:
                        raise RuntimeError(f"query parity changed: {name}/{label}")
                    dest.extend(timing["samples_ms"])
            stats = lambda xs: {"median_ms": statistics.median(xs), "p95_ms": sorted(xs)[math.ceil(.95*len(xs))-1], "samples_ms": xs}
            result["queries"][name] = {"sql": query.strip(), "rows": len(pg_first), "parity": True, "result_sha256": hashlib.sha256(canonical(pg_first).encode()).hexdigest(), "postgres": stats(p_times), "duckdb": stats(d_times), "postgres_first_ms": pg_cold["median_ms"], "duckdb_first_ms": duck_cold["median_ms"], "sample": pg_first[:3]}
            if name == "fleet_reporting_change":
                result["study_examples"] = [
                    {"status": "retrieval probe; entity links not approved for articulation",
                     "entity": {"sport": row[0], "type": row[1], "id": row[2], "name": row[3]},
                     "study": {"kind": "recorded_reporting_change", "scope": "Distinct articles and publisher labels linked by Graph extraction; not total media coverage or independent corroboration",
                               "current_window": {"from_inclusive": manifest["as_of"] - 7 * 86400, "to_exclusive": manifest["as_of"], "articles": row[4], "publisher_labels": row[6]},
                               "previous_window": {"from_inclusive": manifest["as_of"] - 14 * 86400, "to_exclusive": manifest["as_of"] - 7 * 86400, "articles": row[5], "publisher_labels": row[7]},
                               "article_count_change": row[4] - row[5]},
                     "provenance": {"article_ids": row[10], "snapshot": manifest["mvcc_snapshot"]}}
                    for row in pg_first if (row[0], row[1], row[2]) in {("FOOTBALL", "team", 18), ("FOOTBALL", "team", 15), ("NBA", "team", 14)}]
    duck.close()
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"loads_seconds": {"export": manifest["export_seconds"], "duckdb": result["duckdb_load_seconds"], "postgres": result["postgres_load_index_analyze_seconds"]}, "queries": {k: {"rows": v["rows"], "parity": v["parity"], "postgres_ms": v["postgres"]["median_ms"], "duckdb_ms": v["duckdb"]["median_ms"]} for k, v in result["queries"].items()}}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    exp = sub.add_parser("export")
    exp.add_argument("--source", required=True, help="Postgres DSN through SSH tunnel; always read-only")
    exp.add_argument("--snapshot", required=True, type=Path)
    bench = sub.add_parser("benchmark")
    bench.add_argument("--postgres", required=True, help="Disposable local Postgres DSN; TEMP tables only")
    bench.add_argument("--snapshot", required=True, type=Path)
    bench.add_argument("--output", required=True, type=Path)
    bench.add_argument("--rounds", type=int, default=15)
    args = parser.parse_args()
    if args.command == "export":
        print(json.dumps(export(args.source, args.snapshot), indent=2))
    else:
        if args.rounds < 1:
            parser.error("rounds must be positive")
        benchmark(args.postgres, args.snapshot, args.output, args.rounds)

#!/usr/bin/env python3
"""Aggregate-only audit of a private, bounded Harvester shadow probe manifest.

DATABASE_PRIVATE_URL=... python3 rust/examples/harvester_shadow_probe_check.py
    /tmp/harvester-shadow-probe-run333-20260927.csv

The manifest and publisher text stay on the production host. This prints only
counts and exact-byte verification totals; it performs no database writes.
"""

import csv
import datetime as dt
import json
import os
import re
import subprocess
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    db_url = os.environ.get("DATABASE_PRIVATE_URL") or os.environ.get("DATABASE_URL")
    if not db_url:
        raise SystemExit("DATABASE_PRIVATE_URL or DATABASE_URL is required")
    with open(sys.argv[1], newline="", encoding="utf-8") as handle:
        rows = list(csv.reader(handle))
    if not 1 <= len(rows) <= 20:
        raise SystemExit("expected 1–20 probe articles")
    values = []
    seen = set()
    for row in rows:
        if len(row) != 5:
            raise SystemExit("invalid probe manifest row")
        article_id, sport, old_choice, attempts, enqueued_at = row
        if not article_id.isdecimal() or not attempts.isdecimal():
            raise SystemExit("invalid numeric probe field")
        if not re.fullmatch(r"[A-Z_]+", sport):
            raise SystemExit("invalid sport field")
        if old_choice not in ("relevant", "irrelevant"):
            raise SystemExit("invalid old comparison stratum")
        if article_id in seen:
            raise SystemExit("duplicate probe article")
        seen.add(article_id)
        stamp = dt.datetime.fromisoformat(enqueued_at).isoformat()
        values.append(f"({article_id},'{sport}',{attempts},'{stamp}'::timestamptz)")

    sql = f"""
WITH probe(article_id,sport,attempts_before,enqueued_at) AS (
    VALUES {','.join(values)}
), gates AS (
    SELECT g.* FROM public.harvester_headline_gates g JOIN probe p
      ON p.article_id=g.article_id AND p.sport=g.sport
     WHERE g.contract_version='harvest-headline-v2'
       AND g.created_at>=p.enqueued_at
), contexts AS (
    SELECT c.* FROM public.harvester_classifications c JOIN probe p
      ON p.article_id=c.article_id AND p.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
       AND c.created_at>=p.enqueued_at
), work AS (
    SELECT w.* FROM public.pipeline_work w JOIN probe p
      ON p.article_id=w.entity_id AND p.sport=w.sport
     WHERE w.stage='harvester' AND w.entity_type='article'
       AND w.input_version LIKE 'harvest-context-v5:shadow-probe:%'
)
SELECT json_build_object(
    'probe_articles',(SELECT count(*) FROM probe),
    'active_work',(SELECT count(*) FROM work WHERE status IN ('pending','running')),
    'retryable_work',(SELECT count(*) FROM work WHERE status='failed' AND attempts<5),
    'dead_work',(SELECT count(*) FROM work WHERE status='failed' AND attempts>=5),
    'headline_gate_edges',(SELECT count(*) FROM gates),
    'articles_with_gates',(SELECT count(DISTINCT article_id) FROM gates),
    'admitted_articles',(SELECT count(DISTINCT article_id) FROM gates WHERE admitted),
    'all_negative_articles',(SELECT count(*) FROM probe p
      WHERE EXISTS (SELECT 1 FROM gates g WHERE g.article_id=p.article_id)
        AND NOT EXISTS (SELECT 1 FROM gates g WHERE g.article_id=p.article_id AND g.admitted)),
    'all_negative_without_new_fetch',(SELECT count(*) FROM probe p
      JOIN public.harvester_acquisitions a USING(article_id)
      WHERE a.attempts=p.attempts_before
        AND EXISTS (SELECT 1 FROM gates g WHERE g.article_id=p.article_id)
        AND NOT EXISTS (SELECT 1 FROM gates g WHERE g.article_id=p.article_id AND g.admitted)),
    'new_acquisitions',(SELECT count(*) FROM probe p JOIN public.harvester_acquisitions a USING(article_id)
      WHERE a.attempts>p.attempts_before AND a.updated_at>=p.enqueued_at),
    'new_acquisition_errors',(SELECT count(*) FROM probe p JOIN public.harvester_acquisitions a USING(article_id)
      WHERE a.attempts>p.attempts_before AND a.updated_at>=p.enqueued_at
        AND a.status IN ('retryable_error','blocked','low_content','classification_error')),
    'context_edges',(SELECT count(*) FROM contexts),
    'exact_context_edges',(SELECT count(*) FROM contexts c JOIN public.news_articles a ON a.id=c.article_id
      WHERE a.title=c.headline AND a.full_text IS NOT NULL
        AND encode(sha256(convert_to(a.full_text,'UTF8')),'hex')=c.body_sha256
        AND substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                      FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8')
        AND substring(convert_to(a.full_text,'UTF8') FROM c.model_input_start+1
                      FOR c.model_input_end-c.model_input_start)=convert_to(c.model_input_text,'UTF8')),
    'character_assignments',(SELECT count(*) FROM public.harvester_assignments d
      JOIN contexts c ON c.id=d.classification_id)
)
"""
    result = subprocess.run(
        ["psql", db_url, "-X", "-qAt", "-v", "ON_ERROR_STOP=1"],
        input=sql,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise SystemExit(f"probe check failed: {result.stderr.strip()}")
    print(json.dumps(json.loads(result.stdout), sort_keys=True))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Check Graph retirement on an isolated, fully migrated classifier_test database."""
import os
from pathlib import Path
import subprocess

url = os.environ["TEST_DATABASE_URL"]
def sql(statement):
    result = subprocess.run(["psql", url, "-X", "-v", "ON_ERROR_STOP=1", "-Atc", statement], capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    return result.stdout.strip()

assert sql("SELECT current_database()").startswith("classifier_test"), "disposable classifier_test database required"
migration = Path(__file__).resolve().parents[2] / "sql/migrations/308_retire_graph.sql"
history = ["graph_extractions", "narrative_events", "storyline_articles", "entity_candidates", "candidate_mentions", "persons", "classifier_sources"]
def counts():
    return {table: sql(f"SELECT count(*) FROM public.{table}") for table in history}

try:
    sql("INSERT INTO news_articles(id,url_hash,url,title,source) VALUES"
        "(9991306,md5('graph-retirement-test'),'https://example.invalid/graph-retirement','Graph retirement history fixture','Fixture Wire'); "
        "INSERT INTO graph_extractions(article_id,sport,input_hash,prompt_version,model_version,parser_outcome) "
        "VALUES(9991306,'NBA','fixture','fixture','fixture','extracted'); "
        "INSERT INTO entity_candidates(id,idempotency_key,norm_name,kind_hint,sport) "
        "VALUES(9991304,'graph-retirement-test','fixture person','person','NBA'); "
        "INSERT INTO candidate_mentions(candidate_id,article_id,quote) VALUES(9991304,9991306,'Fixture Person')")
    before = counts()
    sql("ALTER TABLE pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_graph_retired; "
        "INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,status,claim_token,running_input_version) VALUES "
        "('graph','article',9991301,'NBA','pending',NULL,NULL),"
        "('graph','article',9991302,'NBA','running',gen_random_uuid(),'old-graph'),"
        "('graph','article',9991303,'NBA','failed',NULL,NULL),"
        "('investigate_entity','candidate',9991304,'NBA','pending',NULL,NULL)")
    for _ in range(2):
        result = subprocess.run(["psql", url, "-X", "-v", "ON_ERROR_STOP=1", "-f", str(migration)], capture_output=True, text=True)
        assert result.returncode == 0, result.stdout + result.stderr
    assert sql("SELECT count(*) FROM pipeline_work WHERE stage='graph'") == "0"
    assert sql("SELECT count(*) FROM pipeline_stage_progress WHERE stage='graph'") == "0"
    assert sql("SELECT count(*) FROM pipeline_work WHERE stage='investigate_entity' AND entity_id=9991304") == "1"
    assert counts() == before, "retained evidence changed"
    sql("DO $$ BEGIN BEGIN "
        "INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('graph','article',9991305,'NBA'); "
        "RAISE EXCEPTION 'retired Graph enqueue accepted'; "
        "EXCEPTION WHEN check_violation THEN "
        "IF SQLERRM NOT LIKE '%pipeline_work_graph_retired%' THEN RAISE; END IF; "
        "END; END $$")
finally:
    sql("DELETE FROM pipeline_work WHERE entity_id BETWEEN 9991301 AND 9991306; "
        "DELETE FROM candidate_mentions WHERE candidate_id=9991304; "
        "DELETE FROM entity_candidates WHERE id=9991304; "
        "DELETE FROM news_articles WHERE id=9991306")
print("Graph claims fenced, future offers rejected, Investigator preserved, historical row counts unchanged; migration replay passed.")

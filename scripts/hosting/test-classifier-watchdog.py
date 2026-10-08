#!/usr/bin/env python3
"""Exercise real watchdog SQL on an isolated, fully migrated database."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

url = os.environ["TEST_DATABASE_URL"]
def sql(statement):
    return subprocess.check_output(["psql", url, "-X", "-v", "ON_ERROR_STOP=1", "-Atc", statement], text=True).strip()
assert sql("SELECT count(*) FROM classifier_sources") == "0", "empty migrated fixture database required"
assert sql("SELECT current_database()").startswith("classifier_test"), "disposable classifier_test database required"
source = Path(__file__).with_name("cron-watchdog.sh")
with tempfile.TemporaryDirectory(prefix="classifier-watchdog-check-") as directory:
    target = Path(directory) / "scripts/hosting/cron-watchdog.sh"
    target.parent.mkdir(parents=True)
    shutil.copy2(source, target)
    try:
        sql("INSERT INTO news_articles(id,url_hash,url,title,fetched_at) VALUES(9991041,'watchdog-fixture','https://example.invalid/watchdog','Watchdog fixture',now()); "
            "INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('classifier_acquire','article',9991041,'NBA')")
        def run(mode, code, expected):
            result = subprocess.run(["bash", str(target), url], env={**os.environ, "WATCHDOG_MODE": mode}, capture_output=True, text=True)
            assert result.returncode == code and expected in result.stdout, result.stdout + result.stderr
            return result.stdout
        paused = run("paused", 0, "all checks OK")
        assert "drain_alive" not in paused and "ALARM" not in paused
        run("acquire", 1, "ALARM drain_alive")
        sql("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source) VALUES(9991041,'NBA','watchdog','watchdog','{}')")
        run("acquire", 0, "OK drain_alive")
        sql("UPDATE pipeline_work SET status='failed',attempts=5 WHERE entity_id=9991041 AND stage='classifier_acquire'")
        run("acquire", 1, "ALARM dead_letters")
        run("paused", 0, "all checks OK")
    finally:
        sql("DELETE FROM pipeline_work WHERE entity_id=9991041 AND stage='classifier_acquire'; "
            "DELETE FROM news_articles WHERE id=9991041; DELETE FROM pipeline_runs WHERE job='watchdog'")
print("Paused backlog, active progress, stalled acquisition and dead-letter checks pass.")

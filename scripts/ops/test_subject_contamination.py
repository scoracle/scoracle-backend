"""Run only against an isolated TEST_DATABASE_URL with the current schema."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

database = os.environ["TEST_DATABASE_URL"]


def sql(statement):
    return subprocess.check_output(
        ["psql", database, "-X", "-v", "ON_ERROR_STOP=1", "-Atc", statement], text=True
    ).strip()


assert "test" in sql("SELECT current_database()"), "disposable test database required"
repair = Path(__file__).with_name("repair_subject_contamination.sql")
try:
    sql("""
    SET session_replication_role=replica;
    INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_REPAIR_A','Repair A',2026),('ZZ_REPAIR_B','Repair B',2026);
    INSERT INTO source_documents(id,url,retained_excerpt) VALUES(9698002,'https://example.invalid/repair','Fixture');
    INSERT INTO teams(id,sport,name,logo_url,venue_name) VALUES
      (9698001,'ZZ_REPAIR_A','Club A','wrong-logo','Wrong Arena'),
      (9698001,'ZZ_REPAIR_B','Club B','wrong-logo','Wrong Arena');
    INSERT INTO players(id,sport,name,date_of_birth) VALUES
      (9698001,'ZZ_REPAIR_A','Player A','1999-01-01'),(9698001,'ZZ_REPAIR_B','Player B','1999-01-01');
    INSERT INTO entity_facts(entity_type,entity_id,sport,fact_type,value_text,source_document_id) VALUES
      ('team',9698001,'ZZ_REPAIR_A','logo_url','own-logo',9698002),
      ('team',9698001,'ZZ_REPAIR_B','logo_url','wrong-logo',9698002),
      ('team',9698001,'ZZ_REPAIR_B','venue_name','Wrong Arena',9698002),
      ('player',9698001,'ZZ_REPAIR_A','date_of_birth','1990-01-01',9698002),
      ('player',9698001,'ZZ_REPAIR_B','date_of_birth','1999-01-01',9698002);
    INSERT INTO insider_scores(id,sport,entity_type,entity_id,score,read,prompt_version,reporting_date) VALUES
      (9698003,'ZZ_REPAIR_A','player',9698001,88,'Unsafe reading','insider-source-v3','2026-10-07'),
      (9698004,'ZZ_REPAIR_A','player',9698001,1,'Safe reading','insider-source-v10-bounded-reports','2026-10-08');
    INSERT INTO transfer_rumors(id,sport,team_id,player_id,trigger_type,prompt_version,reporting_date) VALUES
      (9698005,'ZZ_REPAIR_A',9698001,9698001,'manual','insider-source-v3','2026-10-07');
    INSERT INTO sigil_synthesis(id,sport,entity_type,entity_id,trigger_type,reading,input_components,reporting_date) VALUES
      (9698006,'ZZ_REPAIR_A','player',9698001,'manual','Contaminated','{"insider":{"body":"Unsafe reading"}}','2026-10-08'),
      (9698007,'ZZ_REPAIR_A','player',9698001,'manual','Clean','{"insider":{"body":"Safe reading"}}','2026-10-08');
    """)
    subprocess.run(["psql", database, "-X", "-f", str(repair)], check=True, capture_output=True)
    assert sql("SELECT count(*) FROM insider_scores WHERE id=9698003") == "1", "preview mutated products"
    assert sql("SELECT logo_url FROM teams WHERE id=9698001 AND sport='ZZ_REPAIR_A'") == "wrong-logo"
    with tempfile.TemporaryDirectory() as directory:
        backup = Path(directory) / "rows.jsonl"
        subprocess.run(
            ["psql", database, "-X", "-v", "apply=true", "-v", f"backup_file={backup}", "-f", str(repair)],
            check=True, capture_output=True,
        )
        rows = [json.loads(line) for line in backup.read_text().splitlines()]
        assert any(r.get("row", {}).get("id") == 9698003 for r in rows), "missing recovery row"
        assert sql("SELECT count(*) FROM insider_scores WHERE id IN(9698003,9698004)") == "1"
        assert sql("SELECT count(*) FROM transfer_rumors WHERE id=9698005") == "0"
        assert sql("SELECT count(*) FROM sigil_synthesis WHERE id IN(9698006,9698007)") == "1"
        assert sql("SELECT logo_url||'|'||coalesce(venue_name,'NULL') FROM teams WHERE id=9698001 AND sport='ZZ_REPAIR_A'") == "own-logo|NULL"
        assert sql("SELECT logo_url||'|'||venue_name FROM teams WHERE id=9698001 AND sport='ZZ_REPAIR_B'") == "wrong-logo|Wrong Arena"
        assert sql("SELECT date_of_birth FROM players WHERE id=9698001 AND sport='ZZ_REPAIR_A'") == "1990-01-01"
        subprocess.run(["psql", database, "-X", "-f", str(repair)], check=True, capture_output=True)
    print("Preview, archive, unsafe cohort withdrawal, exact Sigil dependency and sport-scoped metadata repair passed")
finally:
    sql("""
    DELETE FROM sigil_synthesis WHERE id IN(9698006,9698007);
    DELETE FROM transfer_rumors WHERE id=9698005;
    DELETE FROM insider_scores WHERE id IN(9698003,9698004);
    DELETE FROM entity_facts WHERE source_document_id=9698002;
    DELETE FROM players WHERE id=9698001;
    DELETE FROM teams WHERE id=9698001;
    DELETE FROM source_documents WHERE id=9698002;
    DELETE FROM sports WHERE id IN('ZZ_REPAIR_A','ZZ_REPAIR_B');
    """)

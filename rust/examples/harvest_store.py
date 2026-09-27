"""Persist offline trial source text and proposed character contexts in SQLite.

This is an experiment archive, not a production persistence or queue adapter.
python3 examples/harvest_store.py --corpus articles.jsonl --database trial.sqlite RUN.jsonl ...
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--database", type=Path, required=True)
    parser.add_argument("runs", type=Path, nargs="+")
    args = parser.parse_args()
    articles = {r["article_id"]: r for r in map(json.loads, args.corpus.read_text().splitlines())}
    with sqlite3.connect(args.database) as db:
        db.execute("PRAGMA foreign_keys=ON")
        db.executescript("""
        CREATE TABLE IF NOT EXISTS runs (
          run_id TEXT PRIMARY KEY, artifact TEXT NOT NULL, corpus_sha256 TEXT NOT NULL,
          imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE IF NOT EXISTS harvest_records (
          run_id TEXT NOT NULL REFERENCES runs(run_id), article_id INTEGER NOT NULL,
          disposition TEXT NOT NULL CHECK(disposition IN ('accept','review','reject','error')),
          source_text TEXT, full_source_text TEXT NOT NULL, packet_json TEXT NOT NULL,
          PRIMARY KEY(run_id,article_id));
        CREATE TABLE IF NOT EXISTS character_contexts (
          run_id TEXT NOT NULL, article_id INTEGER NOT NULL, character TEXT NOT NULL,
          headline TEXT NOT NULL, source TEXT NOT NULL, url TEXT NOT NULL, text TEXT NOT NULL,
          PRIMARY KEY(run_id,article_id,character),
          FOREIGN KEY(run_id,article_id) REFERENCES harvest_records(run_id,article_id));
        """)
        for path in args.runs:
            run_id = hashlib.sha256(path.read_bytes()).hexdigest()
            if db.execute("SELECT 1 FROM runs WHERE run_id=?", (run_id,)).fetchone():
                print(f"already imported {path.name}")
                continue
            with db:
                db.execute("INSERT INTO runs(run_id,artifact,corpus_sha256) VALUES(?,?,?)",
                           (run_id, path.name, hashlib.sha256(args.corpus.read_bytes()).hexdigest()))
                for row in map(json.loads, path.read_text().splitlines()):
                    article = articles[row["article_id"]]
                    excerpt = row.get("excerpt")
                    if excerpt:
                        original = article["body"].encode()[excerpt["start"]:excerpt["end"]].decode()
                        if original != excerpt["text"]:
                            raise ValueError(f"non-verbatim excerpt: {row['article_id']}")
                    db.execute("INSERT INTO harvest_records VALUES(?,?,?,?,?,?)",
                               (run_id, row["article_id"], row["disposition"],
                                excerpt["text"] if excerpt else None, article["body"], json.dumps(row, ensure_ascii=False)))
                    if row["disposition"] == "accept":
                        # v2 uses stable plugin IDs; retain import compatibility with
                        # historical v1 experiment packets.
                        for character in row.get("character_tags", row.get("proposed_characters", [])):
                            db.execute("INSERT INTO character_contexts VALUES(?,?,?,?,?,?,?)",
                                       (run_id, row["article_id"], character, article["title"],
                                        article["source"], article["url"], excerpt["text"]))
            counts = dict(db.execute("SELECT disposition,count(*) FROM harvest_records WHERE run_id=? GROUP BY disposition", (run_id,)))
            contexts = db.execute("SELECT count(*) FROM character_contexts WHERE run_id=?", (run_id,)).fetchone()[0]
            print(json.dumps({"artifact": path.name, "run_id": run_id, "counts": counts, "proposed_contexts": contexts}))


if __name__ == "__main__":
    main()

"""Check the study's meaning, independently of cross-engine equivalence."""
import unittest

import duckdb

from journalist_memory_probe import TABLES, queries


class ReportingChangeTest(unittest.TestCase):
    def test_windows_distinct_reporting_and_origin(self):
        now, week = 2_000_000, 7 * 86400
        with duckdb.connect() as db:
            for table in ("events", "names"):
                db.execute(f"CREATE TABLE {table} ({TABLES[table][0]})")
            db.execute("INSERT INTO names VALUES ('NBA','team',1,'Example')")
            # Same article can have several extracted predicates; self-edges and
            # repeated publishers must not inflate article/publisher counts.
            rows = [
                (1, 10, now - week, "A", "extraction"),
                (2, 10, now - week, "A", "extraction"),
                (3, 11, now - 1, "A", "extraction"),
                (4, 12, now - 2 * week, "B", "extraction"),
                (5, 13, now - week - 1, "C", "extraction"),
                (6, 14, now - 2 * week - 1, "D", "extraction"),
                (7, 15, now, "E", "extraction"),
                (8, 16, now - 1, "F", "junction"),
            ]
            for event_id, article_id, date, publisher, origin in rows:
                db.execute("INSERT INTO events VALUES (?, 'NBA', 'team', 1, 'praise', 'team', 1, 'reported', ?, ?, ?, ?, 'fixture')",
                           [event_id, article_id, date, publisher, origin])
            result = db.execute(queries(now)["fleet_reporting_change"]).fetchall()
            self.assertEqual(result, [("NBA", "team", 1, "Example", 2, 2, 1, 2,
                                       now - 2 * week, now - 1, [10, 11, 12, 13])])


if __name__ == "__main__":
    unittest.main()

"""Leakage, label binding, and recall accounting tests; no model download needed."""
import copy
import unittest
from harvest_training_data import build, digest
from harvest_laya_train import check_splits, metrics, select_thresholds


class TrainingContractTests(unittest.TestCase):
    def source(self, aid=1):
        request = {"state":f"source {aid}", "questions":{
            "entity":{"criteria":{"subject":"involved", "absent":"absent"}},
            "content":{"criteria":{"reporting":"news", "listing":"table"}}}}
        return {"article_id":aid, "request":request}

    def label(self, source, group="same-story"):
        return {"article_id":source["article_id"], "story_group":group,
                "request_sha256":digest(source["request"]), "labels":{"entity":"subject"},
                "label_source":"test", "human_reviewed":False}

    def test_related_stories_stay_together_and_unknown_fields_stay_unknown(self):
        a, b = self.source(), self.source(2)
        rows = build([a,b], [self.label(a),self.label(b)])
        self.assertEqual(rows[0]["split"], rows[1]["split"])
        self.assertNotIn("content", rows[0]["labels"])

    def test_modified_input_and_invalid_labels_are_rejected(self):
        a = self.source()
        annotation = self.label(a)
        changed = copy.deepcopy(a)
        changed["request"]["state"] = "different story"
        with self.assertRaises(ValueError):
            build([changed],[annotation])
        annotation["labels"]["entity"] = "invented-class"
        with self.assertRaises(ValueError):
            build([a],[annotation])

    def test_story_leakage_and_missing_splits_are_rejected(self):
        with self.assertRaises(ValueError):
            check_splits([{"article_id":1,"story_group":"x","split":"train"},
                          {"article_id":2,"story_group":"x","split":"test"}])
        with self.assertRaises(ValueError):
            check_splits([{"article_id":1,"story_group":"x","split":"train"}])

    def cases(self):
        return [{"labels":{"entity":label,"content":"reporting"},
                 "scores":{"entity":{"subject":score,"opponent":0.0},
                           "content":{"reporting":0.9}}}
                for label,score in [("subject",0.9),("subject",0.6),("subject",0.4),
                                    ("subject",0.2),("absent",0.3),("absent",0.1)]]

    def test_recall_is_retained_news_not_accuracy_or_accept_rate(self):
        m = metrics(self.cases(),0.4,0.5)
        self.assertEqual(m["recall"],0.75)
        self.assertEqual(m["precision"],1.0)
        self.assertEqual(m["forward_rate"],0.5)
        all_pass = metrics(self.cases(),0,0)
        self.assertEqual(all_pass["recall"],1.0)
        self.assertAlmostEqual(all_pass["precision"],4/6)

    def test_threshold_selection_meets_calibration_target_and_unknowns_are_excluded(self):
        rows = self.cases()
        selected = select_thresholds(rows,0.75)
        self.assertGreaterEqual(metrics(rows,selected["entity"],selected["reporting"])["recall"],0.75)
        rows.append({"labels":{"entity":"subject"},"scores":{}})
        self.assertEqual(metrics(rows,0.4,0.5)["articles"],6)
        with self.assertRaises(ValueError):
            select_thresholds([],0.75)

    def test_calibrated_narrow_score_ranges_are_not_rounded_to_pass_everything(self):
        rows = self.cases()
        for r in rows:
            r["scores"]["entity"]["subject"] = 0.245 + 0.01*r["scores"]["entity"]["subject"]
        policy = select_thresholds(rows,0.75)
        m = metrics(rows,policy["entity"],policy["reporting"])
        self.assertEqual(m["recall"],0.75)
        self.assertEqual(m["precision"],1.0)

    def test_story_coverage_does_not_count_duplicate_reports_twice(self):
        rows = self.cases()
        rows[0]["story_group"] = rows[1]["story_group"] = "one-event"
        m = metrics(rows,0.4,0.5)
        self.assertEqual(m["positives"],4)
        self.assertEqual(m["positive_story_groups"],3)
        self.assertEqual(m["retained_story_groups"],2)
        self.assertAlmostEqual(m["story_recall"],2/3)


if __name__ == "__main__":
    unittest.main()

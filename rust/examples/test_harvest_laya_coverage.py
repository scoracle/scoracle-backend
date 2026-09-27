"""SDK boundary checks; run in the Laya environment with HARVESTER_LAYA_MODEL_DIR.

Uses the real installed SDK and cached tokenizer, without loading model weights,
making network requests or running inference.
"""
import os
import unittest
from pathlib import Path

from harvest_laya_server import complete_question_tokens


@unittest.skipUnless(os.environ.get("HARVESTER_LAYA_MODEL_DIR"), "requires cached Laya tokenizer and SDK")
class QuestionCoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from transformers import AutoTokenizer
        cls.tokenizer = AutoTokenizer.from_pretrained(
            str(Path(os.environ["HARVESTER_LAYA_MODEL_DIR"]) / "tokenizer"),
            local_files_only=True,
        )

    def frames(self, question):
        from laya.agent import Agent
        from laya.common import build_sequence, render_options
        Agent._check_question("test", question)
        q = Agent._to_internal(question)
        actual, _ = build_sequence(self.tokenizer, "", q, max_len=512,
                                   head_max_len=192, state_ids=[])
        expected = complete_question_tokens(self.tokenizer, q, render_options(q))
        return actual, expected

    def test_short_questions_match_actual_sdk_for_every_native_type(self):
        for kind, criteria in [
            ("choice", {"irrelevant": "No source evidence", "relevant": "Source evidence"}),
            ("noul", {"false": "Not reported", "true": "Reported"}),
            ("score", ["Low", "Medium", "High"]),
        ]:
            with self.subTest(kind=kind):
                actual, expected = self.frames({"type": kind,
                    "instructions": "The source reports a result for Équipe.", "criteria": criteria})
                self.assertEqual(actual, expected)

    def test_long_instructions_cannot_claim_complete_question_coverage(self):
        actual, expected = self.frames({"type": "choice",
            "instructions": "qualifying evidence " * 200,
            "criteria": {"no": "Absent", "yes": "Present"}})
        self.assertNotEqual(actual, expected)

    def test_single_long_criterion_cannot_claim_complete_question_coverage(self):
        actual, expected = self.frames({"type": "choice", "instructions": "Evidence?",
            "criteria": {"no": "Absent", "yes": "source qualification " * 80}})
        self.assertNotEqual(actual, expected)

    def test_combined_option_budget_cannot_silently_shorten_criteria(self):
        actual, expected = self.frames({"type": "choice", "instructions": "Evidence?",
            "criteria": {str(i): "source qualification " * 10 for i in range(12)}})
        self.assertNotEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()

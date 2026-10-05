"""Protocol check only: fake replies never stand in for live model/DB evidence."""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import influencer_tool_compare as compare


class AcquisitionComparisonTest(unittest.TestCase):
    def test_invalid_arguments_never_receive_evidence(self):
        evidence = {"status": "available", "source": {"publisher_excerpt": "Test evidence."}}
        messages = [{"role": "system", "content": "Read the assigned source with read_source, then describe it."}, {"role": "user", "content": "Test subject"}]
        first = {"messages": messages, "tools": [{}], "options": {}}
        final = {"messages": messages + [{"role": "tool", "content": json.dumps(evidence)}], "tools": [], "options": {}, "format": {"type": "object"}}
        capture = {"first": {"request_body": json.dumps(first)}, "second": {"request_body": json.dumps(final)}, "tool_result": evidence}
        for args in ({}, {"entity_id": 999}):
            with self.subTest(args=args), tempfile.TemporaryDirectory() as directory:
                source, output = Path(directory) / "input.json", Path(directory) / "output.jsonl"
                source.write_text(json.dumps(capture))
                def chat(_, request):
                    message = {"role": "assistant", "content": '{"body":"Test."}'}
                    if request.get("tools"):
                        message["tool_calls"] = [{"function": {"name": "read_source", "arguments": args}}]
                    return {"response": {"done": True, "done_reason": "stop", "message": message}}
                argv = ["compare", "--output", str(output), "--temperature", "1", "--repeats", "1", str(source)]
                with patch("sys.argv", argv), patch.object(compare, "chat", side_effect=chat) as model, contextlib.redirect_stdout(io.StringIO()):
                    compare.main()
                native, prepared = [json.loads(line) for line in output.read_text().splitlines()]
                self.assertEqual(native["error"], "invalid_first_call" if args else None)
                self.assertEqual(native["evidence_deliveries"], 0 if args else 1)
                self.assertEqual(model.call_count, 2 if args else 3)
                self.assertEqual(native["evidence_sha256"], prepared["evidence_sha256"])
                delivered = model.call_args_list[-1].args[1]["messages"][1]["content"].split("\nSource result:\n")[1]
                self.assertEqual(json.loads(delivered), evidence)


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from run_deepswe_codex_batch import parse_summary_from_log


class RunDeepSweCodexBatchTests(unittest.TestCase):
    def test_parse_summary_prefers_trailing_json_object(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            log_path = Path(tmp) / "stdout.log"
            trailing_summary = {
                "schema": "anchor.native_deepswe_codex_batch.v1",
                "task_id": "demo-task",
                "aggregate": {"completed": 1},
            }
            log_path.write_text(
                '\n'.join(
                    [
                        '[anchor-benchmark] starting',
                        '{"run": 1, "exit_code": 0}',
                        json.dumps(trailing_summary, indent=2, sort_keys=True),
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            summary = parse_summary_from_log(log_path)

            self.assertEqual(summary, trailing_summary)

    def test_parse_summary_returns_none_without_valid_trailing_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            log_path = Path(tmp) / "stdout.log"
            log_path.write_text(
                '[anchor-benchmark] starting\n{"run": 1, "exit_code": 0}\nnot json\n',
                encoding="utf-8",
            )

            summary = parse_summary_from_log(log_path)

            self.assertIsNone(summary)


if __name__ == "__main__":
    unittest.main()

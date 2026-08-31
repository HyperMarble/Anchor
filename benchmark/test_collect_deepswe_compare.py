from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from collect_deepswe_compare import first_trial_result, trace_summary


class CollectDeepSWECompareTests(unittest.TestCase):
    def test_first_trial_result_reads_nested_result_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            job_dir = Path(tmp)
            nested = job_dir / "runs" / "trial-001"
            nested.mkdir(parents=True)
            (nested / "result.json").write_text(
                json.dumps(
                    {
                        "task_name": "python-statemachine-state-data-scoping",
                        "trial_name": "anchor",
                    }
                ),
                encoding="utf-8",
            )

            result = first_trial_result(job_dir)

            self.assertIsNotNone(result)
            self.assertEqual(result["task_name"], "python-statemachine-state-data-scoping")
            self.assertEqual(result["trial_name"], "anchor")

    def test_trace_summary_counts_anchor_usage_flags(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            trace = Path(tmp) / "trace.jsonl"
            trace.write_text(
                "\n".join(
                    [
                        json.dumps(
                            {
                                "classification": {
                                    "uses_anchor": True,
                                    "runs_tests": True,
                                    "raw_read": False,
                                    "raw_write": False,
                                }
                            }
                        ),
                        json.dumps(
                            {
                                "classification": {
                                    "uses_anchor": False,
                                    "runs_tests": False,
                                    "raw_read": True,
                                    "raw_write": True,
                                    "source_like": True,
                                }
                            }
                        ),
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            summary = trace_summary(trace)

            self.assertEqual(
                summary,
                {
                    "events": 2,
                    "uses_anchor": 1,
                    "runs_tests": 1,
                    "raw_reads": 1,
                    "raw_writes": 1,
                    "source_like_raw_writes": 1,
                },
            )


if __name__ == "__main__":
    unittest.main()

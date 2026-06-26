from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from execution_spec import execution_spec_requirement, parse_execution_spec_metrics


VALID_SPEC = """ExecutionSpec:
  goal: >
    Add deprecation response headers.
  agent_understanding: >
    The task is about runtime route behavior, not only OpenAPI schema.
  expected_behavior:
    - Deprecated routes emit response headers.
  required_edges:
    - Nested router override precedence must be preserved.
    - Omitted values must differ from explicit false and none values.
  non_goals:
    - Do not rewrite unrelated routing behavior.
  search_terms:
    - APIRoute
  likely_files:
    - fastapi/routing.py
  verification_requirements:
    - Runtime TestClient checks are required.
    - Compile-only, lint-only, import-only, and OpenAPI-only checks are not sufficient because runtime behavior and edge-case semantics must be verified.
  quality_constraints:
    - Keep the implementation minimal and local to the behavior owner.
"""


def write_jsonl(events: list[dict]) -> Path:
    tmp = tempfile.NamedTemporaryFile("w", delete=False)
    with tmp:
        for event in events:
            tmp.write(json.dumps(event) + "\n")
    return Path(tmp.name)


class ExecutionSpecTests(unittest.TestCase):
    def test_requirement_uses_explicit_execution_spec_label(self) -> None:
        requirement = execution_spec_requirement()

        self.assertIn("ExecutionSpec:", requirement)
        self.assertIn("goal:", requirement)
        self.assertIn("verification_requirements:", requirement)

    def test_accepts_spec_before_first_action(self) -> None:
        path = write_jsonl(
            [
                {
                    "type": "item.completed",
                    "item": {"type": "agent_message", "text": VALID_SPEC},
                },
                {
                    "type": "item.started",
                    "item": {"type": "command_execution", "command": "rg APIRoute"},
                },
            ]
        )

        metrics = parse_execution_spec_metrics(path)

        self.assertTrue(metrics["accepted"])
        self.assertTrue(metrics["spec_before_action"])
        self.assertTrue(metrics["has_nested_override_edge"])
        self.assertTrue(metrics["has_omitted_explicit_edge"])

    def test_rejects_action_before_spec(self) -> None:
        path = write_jsonl(
            [
                {
                    "type": "item.started",
                    "item": {"type": "command_execution", "command": "rg APIRoute"},
                },
                {
                    "type": "item.completed",
                    "item": {"type": "agent_message", "text": VALID_SPEC},
                },
            ]
        )

        metrics = parse_execution_spec_metrics(path)

        self.assertFalse(metrics["accepted"])
        self.assertFalse(metrics["spec_before_action"])

    def test_rejects_missing_insufficient_checks_rule(self) -> None:
        path = write_jsonl(
            [
                {
                    "type": "item.completed",
                    "item": {
                        "type": "agent_message",
                        "text": VALID_SPEC.replace(
                            "Compile-only, lint-only, import-only, and OpenAPI-only checks are not sufficient because runtime behavior and edge-case semantics must be verified.",
                            "Run tests.",
                        ),
                    },
                }
            ]
        )

        metrics = parse_execution_spec_metrics(path)

        self.assertFalse(metrics["accepted"])
        self.assertFalse(metrics["has_exact_insufficient_checks"])

    def test_missing_log_is_not_accepted(self) -> None:
        metrics = parse_execution_spec_metrics(Path("/tmp/anchor-missing-spec-log.jsonl"))

        self.assertFalse(metrics["accepted"])
        self.assertFalse(metrics["has_required_sections"])


if __name__ == "__main__":
    unittest.main()

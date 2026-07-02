from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def execution_spec_requirement() -> str:
    return """ExecutionSpec gate:
Before any code search, code read, source edit, or verification command, write a compact ExecutionSpec as your first response. Do not inspect files before this spec.

Use this exact YAML shape:

ExecutionSpec:
  goal: >
    One sentence describing the behavior or outcome to implement.

  agent_understanding: >
    One short paragraph explaining what you believe the user/task is asking for. Preserve explicit requirements instead of compressing them into broad file areas.

  expected_behavior:
    - Concrete behavior that must work.

  required_edges:
    - Edge case, precedence rule, compatibility rule, or invariant that must not break.
    - Include subtle nested/override/chaining behavior when the task mentions propagation or inheritance.
    - Distinguish omitted values from explicit false/none/default values when relevant.

  non_goals:
    - Things you should avoid changing.

  search_terms:
    - Symbol, function, class, API, error string, config key, or behavior term likely to locate the owner code.

  likely_files:
    - Optional path if already known from the prompt or project context.

  verification_requirements:
    - Specific behavior checks required before handoff.
    - Compile-only, lint-only, import-only, and OpenAPI-only checks are not sufficient because runtime behavior and edge-case semantics must be verified.
    - Include nested/override/chaining tests when required_edges mention propagation or inheritance.

  quality_constraints:
    - Keep the implementation minimal and local to the behavior owner.
    - Avoid broad mechanical rewrites unless every affected call path is reviewed.

Rules:
- Be specific.
- If the task has explicit requirements, preserve them instead of compressing them.
- If the prompt is vague, state assumptions clearly.
- If a required behavior is uncertain, put it under required_edges or verification_requirements.
- The verification_requirements section must include the exact insufficient-checks bullet from the template unless the task is purely documentation-only.
- After writing the ExecutionSpec, continue with the Anchor workflow below.
"""


def parse_execution_spec_metrics(path: Path) -> dict[str, Any]:
    required_sections = [
        "goal:",
        "agent_understanding:",
        "expected_behavior:",
        "required_edges:",
        "non_goals:",
        "search_terms:",
        "likely_files:",
        "verification_requirements:",
        "quality_constraints:",
    ]
    first_message_line: int | None = None
    first_action_line: int | None = None
    first_message = ""

    if not path.exists():
        return {
            "first_message_line": None,
            "first_action_line": None,
            "spec_before_action": False,
            "missing_sections": required_sections,
            "has_required_sections": False,
            "has_exact_insufficient_checks": False,
            "has_nested_override_edge": False,
            "has_omitted_explicit_edge": False,
            "accepted": False,
            "preview": "",
        }

    for line_no, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        item = event.get("item") or {}
        item_type = item.get("type")
        event_type = event.get("type")

        if first_action_line is None and item_type in {"command_execution", "file_change"}:
            first_action_line = line_no

        if (
            first_message_line is None
            and event_type == "item.completed"
            and item_type == "agent_message"
        ):
            first_message_line = line_no
            first_message = str(item.get("text", ""))

    lowered = first_message.lower()
    missing_sections = [section for section in required_sections if section not in lowered]
    exact_insufficient = (
        "compile-only, lint-only, import-only, and openapi-only checks are not sufficient"
        in lowered
    )
    spec_before_action = bool(
        first_message_line is not None
        and (first_action_line is None or first_message_line < first_action_line)
    )
    return {
        "first_message_line": first_message_line,
        "first_action_line": first_action_line,
        "spec_before_action": spec_before_action,
        "missing_sections": missing_sections,
        "has_required_sections": not missing_sections,
        "has_exact_insufficient_checks": exact_insufficient,
        "has_nested_override_edge": "nested" in lowered
        and ("override" in lowered or "precedence" in lowered),
        "has_omitted_explicit_edge": "omitted" in lowered
        and ("false" in lowered or "none" in lowered),
        "accepted": bool(spec_before_action and not missing_sections and exact_insufficient),
        "preview": " ".join(first_message.split())[:500],
    }

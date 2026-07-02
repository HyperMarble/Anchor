#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import re
import sys
import time
from pathlib import Path


TRACE_DIR = Path(os.environ.get("ANCHOR_TRACE_DIR", "/anchor-traces"))
MODE = os.environ.get("ANCHOR_HOOK_MODE", "warn").lower()


def read_payload() -> dict:
    try:
        return json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        return {"_parse_error": True}


def append_trace(payload: dict, classification: dict) -> None:
    TRACE_DIR.mkdir(parents=True, exist_ok=True)
    event = {
        "ts": time.time(),
        "hook_event": payload.get("hook_event_name") or payload.get("hookEventName"),
        "tool_name": payload.get("tool_name"),
        "tool_input": payload.get("tool_input"),
        "classification": classification,
    }
    with (TRACE_DIR / "claude-anchor-tools.jsonl").open("a", encoding="utf-8") as f:
        f.write(json.dumps(event, sort_keys=True) + "\n")


def classify(payload: dict) -> dict:
    tool = payload.get("tool_name") or ""
    tool_input = payload.get("tool_input") or {}
    command = tool_input.get("command", "") if isinstance(tool_input, dict) else ""
    path = ""
    if isinstance(tool_input, dict):
        path = tool_input.get("file_path") or tool_input.get("path") or ""

    uses_anchor = tool == "Bash" and re.search(r"(^|[;&|()\s])anchor(\s|$)", command)
    runs_tests = tool == "Bash" and re.search(
        r"\b(pytest|cargo test|npm test|pnpm test|yarn test|go test|mvn test|gradle test)\b",
        command,
    )
    raw_read = tool in {"Read", "Grep", "Glob"}
    raw_write = tool in {"Edit", "Write", "MultiEdit"}

    source_like = bool(re.search(r"\.(py|ts|tsx|js|jsx|rs|go|java|c|cc|cpp|h|hpp)$", path))

    return {
        "uses_anchor": uses_anchor,
        "runs_tests": runs_tests,
        "raw_read": raw_read,
        "raw_write": raw_write,
        "source_like": source_like,
        "mode": MODE,
    }


def pre_tool_response(classification: dict) -> dict | None:
    if MODE not in {"strict", "block"}:
        if classification["raw_read"]:
            return {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "additionalContext": "Anchor benchmark note: prefer `anchor context` before broad raw source reads when practical.",
                }
            }
        if classification["raw_write"] and classification["source_like"]:
            return {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "additionalContext": "Anchor benchmark note: prefer `anchor edit --symbol` for scoped source edits when Anchor can express the change.",
                }
            }
        return None

    if classification["raw_write"] and classification["source_like"]:
        return {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "Anchor strict benchmark mode: source writes must go through `anchor edit` unless Anchor cannot express the edit.",
            }
        }

    return None


def main() -> int:
    payload = read_payload()
    classification = classify(payload)
    append_trace(payload, classification)

    event = payload.get("hook_event_name") or payload.get("hookEventName")
    if event == "PreToolUse":
        response = pre_tool_response(classification)
        if response is not None:
            print(json.dumps(response))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

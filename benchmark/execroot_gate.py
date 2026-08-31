from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any


SOURCE_SUFFIXES = {
    ".c",
    ".cc",
    ".cpp",
    ".cs",
    ".go",
    ".h",
    ".hpp",
    ".java",
    ".js",
    ".jsx",
    ".kt",
    ".mjs",
    ".py",
    ".rs",
    ".scala",
    ".swift",
    ".ts",
    ".tsx",
}


def execroot_acceptance_gate(
    execroot: Path,
    changed_paths: list[str],
    agent_log_path: Path | None,
    anchor_bin: Path | None,
) -> dict[str, Any]:
    changed_source = [path for path in changed_paths if _is_source_path(path)]
    events = _load_anchor_events(execroot)
    commands = _load_anchor_commands(agent_log_path, anchor_bin)
    contract_paths = _semantic_contract_paths(execroot)
    read_paths = _anchor_event_read_paths(events)
    read_paths.update(_command_read_paths(commands, execroot))
    outside_contract = sorted(path for path in changed_source if path not in contract_paths)
    unread_changes = sorted(
        path for path in changed_source if path in contract_paths and path not in read_paths
    )
    accepted = not changed_source or (
        bool(contract_paths) and not outside_contract and not unread_changes
    )
    reasons = []
    if changed_source and not contract_paths:
        reasons.append("missing_semantic_contract")
    if outside_contract:
        reasons.append("changed_file_outside_semantic_contract")
    if unread_changes:
        reasons.append("changed_file_without_semantic_read")
    return {
        "accepted": accepted,
        "changed_source_paths": changed_source,
        "semantic_contract_paths": sorted(contract_paths),
        "semantic_read_paths": sorted(read_paths),
        "uncovered_changed_source_paths": outside_contract,
        "unread_changed_source_paths": unread_changes,
        "query_count": _semantic_command_count(commands) + _contract_event_count(events),
        "read_count": len(read_paths),
        "reasons": reasons,
    }


def _semantic_contract_paths(execroot: Path) -> set[str]:
    root = execroot / ".anchor" / "semantic" / "current" / "by-task"
    if not root.exists():
        return set()
    paths: set[str] = set()
    for subdir in ("owners", "files", "tests"):
        for doc in (root / subdir).glob("*.md"):
            path = _markdown_declared_path(doc)
            if path:
                paths.add(path)
    return paths


def _anchor_event_read_paths(events: list[dict[str, Any]]) -> set[str]:
    statuses = {"ok", "cached", "refreshed", "resolved_around"}
    paths: set[str] = set()
    for event in events:
        if event.get("event_type") not in {"context.read", "view.read"}:
            continue
        if event.get("status") not in statuses:
            continue
        path = _normal_source_path(event.get("path"))
        if path:
            paths.add(path)
    return paths


def _command_read_paths(commands: list[str], execroot: Path) -> set[str]:
    paths: set[str] = set()
    for command in commands:
        paths.update(_handle_paths(command))
        for doc in _semantic_docs_referenced_by_command(command, execroot):
            path = _markdown_declared_path(doc)
            if path:
                paths.add(path)
    return paths


def _handle_paths(command: str) -> set[str]:
    paths: set[str] = set()
    for match in re.finditer(r"\b(?:chunk|file|test):([^\s#`'\";]+)", command):
        path = _normal_source_path(match.group(1))
        if path:
            paths.add(path)
    return paths


def _semantic_docs_referenced_by_command(command: str, execroot: Path) -> list[Path]:
    docs: list[Path] = []
    normalized_command = command.replace("\\", "/")
    patterns = re.findall(
        r"(?:/[^`'\"\s;]*)?\.anchor/semantic/current/[^`'\"\s;]+",
        normalized_command,
    )
    for pattern in patterns:
        cleaned = pattern.rstrip(",.:)")
        path_pattern = _semantic_pattern_to_path(cleaned, execroot)
        if not path_pattern:
            continue
        if any(ch in str(path_pattern) for ch in "*?["):
            docs.extend(sorted(path_pattern.parent.glob(path_pattern.name)))
        elif path_pattern.is_file() and path_pattern.name != "index.md":
            docs.append(path_pattern)
    return [path for path in docs if path.is_file()]


def _semantic_pattern_to_path(pattern: str, execroot: Path) -> Path | None:
    marker = ".anchor/semantic/current/"
    if marker not in pattern:
        return None
    relative = pattern[pattern.index(marker) :]
    return execroot / relative


def _markdown_declared_path(doc: Path) -> str | None:
    try:
        text = doc.read_text(errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        if not line.startswith("path:"):
            continue
        match = re.search(r"`([^`]+)`", line)
        if match:
            return _normal_source_path(match.group(1))
        return _normal_source_path(line.split(":", 1)[1].strip())
    return None


def _normal_source_path(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    path = value.strip().strip("`'\"")
    if not path:
        return None
    path = path.replace("\\", "/")
    if path.startswith("./"):
        path = path[2:]
    return path


def _load_anchor_events(execroot: Path) -> list[dict[str, Any]]:
    path = execroot / ".anchor" / "events" / "events.jsonl"
    if not path.exists():
        return []
    events: list[dict[str, Any]] = []
    for line in path.read_text().splitlines():
        try:
            events.append(json.loads(line))
        except Exception:
            continue
    return events


def _load_anchor_commands(agent_log_path: Path | None, anchor_bin: Path | None) -> list[str]:
    if not agent_log_path or not agent_log_path.exists():
        return []
    anchor_text = str(anchor_bin) if anchor_bin else "anchor"
    commands = []
    for line in agent_log_path.read_text(errors="replace").splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        item = event.get("item") or {}
        command = str(item.get("command", ""))
        if _is_completed_relevant_command(event, item, command, anchor_text):
            commands.append(command)
    return commands


def _is_completed_relevant_command(
    event: dict[str, Any],
    item: dict[str, Any],
    command: str,
    anchor_text: str,
) -> bool:
    normalized_command = command.replace("\\", "/")
    normalized_anchor = anchor_text.replace("\\", "/")
    return (
        event.get("type") == "item.completed"
        and item.get("type") == "command_execution"
        and item.get("exit_code") in (0, None)
        and (
            normalized_anchor in normalized_command
            or " anchor " in normalized_command
            or ".anchor/semantic/current" in normalized_command
        )
    )


def _is_source_path(path: str) -> bool:
    return Path(path).suffix in SOURCE_SUFFIXES


def _semantic_command_count(commands: list[str]) -> int:
    return sum(1 for command in commands if " semantic" in command)


def _contract_event_count(events: list[dict[str, Any]]) -> int:
    return sum(
        1
        for event in events
        if event.get("event_type") in {"semantic.contract", "semantic.materialize"}
    )

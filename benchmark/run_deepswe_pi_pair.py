#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any

from run_deepswe_codex_pair import (
    ANCHOR_ROOT,
    DEFAULT_DEEPSWE_ROOT,
    anchor_artifacts,
    anchor_prompt,
    baseline_prompt,
    classify_agent_log,
    clean_copy_from_image,
    ensure_docker_ready,
    file_size,
    git_metrics,
    load_task,
    parse_command_metrics,
    verify_with_task_image,
)


DEFAULT_WORK_ROOT = Path("/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe-pi")


def ensure_pi_available(pi_bin: str) -> None:
    resolved = shutil.which(pi_bin)
    if resolved:
        return
    raise SystemExit(
        f"Pi CLI is not available on PATH as '{pi_bin}'.\n\n"
        "Install Pi, then rerun the benchmark:\n"
        "  npm install -g --ignore-scripts @earendil-works/pi-coding-agent\n"
        "or:\n"
        "  curl -fsSL https://pi.dev/install.sh | sh\n"
    )


def run_pi(
    mode: str,
    repo_dir: Path,
    prompt: str,
    out_path: Path,
    timeout_sec: int,
    pi_bin: str,
    provider: str | None,
    model: str | None,
) -> dict[str, Any]:
    env = dict(os.environ)
    env["PATH"] = f"{ANCHOR_ROOT / 'target' / 'debug'}:{ANCHOR_ROOT / 'target' / 'release'}:{env.get('PATH', '')}"
    env.setdefault("PI_SKIP_VERSION_CHECK", "1")

    cmd = [
        pi_bin,
        "--mode",
        "json",
        "--no-session",
        "--no-context-files",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
    ]
    if provider:
        cmd.extend(["--provider", provider])
    if model:
        cmd.extend(["--model", model])
    cmd.extend(["-p", prompt])

    started = time.time()
    with out_path.open("w") as out:
        proc = subprocess.run(
            cmd,
            cwd=repo_dir,
            env=env,
            text=True,
            stdout=out,
            stderr=subprocess.STDOUT,
            timeout=timeout_sec,
        )
    return {
        "mode": mode,
        "exit_code": proc.returncode,
        "duration_sec": round(time.time() - started, 3),
        "provider": provider,
        "model": model,
    }


def parse_pi_tool_counts(path: Path) -> dict[str, int]:
    counts: dict[str, int] = {}
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") != "tool_execution_start":
            continue
        name = event.get("toolName")
        if isinstance(name, str):
            counts[name] = counts.get(name, 0) + 1
    return counts


def find_command_text(value: Any) -> str | None:
    if isinstance(value, dict):
        for key in ("command", "cmd", "script"):
            item = value.get(key)
            if isinstance(item, str) and item.strip():
                return item
        for item in value.values():
            found = find_command_text(item)
            if found:
                return found
    elif isinstance(value, list):
        for item in value:
            found = find_command_text(item)
            if found:
                return found
    return None


def classify_anchor_command(command: str, metrics: dict[str, int], anchor_bin: Path | None) -> None:
    anchor_text = str(anchor_bin) if anchor_bin else "anchor"
    if anchor_text in command or " anchor " in command:
        metrics["anchor_commands"] += 1
        for name, key in [
            (" build", "anchor_builds"),
            (" task", "anchor_tasks"),
            (" context", "anchor_contexts"),
            (" edit", "anchor_edits"),
            (" write", "anchor_writes"),
            (" check", "anchor_checks"),
            (" status", "anchor_statuses"),
            (" receipt", "anchor_receipts"),
            (" gate", "anchor_gates"),
        ]:
            if name in command:
                metrics[key] += 1
    elif any(token in command for token in ("sed -n", "cat ", "nl -ba", "rg ")):
        metrics["raw_read_like_commands"] += 1


def parse_pi_command_metrics(path: Path, anchor_bin: Path | None = None) -> dict[str, int]:
    metrics = parse_command_metrics(path, anchor_bin)
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") != "tool_execution_end" or event.get("toolName") != "bash":
            continue

        metrics["completed_commands"] += 1
        if event.get("isError"):
            metrics["failed_commands"] += 1

        command = find_command_text(event.get("args"))
        if command:
            classify_anchor_command(command, metrics, anchor_bin)
    return metrics


def run_mode(args: argparse.Namespace, task: dict[str, Any], mode: str, out_root: Path) -> dict[str, Any]:
    run_dir = out_root / mode
    repo_dir = run_dir / "repo"
    logs_dir = run_dir / "logs"
    run_dir.mkdir(parents=True, exist_ok=True)
    logs_dir.mkdir(parents=True, exist_ok=True)

    clean_copy_from_image(task["docker_image"], repo_dir)
    prompt = (
        anchor_prompt(task["instruction"], args.anchor_bin)
        if mode == "anchor"
        else baseline_prompt(task["instruction"])
    )
    (run_dir / "prompt.txt").write_text(prompt)

    agent_log = run_dir / "pi.jsonl"
    agent = run_pi(
        mode,
        repo_dir,
        prompt,
        agent_log,
        args.agent_timeout_sec,
        args.pi_bin,
        args.pi_provider,
        args.pi_model,
    )
    agent["log"] = classify_agent_log(agent_log, agent["exit_code"])

    metrics = git_metrics(repo_dir)
    if metrics["patch_bytes"] == 0 and agent["exit_code"] != 0:
        verify = {
            "reward": "skipped",
            "passed": False,
            "base_exit": None,
            "new_exit": None,
            "model_patch_bytes": 0,
            "skip_reason": "agent_failed_without_patch",
        }
    else:
        verify = verify_with_task_image(task, repo_dir, logs_dir)

    result = {
        "mode": mode,
        "agent": agent,
        "git": metrics,
        "verify": verify,
        "tool_counts": parse_pi_tool_counts(agent_log),
        "command_metrics": parse_pi_command_metrics(agent_log, args.anchor_bin),
        "agent_log_bytes": file_size(agent_log),
        "anchor": anchor_artifacts(repo_dir, args.anchor_bin) if mode == "anchor" else None,
    }
    (run_dir / "result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description="Run baseline vs Anchor Pi on a DeepSWE task.")
    parser.add_argument("task_id", nargs="?", default="python-statemachine-state-data-scoping")
    parser.add_argument("--deepswe-root", type=Path, default=DEFAULT_DEEPSWE_ROOT)
    parser.add_argument("--work-root", type=Path, default=DEFAULT_WORK_ROOT)
    parser.add_argument("--mode", choices=["both", "baseline", "anchor"], default="both")
    parser.add_argument("--agent-timeout-sec", type=int, default=5400)
    parser.add_argument("--anchor-bin", type=Path, default=ANCHOR_ROOT / "target" / "debug" / "anchor")
    parser.add_argument("--pi-bin", default="pi")
    parser.add_argument("--pi-provider", default=None)
    parser.add_argument("--pi-model", default=None)
    args = parser.parse_args()

    ensure_pi_available(args.pi_bin)
    ensure_docker_ready()

    task_dir = args.deepswe_root / "tasks" / args.task_id
    if not task_dir.exists():
        raise SystemExit(f"missing task: {task_dir}")

    task = load_task(task_dir)
    out_root = args.work_root / f"{args.task_id}-{time.strftime('%Y%m%d-%H%M%S')}"
    out_root.mkdir(parents=True, exist_ok=True)
    (out_root / "task.json").write_text(
        json.dumps(task | {"test_patch": str(task["test_patch"])}, indent=2, sort_keys=True) + "\n"
    )

    modes = ["baseline", "anchor"] if args.mode == "both" else [args.mode]
    results = [run_mode(args, task, mode, out_root) for mode in modes]
    summary = {
        "schema": "anchor.native_deepswe_pi_pair.v1",
        "task_id": args.task_id,
        "out_dir": str(out_root),
        "results": results,
    }
    (out_root / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

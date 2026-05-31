#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path


DEFAULT_DEEPSWE_ROOT = Path("/Volumes/Hak_SSD/deep-swe")


def load_manifest(root: Path) -> dict:
    manifest = root / "tasks" / "manifest.json"
    if not manifest.exists():
        raise SystemExit(f"missing DeepSWE manifest: {manifest}")
    return json.loads(manifest.read_text())


def load_task_toml(root: Path, task_id: str) -> dict:
    task_toml = root / "tasks" / task_id / "task.toml"
    if not task_toml.exists():
        raise SystemExit(f"missing task.toml for {task_id}: {task_toml}")
    return tomllib.loads(task_toml.read_text())


def check_colima() -> dict:
    result = {
        "docker_context": None,
        "docker_info_ok": False,
        "error": None,
    }
    try:
        context = subprocess.run(
            ["docker", "context", "show"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        result["docker_context"] = context.stdout.strip() or None
    except FileNotFoundError:
        result["error"] = "docker CLI not found"
        return result

    info = subprocess.run(
        ["docker", "info", "--format", "{{json .ClientInfo.Context}}"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if info.returncode == 0:
        result["docker_info_ok"] = True
    else:
        result["error"] = info.stderr.strip() or info.stdout.strip()
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect local DeepSWE tasks for Anchor benchmarking.")
    parser.add_argument("--deep-swe-root", type=Path, default=DEFAULT_DEEPSWE_ROOT)
    parser.add_argument("--summary", action="store_true")
    parser.add_argument("--language")
    parser.add_argument("--limit", type=int, default=20)
    parser.add_argument("--task-id")
    parser.add_argument("--check-colima", action="store_true")
    args = parser.parse_args()

    manifest = load_manifest(args.deep_swe_root)
    tasks = manifest.get("tasks", [])

    if args.summary:
        by_language = Counter(task.get("language", "unknown") for task in tasks)
        by_category = Counter(task.get("category", "unknown") for task in tasks)
        print(json.dumps({
            "dataset": manifest.get("dataset"),
            "task_count": len(tasks),
            "languages": dict(sorted(by_language.items())),
            "categories": dict(sorted(by_category.items())),
        }, indent=2))

    filtered = tasks
    if args.language:
        filtered = [task for task in filtered if task.get("language") == args.language]
    if args.task_id:
        filtered = [task for task in filtered if task.get("task_id") == args.task_id]

    if args.language or args.task_id:
        rows = []
        for task in filtered[: args.limit]:
            task_dir = args.deep_swe_root / "tasks" / task["task_id"]
            task_meta = load_task_toml(args.deep_swe_root, task["task_id"])
            rows.append({
                "task_id": task["task_id"],
                "language": task.get("language"),
                "repo": task.get("repo"),
                "repository_url": task.get("repository_url"),
                "base_commit_hash": task_meta.get("metadata", {}).get("base_commit_hash"),
                "instruction": str(task_dir / "instruction.md"),
                "test_script": str(task_dir / "tests" / "test.sh"),
                "test_patch": str(task_dir / "tests" / "test.patch"),
                "docker_image": task_meta.get("environment", {}).get("docker_image"),
                "allow_internet": task_meta.get("environment", {}).get("allow_internet"),
            })
        print(json.dumps(rows, indent=2))

    if args.check_colima:
        print(json.dumps({"colima": check_colima()}, indent=2))

    return 0


if __name__ == "__main__":
    sys.exit(main())


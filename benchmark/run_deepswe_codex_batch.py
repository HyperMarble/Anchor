#!/usr/bin/env python3
from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any


ANCHOR_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BATCH_ROOT = Path("/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe-codex-batch")


def parse_summary_from_log(log_path: Path) -> dict[str, Any] | None:
    text = log_path.read_text(errors="replace")
    decoder = json.JSONDecoder()
    for start in range(len(text) - 1, -1, -1):
        if text[start] != "{":
            continue
        try:
            summary, end = decoder.raw_decode(text[start:])
        except json.JSONDecodeError:
            continue
        if isinstance(summary, dict) and not text[start + end :].strip():
            return summary
    return None


def metric(summary: dict[str, Any], path: list[str], default: Any = None) -> Any:
    current: Any = summary
    for key in path:
        if not isinstance(current, dict) or key not in current:
            return default
        current = current[key]
    return current


def run_one(args: argparse.Namespace, batch_dir: Path, index: int) -> dict[str, Any]:
    run_dir = batch_dir / f"run-{index}"
    run_dir.mkdir(parents=True, exist_ok=True)
    log_path = run_dir / "stdout.log"
    work_root = run_dir / "work"

    cmd = [
        "python3",
        str(ANCHOR_ROOT / "benchmark" / "run_deepswe_codex_pair.py"),
        args.task_id,
        "--work-root",
        str(work_root),
        "--agent-timeout-sec",
        str(args.agent_timeout_sec),
    ]
    if args.codex_model:
        cmd.extend(["--codex-model", args.codex_model])

    started = time.time()
    env = dict(os.environ)
    if args.docker_host:
        env["DOCKER_HOST"] = args.docker_host

    with log_path.open("w") as log:
        proc = subprocess.run(
            cmd,
            cwd=ANCHOR_ROOT,
            env=env,
            text=True,
            stdout=log,
            stderr=subprocess.STDOUT,
        )

    summary = parse_summary_from_log(log_path)
    return {
        "run": index,
        "exit_code": proc.returncode,
        "duration_sec": round(time.time() - started, 3),
        "log": str(log_path),
        "summary": summary,
        "out_dir": metric(summary or {}, ["out_dir"]),
        "read_this_first": metric(summary or {}, ["product_comparison", "read_this_first"], []),
        "efficiency_delta": metric(summary or {}, ["product_comparison", "efficiency_delta"], {}),
        "quality_delta": metric(summary or {}, ["product_comparison", "quality_delta"], {}),
        "safety_delta": metric(summary or {}, ["product_comparison", "safety_delta"], {}),
    }


def numeric_values(results: list[dict[str, Any]], metric_name: str) -> list[float]:
    values: list[float] = []
    for result in results:
        value = (
            result.get("efficiency_delta", {})
            .get(metric_name, {})
            .get("improvement")
        )
        if isinstance(value, int | float):
            values.append(float(value))
    return values


def average(values: list[float]) -> float | None:
    if not values:
        return None
    return sum(values) / len(values)


def aggregate(results: list[dict[str, Any]]) -> dict[str, Any]:
    completed = [r for r in results if r["exit_code"] == 0 and r.get("summary")]
    quality = [r.get("quality_delta", {}) for r in completed]
    safety = [r.get("safety_delta", {}) for r in completed]

    return {
        "runs": len(results),
        "completed": len(completed),
        "failed_processes": [r["run"] for r in results if r["exit_code"] != 0],
        "avg_efficiency_improvement": {
            "agent_log_bytes": average(numeric_values(completed, "agent_log_bytes")),
            "patch_bytes": average(numeric_values(completed, "patch_bytes")),
            "diff_lines": average(numeric_values(completed, "diff_lines")),
            "raw_read_like_commands": average(numeric_values(completed, "raw_read_like_commands")),
        },
        "quality": {
            "baseline_passed": sum(1 for q in quality if q.get("baseline_passed")),
            "anchor_passed": sum(1 for q in quality if q.get("anchor_passed")),
            "anchor_rewards": [q.get("anchor_reward") for q in quality],
            "baseline_rewards": [q.get("baseline_reward") for q in quality],
            "anchor_quality_scores": [q.get("anchor_quality_score") for q in quality],
        },
        "safety": {
            "raw_terminal_writes": [s.get("anchor_raw_terminal_writes") for s in safety],
            "unrecorded_changed_files": [s.get("anchor_unrecorded_changed_files") for s in safety],
            "guarded_writes": [s.get("anchor_guarded_writes") for s in safety],
            "stale_write_blocks": [s.get("anchor_stale_write_blocks") for s in safety],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run DeepSWE Codex pair samples in parallel.")
    parser.add_argument("task_id", nargs="?", default="python-statemachine-state-data-scoping")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--concurrency", type=int, default=5)
    parser.add_argument("--batch-root", type=Path, default=DEFAULT_BATCH_ROOT)
    parser.add_argument("--agent-timeout-sec", type=int, default=5400)
    parser.add_argument("--docker-host", default=os.environ.get("DOCKER_HOST"))
    parser.add_argument("--codex-model", default=None)
    args = parser.parse_args()

    if args.runs < 1:
        raise SystemExit("--runs must be >= 1")
    if args.concurrency < 1:
        raise SystemExit("--concurrency must be >= 1")

    batch_dir = args.batch_root / f"{args.task_id}-{time.strftime('%Y%m%d-%H%M%S')}"
    batch_dir.mkdir(parents=True, exist_ok=True)

    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [pool.submit(run_one, args, batch_dir, index) for index in range(1, args.runs + 1)]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            print(
                json.dumps(
                    {
                        "run": result["run"],
                        "exit_code": result["exit_code"],
                        "out_dir": result["out_dir"],
                        "read_this_first": result["read_this_first"],
                    },
                    sort_keys=True,
                ),
                flush=True,
            )

    results.sort(key=lambda item: item["run"])
    summary = {
        "schema": "anchor.native_deepswe_codex_batch.v1",
        "task_id": args.task_id,
        "batch_dir": str(batch_dir),
        "runs": results,
        "aggregate": aggregate(results),
    }
    (batch_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

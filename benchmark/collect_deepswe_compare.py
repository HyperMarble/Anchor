#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return json.loads(path.read_text())


def first_trial_result(job_dir: Path) -> dict[str, Any] | None:
    for path in sorted(job_dir.glob("*/result.json")):
        data = load_json(path)
        if data and "task_name" in data:
            return data
    return None


def job_summary(job_dir: Path | None) -> dict[str, Any] | None:
    if job_dir is None:
        return None
    job = load_json(job_dir / "result.json") or {}
    trial = first_trial_result(job_dir) or {}
    exception = trial.get("exception_info") or {}
    verifier = trial.get("verifier_result") or {}
    agent = trial.get("agent_result") or {}

    return {
        "job_dir": str(job_dir),
        "job_finished_at": job.get("finished_at"),
        "task_name": trial.get("task_name"),
        "trial_name": trial.get("trial_name"),
        "errored_trials": job.get("stats", {}).get("n_errored_trials"),
        "completed_trials": job.get("stats", {}).get("n_completed_trials"),
        "exception_type": exception.get("exception_type"),
        "exception_message": exception.get("exception_message"),
        "agent_result": agent,
        "verifier_result": verifier,
    }


def trace_summary(trace: Path | None) -> dict[str, Any] | None:
    if trace is None or not trace.exists():
        return None
    counts = {
        "events": 0,
        "uses_anchor": 0,
        "runs_tests": 0,
        "raw_reads": 0,
        "raw_writes": 0,
        "source_like_raw_writes": 0,
    }
    for line in trace.read_text().splitlines():
        if not line.strip():
            continue
        event = json.loads(line)
        cls = event.get("classification") or {}
        counts["events"] += 1
        counts["uses_anchor"] += int(bool(cls.get("uses_anchor")))
        counts["runs_tests"] += int(bool(cls.get("runs_tests")))
        counts["raw_reads"] += int(bool(cls.get("raw_read")))
        counts["raw_writes"] += int(bool(cls.get("raw_write")))
        counts["source_like_raw_writes"] += int(
            bool(cls.get("raw_write")) and bool(cls.get("source_like"))
        )
    return counts


def artifact_summary(artifact_dir: Path | None) -> dict[str, Any] | None:
    if artifact_dir is None:
        return None
    receipt = load_json(artifact_dir / "anchor-receipt.json")
    return {
        "artifact_dir": str(artifact_dir),
        "has_receipt": receipt is not None,
        "has_status": (artifact_dir / "anchor-status.xml").exists(),
        "has_trace": (artifact_dir / "anchor-trace.xml").exists(),
        "has_gate": (artifact_dir / "anchor-gate.xml").exists(),
        "quality": (receipt or {}).get("quality"),
        "summary": (receipt or {}).get("summary"),
    }


def optional_path(value: str | None) -> Path | None:
    return Path(value).expanduser().resolve() if value else None


def main() -> int:
    parser = argparse.ArgumentParser(description="Collect baseline vs Anchor DeepSWE results.")
    parser.add_argument("--baseline-job")
    parser.add_argument("--anchor-job")
    parser.add_argument("--anchor-trace")
    parser.add_argument("--anchor-artifacts")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()

    result = {
        "schema": "anchor.deepswe.compare.v1",
        "baseline": job_summary(optional_path(args.baseline_job)),
        "anchor": job_summary(optional_path(args.anchor_job)),
        "anchor_trace": trace_summary(optional_path(args.anchor_trace)),
        "anchor_artifacts": artifact_summary(optional_path(args.anchor_artifacts)),
    }

    text = json.dumps(result, indent=2, sort_keys=True)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

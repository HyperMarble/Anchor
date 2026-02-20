#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
from statistics import mean
from typing import Any, Dict, List, Optional, Tuple


def load_jsonl(path: Path) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def avg(vals: List[float]) -> Optional[float]:
    if not vals:
        return None
    return float(mean(vals))


def pair_by_task(
    rows: List[Dict[str, Any]], a: str, b: str
) -> Dict[str, Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]]:
    grouped: Dict[str, Dict[str, List[Dict[str, Any]]]] = {}
    for r in rows:
        grouped.setdefault(r["task_id"], {}).setdefault(r["profile"], []).append(r)
    out: Dict[str, Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]] = {}
    for task_id, by_profile in grouped.items():
        if a in by_profile and b in by_profile:
            out[task_id] = (by_profile[a], by_profile[b])
    return out


def metric_wins(
    task_pairs: Dict[str, Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]]
) -> Dict[str, int]:
    wins = {
        "speed": 0,
        "efficiency": 0,
        "quality": 0,
        "performance": 0,
        "token_usage": 0,
        "tool_calls": 0,
    }
    considered = {k: 0 for k in wins}

    for _, (a_runs, b_runs) in task_pairs.items():
        a_speed = avg([r["speed_sec"] for r in a_runs])
        b_speed = avg([r["speed_sec"] for r in b_runs])
        if a_speed is not None and b_speed is not None:
            considered["speed"] += 1
            if a_speed < b_speed:
                wins["speed"] += 1

        a_eff = avg([r["efficiency"] for r in a_runs])
        b_eff = avg([r["efficiency"] for r in b_runs])
        if a_eff is not None and b_eff is not None:
            considered["efficiency"] += 1
            if a_eff > b_eff:
                wins["efficiency"] += 1

        a_quality = avg(
            [
                (r["checks_passed"] / r["checks_total"]) if r["checks_total"] > 0 else 0.0
                for r in a_runs
            ]
        )
        b_quality = avg(
            [
                (r["checks_passed"] / r["checks_total"]) if r["checks_total"] > 0 else 0.0
                for r in b_runs
            ]
        )
        if a_quality is not None and b_quality is not None:
            considered["quality"] += 1
            if a_quality > b_quality:
                wins["quality"] += 1

        a_perf = avg([r["eval_runtime_sec"] for r in a_runs])
        b_perf = avg([r["eval_runtime_sec"] for r in b_runs])
        if a_perf is not None and b_perf is not None:
            considered["performance"] += 1
            if a_perf < b_perf:
                wins["performance"] += 1

        a_tok_values = [r["token_usage"] for r in a_runs if r.get("token_usage") is not None]
        b_tok_values = [r["token_usage"] for r in b_runs if r.get("token_usage") is not None]
        a_tok = avg([float(v) for v in a_tok_values])
        b_tok = avg([float(v) for v in b_tok_values])
        if a_tok is not None and b_tok is not None:
            considered["token_usage"] += 1
            if a_tok < b_tok:
                wins["token_usage"] += 1

        a_tools_values = [r["tool_calls"] for r in a_runs if r.get("tool_calls") is not None]
        b_tools_values = [r["tool_calls"] for r in b_runs if r.get("tool_calls") is not None]
        a_tools = avg([float(v) for v in a_tools_values])
        b_tools = avg([float(v) for v in b_tools_values])
        if a_tools is not None and b_tools is not None:
            considered["tool_calls"] += 1
            if a_tools < b_tools:
                wins["tool_calls"] += 1

    return {"wins": wins, "considered": considered}


def main() -> int:
    parser = argparse.ArgumentParser(description="Score benchmark results")
    parser.add_argument("--results", required=True, type=Path)
    parser.add_argument("--a", required=True, help="Profile A (candidate)")
    parser.add_argument("--b", required=True, help="Profile B (baseline)")
    args = parser.parse_args()

    rows = load_jsonl(args.results)
    pairs = pair_by_task(rows, args.a, args.b)
    summary = metric_wins(pairs)
    wins = summary["wins"]
    considered = summary["considered"]

    print(f"Compared profiles: {args.a} vs {args.b}")
    print(f"Tasks compared: {len(pairs)}")
    print()
    print("Metric win rates for profile A:")
    for metric in [
        "speed",
        "efficiency",
        "quality",
        "performance",
        "token_usage",
        "tool_calls",
    ]:
        c = considered[metric]
        w = wins[metric]
        if c == 0:
            print(f"- {metric}: n/a")
        else:
            pct = round((w / c) * 100.0, 2)
            print(f"- {metric}: {w}/{c} ({pct}%)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

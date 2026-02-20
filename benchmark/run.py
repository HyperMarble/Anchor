#!/usr/bin/env python3
import argparse
import json
import os
import re
import shlex
import subprocess
import tempfile
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def load_json(path: Path) -> Dict[str, Any]:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def load_jsonl(path: Path) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
    return rows


def write_jsonl(path: Path, row: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=True) + "\n")


def run_cmd(
    cmd: str,
    cwd: Path,
    env: Dict[str, str],
    timeout_sec: int,
) -> Dict[str, Any]:
    start = time.time()
    p = subprocess.run(
        cmd,
        cwd=str(cwd),
        env=env,
        shell=True,
        capture_output=True,
        text=True,
        timeout=timeout_sec,
    )
    end = time.time()
    return {
        "cmd": cmd,
        "exit_code": p.returncode,
        "stdout": p.stdout,
        "stderr": p.stderr,
        "duration_sec": round(end - start, 4),
    }


def parse_metric(text: str, regex: Optional[str]) -> Optional[int]:
    if not regex:
        return None
    m = re.search(regex, text, re.IGNORECASE | re.MULTILINE)
    if not m:
        return None
    try:
        return int(m.group(1))
    except (ValueError, IndexError):
        return None


def clone_repo(source: str, commit: str, run_dir: Path, timeout_sec: int) -> None:
    run_dir.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        f"git clone --quiet {shlex.quote(source)} {shlex.quote(str(run_dir))}",
        shell=True,
        check=True,
        text=True,
        capture_output=True,
        timeout=timeout_sec,
    )
    subprocess.run(
        f"git checkout --quiet {shlex.quote(commit)}",
        shell=True,
        check=True,
        text=True,
        capture_output=True,
        cwd=str(run_dir),
        timeout=timeout_sec,
    )


def build_prompt(task: Dict[str, Any], profile: Dict[str, Any]) -> str:
    mode_hint = profile.get("mode_hint", "")
    extra_rules = profile.get("prompt_rules", [])
    rules_text = "\n".join(f"- {r}" for r in extra_rules) if extra_rules else ""
    parts = [
        f"Task ID: {task['task_id']}",
        f"Title: {task['title']}",
        f"Issue: {task['issue']}",
        f"Mode: {mode_hint}",
        "Execution Rules:",
        "- Do not ask clarifying questions; make reasonable assumptions and proceed.",
        "- Implement only what is required to satisfy the issue and checks.",
        "- Keep the patch minimal and avoid unrelated edits.",
        "- After editing, run relevant checks and iterate until passing or blocked.",
        "- If blocked, stop and state the blocker in output.",
        rules_text,
        "Task Instructions:",
        task["instructions"].strip(),
    ]
    return "\n\n".join(p for p in parts if p).strip() + "\n"


def apply_template(template: str, values: Dict[str, str]) -> str:
    out = template
    for k, v in values.items():
        out = out.replace("{" + k + "}", v)
    return out


@dataclass
class RunResult:
    task_id: str
    profile: str
    success: bool
    speed_sec: float
    eval_runtime_sec: float
    checks_total: int
    checks_passed: int
    efficiency: float
    token_usage: Optional[int]
    tool_calls: Optional[int]
    agent_exit_code: int
    output_dir: str
    started_at: str
    ended_at: str

    def to_dict(self) -> Dict[str, Any]:
        return {
            "task_id": self.task_id,
            "profile": self.profile,
            "success": self.success,
            "speed_sec": self.speed_sec,
            "eval_runtime_sec": self.eval_runtime_sec,
            "checks_total": self.checks_total,
            "checks_passed": self.checks_passed,
            "efficiency": self.efficiency,
            "token_usage": self.token_usage,
            "tool_calls": self.tool_calls,
            "agent_exit_code": self.agent_exit_code,
            "output_dir": self.output_dir,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
        }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Anchor agent benchmark")
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--profiles", required=True, type=Path)
    parser.add_argument("--tasks", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    config = load_json(args.config)
    profiles = load_json(args.profiles)
    tasks = load_jsonl(args.tasks)

    timeout_sec = int(config.get("timeout_sec", 1800))
    repeats = int(config.get("repeats", 1))
    out_root = Path(config.get("run_artifacts_dir", "benchmark/runs"))
    out_root.mkdir(parents=True, exist_ok=True)

    for task in tasks:
        source_repo = task["repo_source"]
        base_commit = task["base_commit"]
        eval_cmds = task.get("eval_cmds", [])
        setup_cmds = task.get("setup_cmds", [])
        for profile_name, profile in profiles.items():
            for run_idx in range(repeats):
                started = now_iso()
                run_stamp = f"{task['task_id']}__{profile_name}__r{run_idx+1}"
                run_dir = out_root / run_stamp
                clone_repo(source_repo, base_commit, run_dir, timeout_sec)

                env = os.environ.copy()
                for k, v in profile.get("env", {}).items():
                    env[k] = str(v)

                logs: Dict[str, Any] = {"setup": [], "eval": []}

                for c in setup_cmds + profile.get("pre_cmds", []):
                    logs["setup"].append(run_cmd(c, run_dir, env, timeout_sec))

                prompt = build_prompt(task, profile)
                prompt_file = run_dir / ".benchmark_prompt.txt"
                prompt_file.write_text(prompt, encoding="utf-8")

                cmd_tmpl = profile["agent_cmd_template"]
                agent_cmd = apply_template(
                    cmd_tmpl,
                    {
                        "repo_path": str(run_dir),
                        "prompt_file": str(prompt_file),
                    },
                )

                t0 = time.time()
                agent_res = run_cmd(agent_cmd, run_dir, env, timeout_sec)
                t1 = time.time()

                eval_start = time.time()
                checks_total = 0
                checks_passed = 0
                for c in eval_cmds:
                    checks_total += 1
                    r = run_cmd(c, run_dir, env, timeout_sec)
                    logs["eval"].append(r)
                    if r["exit_code"] == 0:
                        checks_passed += 1
                eval_end = time.time()

                combined_output = (
                    (agent_res.get("stdout") or "")
                    + "\n"
                    + (agent_res.get("stderr") or "")
                )
                token_usage = parse_metric(
                    combined_output, profile.get("token_usage_regex")
                )
                tool_calls = parse_metric(
                    combined_output, profile.get("tool_calls_regex")
                )

                speed_sec = round(t1 - t0, 4)
                eval_runtime_sec = round(eval_end - eval_start, 4)
                efficiency = round(
                    (checks_passed / speed_sec) if speed_sec > 0 else 0.0,
                    6,
                )
                success = checks_total > 0 and checks_passed == checks_total

                (run_dir / ".benchmark_logs.json").write_text(
                    json.dumps(
                        {
                            "task": task,
                            "profile": profile_name,
                            "agent_cmd": agent_cmd,
                            "agent": agent_res,
                            "logs": logs,
                        },
                        ensure_ascii=True,
                        indent=2,
                    ),
                    encoding="utf-8",
                )

                ended = now_iso()
                result = RunResult(
                    task_id=task["task_id"],
                    profile=profile_name,
                    success=success,
                    speed_sec=speed_sec,
                    eval_runtime_sec=eval_runtime_sec,
                    checks_total=checks_total,
                    checks_passed=checks_passed,
                    efficiency=efficiency,
                    token_usage=token_usage,
                    tool_calls=tool_calls,
                    agent_exit_code=agent_res["exit_code"],
                    output_dir=str(run_dir),
                    started_at=started,
                    ended_at=ended,
                )
                write_jsonl(args.out, result.to_dict())
                print(
                    f"[{task['task_id']}] [{profile_name}] "
                    f"success={success} speed={speed_sec}s "
                    f"checks={checks_passed}/{checks_total}"
                )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

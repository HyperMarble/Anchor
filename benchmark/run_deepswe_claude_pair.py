#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore


ANCHOR_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DEEPSWE_ROOT = Path("/Volumes/Hak_SSD/deep-swe")
DEFAULT_WORK_ROOT = Path("/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe")


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    stdout: Any = subprocess.PIPE,
    stderr: Any = subprocess.STDOUT,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        env=env,
        text=True,
        stdout=stdout,
        stderr=stderr,
        check=check,
    )


def load_task(task_dir: Path) -> dict[str, Any]:
    data = tomllib.loads((task_dir / "task.toml").read_text())
    return {
        "task_id": data["metadata"]["task_id"],
        "repo_url": data["metadata"]["repository_url"],
        "base_commit": data["metadata"]["base_commit_hash"],
        "docker_image": data["environment"]["docker_image"],
        "instruction": (task_dir / "instruction.md").read_text().strip(),
        "test_patch": task_dir / "tests" / "test.patch",
    }


def clean_copy_from_image(image: str, repo_dir: Path) -> None:
    if repo_dir.exists():
        shutil.rmtree(repo_dir)
    repo_dir.mkdir(parents=True)

    container_name = f"anchor-native-prepare-{os.getpid()}"
    try:
        run(["docker", "create", "--name", container_name, image, "sleep", "infinity"])
        run(["docker", "cp", f"{container_name}:/app/.", str(repo_dir)])
    finally:
        run(["docker", "rm", "-f", container_name], check=False)

    git_dir = repo_dir / ".git"
    if git_dir.exists():
        shutil.rmtree(git_dir)
    run(["git", "init", "-q"], cwd=repo_dir)
    run(["git", "config", "user.email", "anchor-benchmark@example.invalid"], cwd=repo_dir)
    run(["git", "config", "user.name", "Anchor Benchmark"], cwd=repo_dir)
    run(["git", "add", "-A"], cwd=repo_dir)
    run(["git", "commit", "-q", "-m", "base"], cwd=repo_dir)


def baseline_prompt(instruction: str) -> str:
    return f"""You are solving a software engineering benchmark task.

Task:
{instruction}

Rules:
- Work only inside the current repository.
- Do not use web search, web fetch, network browsing, or external issue pages.
- Do not inspect benchmark solution files, hidden tests, parent directories, or runner files.
- Do not use git log, git show, git blame, or remote history to infer the answer.
- You may inspect source files, edit code, and run local tests or build commands.
- Keep the patch scoped to the task.
"""


def anchor_prompt(instruction: str, anchor_bin: Path) -> str:
    return f"""You are solving a software engineering benchmark task.

Task:
{instruction}

Rules:
- Work only inside the current repository.
- Do not use web search, web fetch, network browsing, or external issue pages.
- Do not inspect benchmark solution files, hidden tests, parent directories, or runner files.
- Do not use git log, git show, git blame, or remote history to infer the answer.
- Keep the patch scoped to the task.

Use Anchor as the execution harness.

Anchor command:
{anchor_bin}

Required workflow:
1. Run `{anchor_bin} build`.
2. Use `{anchor_bin} context <symbol>` for source understanding before broad raw reads.
3. Use `{anchor_bin} edit <path> --symbol <symbol_name> --content '<full replacement symbol>'` for scoped source edits when possible.
4. Run checks through `{anchor_bin} check -- <command>` when practical.
5. Before finishing, run `{anchor_bin} status`, `{anchor_bin} receipt`, and `{anchor_bin} gate --min-score 85`.

The benchmark is measuring whether Anchor improves efficiency, scope, quality, and safety. Do not silently bypass Anchor if it fails; record what failed.
"""


def run_claude(mode: str, repo_dir: Path, prompt: str, out_path: Path, timeout_sec: int) -> dict[str, Any]:
    env = dict(os.environ)
    env["PATH"] = f"{ANCHOR_ROOT / 'target' / 'debug'}:{ANCHOR_ROOT / 'target' / 'release'}:{env.get('PATH', '')}"
    env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"] = "1"
    cmd = [
        "claude",
        "--verbose",
        "--output-format=stream-json",
        "--permission-mode=bypassPermissions",
        "--disallowedTools",
        "EnterPlanMode",
        "--disallowedTools",
        "WebFetch",
        "--disallowedTools",
        "WebSearch",
        "--print",
        prompt,
    ]

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
    }


def parse_tool_counts(path: Path) -> dict[str, int]:
    counts: dict[str, int] = {}

    def walk(value: Any) -> None:
        if isinstance(value, dict):
            if value.get("type") == "tool_use" and isinstance(value.get("name"), str):
                counts[value["name"]] = counts.get(value["name"], 0) + 1
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            walk(json.loads(line))
        except json.JSONDecodeError:
            continue
    return counts


def classify_agent_log(path: Path, exit_code: int) -> dict[str, Any]:
    status = "ok" if exit_code == 0 else "failed"
    reason = None
    rate_limit = None
    result_text = None

    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue

        if event.get("type") == "rate_limit_event":
            status = "rate_limited"
            rate_limit = event.get("rate_limit_info")
            continue

        if event.get("type") == "result":
            if event.get("is_error"):
                reason = event.get("terminal_reason") or event.get("subtype") or "error"
            if isinstance(event.get("result"), str):
                result_text = event["result"][:500]

    return {
        "status": status,
        "reason": reason,
        "rate_limit": rate_limit,
        "result_preview": result_text,
    }


def git_metrics(repo_dir: Path) -> dict[str, Any]:
    changed = run(["git", "diff", "--name-only"], cwd=repo_dir).stdout.splitlines()
    diff = run(["git", "diff", "--binary"], cwd=repo_dir).stdout
    return {
        "changed_files": len(changed),
        "changed_file_list": changed,
        "diff_lines": len(diff.splitlines()),
        "patch_bytes": len(diff.encode()),
    }


def verify_with_task_image(task: dict[str, Any], repo_dir: Path, logs_dir: Path) -> dict[str, Any]:
    logs_dir.mkdir(parents=True, exist_ok=True)
    tests_dir = logs_dir / "tests"
    tests_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(task["test_patch"], tests_dir / "test.patch")

    verifier = logs_dir / "verify.sh"
    verifier.write_text(
        r'''#!/usr/bin/env bash
set -uo pipefail
mkdir -p /logs/verifier /logs/artifacts
echo "verifier started" > /logs/verifier/preflight.txt
cd /app || exit 6
git config --global --add safe.directory "$(pwd)" 2>/dev/null || true
git diff --binary > /logs/artifacts/model.patch

python3 - <<'PY'
import re
from pathlib import Path
patch = Path("/tests/test.patch").read_text(encoding="utf-8")
files = set()
for line in patch.splitlines():
    m = re.match(r"^diff --git \"?a/(.+?)\"? \"?b/(.+?)\"?$", line)
    if m:
        files.add(m.group(2))
for f in sorted(files):
    print(f)
PY

if ! git apply --whitespace=nowarn /tests/test.patch >/logs/verifier/apply.stdout 2>/logs/verifier/apply.stderr; then
  echo 0 > /logs/verifier/reward.txt
  exit 0
fi

chmod +x /app/test.sh
bash /app/test.sh base >/logs/verifier/base.stdout 2>/logs/verifier/base.stderr
base=$?
bash /app/test.sh new >/logs/verifier/new.stdout 2>/logs/verifier/new.stderr
new=$?
echo "$base" > /logs/verifier/base.exit
echo "$new" > /logs/verifier/new.exit
if [ "$base" -eq 0 ] && [ "$new" -eq 0 ]; then
  echo 1 > /logs/verifier/reward.txt
else
  echo 0 > /logs/verifier/reward.txt
fi
''',
    )
    verifier.chmod(0o755)

    docker_stdout = logs_dir / "docker-verify.stdout"
    docker_stderr = logs_dir / "docker-verify.stderr"
    with docker_stdout.open("w") as out, docker_stderr.open("w") as err:
        proc = subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "-v",
                f"{repo_dir}:/app",
                "-v",
                f"{tests_dir}:/tests:ro",
                "-v",
                f"{logs_dir}:/logs",
                "-v",
                f"{verifier}:/verify.sh:ro",
                task["docker_image"],
                "bash",
                "/verify.sh",
            ],
            text=True,
            stdout=out,
            stderr=err,
        )

    reward_path = logs_dir / "verifier" / "reward.txt"
    reward = reward_path.read_text().strip() if reward_path.exists() else "missing"
    return {
        "reward": reward,
        "passed": reward == "1",
        "base_exit": read_optional(logs_dir / "verifier" / "base.exit"),
        "new_exit": read_optional(logs_dir / "verifier" / "new.exit"),
        "model_patch_bytes": file_size(logs_dir / "artifacts" / "model.patch"),
        "docker_exit": proc.returncode,
        "preflight": (logs_dir / "verifier" / "preflight.txt").exists(),
        "stdout_bytes": file_size(docker_stdout),
        "stderr_bytes": file_size(docker_stderr),
    }


def read_optional(path: Path) -> str | None:
    return path.read_text().strip() if path.exists() else None


def file_size(path: Path) -> int:
    return path.stat().st_size if path.exists() else 0


def anchor_artifacts(repo_dir: Path, anchor_bin: Path) -> dict[str, Any]:
    anchor_root = repo_dir / ".anchor"
    event_file = anchor_root / "events" / "events.jsonl"
    receipt = None
    if anchor_root.exists():
        try:
            receipt = json.loads(run([str(anchor_bin), "-r", str(repo_dir), "receipt"]).stdout)
        except Exception:
            receipt = None
    return {
        "event_count": len(event_file.read_text().splitlines()) if event_file.exists() else 0,
        "has_anchor_dir": anchor_root.exists(),
        "receipt_quality": (receipt or {}).get("quality"),
    }


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

    agent = run_claude(mode, repo_dir, prompt, run_dir / "claude.jsonl", args.agent_timeout_sec)
    agent["log"] = classify_agent_log(run_dir / "claude.jsonl", agent["exit_code"])
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
    tools = parse_tool_counts(run_dir / "claude.jsonl")
    result = {
        "mode": mode,
        "agent": agent,
        "git": metrics,
        "verify": verify,
        "tool_counts": tools,
        "agent_log_bytes": file_size(run_dir / "claude.jsonl"),
        "anchor": anchor_artifacts(repo_dir, args.anchor_bin) if mode == "anchor" else None,
    }
    (run_dir / "result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description="Run baseline vs Anchor Claude on a DeepSWE task without Pier.")
    parser.add_argument("task_id", nargs="?", default="python-statemachine-state-data-scoping")
    parser.add_argument("--deepswe-root", type=Path, default=DEFAULT_DEEPSWE_ROOT)
    parser.add_argument("--work-root", type=Path, default=DEFAULT_WORK_ROOT)
    parser.add_argument("--mode", choices=["both", "baseline", "anchor"], default="both")
    parser.add_argument("--agent-timeout-sec", type=int, default=5400)
    parser.add_argument("--anchor-bin", type=Path, default=ANCHOR_ROOT / "target" / "debug" / "anchor")
    args = parser.parse_args()

    task_dir = args.deepswe_root / "tasks" / args.task_id
    if not task_dir.exists():
        raise SystemExit(f"missing task: {task_dir}")

    task = load_task(task_dir)
    out_root = args.work_root / f"{args.task_id}-{time.strftime('%Y%m%d-%H%M%S')}"
    out_root.mkdir(parents=True, exist_ok=True)
    (out_root / "task.json").write_text(json.dumps(task | {"test_patch": str(task["test_patch"])}, indent=2, sort_keys=True) + "\n")

    modes = ["baseline", "anchor"] if args.mode == "both" else [args.mode]
    results = [run_mode(args, task, mode, out_root) for mode in modes]
    summary = {
        "schema": "anchor.native_deepswe_pair.v1",
        "task_id": args.task_id,
        "out_dir": str(out_root),
        "results": results,
    }
    (out_root / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

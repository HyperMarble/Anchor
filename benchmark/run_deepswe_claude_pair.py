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

from execution_spec import execution_spec_requirement, parse_execution_spec_metrics

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore


ANCHOR_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DEEPSWE_ROOT = Path("/Volumes/Hak_SSD/deep-swe")
DEFAULT_WORK_ROOT = Path("/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe")
DEFAULT_DOCKER_PLATFORM = os.environ.get("ANCHOR_DOCKER_PLATFORM", "linux/amd64")


def progress(message: str) -> None:
    print(f"[anchor-benchmark] {message}", flush=True)


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    stdout: Any = subprocess.PIPE,
    stderr: Any = subprocess.STDOUT,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            cmd,
            cwd=str(cwd) if cwd else None,
            env=env,
            text=True,
            stdout=stdout,
            stderr=stderr,
            check=check,
        )
    except subprocess.CalledProcessError as exc:
        output = exc.stdout or exc.stderr or ""
        raise RuntimeError(
            f"command failed with exit {exc.returncode}: {' '.join(map(str, exc.cmd))}\n{output}"
        ) from exc


def docker_wait_running(container_name: str, timeout_sec: int = 120) -> None:
    deadline = time.time() + timeout_sec
    progress(f"waiting for container {container_name} to run")
    while time.time() < deadline:
        inspect = subprocess.run(
            [
                "docker",
                "inspect",
                "-f",
                "{{.State.Running}} {{.State.Status}} {{.State.ExitCode}} {{.State.Error}}",
                container_name,
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        output = inspect.stdout.strip()
        if inspect.returncode == 0:
            parts = output.split(maxsplit=3)
            running = parts[0] == "true" if parts else False
            status = parts[1] if len(parts) > 1 else "unknown"
            exit_code = parts[2] if len(parts) > 2 else "unknown"
            error = parts[3] if len(parts) > 3 else ""
            if running:
                progress(f"container {container_name} is running")
                return
            if status in {"exited", "dead"}:
                logs = subprocess.run(
                    ["docker", "logs", container_name],
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                ).stdout[-4000:]
                raise RuntimeError(
                    f"container {container_name} exited before verifier setup "
                    f"(status={status}, exit={exit_code}, error={error}). "
                    f"Image platform is forced to {DEFAULT_DOCKER_PLATFORM}; "
                    "if this is an amd64 image on arm64 Colima, make sure Colima supports x86_64 emulation.\n"
                    f"{logs}"
                )
        time.sleep(1)

    raise RuntimeError(f"timed out waiting for container {container_name} to run")


def ensure_docker_ready() -> None:
    probe = subprocess.run(
        ["docker", "info"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if probe.returncode == 0:
        return

    context = subprocess.run(
        ["docker", "context", "ls"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    hak_colima = subprocess.run(
        ["/bin/zsh", "-lc", "COLIMA_HOME=/Volumes/Hak_SSD/colima colima status"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )

    raise SystemExit(
        "Docker is not reachable for the DeepSWE benchmark.\n\n"
        f"docker info output:\n{probe.stdout}\n"
        f"docker context ls output:\n{context.stdout}\n"
        f"Hak_SSD Colima status:\n{hak_colima.stdout}\n"
        "Fix for the intended Hak_SSD Docker store:\n"
        "  COLIMA_HOME=/Volumes/Hak_SSD/colima colima start\n"
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
        progress(f"creating source container {container_name}")
        run(
            [
                "docker",
                "create",
                "--platform",
                DEFAULT_DOCKER_PLATFORM,
                "--name",
                container_name,
                "--entrypoint",
                "sleep",
                image,
                "infinity",
            ]
        )
        progress(f"copying /app from {container_name}")
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
- Do not add or edit test files as final changes unless the task explicitly requires it. Hidden tests evaluate the result.
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
- Do not add or edit test files as final changes unless the task explicitly requires it. Hidden tests evaluate the result.

Use Anchor as the execution harness.

Anchor command:
{anchor_bin}

{execution_spec_requirement()}

Required workflow:
1. After the ExecutionSpec, start with one Anchor task intake: `{anchor_bin} task "<short task summary>" --limit 8 --context-limit 4`. Anchor prepares the index automatically.
2. Use `{anchor_bin} context <symbol> --limit 1` only when the intake is missing a specific symbol you need.
3. Do not run `{anchor_bin} build` unless Anchor reports a stale/missing index error. If you manually run it, run it at most once.
4. Do not use broad raw source reads (`cat`, `sed`, `nl`, large `rg` dumps) as primary exploration. If Anchor returns zero results or errors, use the narrowest raw read possible and keep going.
5. Make source edits through `{anchor_bin} edit <path> --symbol <symbol_name> --content '<full replacement symbol>'` or `{anchor_bin} write <path> <content>`. If a raw edit is unavoidable, keep it narrow.
6. Run at least one focused test-like verification command through `{anchor_bin} check -- <command>` before finishing. Prefer the `<preferred_check>` or `<check_hints>` from the task intake when present. Lint and smoke scripts are useful but do not replace a test-like check. If Anchor prints `<handoff_gate status="blocked" .../>`, resolve the reason before finishing. Do not spend time running Anchor status, receipt, or gate; the benchmark runner collects those after the session.

The benchmark is measuring Anchor as the controlled execution harness, not as optional decoration. The run is invalid if you silently bypass Anchor for source understanding or source writes.
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


def parse_command_metrics(path: Path, anchor_bin: Path | None = None) -> dict[str, int]:
    metrics = {
        "completed_commands": 0,
        "failed_commands": 0,
        "anchor_commands": 0,
        "anchor_tasks": 0,
        "anchor_builds": 0,
        "anchor_contexts": 0,
        "anchor_edits": 0,
        "anchor_writes": 0,
        "anchor_checks": 0,
        "anchor_statuses": 0,
        "anchor_receipts": 0,
        "anchor_gates": 0,
        "raw_read_like_commands": 0,
    }
    anchor_text = str(anchor_bin) if anchor_bin else "anchor"

    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        item = event.get("item") or {}
        if event.get("type") != "item.completed" or item.get("type") != "command_execution":
            continue

        command = str(item.get("command", ""))
        metrics["completed_commands"] += 1
        if item.get("exit_code") not in (0, None):
            metrics["failed_commands"] += 1

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

    return metrics


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


def is_untracked_test_path(path: str) -> bool:
    p = Path(path)
    parts = set(p.parts)
    return "tests" in parts or p.name.startswith("test_") or p.name.endswith("_test.py")


def is_internal_untracked_path(path: str) -> bool:
    p = Path(path)
    parts = set(p.parts)
    return bool(
        parts
        & {
            ".anchor",
            ".cache",
            ".mypy_cache",
            ".pytest_cache",
            ".ruff_cache",
            ".venv",
            "__pycache__",
        }
    ) or p.suffix in {".pyc", ".pyo"}


def prepare_untracked_for_patch(repo_dir: Path) -> dict[str, list[str]]:
    untracked = run(["git", "ls-files", "--others", "--exclude-standard"], cwd=repo_dir).stdout.splitlines()
    patchable = [
        path
        for path in untracked
        if not is_untracked_test_path(path) and not is_internal_untracked_path(path)
    ]
    ignored = sorted(set(untracked) - set(patchable))
    if patchable:
        run(["git", "add", "--intent-to-add", "--", *patchable], cwd=repo_dir)
    return {
        "untracked_before": untracked,
        "untracked_patchable": patchable,
        "untracked_ignored": ignored,
    }


def git_metrics(repo_dir: Path) -> dict[str, Any]:
    untracked_info = prepare_untracked_for_patch(repo_dir)
    changed = run(["git", "diff", "--name-only"], cwd=repo_dir).stdout.splitlines()
    untracked_after_prepare = run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=repo_dir
    ).stdout.splitlines()
    all_changed = sorted(set(changed) | set(untracked_info["untracked_before"]))
    diff = run(["git", "diff", "--binary"], cwd=repo_dir).stdout
    return {
        "changed_files": len(all_changed),
        "changed_file_list": all_changed,
        "patch_files": len(changed),
        "patch_file_list": changed,
        "untracked_files": len(untracked_info["untracked_before"]),
        "untracked_file_list": untracked_info["untracked_before"],
        "untracked_patchable_files": len(untracked_info["untracked_patchable"]),
        "untracked_patchable_file_list": untracked_info["untracked_patchable"],
        "untracked_ignored_files": len(untracked_info["untracked_ignored"]),
        "untracked_ignored_file_list": untracked_info["untracked_ignored"],
        "untracked_after_prepare_files": len(untracked_after_prepare),
        "untracked_after_prepare_file_list": untracked_after_prepare,
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
git status --porcelain --untracked-files=all > /logs/verifier/pre_clean_status.txt
python3 - <<'PY'
import subprocess
from pathlib import Path

def is_test_path(path: str) -> bool:
    p = Path(path)
    parts = set(p.parts)
    return "tests" in parts or p.name.startswith("test_") or p.name.endswith("_test.py")

def is_internal_path(path: str) -> bool:
    p = Path(path)
    parts = set(p.parts)
    return bool(
        parts
        & {
            ".anchor",
            ".cache",
            ".mypy_cache",
            ".pytest_cache",
            ".ruff_cache",
            ".venv",
            "__pycache__",
        }
    ) or p.suffix in {".pyc", ".pyo"}

paths = subprocess.run(
    ["git", "ls-files", "--others", "--exclude-standard"],
    text=True,
    stdout=subprocess.PIPE,
    check=True,
).stdout.splitlines()
patchable = [p for p in paths if not is_test_path(p) and not is_internal_path(p)]
ignored = sorted(set(paths) - set(patchable))
if patchable:
    subprocess.run(["git", "add", "--intent-to-add", "--", *patchable], check=True)
Path("/logs/verifier/untracked_patchable.txt").write_text("\n".join(patchable) + ("\n" if patchable else ""))
Path("/logs/verifier/untracked_ignored.txt").write_text("\n".join(ignored) + ("\n" if ignored else ""))
PY
git clean -fdx >/logs/verifier/clean.stdout 2>/logs/verifier/clean.stderr || true
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
  echo 1 > /logs/verifier/test_apply.exit
  echo 0 > /logs/verifier/reward.txt
  exit 0
fi
echo 0 > /logs/verifier/test_apply.exit

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
    container_name = f"anchor-native-verify-{os.getpid()}-{int(time.time() * 1000)}"
    with docker_stdout.open("w") as out, docker_stderr.open("w") as err:
        proc = subprocess.CompletedProcess(["docker", "exec", container_name], returncode=125)
        try:
            progress(f"creating verifier container {container_name}")
            run(
                [
                    "docker",
                    "create",
                    "--platform",
                    DEFAULT_DOCKER_PLATFORM,
                    "--name",
                    container_name,
                    "--entrypoint",
                    "sleep",
                    task["docker_image"],
                    "infinity",
                ],
                stdout=out,
                stderr=err,
            )
            progress(f"starting verifier container {container_name}")
            run(["docker", "start", container_name], stdout=out, stderr=err)
            docker_wait_running(container_name)
            progress("preparing verifier filesystem")
            run(["docker", "exec", container_name, "mkdir", "-p", "/tests", "/logs"], stdout=out, stderr=err)
            run(["docker", "cp", f"{repo_dir}/.", f"{container_name}:/app"], stdout=out, stderr=err)
            run(["docker", "cp", str(tests_dir / "test.patch"), f"{container_name}:/tests/test.patch"], stdout=out, stderr=err)
            run(["docker", "cp", str(verifier), f"{container_name}:/verify.sh"], stdout=out, stderr=err)
            progress("running verifier")
            proc = subprocess.run(
                ["docker", "exec", container_name, "bash", "/verify.sh"],
                text=True,
                stdout=out,
                stderr=err,
            )
            progress("copying verifier logs")
            run(["docker", "cp", f"{container_name}:/logs/.", str(logs_dir)], stdout=out, stderr=err, check=False)
        finally:
            run(["docker", "rm", "-f", container_name], stdout=out, stderr=err, check=False)

    reward_path = logs_dir / "verifier" / "reward.txt"
    reward = reward_path.read_text().strip() if reward_path.exists() else "missing"
    base_exit = read_optional(logs_dir / "verifier" / "base.exit")
    new_exit = read_optional(logs_dir / "verifier" / "new.exit")
    test_apply_exit = read_optional(logs_dir / "verifier" / "test_apply.exit")
    return {
        "reward": reward,
        "passed": reward == "1",
        "test_apply_exit": test_apply_exit,
        "test_apply_succeeded": test_apply_exit == "0",
        "test_apply_stderr": read_optional(logs_dir / "verifier" / "apply.stderr"),
        "base_exit": base_exit,
        "new_exit": new_exit,
        "base_timeout": base_exit == "124",
        "new_timeout": new_exit == "124",
        "model_patch_bytes": file_size(logs_dir / "artifacts" / "model.patch"),
        "untracked_patchable_file_list": read_lines(logs_dir / "verifier" / "untracked_patchable.txt"),
        "untracked_ignored_file_list": read_lines(logs_dir / "verifier" / "untracked_ignored.txt"),
        "docker_exit": proc.returncode,
        "preflight": (logs_dir / "verifier" / "preflight.txt").exists(),
        "stdout_bytes": file_size(docker_stdout),
        "stderr_bytes": file_size(docker_stderr),
    }


def read_optional(path: Path) -> str | None:
    return path.read_text().strip() if path.exists() else None


def read_lines(path: Path) -> list[str]:
    return path.read_text().splitlines() if path.exists() else []


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
        "command_metrics": parse_command_metrics(run_dir / "claude.jsonl", args.anchor_bin),
        "execution_spec": parse_execution_spec_metrics(run_dir / "claude.jsonl"),
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
    ensure_docker_ready()

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
    try:
        raise SystemExit(main())
    except RuntimeError as exc:
        raise SystemExit(str(exc))

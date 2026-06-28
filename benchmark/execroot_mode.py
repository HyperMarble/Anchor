from __future__ import annotations

import os
import shutil
import subprocess
import json
from pathlib import Path
from typing import Any, Callable

from execroot_gate import execroot_acceptance_gate


Progress = Callable[[str], None]

SKIP_DIRS = {
    ".anchor",
    ".cache",
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
}

def prepare_execroot(
    repo_dir: Path,
    run_dir: Path,
    progress: Progress | None = None,
    *,
    instruction: str | None = None,
    anchor_bin: Path | None = None,
) -> Path:
    execroot = run_dir / "execroot"
    if execroot.exists():
        shutil.rmtree(execroot)
    _copy_repo(repo_dir, execroot)
    _init_git_base(execroot)
    if instruction and anchor_bin:
        _prepare_anchor_action(execroot, instruction, anchor_bin, progress)
    if progress:
        progress(f"prepared execroot {execroot}")
    return execroot


def apply_execroot_patch(
    execroot: Path,
    repo_dir: Path,
    logs_dir: Path,
    progress: Progress | None = None,
    *,
    agent_log_path: Path | None = None,
    anchor_bin: Path | None = None,
) -> dict[str, Any]:
    logs_dir.mkdir(parents=True, exist_ok=True)
    changed_paths = _changed_paths(execroot)
    gate = execroot_acceptance_gate(execroot, changed_paths, agent_log_path, anchor_bin)
    _clean_internal_outputs(execroot)

    patch = _patch_bytes(execroot)
    patch_path = logs_dir / "execroot.patch"
    patch_path.write_bytes(patch)

    result: dict[str, Any] = {
        "execroot": str(execroot),
        "patch_path": str(patch_path),
        "patch_bytes": len(patch),
        "changed_paths": changed_paths,
        "apply_exit_code": None,
        "accepted": gate["accepted"],
        "gate": gate,
    }
    if not patch.strip():
        return result
    if not gate["accepted"]:
        (logs_dir / "execroot-rejected.json").write_text(_json_dump(gate))
        return result

    if progress:
        progress(f"applying execroot patch with {len(changed_paths)} changed file(s)")
    proc = subprocess.run(
        ["git", "apply", "--whitespace=nowarn", str(patch_path)],
        cwd=repo_dir,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    (logs_dir / "execroot-apply.stdout").write_text(proc.stdout)
    (logs_dir / "execroot-apply.stderr").write_text(proc.stderr)
    result["apply_exit_code"] = proc.returncode
    if proc.returncode != 0:
        raise RuntimeError(f"execroot patch apply failed: {proc.stderr.strip()}")
    return result


def _copy_repo(source: Path, dest: Path) -> None:
    shutil.copytree(source, dest, symlinks=True, ignore=_ignore_names)


def _prepare_anchor_action(
    execroot: Path,
    instruction: str,
    anchor_bin: Path,
    progress: Progress | None,
) -> None:
    action_dir = execroot / ".anchor" / "action"
    action_dir.mkdir(parents=True, exist_ok=True)
    (action_dir / "instruction.md").write_text(instruction)
    (action_dir / "spec-query.md").write_text(_spec_query_markdown())
    (execroot / "ANCHOR_ACTION.md").write_text(_action_markdown(instruction))
    if progress:
        progress("prepared Anchor action packet")


def _action_markdown(instruction: str) -> str:
    return f"""# Anchor Action Workspace

The repository is an Anchor action workspace. Source edits are provisional until
Anchor accepts the patch.

## Task

{instruction.strip()}

## Required Work Shape

1. Write the required `ExecutionSpec` before any code search/read/edit.
2. Build the first Anchor semantic contract from the spec fields, not from the raw prompt.
3. Materialize it with `anchor semantic "<spec-derived contract>" --limit 8 --context-limit 6`.
4. Inspect `.anchor/semantic/current/index.md`, owner files, and verification files before source edits.
5. Make the smallest source patch that satisfies the spec.
6. Run focused runtime verification before finishing.

Patches that change source code without Anchor semantic/read provenance are rejected.
"""


def _spec_query_markdown() -> str:
    return """# Spec-Derived Query

Use the ExecutionSpec as the semantic workspace contract.

Build the first Anchor semantic contract from these fields:

- goal
- expected_behavior
- required_edges
- search_terms
- likely_files
- verification_requirements

Do not start code navigation from the raw task prompt when the spec is available.
"""


def _ignore_names(_dir: str, names: list[str]) -> set[str]:
    return {name for name in names if name in SKIP_DIRS}


def _init_git_base(execroot: Path) -> None:
    _run(["git", "init", "-q"], execroot)
    _run(["git", "config", "user.email", "anchor-execroot@example.invalid"], execroot)
    _run(["git", "config", "user.name", "Anchor Execroot"], execroot)
    _run(["git", "add", "-A"], execroot)
    _run(["git", "commit", "-q", "--allow-empty", "-m", "anchor execroot base"], execroot)


def _changed_paths(execroot: Path) -> list[str]:
    _run(["git", "add", "-N", "."], execroot)
    output = _run(["git", "diff", "--name-only"], execroot)
    return sorted(
        path
        for path in output.stdout.splitlines()
        if path and path != "ANCHOR_ACTION.md" and not path.startswith(".anchor/")
    )


def _patch_bytes(execroot: Path) -> bytes:
    _run(["git", "add", "-N", "."], execroot)
    return _run(
        ["git", "diff", "--binary", "--", ".", ":(exclude)ANCHOR_ACTION.md", ":(exclude).anchor"],
        execroot,
    ).stdout.encode()


def _clean_internal_outputs(root: Path) -> None:
    (root / "ANCHOR_ACTION.md").unlink(missing_ok=True)
    for current, dirs, files in os.walk(root):
        dirs[:] = [name for name in dirs if name != ".git"]
        for name in list(dirs):
            if name in SKIP_DIRS:
                shutil.rmtree(Path(current) / name, ignore_errors=True)
                dirs.remove(name)
        for name in files:
            path = Path(current) / name
            if path.suffix in {".pyc", ".pyo"}:
                path.unlink(missing_ok=True)


def _json_dump(value: dict[str, Any]) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def _run(cmd: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(cmd, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if proc.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed: {proc.stderr.strip()}")
    return proc

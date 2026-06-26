from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path
from typing import Any, Callable


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


def prepare_execroot(repo_dir: Path, run_dir: Path, progress: Progress | None = None) -> Path:
    execroot = run_dir / "execroot"
    if execroot.exists():
        shutil.rmtree(execroot)
    _copy_repo(repo_dir, execroot)
    _init_git_base(execroot)
    if progress:
        progress(f"prepared execroot {execroot}")
    return execroot


def apply_execroot_patch(
    execroot: Path,
    repo_dir: Path,
    logs_dir: Path,
    progress: Progress | None = None,
) -> dict[str, Any]:
    logs_dir.mkdir(parents=True, exist_ok=True)
    _clean_internal_outputs(execroot)

    changed_paths = _changed_paths(execroot)
    patch = _patch_bytes(execroot)
    patch_path = logs_dir / "execroot.patch"
    patch_path.write_bytes(patch)

    result: dict[str, Any] = {
        "execroot": str(execroot),
        "patch_path": str(patch_path),
        "patch_bytes": len(patch),
        "changed_paths": changed_paths,
        "apply_exit_code": None,
    }
    if not patch.strip():
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
    return sorted(path for path in output.stdout.splitlines() if path)


def _patch_bytes(execroot: Path) -> bytes:
    _run(["git", "add", "-N", "."], execroot)
    return _run(["git", "diff", "--binary"], execroot).stdout.encode()


def _clean_internal_outputs(root: Path) -> None:
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


def _run(cmd: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(cmd, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if proc.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed: {proc.stderr.strip()}")
    return proc

#!/usr/bin/env python3
"""Replay real git commits as Anchor task intakes and score the result.

For each sampled commit: check out the parent in a worktree, run
`anchor task "<commit subject>"`, then measure against ground truth (the
files the commit actually changed):

  efficiency  = 1 - (anchor task output bytes / bytes of changed source files)
                (conservative: real agents read more than just the changed
                files, so true savings are at least this)
  quality     = recall: fraction of changed source files that appear in the
                saved task workspace (active paths, slices, related, tests)

Usage: bench_workspace_replay.py <anchor-binary> <repo> [repo...]
"""

import json
import random
import subprocess
import sys
import tempfile
from pathlib import Path

COMMITS_PER_REPO = 8
MIN_SUBJECT_LEN = 20
MAX_CHANGED_SOURCE_FILES = 6
SOURCE_SUFFIXES = (
    ".py", ".rs", ".go", ".ts", ".tsx", ".js", ".jsx", ".java", ".rb", ".cs",
)
SKIP_SUBJECT_PREFIXES = ("merge", "wip", "chore", "bump", "release", "revert")


def run(cmd, cwd=None, timeout=180):
    return subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout
    )


def changed_source_files(repo, commit):
    diff = run(
        ["git", "diff", "--name-status", f"{commit}~1", commit], cwd=repo
    ).stdout
    changed = []
    for line in diff.splitlines():
        parts = line.split("\t")
        if len(parts) < 2:
            continue
        status, path = parts[0], parts[-1]
        # Only modified files count: added files cannot be "found" at the
        # parent commit, deleted files have no after-state to edit.
        if status.startswith("M") and path.endswith(SOURCE_SUFFIXES):
            changed.append(path)
    return changed


def sample_commits(repo):
    log = run(
        ["git", "log", "--no-merges", "--format=%H\t%s", "-400"], cwd=repo
    ).stdout
    candidates = []
    for line in log.splitlines():
        sha, _, subject = line.partition("\t")
        lowered = subject.lower()
        if len(sub
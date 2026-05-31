#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:-python-statemachine-state-data-scoping}"
MODE="${ANCHOR_HOOK_MODE:-warn}"
ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
DEEPSWE_ROOT="${DEEPSWE_ROOT:-/Volumes/Hak_SSD/deep-swe}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
JOBS_DIR="$WORK_ROOT/pier-jobs"
TRACE_DIR="$WORK_ROOT/traces/$TASK_ID-pier-anchor-claude-$(date +%Y%m%d-%H%M%S)"
TASK_DIR="$DEEPSWE_ROOT/tasks/$TASK_ID"
ANCHOR_BIN="${ANCHOR_LINUX_BIN:-$WORK_ROOT/bin/anchor-linux}"
JOB_NAME="anchor-pier-claude-${TASK_ID}-$(date +%Y%m%d-%H%M%S)"
HARNESS_DIR="$ANCHOR_ROOT/benchmark/claude"
PROMPT_TEMPLATE="$HARNESS_DIR/CLAUDE.anchor.pier.md"

if [[ ! -d "$TASK_DIR" ]]; then
  echo "missing DeepSWE task: $TASK_DIR" >&2
  exit 2
fi

if [[ ! -x "$ANCHOR_BIN" ]]; then
  echo "missing Linux Anchor binary: $ANCHOR_BIN" >&2
  echo "Build it first: benchmark/build_anchor_linux.sh" >&2
  exit 2
fi

if ! command -v pier >/dev/null 2>&1; then
  echo "pier CLI is required but was not found" >&2
  echo "Install it with: uv tool install datacurve-pier" >&2
  exit 2
fi

docker context use colima >/dev/null
if ! docker info >/dev/null 2>&1; then
  echo "Docker cannot reach the Colima daemon." >&2
  echo "Start Colima with: COLIMA_HOME=/Volumes/Hak_SSD/colima colima start" >&2
  exit 3
fi

mkdir -p "$JOBS_DIR" "$TRACE_DIR"

MOUNTS_JSON=$(python3 - "$ANCHOR_BIN" "$HARNESS_DIR" "$TRACE_DIR" <<'PY'
import json
import sys
from pathlib import Path

anchor_bin, harness_dir, trace_dir = sys.argv[1:]
mounts = [
    f"{anchor_bin}:/usr/local/bin/anchor:ro",
    f"{harness_dir}:/anchor-harness:ro",
    f"{trace_dir}:/anchor-traces",
]
claude_json = Path.home() / ".claude.json"
if claude_json.exists():
    mounts.append(f"{claude_json}:/root/.claude.json:ro")
print(json.dumps(mounts))
PY
)

pier run \
  --path "$TASK_DIR" \
  --env docker \
  --agent claude-code \
  --agent-kwarg "prompt_template_path=$PROMPT_TEMPLATE" \
  --agent-env "ANCHOR_HOOK_MODE=$MODE" \
  --agent-env "ANCHOR_TRACE_DIR=/anchor-traces" \
  --jobs-dir "$JOBS_DIR" \
  --job-name "$JOB_NAME" \
  --n-concurrent 1 \
  --n-attempts 1 \
  --mounts-json "$MOUNTS_JSON" \
  --yes

echo "job=$JOBS_DIR/$JOB_NAME"
echo "trace=$TRACE_DIR/claude-anchor-tools.jsonl"
echo "artifacts=$JOBS_DIR/$JOB_NAME"

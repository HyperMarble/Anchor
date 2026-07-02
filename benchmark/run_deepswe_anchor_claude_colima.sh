#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:-python-statemachine-state-data-scoping}"
MODE="${ANCHOR_HOOK_MODE:-warn}"
ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
DEEPSWE_ROOT="${DEEPSWE_ROOT:-/Volumes/Hak_SSD/deep-swe}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
JOBS_DIR="$WORK_ROOT/harbor-jobs"
TRACE_DIR="$WORK_ROOT/traces/$TASK_ID-anchor-claude-$(date +%Y%m%d-%H%M%S)"
ARTIFACT_DIR="$WORK_ROOT/artifacts/$TASK_ID-anchor-claude-$(date +%Y%m%d-%H%M%S)"
TASK_DIR="$DEEPSWE_ROOT/tasks/$TASK_ID"
ANCHOR_BIN="${ANCHOR_LINUX_BIN:-$WORK_ROOT/bin/anchor-linux}"
JOB_NAME="anchor-claude-${TASK_ID}-$(date +%Y%m%d-%H%M%S)"
HARNESS_DIR="$ANCHOR_ROOT/benchmark/claude"

if [[ ! -d "$TASK_DIR" ]]; then
  echo "missing DeepSWE task: $TASK_DIR" >&2
  exit 2
fi

if [[ ! -x "$ANCHOR_BIN" ]]; then
  echo "missing Linux Anchor binary: $ANCHOR_BIN" >&2
  echo "Build it first: benchmark/build_anchor_linux.sh" >&2
  exit 2
fi

if ! command -v harbor >/dev/null 2>&1; then
  echo "harbor CLI is required but was not found" >&2
  exit 2
fi

docker context use colima >/dev/null
if ! docker info >/dev/null 2>&1; then
  echo "Docker cannot reach the Colima daemon." >&2
  echo "Start Colima with: COLIMA_HOME=/Volumes/Hak_SSD/colima colima start" >&2
  exit 3
fi

mkdir -p "$JOBS_DIR" "$TRACE_DIR" "$ARTIFACT_DIR"

MOUNTS_JSON=$(cat <<JSON
[
  "$ANCHOR_BIN:/usr/local/bin/anchor:ro",
  "$HARNESS_DIR:/anchor-harness:ro",
  "$HARNESS_DIR/CLAUDE.anchor.md:/workspace/CLAUDE.md:ro",
  "$HARNESS_DIR:/workspace/.claude:ro",
  "$TRACE_DIR:/anchor-traces",
  "$ARTIFACT_DIR:/anchor-artifacts"
]
JSON
)

harbor run \
  --path "$TASK_DIR" \
  --env docker \
  --agent claude-code \
  --jobs-dir "$JOBS_DIR" \
  --job-name "$JOB_NAME" \
  --n-concurrent 1 \
  --n-attempts 1 \
  --mounts-json "$MOUNTS_JSON" \
  --agent-env "ANCHOR_HOOK_MODE=$MODE" \
  --agent-env "ANCHOR_TRACE_DIR=/anchor-traces" \
  --yes

echo "job=$JOBS_DIR/$JOB_NAME"
echo "trace=$TRACE_DIR/claude-anchor-tools.jsonl"
echo "artifacts=$ARTIFACT_DIR"

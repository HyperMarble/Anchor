#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:-python-statemachine-state-data-scoping}"
ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
DEEPSWE_ROOT="${DEEPSWE_ROOT:-/Volumes/Hak_SSD/deep-swe}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
JOBS_DIR="$WORK_ROOT/harbor-jobs"
TASK_DIR="$DEEPSWE_ROOT/tasks/$TASK_ID"
JOB_NAME="baseline-claude-${TASK_ID}-$(date +%Y%m%d-%H%M%S)"

if [[ ! -d "$TASK_DIR" ]]; then
  echo "missing DeepSWE task: $TASK_DIR" >&2
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

mkdir -p "$JOBS_DIR"
cd "$ANCHOR_ROOT"

harbor run \
  --path "$TASK_DIR" \
  --env docker \
  --agent claude-code \
  --jobs-dir "$JOBS_DIR" \
  --job-name "$JOB_NAME" \
  --n-concurrent 1 \
  --n-attempts 1 \
  --yes

echo "job=$JOBS_DIR/$JOB_NAME"

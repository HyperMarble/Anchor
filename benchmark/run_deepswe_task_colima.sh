#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:-python-statemachine-state-data-scoping}"
ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
DEEPSWE_ROOT="${DEEPSWE_ROOT:-/Volumes/Hak_SSD/deep-swe}"
COLIMA_HOME="${COLIMA_HOME:-/Volumes/Hak_SSD/colima}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
JOBS_DIR="$WORK_ROOT/harbor-jobs"
TASK_DIR="$DEEPSWE_ROOT/tasks/$TASK_ID"
JOB_NAME="anchor-deepswe-${TASK_ID}-$(date +%Y%m%d-%H%M%S)"

if [[ ! -d "$TASK_DIR" ]]; then
  echo "missing DeepSWE task: $TASK_DIR" >&2
  exit 2
fi

if ! command -v harbor >/dev/null 2>&1; then
  echo "harbor CLI is required but was not found" >&2
  exit 2
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker CLI is required because Colima exposes a Docker-compatible backend" >&2
  exit 2
fi

if ! command -v colima >/dev/null 2>&1; then
  echo "colima is required but was not found" >&2
  exit 2
fi

mkdir -p "$JOBS_DIR"

export COLIMA_HOME
docker context use colima >/dev/null

if ! docker info >/dev/null 2>&1; then
  echo "Docker cannot reach the Colima daemon." >&2
  echo "Current Docker context: $(docker context show 2>/dev/null || true)" >&2
  echo "Expected SSD-backed COLIMA_HOME=$COLIMA_HOME" >&2
  exit 3
fi

if ! colima status >/dev/null 2>&1; then
  echo "warning: colima status failed, but docker info works on context $(docker context show)" >&2
fi

cd "$ANCHOR_ROOT"

harbor run \
  --path "$TASK_DIR" \
  --env docker \
  --agent oracle \
  --jobs-dir "$JOBS_DIR" \
  --job-name "$JOB_NAME" \
  --n-concurrent 1 \
  --n-attempts 1 \
  --yes

echo "$JOBS_DIR/$JOB_NAME"

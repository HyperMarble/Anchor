#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:-python-statemachine-state-data-scoping}"
DEEPSWE_ROOT="${DEEPSWE_ROOT:-/Volumes/Hak_SSD/deep-swe}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
JOBS_DIR="$WORK_ROOT/pier-jobs"
TASK_DIR="$DEEPSWE_ROOT/tasks/$TASK_ID"
JOB_NAME="baseline-pier-claude-${TASK_ID}-$(date +%Y%m%d-%H%M%S)"

if [[ ! -d "$TASK_DIR" ]]; then
  echo "missing DeepSWE task: $TASK_DIR" >&2
  exit 2
fi

if ! command -v pier >/dev/null 2>&1; then
  echo "pier CLI is required but was not found" >&2
  echo "Install it with: uv tool install datacurve-pier" >&2
  exit 2
fi

if [[ -z "${ANTHROPIC_API_KEY:-}" && -z "${ANTHROPIC_AUTH_TOKEN:-}" && -z "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]]; then
  echo "Claude Code needs container-visible auth for Pier." >&2
  echo "Export one of: ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, CLAUDE_CODE_OAUTH_TOKEN." >&2
  echo "Local desktop login/keychain auth is not visible inside the DeepSWE container." >&2
  exit 2
fi

docker context use colima >/dev/null
if ! docker info >/dev/null 2>&1; then
  echo "Docker cannot reach the Colima daemon." >&2
  echo "Start Colima with: COLIMA_HOME=/Volumes/Hak_SSD/colima colima start" >&2
  exit 3
fi

mkdir -p "$JOBS_DIR"

MOUNTS_JSON=$(python3 - <<'PY'
import json
from pathlib import Path

mounts = []
claude_json = Path.home() / ".claude.json"
if claude_json.exists():
    mounts.append(f"{claude_json}:/root/.claude.json:ro")
    mounts.append(f"{claude_json}:/logs/agent/sessions/.claude.json:ro")
print(json.dumps(mounts))
PY
)

pier run \
  --path "$TASK_DIR" \
  --env docker \
  --agent claude-code \
  --jobs-dir "$JOBS_DIR" \
  --job-name "$JOB_NAME" \
  --n-concurrent 1 \
  --n-attempts 1 \
  --mounts-json "$MOUNTS_JSON" \
  ${ANTHROPIC_API_KEY:+--agent-env "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY"} \
  ${ANTHROPIC_AUTH_TOKEN:+--agent-env "ANTHROPIC_AUTH_TOKEN=$ANTHROPIC_AUTH_TOKEN"} \
  ${CLAUDE_CODE_OAUTH_TOKEN:+--agent-env "CLAUDE_CODE_OAUTH_TOKEN=$CLAUDE_CODE_OAUTH_TOKEN"} \
  --yes

echo "job=$JOBS_DIR/$JOB_NAME"

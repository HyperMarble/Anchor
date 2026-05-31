#!/usr/bin/env bash
set -euo pipefail

ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
WORK_DIR="$WORK_ROOT/current-anchor-probe"
REPO_DIR="$WORK_DIR/repo"
RESULT_DIR="$ANCHOR_ROOT/benchmark/results"
RESULT_FILE="$RESULT_DIR/current_anchor_probe.json"
ANCHOR_BIN="${ANCHOR_BIN:-$ANCHOR_ROOT/target/debug/anchor}"

mkdir -p "$WORK_ROOT" "$RESULT_DIR"
rm -rf "$WORK_DIR"
mkdir -p "$REPO_DIR"

cd "$ANCHOR_ROOT"
cargo build --quiet --bin anchor

cat > "$REPO_DIR/app.py" <<'PY'
def payment_lock(user_id):
    return f"lock:{user_id}"


def refund_payment(user_id, amount):
    lock = payment_lock(user_id)
    if amount <= 0:
        raise ValueError("amount must be positive")
    return {"user_id": user_id, "amount": amount, "lock": lock}


def invoice_status(invoice):
    if invoice.get("paid"):
        return "paid"
    return "pending"
PY

git -C "$REPO_DIR" init -q
git -C "$REPO_DIR" config user.email "anchor-benchmark@example.invalid"
git -C "$REPO_DIR" config user.name "Anchor Benchmark"
git -C "$REPO_DIR" add app.py
git -C "$REPO_DIR" commit -q -m "initial probe repo"

"$ANCHOR_BIN" -r "$REPO_DIR" build > "$WORK_DIR/build.out"
"$ANCHOR_BIN" -r "$REPO_DIR" context refund_payment --limit 1 > "$WORK_DIR/context_1.out"
"$ANCHOR_BIN" -r "$REPO_DIR" context refund_payment --limit 1 > "$WORK_DIR/context_2.out"

NEW_SYMBOL='def refund_payment(user_id, amount):
    lock = payment_lock(user_id)
    if amount <= 0:
        raise ValueError("amount must be positive")
    if amount > 10_000:
        raise ValueError("manual review required")
    return {"user_id": user_id, "amount": amount, "lock": lock, "reviewed": amount > 1000}'

"$ANCHOR_BIN" -r "$REPO_DIR" edit app.py --symbol refund_payment --content "$NEW_SYMBOL" > "$WORK_DIR/edit.out"
"$ANCHOR_BIN" -r "$REPO_DIR" context refund_payment --limit 1 > "$WORK_DIR/context_after_edit.out"
"$ANCHOR_BIN" -r "$REPO_DIR" check -- sh -c 'test -f app.py' > "$WORK_DIR/check.out"
"$ANCHOR_BIN" -r "$REPO_DIR" status > "$WORK_DIR/status.out"

raw_bytes="$(wc -c < "$REPO_DIR/app.py" | tr -d ' ')"
context_bytes="$(wc -c < "$WORK_DIR/context_1.out" | tr -d ' ')"
context_after_bytes="$(wc -c < "$WORK_DIR/context_after_edit.out" | tr -d ' ')"
status_bytes="$(wc -c < "$WORK_DIR/status.out" | tr -d ' ')"
diff_lines="$(git -C "$REPO_DIR" diff -- app.py | wc -l | tr -d ' ')"
changed_files="$(git -C "$REPO_DIR" diff --name-only | wc -l | tr -d ' ')"
event_count="$(grep -c '^' "$REPO_DIR/.anchor/events/events.jsonl" 2>/dev/null || true)"
manual_review_visible="false"
if grep -q "manual review required" "$WORK_DIR/context_after_edit.out"; then
  manual_review_visible="true"
fi
status_context_used="false"
if grep -q 'name="context_used" status="ok"' "$WORK_DIR/status.out"; then
  status_context_used="true"
fi
status_edits_applied="false"
if grep -q 'name="edits_applied" status="ok"' "$WORK_DIR/status.out"; then
  status_edits_applied="true"
fi

cat > "$RESULT_FILE" <<JSON
{
  "benchmark": "current_anchor_probe",
  "repo": "$REPO_DIR",
  "anchor_bin": "$ANCHOR_BIN",
  "features_checked": [
    "build",
    "context",
    "repeat_context",
    "symbol_edit",
    "auto_reindex_after_edit",
    "execution_events",
    "check_events",
    "status_signals"
  ],
  "raw_file_bytes_after_edit": $raw_bytes,
  "context_bytes_before_edit": $context_bytes,
  "context_bytes_after_edit": $context_after_bytes,
  "status_bytes": $status_bytes,
  "changed_files": $changed_files,
  "git_diff_lines": $diff_lines,
  "event_count": $event_count,
  "edited_symbol_visible_after_reindex": $manual_review_visible,
  "status_context_used": $status_context_used,
  "status_edits_applied": $status_edits_applied
}
JSON

cat "$RESULT_FILE"

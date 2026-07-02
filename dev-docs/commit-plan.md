# Anchor Commit Plan

This file is a staging guide for future commits.

Do not treat the current working tree as one large commit. Each feature slice
should be committed separately with its regression tests.

## Commit Rules

- One feature per commit.
- Regression tests go in the same commit as the feature they prove.
- Docs for that feature go in the same commit only when they explain the changed behavior.
- Benchmark harness changes should be committed separately from product behavior.
- R&D notes, especially Zev hypotheses, should be committed separately from Anchor runtime code.
- Do not mix cleanup, benchmark wiring, docs, and product logic in one commit.

## Planned Commit Slices

### 1. Task Intake and Auto Indexing

Feature:

- Add `anchor task <intent>`.
- Auto-build indexes before `task`, `context`, `search`, and `map` when missing.
- Include ranked symbols, code slices, related files, likely tests, and historical hints.

Regression tests:

- `tests/test_cli_build.rs`
  - `cli_search_auto_builds_once_before_reads`
  - `cli_task_intake_auto_builds_and_records_one_intake_event`
  - `cli_context_truncates_large_default_symbol_output`

Suggested commit:

```text
feat(cli): add task intake and automatic indexing
```

### 2. Git-Native Behavioral Index

Feature:

- Build `.anchor/index/history.json` from local Git history.
- Track co-changed files, path history, recency scores, and top-24 adjacency.
- Use historical related files/tests in `anchor task`.

Regression tests:

- `tests/test_cli_build.rs`
  - `cli_task_intake_uses_git_history_for_related_tests`

Suggested commit:

```text
feat(history): add git-native behavioral index
```

### 3. Binary/Text Indexing Guard

Feature:

- Skip binary/media/non-text paths before UTF-8 reads during indexing.

Regression tests:

- `tests/test_cli_build.rs`
  - `cli_build_skips_binary_assets_before_utf8_read`

Suggested commit:

```text
fix(index): skip non-text assets before parsing
```

### 4. Compact Write Receipts

Feature:

- Write/edit outputs use hashes and metadata instead of echoing full old/new content.
- Include before/after hashes, content hashes, line counts, byte counts, replacements.

Regression tests:

- `tests/test_cli_auto_reindex.rs`
- `tests/test_cli_symbol_edit.rs`

Suggested commit:

```text
feat(write): emit compact hash-based write receipts
```

### 5. Changed-Range Write Summaries

Feature:

- Single-file write receipts include compact changed line spans.

Regression tests:

- `src/cli/write.rs`
  - `regression_line_change_summary_detects_middle_replace`
  - `regression_line_change_summary_detects_insert`
- `tests/test_cli_write_guard.rs`
  - changed-range receipt assertions

Suggested commit:

```text
feat(write): include changed line ranges in receipts
```

### 6. Explicit Stale Write Guard

Feature:

- Add `--expect-hash` to `anchor write` and `anchor edit`.
- Block stale writes before mutation when the current file hash does not match.
- Emit stale-file receipt and guard event.

Regression tests:

- `tests/test_cli_write_guard.rs`
  - `cli_edit_expect_hash_blocks_stale_file_without_mutating`
  - `cli_edit_expect_hash_allows_matching_file`
  - `cli_write_expect_hash_missing_allows_new_file`

Suggested commit:

```text
feat(write): block stale writes with expected file hashes
```

### 7. Automatic Stale Write Guard From Context Provenance

Feature:

- Store `source_hash` and `slice_hash` on `context.read` events.
- Print file hashes in context/task output.
- Use the latest same-session/same-agent context hash automatically when writing without `--expect-hash`.

Regression tests:

- `tests/test_cli_context_event_log.rs`
  - source/slice hash metadata assertions
- `tests/test_cli_write_guard.rs`
  - `cli_edit_without_expect_hash_uses_last_context_read_hash`

Suggested commit:

```text
feat(write): derive stale-write guards from context provenance
```

### 8. Event Log Concurrency Hardening

Feature:

- Keep event log JSONL valid under parallel context reads.

Regression tests:

- `tests/test_cli_context_event_log.rs`
  - `cli_parallel_context_reads_keep_event_log_valid_jsonl`

Suggested commit:

```text
fix(events): serialize concurrent event log appends
```

### 9. Deterministic Quality Kernel

Feature:

- Convert event history into stronger deterministic quality signals.
- Add score, risk, flags, and recommendations.
- Track edited-file-without-prior-context, stale-write blocks, guarded writes,
  changed-line totals, oversized edits, and risky paths.
- Attach structured write metadata for hashes, changed ranges, bytes, lines,
  replacement counts, and expected-hash source.

Regression tests:

- `tests/test_cli_quality_profile.rs`
  - `cli_quality_profile_flags_unverified_edit`
  - `cli_quality_profile_flags_risky_path_without_check`
  - `cli_quality_profile_flags_oversized_edit_scope`
- `tests/test_cli_gate.rs`

Suggested commit:

```text
feat(quality): add deterministic execution quality kernel
```

### 10. Raw Repo-Change Quality Audit

Feature:

- Compare current Git worktree changes against successful Anchor write/edit events.
- Flag changed files that have no Anchor write provenance.
- Surface `unrecorded_repo_changes` in receipt/gate/status.
- Ignore Anchor internal files and common cache artifacts.

Regression tests:

- `tests/test_cli_quality_profile.rs`
  - `cli_quality_profile_flags_raw_repo_change_without_anchor_write_event`
  - `cli_quality_profile_does_not_flag_anchor_recorded_write_as_raw_change`

Suggested commit:

```text
feat(quality): flag repo changes without Anchor write provenance
```

### 11. Terminal Run Wrapper

Feature:

- Add `anchor run -- <cmd>`.
- Record terminal command execution as `terminal.run`.
- Detect files newly changed by the command.
- Fail when the terminal command mutates files outside Anchor-controlled writes.
- Record `terminal.raw_write` events and expose `raw_terminal_write` quality flag.

Regression tests:

- `tests/test_cli_run.rs`
  - `cli_run_blocks_raw_terminal_file_mutation`
  - `cli_run_allows_terminal_command_that_mutates_through_anchor`

Suggested commit:

```text
feat(run): audit terminal commands for raw repo mutations
```

### 12. Search Ranking Regression

Feature:

- Prefer source definitions over docs headings where relevant.
- Preserve token/sub-token search regressions.

Regression tests:

- `tests/test_search_regression.rs`

Suggested commit:

```text
fix(search): prefer source definitions in hybrid ranking
```

### 13. Benchmark Harness Updates

Feature:

- Native DeepSWE pair runners for Claude/Codex/Pi.
- Anchor-mode prompt/hook wiring and command metrics.

Regression tests:

- Python syntax check for benchmark scripts.
- Real benchmark runs are evidence, not unit tests.

Suggested commit:

```text
bench: add native deepswe pair runners
```

### 14. Internal Docs and Vision Updates

Feature:

- Update Anchor framing, engineering state, and internal architecture notes.

Regression tests:

- None required unless docs include executable examples.

Suggested commit:

```text
docs: update anchor execution harness vision
```

### 15. Zev Doctor-Normalized Hypothesis

Feature:

- Record R&D hypothesis:
  `raw source -> doctor normalization -> canonical source -> Zev representation`.

Regression tests:

- None. This is hypothesis documentation only.

Suggested commit:

```text
docs(hypothesis): define doctor-normalized zev layer
```

## Current Verification Snapshot

Latest full verification:

```text
cargo fmt
cargo test --test test_cli_write_guard --test test_cli_context_event_log
cargo test
```

Status: passed.

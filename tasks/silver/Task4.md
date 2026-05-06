# Task 4 — Resolve newly valid calls during incremental file updates

## Status

Good but complex. This has the highest chance of being genuinely difficult, but tests must be exact.

## Core bug

`update_file_incremental` resolves calls only for added or changed symbols. If a newly added callee makes an unchanged caller’s existing call resolvable, the unchanged caller is not reprocessed, so the `Calls` edge is missing.

## Suggested task title

```markdown
# Resolve newly valid calls during incremental file updates
```

## instruction.md

```markdown
# Resolve newly valid calls during incremental file updates

Anchor’s incremental graph update should keep call relationships accurate when a file gains new symbols.

A caller may already contain a call expression whose callee was not present in the graph during an earlier extraction. If a later incremental update adds that callee, the graph should create the now-valid call relationship even if the caller’s code did not change.

Currently, incremental updates can leave that relationship missing because unchanged caller symbols are not revisited for call resolution.

## Expected behavior

After `update_file_incremental`, call relationships should reflect the latest extraction for the file.

If a newly added symbol satisfies a previously unresolved call from an unchanged symbol, the graph should add the appropriate call relationship.

The caller’s `call_lines` should also reflect the resolved call line.

Existing behavior for changed callers should continue to work.

Repeated incremental updates with the same extraction should not create duplicate call relationships.

## Cases that must be handled

A file initially containing a caller with an unresolved call should not report a dependency before the callee exists.

After an incremental update adds the missing callee while leaving the caller unchanged, the caller should report the callee as a dependency.

The added callee should report the unchanged caller as a dependent.

The unchanged caller’s call lines should include the resolved call location.

A changed caller should still have its calls refreshed correctly.

Repeating the same incremental update should not duplicate the same call relationship.

## Constraints

Do not replace incremental update with a full graph rebuild.

Do not change the public API of `update_file_incremental`.

Do not create duplicate `Calls` edges for the same caller/callee relationship.

Do not break existing behavior for removed symbols, changed symbols, imports, or contains relationships.

Do not rely on parser-specific behavior in the test if direct `FileExtractions` setup is clearer and more deterministic.

The behavior should be verified through graph dependency, dependent, search, or stats behavior, not by inspecting private graph internals.
```

## reference_plan.md

```markdown
# Reference plan

## Root cause

`update_file_incremental` tracks only added and changed symbols in `needs_call_resolution`. It then resolves calls only when the caller is in that set. When a callee is newly added, unchanged callers that already contain matching calls are skipped, so now-valid call relationships are not created.

## Intended fix

When new symbols are added, call resolution must also consider unchanged callers from the same updated extraction whose calls may now resolve. The fix should not replace incremental update with full rebuild and should avoid duplicate `Calls` edges.

## Test plan

Construct `FileExtractions` directly for deterministic graph state:

1. Initial extraction contains a caller symbol and a call to a callee name that does not exist yet.
2. Build graph and verify no dependency/called_by relationship exists.
3. Incremental extraction adds the callee symbol while keeping caller code unchanged.
4. Verify dependencies, dependents, and call_lines are updated.
5. Verify changed-callers still refresh correctly.
6. Repeat the same update and verify duplicate call edges are not created.

## Difficulty notes

This is a good harder task because the symptom appears after a two-stage incremental update. The agent must understand the interaction between symbol diffing, symbol indexes, call resolution, and duplicate edge prevention.
```

## Correct test split

### pass_to_pass

These should already pass at the base commit:

```json
[
  "incremental_update_initially_leaves_missing_callee_unresolved",
  "incremental_update_still_refreshes_calls_for_changed_callers"
]
```

### fail_to_pass

These should fail at the base commit:

```json
[
  "incremental_update_resolves_call_when_callee_is_added",
  "incremental_update_reports_unchanged_caller_as_dependent",
  "incremental_update_updates_call_lines_for_unchanged_caller",
  "incremental_update_does_not_duplicate_calls_on_repeat_update"
]
```

## Suggested selected test file

```text
tests/integration_incremental_call_resolution.rs
```

## Testing notes

Use direct `FileExtractions` instead of relying on parser output. This makes the tests deterministic and focused on graph update behavior.

Use public methods:

```text
CodeGraph::new
CodeGraph::build_from_extractions
CodeGraph::update_file_incremental
CodeGraph::dependencies
CodeGraph::dependents
CodeGraph::search
CodeGraph::stats
```

Construct initial state:

```text
file: src/main.rs
symbol: run
calls: run -> helper
callee helper does not exist yet
```

Then incremental state:

```text
same file
symbols: run, helper
calls: run -> helper
run code unchanged
```

Expected after update:

```text
dependencies("run") contains helper
dependents("helper") contains run
search("run")[0].call_lines contains the call line
```

For duplicate check, compare `dependencies("run")` count for `helper` after applying the same incremental extraction twice.

## Risk review

Medium-high risk.

Main risks:

- Duplicate prevention may require extra fix work if current `resolve_call` always adds a new edge.
- Test names must match cargo/parser output exactly.
- Do not put the initial unresolved baseline in `fail_to_pass`; it should pass before and after.

---

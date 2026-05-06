# Task 1 — Exclude relationships attached to removed nodes from graph statistics

## Status

Recommended. This is the strongest task.

## Core bug

`CodeGraph::stats()` counts only live nodes for `file_count`, `symbol_count`, and `total_nodes`, but it reports:

```rust
total_edges: self.graph.edge_count()
```

That counts every stored edge, including edges connected to soft-deleted nodes. After `remove_file`, file and symbol counts drop, but stale `Defines`, `Calls`, or other relationships can still inflate `total_edges`.

## Suggested task title

```markdown
# Exclude relationships attached to removed nodes from graph statistics
```

## instruction.md

```markdown
# Exclude relationships attached to removed nodes from graph statistics

Anchor’s graph statistics should describe the currently active code graph. When files, symbols, or graph nodes are removed, the stats output should not continue to count relationships attached to those removed nodes.

Currently, graph queries can avoid returning removed nodes while the relationship count in stats may still include stored edges connected to inactive nodes. That makes the reported graph size misleading after removal, replacement, or rebuild-style flows.

## Expected behavior

Statistics should count only live graph data.

A relationship should be included in the reported relationship count only when both of its endpoints are still active. If either endpoint has been removed, that relationship should not contribute to the stats result.

The node count and relationship count should stay consistent with what users can actually query from the active graph.

## Cases that must be handled

A fresh graph containing only active nodes and active relationships should continue to count those relationships normally.

A relationship whose source node has been removed should not be counted, even if its destination node is still active.

A relationship whose destination node has been removed should not be counted, even if its source node is still active.

A relationship where both endpoints have been removed should not be counted.

A graph containing a mix of active relationships and stale relationships should count only the relationships where both endpoints are active.

After a remove-and-rebuild style flow, stats should reflect only the current active graph. Old relationships attached to the removed version of a file or symbol should not be counted alongside relationships for the replacement version.

## Constraints

Do not change the public behavior of graph queries that already return active nodes.

Do not require callers to manually clean up stale relationships before requesting stats.

Do not special-case a single relationship type. The behavior should apply consistently to graph relationships.

Do not introduce nondeterministic behavior. Repeated calls to stats on the same graph state should return the same result.

The behavior should be verified through graph behavior, not by checking source text or implementation details.
```

## reference_plan.md

```markdown
# Reference plan

## Root cause

`CodeGraph::stats()` already skips soft-deleted nodes when computing file and symbol counts, but it reports `total_edges` using the raw graph edge count. Soft-deleted nodes are intentionally retained until compaction, so edges connected to those nodes may remain stored even though queries treat the nodes as inactive.

That makes the stats result inconsistent: `total_nodes` may describe the live graph while `total_edges` still describes the physical graph storage.

## Intended fix

Compute `total_edges` by iterating over graph edges and counting only edges whose source and target nodes are both live. Reuse the existing live-node predicate rather than special-casing a particular edge type.

The fix should not remove edges, compact the graph, or change query behavior.

## Test plan

Add behavioral tests that build graphs through the public graph API, remove files/nodes through existing graph operations, and assert on `stats().total_edges`.

The tests should cover:

- active relationships still counted,
- stale edges from removed sources ignored,
- stale edges to removed destinations ignored,
- stale edges where both endpoints are removed ignored,
- mixed live and stale edges counted correctly,
- remove/re-add style flow does not count relationships from the removed version.

## Difficulty notes

The task is fair because the instruction only describes the stats behavior. The agent must inspect the graph’s soft-delete behavior and recognize that the raw edge count is inconsistent with live-node counts.
```

## Correct test split

Use six total tests, but do **not** put all six in `fail_to_pass`.

### pass_to_pass

This baseline should already pass at the base commit:

```json
[
  "stats_counts_active_relationships"
]
```

### fail_to_pass

These should fail at the base commit because raw edge count includes stale edges:

```json
[
  "stats_ignores_relationship_from_removed_source",
  "stats_ignores_relationship_to_removed_destination",
  "stats_ignores_relationship_when_both_endpoints_removed",
  "stats_counts_only_live_relationships_in_mixed_graph",
  "stats_does_not_count_stale_edges_after_rebuild"
]
```

## Suggested selected test file

Prefer adding an integration test file:

```text
tests/integration_graph_stats_removed_edges.rs
```

Or add unit tests inside `src/graph/engine.rs` / `src/graph/query.rs` if the template and runner handle unit tests more easily.

## Testing notes

Use public methods:

```text
CodeGraph::new
CodeGraph::add_file
CodeGraph::add_symbol
CodeGraph::add_edge
CodeGraph::remove_file
CodeGraph::stats
```

Avoid directly mutating `node.removed` from tests unless there is no public way to create the state.

For removed source/destination tests, use two files:

```text
src/a.rs -> caller symbol
src/b.rs -> callee symbol
caller --Calls--> callee
```

Then remove one file and check that `total_edges` counts only relationships whose endpoints remain live.

## Risk review

Low risk.

Main thing to avoid:

```text
Do not put stats_counts_active_relationships in fail_to_pass.
```

That baseline active-edge test should pass at the base commit.

---

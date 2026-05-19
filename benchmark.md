# Anchor Benchmark Plan

Anchor should be tested as an agent execution harness, not only as a code graph.

The core comparison is:

```text
Agent + normal tools
vs
Agent + Anchor
```

The benchmark should answer whether Anchor helps an agent complete real coding work with less context, fewer tool calls, fewer wrong edits, better impact awareness, and safer writes.

## 1. SWE-bench Style Benchmark

Use real issue-fixing tasks from GitHub-style repositories.

Measure:

- solve rate
- test pass rate
- wrong-file edits
- number of tool calls
- tokens used
- time taken
- patch size
- regressions introduced

This is the strongest public proof, but it should not be the only benchmark.

## 2. Real Repository Task Benchmark

Use real OSS repositories and Anchor's own codebase.

Example tasks:

- change an API behavior
- fix a parser bug
- rename an internal type safely
- add a feature touching multiple files
- find impact of a function change
- update tests after a signature change

Compare normal tool usage against Anchor-assisted execution.

## 3. Context Quality Benchmark

Test context retrieval before any writing happens.

Question:

```text
What code does the agent need to edit for this task?
```

Score whether the returned context:

- includes the target file
- includes relevant callers
- includes relevant callees
- includes related tests
- excludes irrelevant files
- explains impact clearly

This matters because Anchor's core promise is giving the agent the right working context, not everything.

## 4. Write Safety Benchmark

Test unsafe or risky edit scenarios directly.

Example scenarios:

- two agents try to edit the same symbol
- a function changes but callers are missed
- an API route changes but frontend callers are missed
- a rename touches duplicate symbol names
- an edit targets generated or vendor code
- a patch changes code while related tests fail

Anchor should block, warn, lock, or produce impact before unsafe writes happen.

## Success Criteria

Anchor is effective if it improves agent execution on real coding tasks by reducing:

- unnecessary context
- unnecessary tool calls
- wrong edits
- missed dependencies
- unsafe concurrent writes
- regressions after changes

SWE-bench can prove issue-solving ability, but Anchor also needs dedicated benchmarks for context quality and write safety.

## Pilot 001: Anchor Task 1

Date: 2026-05-14

Task:

```text
Exclude relationships attached to removed nodes from graph statistics.
```

Source:

```text
tasks/silver/Task1.md
```

This is a SWE-bench-style task on Anchor itself. The issue is that `CodeGraph::stats()` reports live node counts but uses the raw stored edge count, so relationships attached to soft-deleted nodes can still be counted.

### Baseline: Normal Tools

Command:

```bash
rg -n "total_edges|edge_count|pub fn stats|struct GraphStats|remove_file" Anchor/src/graph Anchor/src/query Anchor/tests
```

Result:

- found the target implementation: `src/graph/query.rs`
- found the exact suspicious line: `total_edges: self.graph.edge_count()`
- found related removal logic: `src/graph/mutation.rs`
- found existing tests around `remove_file` and stats in `src/graph/engine.rs`
- runtime: about `0.06s`

Baseline verdict:

```text
Normal tools find the bug quickly for this task.
```

### Anchor Tools

Commands:

```bash
anchor --root Anchor search stats
anchor --root Anchor context stats
anchor --root Anchor search GraphStats
anchor --root Anchor search remove_file
anchor --root Anchor map graph
```

Result:

- `search stats` found `GraphStats`, `anchor_stats`, existing tests, and the real `stats` method.
- `context stats` returned the exact implementation in `src/graph/query.rs`.
- `context stats` exposed the bad line: `total_edges: self.graph.edge_count()`.
- `search remove_file` found the soft-delete path and related tests.
- `map graph` was too broad but still showed the graph module as the right area.
- runtime for search/context commands was about `0.04s` to `0.50s`.

Anchor verdict:

```text
Anchor works for this task and gives a better bundled context than raw grep.
```

It does not automatically solve the task, but it gives the agent the important files and symbols without needing several separate reads:

- `stats`
- `GraphStats`
- `remove_file`
- existing remove/stats tests

### Pilot Finding

For this issue, Anchor is useful but not yet dramatically better than normal tools.

Why:

- the bug is simple and text-searchable;
- normal `rg` finds the exact line immediately;
- Anchor adds value by bundling symbol code, callers, callees, and nearby tests;
- Anchor output still includes some noisy import matches.

This is a valid smoke benchmark, but not a strong proof yet. Stronger tasks should require cross-file impact, duplicate symbols, call relationships, or multi-language parsing, where plain grep is weaker.

## Pilot 002: Official SWE-bench Task 1

Date: 2026-05-14

Task:

```text
astropy__astropy-12907
```

Repository:

```text
https://github.com/astropy/astropy.git
```

Base commit:

```text
d16bfe05a744909de4b27f5875fe0d4ed41ce607
```

Issue:

```text
Modeling's separability_matrix does not compute separability correctly for nested CompoundModels.
```

### Baseline: Normal Tools

Focused command:

```bash
rg -n "def _separable|def separability_matrix|def _cstack|def _coord_matrix|def _arith_oper" astropy/modeling -g '*.py'
```

Result:

```text
astropy/modeling/separable.py:66:def separability_matrix(transform):
astropy/modeling/separable.py:130:def _arith_oper(left, right):
astropy/modeling/separable.py:171:def _coord_matrix(model, pos, noutp):
astropy/modeling/separable.py:219:def _cstack(left, right):
astropy/modeling/separable.py:290:def _separable(transform):
```

Runtime:

```text
real 0.01s
```

Baseline verdict:

```text
Normal rg is very strong for this task.
It finds the correct implementation file and likely helper functions immediately.
```

### Anchor Tools

Index/stat command:

```bash
anchor --root /private/tmp/anchor_swe_astropy_12907 stats
```

Result:

```text
files   994
symbols 29652
edges   106967
real    4.99s
```

Search commands:

```bash
anchor --root /private/tmp/anchor_swe_astropy_12907 search separability_matrix
anchor --root /private/tmp/anchor_swe_astropy_12907 search _separable
anchor --root /private/tmp/anchor_swe_astropy_12907 search _cstack
```

Search result summary:

- `search separability_matrix` found the implementation in `astropy/modeling/separable.py`.
- It also found relevant imports and custom separability tests.
- `search _separable` found `_separable`, `is_separable`, and `test_separable`.
- `search _cstack` found both `_cstack` and `test_cstack`.

Focused context commands:

```bash
anchor --root /private/tmp/anchor_swe_astropy_12907 context separability_matrix
anchor --root /private/tmp/anchor_swe_astropy_12907 context _separable
anchor --root /private/tmp/anchor_swe_astropy_12907 context _cstack
```

Context result summary:

- `context separability_matrix` returned the exact function, callers, and callee `_separable`.
- `context _separable` returned recursive callers/callees and showed the `CompoundModel` branch.
- `context _cstack` returned the helper likely related to `&` composition and linked `test_cstack`.

Typical focused-context runtime:

```text
real 0.27s to 0.29s
```

Anchor verdict:

```text
Anchor works on this official SWE-bench task.
It finds the right implementation symbols and gives useful caller/callee context.
```

### Pilot Finding

For `astropy__astropy-12907`, Anchor is useful but does not beat `rg` on first discovery.

Why:

- the issue names `separability_matrix` directly;
- the implementation lives in a clearly named file, `separable.py`;
- a simple text search reaches the target in `0.01s`;
- Anchor indexing costs about `5s` on this repo;
- after indexing, Anchor gives better bundled context: callers, callees, helper functions, and related tests.

This task proves the Python parser/index/search path works on a real OSS repository, but it is not the strongest proof of Anchor's advantage. Better official tasks should be chosen where the issue statement does not directly name the target function or where impact spans multiple files.

### End-to-End Solve Smoke

Using Anchor context, the likely failing path was:

```text
separability_matrix -> _separable -> _cstack
```

The issue behavior matched `_cstack` handling the right side of an `&` composition when that right side is already a matrix from a nested compound model.

Patch applied in the temporary SWE-bench checkout:

```diff
-        cright[-right.shape[0]:, -right.shape[1]:] = 1
+        cright[-right.shape[0]:, -right.shape[1]:] = right
```

Regression test added:

```text
test_nested_compound_model_separability_matrix
```

Patch size:

```text
2 files changed, 11 insertions(+), 1 deletion(-)
```

Verification:

```text
python3 -m py_compile astropy/modeling/separable.py astropy/modeling/tests/test_separable.py
git diff --check
```

Both passed.

Full test command attempted:

```text
python3 -m pytest astropy/modeling/tests/test_separable.py -q
```

Blocked because the local Python environment does not have `pytest`; direct runtime import is also blocked by missing `erfa`.

```text
pytest: ModuleNotFoundError
erfa: ModuleNotFoundError
```

End-to-end verdict:

```text
Anchor helped identify the implementation path and related test quickly.
The actual fix was small and consistent with the issue symptom.
This is a valid solve smoke, but not a fully scored SWE-bench run until the official test environment is available.
```

### Source Anchor Write/Edit Replay

The previous edit attempt used an installed `anchor` binary, which was stale and behaved differently from source. The source CLI was then updated to expose minimal `write` and `edit` commands wired to the existing write module.

Source verification:

```text
cargo check -q
cargo run --quiet -- --help
```

Both succeeded, and source help now exposes:

```text
write <path> <content>
edit <path> ...
```

The Astropy patch was reset and replayed using source Anchor:

```bash
cargo run --quiet -- edit /private/tmp/anchor_swe_astropy_12907/astropy/modeling/separable.py \
  -a replace \
  -p "cright[-right.shape[0]:, -right.shape[1]:] = 1" \
  -c "cright[-right.shape[0]:, -right.shape[1]:] = right"
```

Result:

```text
status: replaced
replacements: 1
```

Regression test was inserted with source Anchor:

```bash
cargo run --quiet -- edit /private/tmp/anchor_swe_astropy_12907/astropy/modeling/tests/test_separable.py \
  -a insert \
  -p "<test_cstack final assert block>" \
  -c "<test_nested_compound_model_separability_matrix>"
```

Result:

```text
status: inserted
lines: 10
```

Final diff:

```text
2 files changed
1 production-line replacement
1 regression test added
```

Verification:

```text
python3 -m py_compile astropy/modeling/separable.py astropy/modeling/tests/test_separable.py
git diff --check
```

Both passed.

Full pytest remains blocked locally:

```text
/Users/hak/.pyenv/versions/3.11.14/bin/python: No module named pytest
```

Important Anchor finding:

```text
Source CLI edit applies the file change, but does not refresh the graph cache afterward.
```

After inserting the new test, this source command still returned no result:

```bash
cargo run --quiet -- --root /private/tmp/anchor_swe_astropy_12907 search test_nested_compound_model_separability_matrix
```

Output:

```text
<results query="test_nested_compound_model_separability_matrix" count="0"/>
```

This means source Anchor currently has an execution harness gap:

```text
write/edit must update or invalidate/rebuild the workspace index.
```

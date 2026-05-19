# Anchor Development Plan

Current date: Friday, May 15, 2026.

Target: get Anchor to a local MVP by Sunday, May 17, 2026, with Sunday reserved mainly for attack testing, edge cases, and benchmark runs.

## Product Thesis

Anchor is an agent execution harness for real codebases.

It should make coding agents:

- cheaper: less prompt/context waste and fewer repeated discovery calls
- safer: hash freshness, range locks, verified apply, write logs
- sharper: less distracting context, better target isolation, fewer wrong-file edits

Anchor is not trying to replace `rg`, Git, tree-sitter, formatters, or test runners. It should sit above them as a deterministic layer.

```text
excellent local tools -> Anchor harness -> coding agent
```

## Go-Style Engineering Rules

Even though Anchor is Rust, we will use Go-style product engineering:

- small concrete pieces
- boring names
- explicit data flow
- early error returns
- no giant abstraction before the second implementation exists
- no framework-shaped code
- tests around behavior, not architecture diagrams
- one small task, one focused commit
- keep the normal path easy to read

Practical rule:

```text
Build the smallest thing that proves one behavior.
Then test it on real code.
Then promote it into production code.
```

## Current Proof

Validator added:

```text
tests/validata.rs
tests/validata/git_style_projection.rs
```

The validator proves the proposed `.anchor` mechanism in test form:

```text
index file
-> store parse object by content hash
-> build symbol index
-> search symbol
-> create context/edit projection
-> acquire range lock
-> reject conflicting lock
-> apply locked edit
-> verify parser still accepts file
-> refresh index with new hash
```

Real VS Code corpus probe:

```text
repo: /Volumes/Hak_SSD/vscode
area: src/vs/workbench/browser
files seen: 96
symbols tested: 50

avg context reduction:     96.88%
median reduction:          98.44%
p90 reduction:             99.55%
min reduction:             47.85%
max reduction:             99.76%

avg full context bytes:    20069.1
avg projection bytes:        510.76

lock conflicts rejected:   50 / 50
verified after edit:       50 / 50
index hash refreshed:      50 / 50
failures:                  0
```

This proves mechanism, not full agent outcome. The next proof must compare normal agent tool use against Anchor tool use on the same issue.

## What Sharper Means

Sharper cannot mean "the model feels smarter." It needs measurable behavior.

Anchor sharpness means the agent gets the right working context with less irrelevant material and produces fewer wrong actions.

Metrics:

- target inclusion rate: required symbol/range/test is included
- distraction exclusion rate: unrelated symbols/files are excluded
- wrong-file edit rate: agent edits files outside the needed set
- patch success rate: patch applies cleanly first try
- test pass rate: relevant tests pass after edit
- retry count: number of failed edit/apply loops
- context precision: useful bytes divided by total bytes sent
- context recall: required bytes included divided by required bytes

For MVP, use this minimum sharper score:

```text
sharpness_score =
  target_included
  + verified_after_edit
  + relevant_tests_passed
  - wrong_file_edits
  - stale_apply_attempts
  - unnecessary_files_read
```

Do not market "sharper" until we have side-by-side agent runs.

## Competitor Map

### Direct Agent Tools

- Claude Code: terminal agent that reads code, edits files, runs commands, and integrates with dev workflows.
- OpenAI Codex CLI: local terminal coding agent that reads, modifies, and runs code.
- Gemini CLI: open-source terminal AI agent from Google.
- Cursor: IDE agent with codebase indexing and context retrieval.
- Sourcegraph Cody: AI coding assistant built on Sourcegraph search and codebase context.
- Aider: terminal pair-programming agent with repo map and edit formats.
- Continue: open-source assistant with configurable context providers.
- Sweep: coding assistant focused on editor workflows, inline editing, commits, and review.

### Adjacent Infrastructure

- ripgrep: best default raw text search backend.
- Git: content addressing, diffs, changed-file truth, history.
- tree-sitter / ast-grep: syntax-aware parsing and structural search.
- Sourcegraph: enterprise code search and code intelligence.

## Viability Check

Anchor is viable only if it is not "another coding agent."

Anchor wins if it becomes the harness layer that existing agents can use:

```text
Claude Code / Codex / Gemini / Cursor / custom agents
        |
        v
Anchor MCP / CLI
        |
        v
repo-local .anchor store + locks + projections + verification
```

Local users will use Anchor if it saves tokens and prevents mistakes without forcing a new IDE.

Teams will pay only if cloud adds team value that local cannot:

- shared locks across developers and agents
- shared audit log
- policy controls for what agents can edit
- CI verification reports
- repo context cache across machines
- model/tool usage analytics
- security/compliance visibility

The local version should stay useful even without cloud. Cloud should sell coordination, audit, and governance, not basic usefulness.

## MVP Scope

The MVP is not the full CodeGraph.

The MVP is:

```text
.anchor/
  objects/
    parses/
    slices/
    patches/
  index/
    paths.json
    symbols.json
  locks/
    ranges/
  projections/
  writes/
```

Required commands:

```text
anchor search <query>
anchor context <hit-id-or-symbol>
anchor edit <symbol-or-range>
anchor verify
```

Keep command count small. Four commands is enough for MVP.

## Architecture

### Search

Use `rg` for raw text search. Do not replace it.

Anchor adds:

- source hash
- line/range metadata
- symbol match if indexed
- compact result IDs
- upgrade path into `context` or `edit`

### Context

Context creates a projection:

```text
source_path
source_hash
symbol/range
slice_hash
prefix_hash
suffix_hash
text
```

The agent receives the projection, not the whole file unless needed.

### Edit

Edit is different from write.

Write creates a new file.

Edit changes an existing file through a projection:

```text
lock range
validate source hash
apply projection edit
verify parse/test
refresh index
append write log
```

### Locks

Locks are automatic.

Read-read is allowed.

Write-write on the same range is blocked.

Write on different ranges is allowed only if hashes still validate at apply time.

### Update

After every successful edit:

```text
rehash file
store parse object
update path index
update symbol index
append write log
```

### Delete

Delete is not just `rm`.

It must:

```text
lock path
record old hash
delete or tombstone indexes
append write log
verify no stale symbol references
```

Delete can wait until after the first MVP unless needed.

## Friday Plan

Goal: finish design and lock the smallest production path.

Tasks:

- replace stale development plan with current MVP plan
- keep validator proof committed separately
- decide exact `.anchor` store structs
- implement production content hash helper
- implement object path helper
- implement basic `.anchor` root discovery
- keep old CodeGraph path untouched unless blocking

Exit criteria:

- design is written
- first production module compiles
- no large refactor

## Saturday Plan

Goal: build local MVP path.

Tasks:

- implement `.anchor` store module
- implement path index
- implement symbol index from parser extraction
- implement projection creation
- implement range locks
- implement locked apply
- implement write log
- expose four CLI commands

Exit criteria:

- can run on a temp repo
- can run on one VS Code file copy
- validator tests still pass
- focused integration tests pass

## Sunday Plan

Goal: attack test, benchmark, and decide if MVP is real.

Tasks:

- run VS Code corpus projection benchmark
- run edge cases:
  - stale file
  - conflicting lock
  - moved symbol
  - deleted file
  - CRLF file
  - syntax error after edit
  - duplicate symbol names
  - large file skip
  - generated/vendor ignored files
- compare normal tools vs Anchor on one real issue
- record metrics in `benchmark/`

Exit criteria:

- no known data-loss bug
- metrics show context reduction without correctness loss
- MVP commands are documented
- known limitations are written down

## Benchmark Plan

For each task, collect:

```text
mode: normal | anchor
model/tool: Codex | Claude Code | Gemini | manual agent
repo
issue
files_read
bytes_read
projection_bytes
tool_calls
edit_attempts
patch_apply_success
stale_rejections
lock_conflicts
tests_run
tests_passed
wall_time
final_verdict
```

Minimum proof:

```text
Anchor uses less context with same or better correctness.
```

Strong proof:

```text
Anchor uses less context, fewer tool calls, fewer retries, and same or better test pass rate.
```

## Business Model Notes

Local Anchor should be free/open because adoption depends on trust and developer habit.

Paid cloud should be for teams:

- shared lock service
- audit trail
- team dashboards
- hosted context cache
- policy management
- CI reports
- enterprise integrations

Do not depend on an "Arc-style no business model" plan. Build the local product so it is useful alone, then make the cloud version obviously valuable for teams.

## Non-Goals Before MVP

- no full repo CodeGraph rebuild
- no custom replacement for ripgrep
- no LSP fleet
- no semantic/vector search dependency
- no cloud dependency
- no giant agent framework
- no more than four user-facing commands

## Immediate Next Task

Build the production `.anchor` store in the smallest possible pieces:

1. content hash
2. object paths
3. root discovery
4. parse object write/read
5. path index
6. symbol index
7. projection creation
8. range lock
9. locked apply
10. write log

Each item should be a small commit with tests.

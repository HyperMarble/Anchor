# Anchor Engineering State

This document tracks what is done, what is not done, and what changed day by day.

It is for developers working on Anchor. It is not marketing copy.

## Current Positioning

Anchor is an agent execution harness that controls how AI coding agents read, write, coordinate, and verify work inside real software projects.

The primary effects are:

- efficiency
- quality
- safety

## Current Product Goal

Anchor v1 should improve normal agent work across three measurable outcomes:

```text
efficiency = less wasted context and fewer repeated/raw commands
quality    = narrower, better-grounded patches that pass relevant checks
safety     = fewer stale, raw, conflicting, or unrecorded writes
```

The current goal is not to add more features. The goal is to make the existing
execution loop measurable and trustworthy:

```text
task/context -> controlled edit/write -> check/run -> receipt/gate
```

Core metrics:

- `context_reads` and `cache_hits` for efficiency
- `changed_file_scope`, `changed_line_total`, and `max_changed_lines` for patch quality
- `checks_ok`, `checks_failed`, and `risky_paths` for verification quality
- `guarded_writes`, `stale_write_blocks`, `lock_blocks`, `raw_terminal_writes`, and `unrecorded_changed_files` for safety

Important distinction:

```text
total session paths != changed file scope
```

Reading many files can be good. Anchor should not punish useful context
gathering as broad edit scope. Broad scope means the patch changed too many
files, not that the agent inspected enough context.

The current implementation is CLI-first.

Cloud/team mode is part of the final product vision but is not implemented yet.

Zev is a later R&D layer and should not block Anchor v1.

## Current Implemented State

### CLI

Current CLI surface includes:

- `anchor context`
- `anchor search`
- `anchor map`
- `anchor write`
- `anchor edit`

Notes:

- `context` is the most important current command.
- `write` and `edit` are the current write path.
- `search` and `map` may need to be reframed, hidden, or redesigned so Anchor does not look like only a code indexer.

### Context

Implemented:

- symbol-level context retrieval
- callers/callees in context output
- context cache behavior
- line-based context projection

Needs work:

- better output format for agents
- clearer context contract
- better freshness metadata
- stronger measurement against raw file reading
- Git-native behavioral signals from commit history

### Git-Native Behavioral Index

Current status:

- concept accepted
- first local implementation added

Implemented:

- `anchor build` derives `.anchor/index/history.json` from local Git history
- history index records scanned commits, co-changed file pairs, and per-path commit counts
- `anchor task` merges historical co-change evidence with parser-derived symbols and call edges
- task intake now prints historical related files and historically related tests
- task intake supplements lexical symbol search with bounded source-backed candidates
- task intake boosts owner symbols for compound intents such as `state data`
- regression test covers a real temporary Git repo where source and test files co-change
- history index v2 stores recency-weighted scores
- history index v2 precomputes top-24 adjacency lists per path
- `anchor task` reads adjacency directly instead of scanning every co-change edge

Definition:

Git should be used as a behavioral parser of the repository. It can show how the
project actually changed across accepted commits: co-changed files, related
tests, reverted paths, high-churn areas, and patch shapes that solved similar
work.

This is different from a static source graph. A graph can say `A calls B`. The
Git-native behavioral layer should say `A and B usually changed together for
this kind of task, test T usually mattered, and file C was previously reverted
as unrelated`.

Required first version:

- derive co-change file pairs from commit history
- identify likely test files from historical co-change
- connect historical paths to current task intent
- merge historical facts with parser-derived symbol facts in `anchor task`

Still needed:

- store history facts as content-addressed objects, not only JSON index files
- connect historical patches to symbols, not only paths
- detect reverted/unrelated historical paths
- score high-churn/risky files
- use previous Anchor execution attempts alongside Git commit history
- incremental history rebuild keyed by last indexed commit

Constraint:

Git does not fully replace language parsing. It provides the historical behavior
layer. Parser/indexer still provides exact current-code facts such as symbol
ranges, source slices, and scoped write targets.

### Writes / Edits

Implemented:

- CLI write path
- CLI edit path
- symbol edit mode
- auto reindex after CLI writes
- compact write receipts with before/after hashes, content hashes, line counts, and byte counts
- compact write receipts include changed line-range summaries for single-file writes
- replacement outputs no longer echo full old/new content
- optional `--expect-hash` freshness guard on CLI writes/edits
- stale file hash mismatch blocks before mutation and records a guard event
- `context.read` events store source/slice hashes in structured metadata
- writes automatically use the latest same-session/same-agent context hash when `--expect-hash` is not provided

Production analogy:

This write freshness check is optimistic concurrency control for source files.
It matches a real infrastructure pattern:

```text
read version X
write only if still version X
reject stale writes
```

Analogous systems:

- HTTP `ETag` + `If-Match`
- DynamoDB conditional writes / optimistic locking
- Kubernetes `resourceVersion`

Anchor's version:

```text
read file hash X through Anchor
write only if file still has hash X
```

Needs work:

- stricter patch scoping
- clearer failure messages
- transaction recording around edits
- write receipt objects in content-addressed storage

### Locks / Multi-Agent Coordination

Implemented:

- lock manager
- lockd client path
- agent owner IDs through `ANCHOR_AGENT_ID`
- stable default agent IDs for normal CLI workflows, derived from the rooted workspace
- tests for same-symbol conflict
- tests for different-symbol parallel work
- tests for file-level lock blocking

Default identity rule:

```text
same CLI workflow + same --root = same default agent id
different intentional agents = set ANCHOR_AGENT_ID explicitly
```

This matters because Anchor is command-line first. If every `anchor context`,
`anchor edit`, and `anchor check` process gets a fresh generated owner, the event
log looks like dozens of agents touched one task and same-session freshness
checks become noisy.

Needs work:

- production lock daemon story
- lock TTL/heartbeat policy
- lock status CLI
- lock ownership inspection
- stale lock recovery UX

### Stale MCP / CodeGraph Cleanup

Done:

- stale MCP code removed in local work
- old CodeGraph code removed in local work

Needs work:

- ensure docs no longer describe MCP as current interface
- ensure README does not promise removed behavior
- decide whether any old concepts return later under new names

## Not Implemented Yet

### Transaction / Provenance Record

Not implemented yet.

Required first version:

- append event for context read
- append event for lock acquire/release
- append event for write/edit
- append event for check result
- session ID and agent ID on each event
- content hash or patch hash where useful

Purpose:

- make agent actions auditable
- create operational memory for future agents
- support receipts and debugging

### Quality Kernel

Current status:

- first deterministic local implementation added

Implemented:

- quality score, risk, flags, and recommendations in `receipt`, `status`, and `gate`
- detects changed-without-context
- detects edited-file-without-prior-context
- detects changed-without-check
- detects failed checks
- detects execution errors
- detects lock conflicts
- detects stale write blocks
- detects broad file scope
- detects oversized changed ranges from write metadata
- detects risky path edits without checks
- detects Git worktree changes with no matching Anchor write/edit provenance
- detects raw terminal mutations when commands are run through `anchor run`
- write/edit events store structured metadata for hashes, changed ranges, bytes, lines, replacement counts, and expected-hash source

Still needed:

- record repeated failure patterns
- connect checks to exact edited symbols/files
- detect unrelated files using task intent and Git-history relatedness
- expose warnings to future agent sessions before they edit
- make thresholds configurable per repo/team
- add true fail-closed filesystem sandbox/write gateway so raw writes are prevented, not only detected

Purpose:

- improve patch quality
- reduce repeated failed attempts
- help agents choose better context/checks

### SDK Layer

Not implemented yet.

Required first version:

- stable local API or protocol around context/edit/lock/history
- language-neutral command contract
- simple client for external tools

Purpose:

- make Anchor usable by Codex, Claude Code, Cursor, OpenCode, CI, and custom agents
- avoid every integration calling raw CLI commands differently

### Team / Cloud Mode

Not implemented yet.

Required later:

- shared sessions
- shared locks
- managed storage
- team dashboard
- policy configuration
- organization-level history

Purpose:

- paid/team value
- coordination across many users and agents

### Zev

Not implemented as production Anchor feature.

Current role:

- research direction
- compact model-native code representation
- later source-to-Zev and Zev-to-source workflow

Constraint:

- Anchor v1 must work without Zev.

## Current Risks

### Product Framing Risk

Risk:

Anchor may look like just another code indexer if docs over-focus on search/map/context.

Mitigation:

Frame Anchor as execution harness:

- context control
- write/edit control
- coordination
- provenance
- quality
- SDK

### Benchmark Risk

Risk:

Benchmarking too early may measure stale assumptions.

Mitigation:

Clean docs and product contract first, then rebuild benchmarks around the real harness model.

### Infrastructure Risk

Risk:

Anchor is infra, so edge cases matter more than demos.

Mitigation:

Before launch, every core path needs regression tests:

- context
- edit
- reindex
- lock conflict
- stale context
- transaction event
- quality check
- recovery path

## Day Log

### 2026-05-29

Decisions:

- Anchor definition locked:
  `Anchor is an agent execution harness that controls how AI coding agents read, write, coordinate, and verify work inside real software projects.`
- Primary effects locked:
  `efficiency`, `quality`, `safety`
- Efficiency expanded to include reading and writing context, not only finding context.
- Anchor and Zev separated:
  Anchor is the execution harness.
  Zev is later compact model-native code representation R&D.
- Docs split into:
  `anchor-vision.md` for product/technical vision.
  `engineering-state.md` for day-wise implementation tracking.

Cleanup:

- removed current benchmark folder
- removed stale memory/development/product/benchmark docs

### 2026-06-04

Decisions:

- Added Git-native behavioral index to the architecture.
- Framing updated:
  `Git history = accepted work history`
  `Anchor provenance = attempted work history`
- Static source graph is not the moat. It is only a derived working view.
- Anchor should combine:
  `Git-derived behavioral facts + parser-derived code facts + execution provenance + locks and checked writes`.

Implementation note:

- `anchor task` now uses parser/index-derived symbols, calls, paths, slices, and
  likely tests plus Git-history co-change signals.
- History lookup is now optimized through top-K adjacency lists instead of a full
  edge scan during task intake.
- Write output is now compact by default: hashes and metadata instead of full
  replacement content, reducing output tokens after edits.
- Single-file write receipts include changed line spans so future quality checks
  can reason about edit scope without reading the full patch.
- CLI writes/edits now support `--expect-hash`, so an agent can bind a write to
  the exact file hash it read. If the file changed, Anchor fails closed before
  mutation and emits a stale-file receipt.
- Automatic stale protection now works when the agent first reads through
  `anchor context` or `anchor task`: Anchor records the source hash and later
  write/edit calls reuse the latest matching context hash for the same session
  and agent.
- Next R&D step is making the history layer incremental/content-addressed and
  adding execution-provenance failure/success memory, not only accepted Git
  history.

Known current state:

- CLI-first implementation
- symbol context/edit/reindex exists
- basic multi-agent locking tests exist
- transaction/provenance not implemented
- quality kernel not implemented
- SDK not implemented
- cloud/team mode not implemented

Next recommended work:

- rewrite root `anchor-explained.md` from `anchor-vision.md`
- rewrite README short and practical
- decide current CLI command names/surface
- add minimal transaction event log
- add regression test for transaction event recording

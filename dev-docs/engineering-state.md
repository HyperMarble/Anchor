# Anchor Engineering State

This document tracks what is done, what is not done, and what changed day by day.

It is for developers working on Anchor. It is not marketing copy.

## Current Positioning

Anchor is an agent execution harness that controls how AI coding agents read, write, coordinate, and verify work inside real software projects.

The primary effects are:

- efficiency
- quality
- safety

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

### Writes / Edits

Implemented:

- CLI write path
- CLI edit path
- symbol edit mode
- auto reindex after CLI writes

Needs work:

- stricter patch scoping
- clearer failure messages
- transaction recording around edits
- quality checks attached to edits

### Locks / Multi-Agent Coordination

Implemented:

- lock manager
- lockd client path
- agent owner IDs through `ANCHOR_AGENT_ID`
- tests for same-symbol conflict
- tests for different-symbol parallel work
- tests for file-level lock blocking

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

Not implemented yet.

Required first version:

- detect broad/unrelated edits
- record pass/fail outcomes
- record repeated failure patterns
- connect checks to edited symbols/files
- expose warnings to future agent sessions

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


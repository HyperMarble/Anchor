# Anchor Explained

Anchor is an execution harness for AI coding agents.

The final version is not just a local CLI and not just a code index. Anchor is
the control plane between agents and real codebases. It gives agents a governed
way to read code, claim work, write changes, coordinate with other sessions,
produce evidence, and hand results back to humans or teams.

Anchor can run locally for a single developer, and it can run as a cloud/team
service for multiple developers and multiple agent sessions working on the same
repository.

## The Final Shape

The intended product looks like this:

```text
Human / team
    |
    v
Anchor workspace
    |
    |-- code understanding
    |-- context serving
    |-- session ownership
    |-- symbol/file locks
    |-- checked writes
    |-- test gates
    |-- provenance log
    |-- replay / audit / rollback
    |
    v
Agents: Codex, Claude Code, Cursor, OpenCode, custom agents
    |
    v
Official source repository
```

The important idea is delegation. A human should be able to give work to one
agent or many agents without losing control of the repository. Anchor owns the
state around the work: what was read, what was claimed, what changed, what tests
ran, what failed, what passed, and who or what is responsible.

## Why Anchor Exists

AI coding agents can already write code. The hard part is not typing code. The
hard part is letting agents act inside real repositories without chaos.

Real agent work needs answers to questions like:

- Which code did the agent read?
- Was that context stale?
- Which symbol or file did the agent claim?
- Did another agent touch the same area?
- What exactly changed?
- What tests or checks ran?
- Can we replay or inspect the work later?
- Who approved it?
- How do we know this was not a blind overwrite?

Normal Git answers some historical questions after the fact. Anchor sits before
and during the work. It coordinates the execution itself.

## The Core Thesis

Anchor turns agent coding from opaque chat into governed execution.

Instead of:

```text
agent reads random files
agent edits files directly
human checks a diff later
```

Anchor aims for:

```text
agent requests context
Anchor returns focused, hashed context
agent claims a symbol or file
Anchor locks that work unit
agent submits a write
Anchor verifies, applies, reindexes, and records it
tests/checks run as gates
the whole session becomes replayable provenance
```

That is why Anchor should be understood as an execution harness, not only a
navigation tool.

## Local And Cloud

Anchor has two deployment shapes.

### Local Anchor

Local Anchor runs inside one developer's workspace. It gives the local agent a
structured way to inspect and mutate the repo.

Local Anchor is useful for:

- single-developer agent sessions
- local CLI workflows
- fast context lookup
- local symbol locks
- local write safety
- local provenance logs

### Anchor Cloud / Team Anchor

Cloud/team Anchor is the shared coordination layer.

In that mode, multiple humans can run their own agents against the same project,
and Anchor coordinates the work:

```text
Developer A -> agent session A1, A2, A3
Developer B -> agent session B1
CI / review bot -> verification session

All sessions -> shared Anchor coordination state
```

Team Anchor is for:

- shared lock state
- shared session state
- multi-agent work ownership
- cloud-managed workspaces
- team-visible traces
- review/audit artifacts
- controlled handoff from agent output to official repository

The cloud version is not a replacement for GitHub. Git remains the source-control
system. Anchor manages agent execution before changes become commits or pull
requests.

## The Two Moats

Anchor has two strategic moats.

### 1. Governed Agent Execution

Anchor's first moat is controlling the agent work loop.

The loop is:

```text
intent -> context -> claim -> lock -> write -> verify -> record -> handoff
```

This is the transaction kernel for coding agents. Every meaningful action should
be represented as an event with inputs, outputs, hashes, ownership, and status.

The final system should make agent work:

- attributable
- inspectable
- replayable
- resumable
- auditable
- rollback-aware
- safe for parallel sessions

This is Anchor's main product layer.

### 2. Zev Later

Zev is the second moat, but it comes after Anchor.

Anchor first controls how agents operate on a codebase. Zev then changes the
representation agents read and write.

Without Zev:

```text
source code -> Anchor context -> agent writes source edits
```

With Zev:

```text
source code -> Zev representation -> Anchor context -> agent writes Zev edits
-> source code
```

Zev is covered later in this document because Anchor must make sense without it.

## Anchor As A Transaction Kernel

The best analogy is not Git. The better analogy is a database transaction log or
a control plane.

An agent write should not be just "text changed." It should become a controlled
transaction:

```text
begin task
  read context A
  claim symbol B
  acquire lock C
  apply patch D
  run check E
  record result F
commit or abort
```

The final Anchor transaction should record:

- session id
- agent id
- model/tool identity where available
- requested task
- context hashes
- target symbols/files
- locks acquired
- before/after hashes
- patch content
- commands/tests run
- outputs
- final status
- approval or rejection

This gives teams a way to answer: what did the agent do, why did it do it, and
can we trust the result?

## Agent Flight Recorder

The second name for this layer is the agent flight recorder.

Agents fail in ways that are hard to debug. A chat transcript is not enough. A
diff is not enough. A commit is not enough.

Anchor should record the execution path:

```text
context read
search query
symbol projection
lock acquired
write requested
write applied
index refreshed
test command
test output
human approval
handoff
```

This creates a timeline that can be inspected after a bad edit, resumed after an
interrupted session, or attached to a review.

The final product should support:

- `anchor trace <session>`
- `anchor replay <session>`
- `anchor attest <session>`
- cloud session timelines
- signed provenance for important changes

The honest goal is not perfect deterministic replay of model behavior. The goal
is reproducible boundaries: restore the workspace state, replay Anchor-applied
edits, rerun recorded checks where possible, and show exact recorded inputs and
outputs.

## Code Understanding

Anchor needs code understanding because agents should not read whole repositories
blindly.

Anchor indexes the repository into:

- paths
- symbols
- calls
- source hashes
- symbol hashes
- projected slices
- search features

The point is not to build a giant graph for its own sake. The point is to serve
the right code at the right time and to know what a write touches.

The final context system should answer:

- "show me this symbol"
- "show me callers and callees"
- "show me related tests"
- "show me the stale/fresh state"
- "show me what another agent is already editing"
- "show me only what changed since I last read"

## Checked Writes

In Anchor, agents should not treat the file system as a raw scratchpad.

The final write path should be:

```text
agent proposes change
Anchor validates target
Anchor checks source freshness
Anchor checks lock ownership
Anchor applies change
Anchor reindexes affected files
Anchor records the event
Anchor returns structured result
```

This creates a narrow waist between agents and the repo. Different agents can
have different frontends, but the write path remains controlled.

## Multi-Agent Coordination

Anchor is built for more than one agent.

A single developer may run multiple agents. A team may have several developers,
each with their own agent sessions. Without coordination, agents can overwrite
each other or waste work by solving the same task.

Anchor coordination is based on:

- unique agent/session owner IDs
- symbol and file locks
- shared task/session state
- stale-context detection
- write ordering
- conflict reporting

The final team version should make this visible:

```text
agent-a owns src/auth.rs:login
agent-b owns src/billing.rs:create_invoice
agent-c is blocked on src/auth.rs:login
CI session is verifying agent-b output
```

This is one of the places where Anchor becomes more than local tooling. It
becomes the coordination layer for parallel AI labor.

## Git-Like, But Not Git

Anchor uses Git-like thinking:

- content-addressed objects
- hashes for source and slices
- indexes
- append-only event records
- projections
- safe handoff points

But Anchor does not need to copy Git's user model:

- no separate commit history inside Anchor
- no branch/merge replacement
- no trying to be source control

Git stores accepted project history. Anchor manages agent execution before and
around that history.

The relationship is:

```text
Anchor governs agent work
Git records accepted source history
```

## Store Model

The final `.anchor/` store should contain:

```text
.anchor/
  objects/
    contexts/
    slices/
    patches/
    commands/
    test-logs/
  index/
    paths.json
    symbols.json
    calls.json
  sessions/
  locks/
  writes/
  events/
    events.jsonl
  attestations/
```

Content-addressed storage matters because unchanged context, patches, and logs
should not be duplicated. It also makes provenance stronger: each event can point
to immutable content hashes.

## Read Flow

Final read flow:

```text
agent asks for context
Anchor resolves query through index
Anchor checks cache and freshness
Anchor returns focused symbol/context bundle
Anchor records what was shown
```

This gives the agent enough code to reason without forcing it to scan large
files. It also lets Anchor later explain what information the agent had before a
write.

## Write Flow

Final write flow:

```text
agent submits write request
Anchor checks target symbol/file
Anchor checks source hash
Anchor acquires lock
Anchor applies change
Anchor updates index
Anchor records event
Anchor releases or transfers lock
```

In cloud/team mode, this can become a reviewable transaction rather than an
immediate local write:

```text
agent proposed patch -> Anchor verification -> human/team approval -> apply
```

## Verification Flow

Anchor should know what was changed and what should be checked.

Verification can include:

- formatting
- unit tests
- type checks
- targeted tests
- dependency impact checks
- security/policy checks
- full fallback checks when impact is unknown

Anchor should not pretend impact analysis is perfect. The safe rule is:

```text
known impact -> targeted checks
unknown impact -> broader checks
```

## Cloud / Team Flow

Final cloud/team Anchor:

```text
team connects repo
Anchor builds index
developers start agent sessions
sessions claim work
Anchor coordinates locks and context
agents submit writes
Anchor records provenance
checks run
team reviews
accepted changes land in Git
```

The cloud version should make the invisible parts visible:

- active sessions
- claimed symbols/files
- blocked agents
- changed areas
- test status
- risk status
- provenance timeline
- handoff to PR/commit

## Where Zev Fits

Zev is not Anchor itself.

Anchor controls execution. Zev changes the representation inside that execution.

The Zev layer should eventually sit here:

```text
source code
  -> source-to-Zev
  -> Anchor context/session/write flow
  -> Zev-to-source
  -> official source code
```

The intended effect:

- fewer tokens per symbol
- more context in the model window
- cleaner writes from agents
- language-neutral reasoning
- easier training data for smaller coding models

Anchor does not depend on Zev to be useful. Zev makes Anchor stronger.

## Current Implementation Status

The current codebase implements the local CLI foundation:

- `anchor build`
- `anchor search`
- `anchor context`
- `anchor context --bundle`
- `anchor status`
- `anchor write`
- `anchor edit`
- `anchor edit --symbol`
- content hashes
- path/symbol/call indexes
- projections with source-hash validation
- persistent context cache
- lockd client
- automatic reindex after writes
- initial execution event log for context, write/edit, and locks
- compact status signals from the event log
- multi-agent conflict regression tests

The current codebase does not yet implement:

- cloud/team service
- session dashboard
- full production provenance receipts
- `anchor trace`
- `anchor replay`
- `anchor attest`
- production fail-closed agent mode
- Zev

The current local foundation is enough to prove the product direction: Anchor can
sit in the agent read/write path and control context, locks, writes, and index
freshness.

## Contributor Mental Model

When adding to Anchor, ask:

1. Does this help agents read less but understand more?
2. Does this make writes safer or more controlled?
3. Does this improve multi-agent coordination?
4. Does this leave evidence for review, replay, or audit?
5. Does this fit the future cloud/team execution harness?

If the answer is no, it may be ordinary code tooling, but it may not belong in
Anchor.

## One-Line Vision

Anchor is the execution control plane for AI coding agents: it coordinates
context, locks, writes, verification, provenance, and team handoff so humans can
delegate real repository work to agents without losing control.

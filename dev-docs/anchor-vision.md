# Anchor Vision

Anchor is an agent execution harness that controls how AI coding agents read, write, coordinate, and verify work inside real software projects.

Anchor is not a code search tool, not a database, not a Git replacement, and not only a local CLI. The CLI is the first interface. The product direction is a harness that sits between coding agents and software projects, controlling the execution path so agents can work with higher efficiency, higher quality, and higher safety.

## Why Anchor Exists

AI coding agents are good at reasoning and writing logic, but real software work is not only reasoning.

A real coding workflow also needs:

- choosing the right context
- avoiding unrelated files
- coordinating parallel work
- applying edits safely
- verifying results
- remembering what failed
- explaining what changed
- recovering from bad execution

Today, most agents interact with repositories through loose shell access, file reads, search commands, and raw edits. That works for small tasks, but it becomes weak when many agents, large codebases, repeated failures, or production-quality expectations enter the workflow.

Anchor exists to move the non-reasoning parts out of the model and into an execution harness.

The agent should mostly handle reasoning and logic.

Anchor should handle controlled execution.

## Primary Effects

Anchor must improve three effects.

### 1. Efficiency

Anchor should reduce the amount of work an agent must do to read, write, and verify code.

Efficiency means:

- fewer tokens spent on irrelevant files
- fewer blind `rg` and file-open loops
- faster access to the right symbol context
- less repeated context rebuilding
- fewer duplicated agent attempts
- less manual coordination when multiple agents run

The core idea is not "give the model more context." The core idea is "give the model the right context and the right write surface."

This is related to known patterns from other domains:

- RAG systems improve model work by retrieving relevant context instead of dumping everything.
- Long-context research shows that too much irrelevant context can hurt performance.
- Build systems use caching and affected-task selection to avoid repeated work.
- Code intelligence tools use symbol and reference data to avoid file-level guessing.

Anchor applies that same principle to coding agents.

### 2. Quality

Anchor should improve the quality of agent changes.

Quality means:

- smaller patches
- cleaner diffs
- fewer unrelated edits
- better scoped changes
- better test/check outcomes
- higher benchmark solve rate on real coding tasks
- fewer repeated failed approaches

This is not only code style. The quality goal is that the same model should produce better software work when it operates through Anchor.

The benchmark question is:

```text
same model + normal workflow
vs
same model + Anchor workflow
```

Anchor should be measured on:

- task solved or not solved
- tokens used
- files read
- symbols read
- patch size
- unrelated files touched
- retries before success
- tests/checks passed
- human review friction

Quality is where Anchor must prove itself. If Anchor only records safer logs but does not improve the coding outcome, adoption will be weaker.

### 3. Safety

Anchor should make agent execution safer and more trustworthy.

Safety means:

- agents do not overwrite each other
- same-symbol conflicts are blocked early
- writes are owned by a session or agent ID
- actions are recorded
- failures are visible
- recovery is possible
- team policies can be enforced

Safety matters for companies, but it also matters for single developers running multiple agents at once.

A single developer using five agents has many of the same coordination problems as a team using five agents.

## Core Layers

### Context Control

Anchor controls what the agent reads.

The agent should not always start from a full file or full repository. Anchor should provide focused context: symbols, callers, callees, related files, and eventually quality/provenance memory.

This improves efficiency and quality.

### Write/Edit Control

Anchor controls what the agent changes.

The agent should not freely rewrite broad parts of the repository when the task is scoped. Anchor should support targeted writes, symbol-level edits, freshness checks, and eventually policy-guided write boundaries.

This improves quality and safety.

The freshness check is a known production pattern applied to agent writes.
Other systems call this optimistic concurrency control, optimistic locking,
conditional writes, or lost-update prevention.

The simple rule is:

```text
read version X
write only if still version X
if changed, reject and reread
```

Examples from existing infrastructure:

- HTTP APIs use `ETag` plus `If-Match` to avoid overwriting a resource changed
  by someone else.
- DynamoDB conditional writes and optimistic locking update an item only if its
  version still matches.
- Kubernetes uses `resourceVersion`; stale updates get conflict responses.

Anchor applies the same idea to source files:

```text
agent reads file hash X through Anchor
agent later writes through Anchor
Anchor checks the file still has hash X

same hash -> write allowed
different hash -> stale write blocked
```

This is mainly useful for multi-agent, team, and long-running work, but it can
also protect a single agent if a formatter, generator, test setup command, or
human changes the file during the task.

### Multi-Agent Coordination

Anchor coordinates many agents working in the same project.

The core mechanism is ownership and locking.

Example:

```text
agent-a works on auth.login
agent-b works on billing.refund
agent-c tries auth.login and gets blocked
```

This is not the same as isolated Git worktrees.

Worktrees isolate work and discover conflicts later.

Anchor locks coordinate work before agents collide.

Both can coexist, but Anchor's value is live coordination at the execution level.

### Transaction / Provenance Record

Anchor records what agents did.

This is not a chat transcript. It is an execution record.

It should answer:

- which agent/session did the work
- what task was being attempted
- what context was read
- what symbol/file was locked
- what changed
- what checks ran
- what failed
- what passed
- what was accepted, reverted, or repeated

This helps humans audit work, but it also helps future agents.

The record becomes operational memory.

### Git-Native Behavioral Index

Anchor should not become just another source graph.

A normal source graph answers static code questions:

- where a symbol lives
- which file contains a function
- what calls what
- which references point to a definition

That is useful, but it is not enough to make agents better at real software work.

Anchor should use Git as a behavioral parser of the repository. Each commit is a
specific tree plus a specific accepted change. Across history, Git can reveal:

- files that usually change together
- tests that usually change or fail near a feature
- symbols or paths that repeatedly appear in similar fixes
- files that were touched and later reverted
- areas with high churn or high regression risk
- patch shapes that solved a class of tasks before

This is different from only asking "what calls this function?" It asks:

```text
when humans or agents changed this behavior before,
what actually had to move?
```

Anchor should combine that historical signal with execution provenance:

- what the agent read
- what it locked
- what it edited
- what failed
- what passed
- what was reverted
- what was accepted

Together, this becomes a historical and execution-aware index:

```text
Git history = accepted work history
Anchor provenance = attempted work history
```

The syntax/parser index still matters for exact symbol ranges, calls, and scoped
writes. But the product moat is not "we have a graph." The stronger direction is:

```text
Git-derived behavioral facts
+ parser-derived code facts
+ execution provenance
+ locks and checked writes
= Anchor execution harness
```

This keeps Git as the foundation while avoiding the trap of becoming a generic
Sourcegraph-style clone.

### Quality Kernel

The quality kernel uses execution memory and verification signals to improve future agent work.

The first version should be deterministic. It should not depend on another model
judging the patch. It should score the execution evidence Anchor already has:

- did the agent read context before editing
- did it read the same file it edited
- did the file change after it was read
- did the write use a freshness guard
- how many files changed
- how many lines changed
- did checks pass or fail
- did the agent touch risky paths such as auth, billing, config, migrations, or security files
- were there lock conflicts or stale-write blocks

The output should be:

```text
score
risk
flags
recommended next actions
```

Later versions should learn from:

- repeated failed edits
- broad patches that were rejected
- tests that usually matter for a symbol
- files that are commonly related
- files that were incorrectly touched before
- checks that must run before accepting a change

The quality kernel is not just security. It is the part that pushes agents toward better software work.

It should improve efficiency and quality first, with safety as a required base.

### SDK Layer

The SDK layer gives tools and agents a stable way to use Anchor.

Codex, Claude Code, Cursor, OpenCode, CI, and custom agents should not each invent their own repository-control layer.

They should call Anchor through a stable interface:

- get context
- acquire ownership
- apply edit
- run/record checks
- inspect transaction history
- release ownership

The SDK is not the product by itself. The SDK is the adoption surface for the execution harness.

### Team / Cloud Mode

Anchor is not only local.

The final product includes shared sessions, team state, managed infrastructure, dashboards, policies, and history across agents and developers.

The open-source core should provide the local execution harness.

The team/cloud layer should provide shared coordination and visibility for users who do not want to operate the infra themselves.

### Zev Later

Zev is a later R&D layer.

Zev is a compact model-native code representation. It is meant to let agents read and write code in a representation designed for models, while Anchor handles source conversion and execution.

Zev is not required for Anchor v1.

Anchor must stand on its own first as the execution harness.

## User Value

### Single Developer With Multiple Agents

This user may run several agents at once.

Anchor helps by:

- preventing agents from editing the same symbol at the same time
- giving each agent focused context
- reducing repeated exploration
- keeping a record of which agent changed what
- making it easier to recover from bad edits

This user pays if Anchor saves time, reduces failed agent runs, and makes parallel agent use practical.

### Small Team

This team may have humans and agents working together.

Anchor helps by:

- reducing review noise
- showing why a change happened
- making agent work easier to audit
- avoiding duplicated or conflicting agent work
- creating a shared execution memory

This team pays if Anchor reduces coordination cost and makes AI-assisted development less chaotic.

### Larger Company

This company cares about reliability, auditability, policy, and workflow control.

Anchor helps by:

- enforcing execution policies
- recording agent actions
- supporting team-wide coordination
- producing receipts for agent work
- connecting checks and quality signals to each change

This company pays if Anchor makes agent-generated code trustworthy enough for production workflows.

## What Must Be True Before Launch

Anchor is infrastructure. It cannot launch as a vague wrapper.

Before a serious launch, Anchor must prove:

- controlled context works on real repositories
- writes are scoped and reliable
- locks prevent real conflicts
- reindexing/freshness avoids stale context
- transaction records are usable
- quality signals improve future runs
- benchmarks show efficiency and/or quality improvement
- failure modes are understood and documented

The promise is not that agents become perfect.

The promise is that agents become cheaper, cleaner, and safer to run through Anchor.

# Current Focus: Anchor + Vals

Date: 2026-06-22

## Decision

Finish Anchor first and use the Vals Fellowship submission as the near-term
external forcing function.

Pause separate model-architecture work, including the Latent Agency Model and
world-model direction, until the Anchor/Vals path is submitted or blocked.

## Why

Anchor is the clearest project right now because it has:

- a concrete problem: AI coding agents execute software tasks poorly when they
  drift, waste context, edit too broadly, skip verification, or collide with
  other agents
- a concrete product shape: execution harness for coding agents
- a concrete measurement path: baseline agent vs same agent through Anchor
- a concrete fellowship angle: benchmark methodology for agent execution quality

The model work is interesting but currently under-specified. It mixed
architecture, objective, scale, and benchmark into one thread, which made the
implementation confusing instead of clarifying the research.

## Anchor Statement

Anchor is an agent-neutral execution harness for AI coding agents.

It provides a new execution path for agents working on software tasks. The
product target is not one feature such as retrieval, Zev, locks, receipts, or a
merge gate. Those are implementation layers. The target is measurable
improvement in:

- efficiency
- quality
- safety

The clearest analogy is systems engineering: Anchor is like a kernel/runtime
optimizer for AI coding agents. The model is still the reasoning engine, but
Anchor optimizes the agent's execution over a codebase: code access, write
freshness, verification, provenance, and multi-agent coordination.

The current execution shape is:

```text
intent -> focused context -> scoped read -> checked write -> targeted verify -> receipt
```

The intended measurable effects are:

- efficiency: less wasted context, fewer repeated/raw code reads, cheaper task
  execution, and lower agent log/token cost
- quality: correct maintainable code, narrower patches, better-grounded edits,
  better verification, fewer hidden regressions, and no accepted AI-slop code
- safety: stale-write blocking, write ownership, receipts, and multi-agent
  coordination without silent code corruption

## Positioning Boundary

Do not shrink Anchor into a safety tool. Safety is one pillar, not the product.

Do not expand Anchor into a JCode-style coding-agent runtime. JCode runs agents
inside JCode. Anchor should make any coding agent run better inside a real repo:
Codex, Claude Code, Cursor, JCode, CI bots, and future multi-agent systems.

Anchor is the execution harness underneath the software task:

```text
agent/runtime -> Anchor execution path -> repo/codebase/checks
```

Its job is to improve how the agent searches, reads, writes, verifies, records,
and coordinates code work.

## Measurement Contract

Anchor only wins if a custom benchmark shows that agents execute software
tasks better through Anchor than without it.

The benchmark should be run through Harbor/Pier-compatible task execution so it
measures real agent behavior instead of toy traces.

The benchmark measures three pillars:

- efficiency: total agent log bytes/tokens, code-read/write tool calls,
  repeated reads, raw read-like commands, patch size, changed-file scope, and
  task duration
- quality: correct final behavior, useful verification, narrow maintainable
  patch, no unresolved failed checks, no hidden regression, and no accepted
  AI-slop-style overengineering
- safety: no stale writes, no unrecorded writes, no broad blind edits, no
  multi-agent collisions, and complete provenance/receipts

The benchmark is the judge. Anchor features are valid only when they improve
one of these three pillars or remove complexity.

## Vals Proposal Direction

The proposal should be framed around benchmarking AI coding-agent execution, not
around pitching Anchor as a product.

Working benchmark shape:

```text
ExecutionBench: benchmark for AI coding-agent execution quality
```

V1 questions:

- Did the agent read the right context?
- Did it waste tokens?
- Did it verify the right behavior?
- Did it leave failed checks unresolved?
- Did it pass tests with bad, ugly, or fragile code?
- Could 5 agents coordinate on the same repo while handling these conditions?

Anchor can be used as a baseline-improvement system during evaluation:

```text
same task + normal agent
vs
same task + agent through Anchor
```

## Out Of Scope For Now

- Latent Agency Model implementation
- world-model architecture
- Zev language/runtime work
- custom 100M+ model training
- cloud product work beyond keeping schemas serializable

These are not dead. They are paused so the Anchor/Vals work can finish.

## Immediate Work

1. Tighten Anchor's current execution loop.
2. Make the benchmark proposal precise.
3. Produce small real measurements where possible.
4. Keep commits small and on `dev`.

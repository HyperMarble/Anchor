# Program Logic JEPA — Model Hypothesis

## Core Idea

Train a custom model to predict hidden representations of program behavior and
business logic, not only the next code token.

Normal code models mostly learn:

```text
code context -> next likely token
```

This hypothesis is:

```text
code + task + tests + traces
  -> encoder
  -> latent logic state

target behavior + execution result + invariant + future state
  -> target encoder
  -> latent target state

predictor learns to match the target logic state
```

The model learns an internal state for what the code does.

## Human Analogy

When a developer dry-runs code mentally, they usually do not reason token by
token. They simulate the behavior:

```text
input arrives
state changes
branch is taken
lock is acquired
record is written
duplicate action is blocked
test invariant holds or fails
```

The goal is to train a model toward that kind of hidden execution reasoning.

## Why This Could Help Generation

This is not only an analysis model. Better generation should come from better
internal behavior understanding.

For example, for:

```text
Add refund retry logic, but prevent double refund.
```

A token-only generator may write code that looks plausible.

A behavior-aware generator should first represent:

```text
payment was captured
refund can be retried only after failure
successful refund must be idempotent
ledger must not duplicate
lock is needed around the critical section
tests should cover duplicate retry behavior
```

Then generation becomes:

```text
task + code context
  -> latent behavior state
  -> code patch
```

Or:

```text
generator proposes patch
Program Logic JEPA checks behavior state
generator revises patch
```

## What It Could Improve

- Better first patches because the model reasons over intended behavior first.
- Fewer logic bugs because code is checked against expected state transitions.
- Better test selection because the model can predict which invariant matters.
- Better edits because the model can detect what behavior must be preserved.
- Less over-engineering if the input code is canonicalized before training.

## Data Sources

Useful training signals may include:

- source code
- unit tests
- failing-to-passing patches
- execution traces
- runtime state snapshots
- type flow and call flow
- git commits
- bug reports linked to fixes
- invariant checks
- business rule docs

The hard part is not naming the idea. The hard part is creating reliable target
representations of behavior.

## Relationship To Zev

This is not an Anchor runtime feature and not the Zev transpiler itself.

Zev can still help as a compact canonical input representation:

```text
raw source
  -> doctor normalization
  -> canonical source
  -> Zev representation
  -> Program Logic JEPA input
```

The important point is that the model should learn from stable program shape,
not messy arbitrary syntax.

## Open Research Question

Can a model trained to predict latent execution or behavior states outperform a
normal next-token code model on:

- bug localization
- patch correctness
- invariant preservation
- test selection
- business logic reasoning
- normal code generation

The smallest useful experiment:

```text
Python functions + unit tests + execution traces
  -> train behavior-state predictor
  -> compare against next-token baseline on bug-fix and test-prediction tasks
```

## Short Name

Program Logic JEPA.

Plain meaning:

> A model that learns to predict what code means and how it behaves, not just
> what code text comes next.

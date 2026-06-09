# Niya — Model Hypothesis

## What Niya Is

Niya is a custom code model we would build from scratch.

It is not Anchor.
It is not the benchmark harness.
It is not the terminal, tools, or sub-agent system.

Anchor controls execution. Niya is the model brain.

The goal is to replace the normal GPT-style reasoning core with a model that
learns hidden representations of program logic, then uses a decoder to produce
code, patches, plans, or text.

## Core Claim

Normal GPT-style code models mostly learn:

```text
context tokens -> next token
```

Niya should learn:

```text
canonical code + task + traces + tests
  -> latent program behavior state
  -> target behavior state
  -> token/code decoder output
```

The model should understand what the code does before it writes the next token.

## Why This Is Different

Embedding search finds code that looks related.

Niya should learn code behavior:

```text
this function changes this state
this caller depends on this invariant
this patch would break this edge case
this test proves this behavior
this code path should be changed instead
```

So Niya is not only retrieval. It is a program-logic model.

## Input Pipeline

The input should be stable before the model sees it:

```text
raw source code
  -> doctor normalization
  -> canonical source
  -> compact Zev representation
  -> Niya encoder
```

Doctor normalization removes noisy style differences.
Zev makes the canonical code compact.
Niya learns the hidden behavior state from that compact representation.

## Model Shape

Niya has three main parts:

```text
encoder
  reads canonical/Zev program context

latent predictor
  predicts program behavior state, execution state, or target patch state

token decoder
  turns the latent behavior plan into source code, Zev code, patches, or text
```

The decoder exists, but next-token prediction is not the main idea.
The main idea is latent program-state prediction.

## Dry-Run State

Humans dry-run code mentally:

```text
input enters
branch is taken
state changes
caller receives value
invariant holds or fails
```

Niya should learn a compact version of that hidden dry-run state.

A useful cache may store:

```text
symbol behavior
caller and callee effects
state transitions
known invariants
test evidence
previous dry-run state
likely failure points
```

This is not the same as a transformer KV cache.
KV cache stores token attention state.
Niya's cache stores program behavior state.

## Output Pipeline

The output can go through multiple forms:

```text
latent behavior state
  -> token decoder
  -> Zev patch or canonical source patch
  -> doctor validation
  -> final source patch
```

The generated code should be doctor-clean by default.

If sub-agents are used, they are workers around the model. They are not the
model itself.

## Package Form

Niya can be distributed as a model package:

```text
niya.pkt
```

The `.pkt` package may contain:

- model weights
- tokenizer or Zev codec
- doctor profiles
- language profiles
- latent-state schema
- decoder config
- training metadata
- evaluation metadata

The package is the model artifact. It is separate from Anchor.

## Training Signals

Useful training data:

- source code
- canonical source after doctor normalization
- Zev representation
- unit tests
- execution traces
- failing-to-passing patches
- runtime state snapshots
- call flow and type flow
- invariants
- bug reports linked to fixes
- before/after behavior

The key challenge is defining the target latent behavior state well enough.

## First Experiment

Start small.

```text
Python functions + tests + traces
  -> canonicalize source
  -> convert to compact representation
  -> train latent behavior predictor
  -> decode or rank correct patch/test behavior
```

Compare against:

- embedding-only retrieval
- normal next-token baseline
- trace-only baseline

Measure:

- patch correctness
- invariant preservation
- test selection accuracy
- token cost
- generated patch size
- failure cases

## Non-Goals

- This is not an Anchor feature.
- This is not only an embedding search system.
- This is not only Zev compression.
- This is not only a linter or doctor.
- This is not a claim that business logic can be solved without tests or traces.

## Plain Definition

Niya is a model that learns hidden program behavior and uses that behavior state
to generate better code.

Short version:

```text
Zev makes code compact.
Doctor makes code canonical.
Niya learns what the code does.
The decoder writes the change back.
Anchor proves and controls execution.
```

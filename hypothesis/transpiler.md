# Anchor Transpiler — Design Hypothesis

## The Idea

Anchor becomes a bidirectional transpiler between Zev (a minimal pipeline language) and any target language (Rust, Python, JavaScript, Go, etc.).

```
READ:   Rust codebase → Anchor → Zev → Agent reads
WRITE:  Agent writes Zev → Anchor → Rust → codebase updated
```

Agent permanently lives in Zev-world. Never touches Rust syntax, Python idioms, or language-specific quirks. Anchor is the membrane between the two worlds.

## Why Zev (Not Zero)

Zero (Vercel Labs) is close but wrong fit:
- Their language, their roadmap, their breaking changes
- Still looks like code, not English
- No ownership over token optimization

Our language design goals:
- Pipeline-first: `>>` operator is the core primitive, not function calls
- No type annotations
- No symbols (`{}`, `&`, `;`) — only `>>` and `->` for pipelines
- Indentation only, no braces
- Agent already thinks in pipeline patterns — pretraining data is full of data flow logic

## Token Savings (Prototype Measurement)

Tested on real MLflow Python functions (139 functions, `python_to_zev.py`):

| Function | Python tokens | Zev tokens | Savings |
|---|---|---|---|
| get_parent_run | 69 | 12 | 83% |
| filter_providers | 116 | 19 | 84% |
| validate_delete_traces | 141 | 22 | 84% |
| register_prompt | 222 | 35 | 84% |

Prototype average: **83.5% reduction** vs Python across 139 functions.
This is a research measurement, not a production guarantee. It must be rerun with
the final Zev syntax, final tokenizer choice, and round-trip correctness checks.

Potential combination with Anchor's other layers:
- Persistent cache (98% savings on unchanged code)
- Symbol slicing (94% savings vs whole file reads)
- Overall session reduction may be high, but must be measured on real tasks

## Syntax Design

```
fn get_parent_run(self, run_id)
  child_run = self.tracking_client.get_run(run_id)
  parent_id = child_run.data.tags.get(PARENT_RUN_ID)
  if parent_id is nothing
    return nothing
  return self.tracking_client.get_run(parent_id)
```

```
fn filter_providers(providers, allowed)
  if allowed is nothing
    return providers
  result = []
  for each p in providers
    name = normalize(p)
    if name not in allowed
      skip
    add p to result
  return result
```

```
fn register_prompt(name, template, is_databricks)
  validate(name)
  if is_databricks
    try
      create_prompt(name)
    or:
      pass
    pv = create_prompt_version(name, template)
    return get_prompt_version(name, pv.version)
  model = get_registered_model(name)
  if model is nothing
    model = create_registered_model(name)
  if model exists and not has_prompt_tag(model)
    fail "Model with same name exists"
  return create_prompt_version(name, template)
```

The pipeline operator — Zev's unique primitive:

```
fn max_val(numbers)
  numbers
  >> first -> m
  >> loop -> x if x > m -> m
  >> out m

fn process_users(db)
  db.query("users")
  >> filter -> u if u.active
  >> map -> u.email
  >> out
```

### Keywords
- `fn` — function
- `is nothing` — null/None check
- `exists` — not null check
- `for each` — iteration
- `skip` — continue
- `add X to Y` — append
- `fail` — raise/throw
- `>>` — pipeline step (core primitive)
- `-> name` — bind pipeline result to name
- `filter ->`, `map ->`, `out`, `first ->` — pipeline steps
- `and`, `or`, `not` — logical (no symbols)
- Indentation only, no braces, no colons

## Architecture: Write Path

```
Agent writes foo.zev
      ↓
anchor transpile foo.zev
      ↓
Anchor detects codebase language (80% .py → Python)
      ↓
Anchor reads call graph: where does this fit?
      ↓
Zev → Python/Rust/JS
      ↓
Write lock applied (lockd)
      ↓
File written
```

## Architecture: Read Path

```
Agent calls anchor_context("Linear.forward")
      ↓
Anchor fetches Rust/Python source (symbol sliced)
      ↓
Anchor converts → Zev
      ↓
Agent receives fewer tokens if the source-to-Zev conversion preserves the needed
logic
```

## Current Evidence

- `python_to_zev.py`: 139 MLflow functions tested in prototype form.
- Prototype measurement showed 83.5% average token savings across the sample.
- This does **not** yet prove production readiness.
- Still required: final grammar, source-to-Zev correctness checks, Zev-to-source
  regeneration, and real benchmark tasks.

## What Needs Building

1. **Final Zev grammar/parser**
2. **`zev_to_python.py`** — write path for Python codebases
3. **`rust_to_zev.py`** / **`zev_to_rust.py`** — read/write path for Rust
4. **`anchor transpile` CLI** — `anchor transpile foo.zev --to python`
5. **Auto language detection** — scan extension counts, pick target
6. **Coordination layer** — reads call graph, places output in correct file
7. **Benchmark harness** — token usage, correctness, edit quality, and task pass rate

## SLM Angle

Fine-tune Gemma (or Phi-3) on Zev only. The model:
- Writes Zev (one simple pipeline language, not 20)
- Reads Zev (converted by Anchor from any source language)
- Never sees Rust ownership, Python GC, JS async weirdness
- potentially more codebase fits in context if the final representation keeps
  the measured reduction

Target result: a smaller specialist model that can reason over Zev efficiently.
This is research work, not proven yet.

Hypothesis: a small model trained on Zev can outperform its size class on
function-level coding tasks because it sees one compact representation instead
of many source-language syntaxes.

Gemma 4 hackathon submission:
- Local Gemma-zev vs GPT-4 on function-level coding tasks
- Benchmark: token usage, latency, correctness
- Anchor handles all transpilation

## Key Insight

Zero (Vercel): designed for agents to write code.
Anchor Transpiler: designed so agents can work through a compact code
representation instead of raw source whenever the conversion is reliable.

Different layer. Different value. Needs proof through round-trip tests and real
task benchmarks.

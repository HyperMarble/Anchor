# Anchor Transpiler — Design Hypothesis

## The Idea

Anchor becomes a bidirectional transpiler between a minimal pseudocode language and any target language (Rust, Python, JavaScript, Go, etc.).

```
READ:   Rust codebase → Anchor → Pseudocode → Agent reads
WRITE:  Agent writes Pseudocode → Anchor → Rust → codebase updated
```

Agent permanently lives in pseudocode-world. Never touches Rust syntax, Python idioms, or language-specific quirks. Anchor is the membrane between the two worlds.

## Why Pseudocode (Not Zero)

Zero (Vercel Labs) is close but wrong fit:
- Their language, their roadmap, their breaking changes
- Still looks like code, not English
- No ownership over token optimization

Our language design goals:
- English-like, reads like instructions
- No type annotations
- No symbols (`{}`, `->`, `&`, `;`)
- Indentation only
- Agent already thinks in this pattern — pretraining data is full of pseudocode

## Token Savings (Measured)

Tested on real MLflow functions (see `token_count.rs`):

| Function | Rust tokens | Pseudocode tokens | Savings vs Rust |
|---|---|---|---|
| get_parent_run | 69 | 36 | 48% |
| filter_providers | 116 | 44 | 62% |
| validate_delete_traces | 141 | 53 | 62% |
| register_prompt | 222 | 82 | 63% |

Average: ~59% reduction vs Rust. Complex functions approach 63-70%.

Combined with Anchor's other layers:
- Persistent cache (98% savings on unchanged code)
- Symbol slicing (94% savings vs whole file reads)
- Overall session: 90%+ token reduction achievable

## Syntax Design (Draft)

```
fn get_parent_run(run_id):
    child = client.get_run(run_id)
    parent_id = child.tags["PARENT_RUN_ID"]
    if parent_id is nothing:
        return nothing
    return client.get_run(parent_id)
```

```
fn filter_providers(providers, allowed):
    if allowed is nothing:
        return providers
    result = []
    for each p in providers:
        name = normalize(p)
        if name not in allowed:
            skip
        add p to result
    return result
```

```
fn register_prompt(name, template, is_databricks):
    validate name
    if is_databricks:
        try create_prompt(name) ignore if already exists
        pv = create_prompt_version(name, template)
        return get_prompt_version(name, pv.version)
    model = get_registered_model(name) or:
        create_registered_model(name)
    if model exists and not has_prompt_tag(model):
        fail "Model with same name exists"
    return create_prompt_version(name, template)
```

### Keywords
- `fn` — function
- `is nothing` — null/None check  
- `exists` — not null check
- `for each` — iteration
- `skip` — continue
- `add X to Y` — append
- `fail` — raise/throw
- `or:` — catch block (simplified)
- `try X ignore if Y` — try/except compressed
- `and`, `or`, `not` — logical (no symbols)
- Indentation only, no braces

## Architecture: Write Path

```
Agent writes foo.pseudo
      ↓
anchor transpile foo.pseudo
      ↓
Anchor detects codebase language (80% .py → Python)
      ↓
Anchor reads call graph: where does this fit?
      ↓
Pseudocode → Python/Rust/JS
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
Anchor converts → pseudocode
      ↓
Agent receives 60% fewer tokens, same information
```

## Proven So Far

- `zero_to_python.rs`: 25/25 tests passing — proves write path works
  - Handles all language constructs: classes, match/case, try/rescue, async/await,
    decorators, logical ops, string interpolation, struct init, generators
  - Tested on real MLflow code: get_parent_run, filter_providers, validate_delete_traces,
    register_prompt core, pagination loops
- `token_count.rs`: proves token savings are real (48-63% vs Rust)

## What Needs Building

1. **Pseudocode parser** — define grammar, recursive descent parser (~500 lines Rust)
2. **Python → pseudocode** (reverse, for read path)
3. **Rust → pseudocode** (read path for Rust codebases)
4. **`anchor transpile` CLI** — `anchor transpile foo.pseudo --to python`
5. **`anchor_transpile` MCP tool** — agent calls directly
6. **Auto language detection** — scan extension counts, pick target
7. **Coordination layer** — reads call graph, places output in correct file

## SLM Angle

Fine-tune Gemma (or Phi-3) on pseudocode only. The model:
- Writes pseudocode (one simple language, not 20)
- Reads pseudocode (converted by Anchor from any source language)
- Never sees Rust ownership, Python GC, JS async weirdness

Result: small model, specialist performance. Runs locally. Zero API cost.

Gemma 4 hackathon submission:
- Local Gemma-pseudo vs GPT-4 on function-level coding tasks
- Benchmark: token usage, latency, correctness
- Anchor handles all transpilation

## Key Insight

Zero (Vercel): designed for agents to write code.
Anchor Transpiler: designed so agents never need to know code exists.

Different layer. Different value. Nobody has this.

# Anchor Agent Benchmark (Python)

This benchmark compares two agent setups on the same tasks:

- `with_anchor` (agent can use Anchor tools)
- `without_anchor` (agent uses normal file traversal/tooling)

It is designed for SWE-bench-style issues plus small edit tasks (file creation, insertion, patching).

## What It Measures

For each run:

- `speed`: wall-clock runtime (seconds)
- `efficiency`: successful checks per second
- `quality`: task checks passed / failed
- `performance`: check-runtime (seconds)
- `token_usage`: parsed from agent output (optional)
- `tool_calls`: parsed from agent output (optional)

Then it computes profile-vs-profile win rates by metric.

## Files

- `benchmark/run.py`: executes tasks and stores run artifacts/results.
- `benchmark/score_results.py`: compares profiles and prints win rates.
- `benchmark/swebench.py`: imports official SWE-bench issues into task format.
- `benchmark/prompt_improvement.py`: local prompt repair smoke benchmark.
- `benchmark/prompt_cases.example.jsonl`: example human prompts for prompt repair.
- `benchmark/config.example.json`: benchmark-level config.
- `benchmark/profiles.example.json`: agent profile templates.
- `benchmark/tasks.example.jsonl`: task templates.

## Quick Start

1. Copy example configs:

```bash
cp benchmark/config.example.json benchmark/config.json
cp benchmark/profiles.example.json benchmark/profiles.json
cp benchmark/tasks.example.jsonl benchmark/tasks.jsonl
```

2. Edit profile commands:

- Set how to run your agent CLI in each profile.
- Example: one profile injects Anchor usage instructions, the other forbids Anchor.

3. Run benchmark:

```bash
python3 benchmark/run.py \
  --config benchmark/config.json \
  --profiles benchmark/profiles.json \
  --tasks benchmark/tasks.jsonl \
  --out benchmark/results.jsonl
```

4. Score benchmark:

```bash
python3 benchmark/score_results.py \
  --results benchmark/results.jsonl \
  --a with_anchor \
  --b without_anchor
```

## Notes

- The runner creates isolated per-run workdirs by cloning task repos and checking out the requested commit.
- `token_usage` and `tool_calls` are optional and depend on your agent output format.
- Keep eval checks deterministic (`pytest`, `cargo test`, smoke script, etc.).

## Prompt Repair Benchmark

`benchmark/prompt_improvement.py` tests the project-aware prompt-repair idea with
local Ollama. It compares a raw human prompt against an Anchor-repaired prompt
that includes verified project facts, likely files, risky assumptions, and checks
to run.

For product design and the planned Rust CLI, see
[Project-Aware Prompt Repair](../docs/prompt-repair.md).

The benchmark reports two early signals:

- `brief_quality`: whether the repaired prompt adds useful repo-grounded targets,
  warnings, and checks without too much bloat.
- downstream score: whether the local model's plan mentions expected project
  facts, avoids wrong tools/frameworks, and does not hallucinate nonexistent
  paths.

Run a small one-case smoke benchmark against the local Anchor repo:

```bash
python3 benchmark/prompt_improvement.py \
  --model qwen2.5-coder:7b \
  --cases benchmark/prompt_cases.example.jsonl \
  --out benchmark/results.prompt_smoke.jsonl
```

The default is intentionally small for early iteration. Use `--limit 0` to run
all prompt cases once the prompt improver is more stable.

Use `--dry-run` to inspect generated prompts without calling Ollama.

## Official SWE-bench Import

Generate tasks directly from official SWE-bench dataset issues:

```bash
pip install datasets

python3 benchmark/swebench.py \
  --dataset princeton-nlp/SWE-bench_Verified \
  --split test \
  --limit 20 \
  --eval-cmd-template "swebench_harness_eval --instance {instance_id}" \
  --out benchmark/tasks.swebench.jsonl
```

Then run benchmark with the generated tasks file:

```bash
python3 benchmark/run.py \
  --config benchmark/config.json \
  --profiles benchmark/profiles.json \
  --tasks benchmark/tasks.swebench.jsonl \
  --out benchmark/results.swebench.jsonl
```

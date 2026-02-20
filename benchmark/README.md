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

- `benchmark/run_benchmark.py`: executes tasks and stores run artifacts/results.
- `benchmark/score_results.py`: compares profiles and prints win rates.
- `benchmark/import_swebench.py`: imports official SWE-bench issues into task format.
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
python3 benchmark/run_benchmark.py \
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

## Official SWE-bench Import

Generate tasks directly from official SWE-bench dataset issues:

```bash
pip install datasets

python3 benchmark/import_swebench.py \
  --dataset princeton-nlp/SWE-bench_Verified \
  --split test \
  --limit 20 \
  --eval-cmd-template "swebench_harness_eval --instance {instance_id}" \
  --out benchmark/tasks.swebench.jsonl
```

Then run benchmark with the generated tasks file:

```bash
python3 benchmark/run_benchmark.py \
  --config benchmark/config.json \
  --profiles benchmark/profiles.json \
  --tasks benchmark/tasks.swebench.jsonl \
  --out benchmark/results.swebench.jsonl
```

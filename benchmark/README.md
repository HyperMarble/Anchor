# Anchor Benchmark Harness

This folder is for measuring the current Anchor harness before rebuilding the full DeepSWE benchmark runner.

Current goal:

- prove what the current Anchor CLI can measure today
- keep the benchmark work on `/Volumes/Hak_SSD`
- avoid writing Docker/Colima data to the main disk
- measure only the transaction/provenance signals that exist in the current CLI

## Prompt Repair Benchmark

`benchmark/prompt_improvement.py` tests the project-aware prompt-repair idea
with local Ollama. It compares a raw human prompt against an Anchor-repaired
prompt that includes verified project facts, source-cited product-memory
evidence, likely files, risky assumptions, and checks to run.

For product design and the planned Rust CLI, see
[Project-Aware Prompt Repair](../docs/prompt-repair.md).

The benchmark reports two early signals:

- `brief_quality`: whether the repaired prompt adds useful repo-grounded
  targets, warnings, and checks without too much bloat.
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

When `.anchor/product_memory.json` exists, the benchmark reuses those cached
README/docs/manifest facts. Without the cache it extracts the same class of
evidence directly from repo-local product files.

Use `--dry-run` to inspect generated prompts without calling Ollama.

## Scripts

### `run_deepswe_task_colima.sh`

Runs a real local DeepSWE task through Harbor using the Docker backend exposed by Colima.

This is the first real-task benchmark path. It does not use Docker Desktop. It uses:

```text
COLIMA_HOME=/Volumes/Hak_SSD/colima
docker context colima
harbor run --env docker
```

Default task:

```text
python-statemachine-state-data-scoping
```

Run:

```bash
benchmark/run_deepswe_task_colima.sh
```

Run a different task:

```bash
benchmark/run_deepswe_task_colima.sh fastapi-implicit-head-options
```

Output goes under:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/harbor-jobs
```

This is currently an environment/verifier path. Anchor-native transaction measurement still requires the event log feature.

### `build_anchor_linux.sh`

Builds a Linux Anchor CLI binary using the Colima Docker backend.

Run:

```bash
benchmark/build_anchor_linux.sh
```

Output:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/bin/anchor-linux
```

### `run_deepswe_anchor_claude_colima.sh`

Runs a real DeepSWE task with Claude Code instructed to use Anchor.

It mounts:

- Linux `anchor` binary at `/usr/local/bin/anchor`
- Anchor benchmark `CLAUDE.md` at `/workspace/CLAUDE.md`
- Claude Code hooks/settings at `/workspace/.claude`
- hook trace output at `/anchor-traces`
- Anchor receipt/status/trace/gate artifacts at `/anchor-artifacts`

Default mode is warning/log mode:

```bash
benchmark/run_deepswe_anchor_claude_colima.sh python-statemachine-state-data-scoping
```

Strict mode blocks direct source writes when possible:

```bash
ANCHOR_HOOK_MODE=strict benchmark/run_deepswe_anchor_claude_colima.sh python-statemachine-state-data-scoping
```

This is the first actual Anchor-assisted benchmark path. It should be compared against the same Claude Code task run without the Anchor harness.

The script prints:

```text
job=...
trace=...
artifacts=...
```

Keep those paths for comparison.

### `run_deepswe_claude_baseline_colima.sh`

Runs the same DeepSWE task with the same `claude-code` Harbor agent, but without Anchor mounts, Anchor instructions, or hooks.

Run:

```bash
benchmark/run_deepswe_claude_baseline_colima.sh python-statemachine-state-data-scoping
```

Use this as the fair baseline for the Anchor-assisted run.

### `run_deepswe_pier_claude_baseline_colima.sh`

Runs a real DeepSWE task through Pier with Claude Code and no Anchor harness.

Pier is the DeepSWE-recommended runner for CLI agents because it is Harbor-compatible but supports agent network allowlists.

Run:

```bash
benchmark/run_deepswe_pier_claude_baseline_colima.sh python-statemachine-state-data-scoping
```

Output goes under:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/pier-jobs
```

### `run_deepswe_pier_anchor_claude_colima.sh`

Runs the same Pier/Claude path with Anchor mounted and injected through Pier's prompt template support.

It mounts:

- Linux `anchor` binary at `/usr/local/bin/anchor`
- Anchor benchmark harness at `/anchor-harness`
- hook trace output at `/anchor-traces`

It writes Anchor receipt/status/trace/gate artifacts into Pier's trial artifacts directory.

Run:

```bash
benchmark/run_deepswe_pier_anchor_claude_colima.sh python-statemachine-state-data-scoping
```

Use this as the preferred current real-agent path. The older Harbor scripts are kept for compatibility.

### `run_deepswe_claude_pair.py`

Runs one DeepSWE task twice with local Claude Code:

- baseline: Claude solves normally
- anchor: Claude is instructed to use Anchor for context, edits, checks, receipt, and gate

This does not use Pier. Anchor owns the runner and metrics.

The runner prepares each repo from the DeepSWE task image, removes upstream git history, initializes a fresh one-commit repository, and does not copy `solution/` or hidden tests into the agent workspace. After Claude finishes, the runner applies `tests/test.patch` and runs the task verifier inside the DeepSWE Docker image.

Run:

```bash
python3 benchmark/run_deepswe_claude_pair.py python-statemachine-state-data-scoping
```

Run only one side:

```bash
python3 benchmark/run_deepswe_claude_pair.py python-statemachine-state-data-scoping --mode baseline
python3 benchmark/run_deepswe_claude_pair.py python-statemachine-state-data-scoping --mode anchor
```

Output goes under:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe
```

The summary records:

- verifier reward and test exits
- duration
- changed files
- diff size
- patch bytes
- Claude tool counts
- Anchor event count and receipt quality for the Anchor run

The Codex runner also reports product-level metrics. These are derived metrics;
they do not change the agent prompt or product behavior.

Efficiency means the task finishes with less waste:

- fewer estimated log tokens
- fewer changed files
- fewer diff lines and patch bytes
- fewer raw read-like commands
- less runtime, when quality is not worse

Quality means the generated patch is correct and grounded:

- verifier reward/pass status
- patch scope
- Anchor receipt quality score and flags
- changed line/file scope from Anchor provenance

Safety means the run avoids work that breaks the repo or wastes future tokens:

- no raw terminal writes
- no unrecorded repo changes
- stale writes are blocked before mutation
- lock conflicts are visible
- checks are recorded

The important rule is:

```text
Do not claim an Anchor win from efficiency alone if quality is worse.
Do not claim a quality win when both runs fail.
Do not treat context reads as broad edit scope.
```

The final JSON contains:

```text
results[].product_metrics
product_comparison.efficiency_delta
product_comparison.quality_delta
product_comparison.safety_delta
product_comparison.read_this_first
```

### `run_deepswe_codex_pair.py`

Runs one DeepSWE task twice with local Codex:

- baseline: Codex solves normally
- anchor: Codex is instructed to use Anchor for context, edits, checks, receipt, and gate

This is the direct benchmark for whether Anchor helps Codex itself. It uses the same repo preparation and verifier path as the Claude pair runner: source is copied from the DeepSWE task image, upstream git history is removed, and the verifier runs inside the task image after the agent finishes.

Run:

```bash
python3 benchmark/run_deepswe_codex_pair.py python-statemachine-state-data-scoping
```

Run only one side:

```bash
python3 benchmark/run_deepswe_codex_pair.py python-statemachine-state-data-scoping --mode baseline
python3 benchmark/run_deepswe_codex_pair.py python-statemachine-state-data-scoping --mode anchor
```

Pin a Codex model:

```bash
python3 benchmark/run_deepswe_codex_pair.py python-statemachine-state-data-scoping --codex-model gpt-5
```

Output goes under:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe-codex
```

The summary records:

- verifier reward and test exits
- duration
- changed files
- diff size
- patch bytes
- Codex JSONL tool counts when present
- Anchor event count and receipt quality for the Anchor run

### `run_deepswe_pi_pair.py`

Runs one DeepSWE task twice with local Pi:

- baseline: Pi solves normally
- anchor: Pi is instructed to use Anchor for context, edits, checks, receipt, and gate

Pi is useful here because it is a small terminal harness with basic tools. That makes it a clean comparison point for whether Anchor helps a minimal agent harness, not only Codex or Claude Code.

Install Pi first if `pi` is not on PATH:

```bash
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
```

Run:

```bash
python3 benchmark/run_deepswe_pi_pair.py python-statemachine-state-data-scoping
```

Run only one side:

```bash
python3 benchmark/run_deepswe_pi_pair.py python-statemachine-state-data-scoping --mode baseline
python3 benchmark/run_deepswe_pi_pair.py python-statemachine-state-data-scoping --mode anchor
```

Pin a Pi provider/model:

```bash
python3 benchmark/run_deepswe_pi_pair.py python-statemachine-state-data-scoping --pi-provider openai --pi-model gpt-5
```

Output goes under:

```text
/Volumes/Hak_SSD/anchor-benchmark-work/native-deepswe-pi
```

The summary records the same metrics as the Codex pair runner so baseline, Codex, Claude, and Pi runs can be compared without changing the verifier.

### `collect_deepswe_compare.py`

Collects one baseline job and one Anchor job into a single JSON comparison.

Run:

```bash
python3 benchmark/collect_deepswe_compare.py \
  --baseline-job /Volumes/Hak_SSD/anchor-benchmark-work/harbor-jobs/<baseline-job> \
  --anchor-job /Volumes/Hak_SSD/anchor-benchmark-work/harbor-jobs/<anchor-job> \
  --anchor-trace /Volumes/Hak_SSD/anchor-benchmark-work/traces/<trace-dir>/claude-anchor-tools.jsonl \
  --anchor-artifacts /Volumes/Hak_SSD/anchor-benchmark-work/artifacts/<artifact-dir>
```

For Pier runs, pass the printed Pier job directories. The collector also searches nested Pier artifact folders:

```bash
python3 benchmark/collect_deepswe_compare.py \
  --baseline-job /Volumes/Hak_SSD/anchor-benchmark-work/pier-jobs/<baseline-job> \
  --anchor-job /Volumes/Hak_SSD/anchor-benchmark-work/pier-jobs/<anchor-job> \
  --anchor-trace /Volumes/Hak_SSD/anchor-benchmark-work/traces/<trace-dir>/claude-anchor-tools.jsonl \
  --anchor-artifacts /Volumes/Hak_SSD/anchor-benchmark-work/pier-jobs/<anchor-job>
```

### `current_anchor_probe.sh`

Legacy probe for the old task/context/query/edit prototype.

Runs a controlled local probe against a small generated Python repo.

It measures:

- automatic `.anchor` preparation through Anchor context/query commands
- `anchor context`
- repeated context read
- symbol-level `anchor edit`
- auto-reindex after edit
- execution event count
- `anchor status` quality/provenance signals
- `anchor receipt` quality score/risk
- `anchor gate` enforcement
- patch size after edit
- raw file bytes vs Anchor context bytes

It does not require Docker.

Run:

```bash
benchmark/current_anchor_probe.sh
```

Output:

```text
benchmark/results/current_anchor_probe.json
```

### `deepswe_task_inventory.py`

Reads the local DeepSWE manifest and task folders.

It helps choose real benchmark tasks before running an agent.

Run:

```bash
python3 benchmark/deepswe_task_inventory.py --summary
python3 benchmark/deepswe_task_inventory.py --language python --limit 10
python3 benchmark/deepswe_task_inventory.py --task-id python-statemachine-state-data-scoping --check-colima
```

The `--check-colima` option only inspects Docker/Colima status. It does not run containers.

## Colima Storage

The local Docker CLI is expected to use the `colima` context.

The desired socket/storage path is under:

```text
/Volumes/Hak_SSD/colima
```

If Colima is not running, benchmark scripts report that clearly and stop before launching container-based work.

Do not use Docker Desktop for these runs.

## What This Can Prove Now

The current Anchor version can measure early efficiency, scope, and provenance signals:

- context size reduction
- whether symbol edits stay scoped
- whether reindexing makes edited symbols visible
- whether context reads and edits were recorded
- whether `anchor status` can summarize context/edit activity
- whether `anchor receipt` exports machine-readable metrics
- whether `anchor gate` enforces the quality score
- patch size

The real-task Colima runner can also prove:

- DeepSWE task folders are valid
- Harbor can launch the task environment
- verifiers can run inside the Colima-backed container
- benchmark artifacts stay under `/Volumes/Hak_SSD`

## What This Cannot Prove Yet

The current Anchor version cannot fully measure:

- rollback/replay
- full quality-kernel improvement
- team/cloud coordination

Those require new Anchor features before the benchmark can measure them honestly.

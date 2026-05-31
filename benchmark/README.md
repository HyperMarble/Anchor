# Anchor Benchmark Harness

This folder is for measuring the current Anchor harness before rebuilding the full DeepSWE benchmark runner.

Current goal:

- prove what the current Anchor CLI can measure today
- keep the benchmark work on `/Volumes/Hak_SSD`
- avoid writing Docker/Colima data to the main disk
- measure only the transaction/provenance signals that exist in the current CLI

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

Default mode is warning/log mode:

```bash
benchmark/run_deepswe_anchor_claude_colima.sh python-statemachine-state-data-scoping
```

Strict mode blocks direct source writes when possible:

```bash
ANCHOR_HOOK_MODE=strict benchmark/run_deepswe_anchor_claude_colima.sh python-statemachine-state-data-scoping
```

This is the first actual Anchor-assisted benchmark path. It should be compared against the same Claude Code task run without the Anchor harness.

### `current_anchor_probe.sh`

Runs a controlled local probe against a small generated Python repo.

It measures:

- `anchor build`
- `anchor context`
- repeated context read
- symbol-level `anchor edit`
- auto-reindex after edit
- execution event count
- `anchor status` quality/provenance signals
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
- patch size

The real-task Colima runner can also prove:

- DeepSWE task folders are valid
- Harbor can launch the task environment
- verifiers can run inside the Colima-backed container
- benchmark artifacts stay under `/Volumes/Hak_SSD`

## What This Cannot Prove Yet

The current Anchor version cannot fully measure:

- execution receipts
- rollback/replay
- full quality-kernel improvement
- team/cloud coordination

Those require new Anchor features before the benchmark can measure them honestly.

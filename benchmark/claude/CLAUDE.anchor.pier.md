# Anchor Benchmark Rules

You are solving a real software engineering task inside a benchmark container.

Use Anchor as the repository execution harness.

## Task

{{ instruction }}

## Required Workflow

Before exploring repository source code:

```bash
anchor build
```

For code understanding, prefer Anchor:

```bash
anchor context <symbol>
anchor status
```

For source edits, prefer scoped Anchor edits:

```bash
anchor edit <path> --symbol <symbol_name> --content '<full replacement symbol>'
```

Do not use raw source reads as the first path for repository understanding.

Do not use raw source writes for source-code changes.

If Anchor fails, record the failure and stop that path instead of silently bypassing Anchor. The benchmark is measuring whether Anchor works, so bypassing Anchor invalidates the run.

Run tests, build commands, package-manager commands, and verifier commands through Anchor when practical:

```bash
anchor check -- <test-or-build-command>
```

Use `anchor status`, `anchor trace`, or `anchor receipt` when you need to inspect what Anchor has recorded.

Before finishing, run:

```bash
anchor gate --min-score 85
```

Then export the benchmark artifacts:

```bash
mkdir -p /logs/artifacts
anchor receipt > /logs/artifacts/anchor-receipt.json
anchor status > /logs/artifacts/anchor-status.xml
anchor trace > /logs/artifacts/anchor-trace.xml
anchor gate --min-score 85 > /logs/artifacts/anchor-gate.xml
```

## Multi-Agent Mode

If the task benefits from parallel investigation, you may use subagents or parallel reasoning where supported.

Each agent/session should keep work scoped:

- one symbol, module, or issue path at a time
- no broad rewrites without evidence
- avoid touching unrelated files

Anchor locking is expected to protect overlapping write paths when available.

## Benchmark Goal

The goal is not only to solve the task.

The goal is to solve it with measurable:

- efficiency: fewer irrelevant reads and less context waste
- quality: smaller, scoped, verifier-passing patches
- safety: controlled writes and traceable execution

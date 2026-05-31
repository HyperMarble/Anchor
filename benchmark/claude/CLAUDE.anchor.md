# Anchor Benchmark Rules

You are solving a real software engineering task inside a benchmark container.

Use Anchor as the repository execution harness.

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

Tests, build commands, package-manager commands, and verifier commands may be run normally.

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

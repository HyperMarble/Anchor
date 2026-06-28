# Project-Aware Prompt Repair

Prompt Repair is Anchor's project-aware prompt engineering layer.

It takes a messy human coding prompt, checks it against the real repository, and
rewrites it into a safer task brief before a coding agent starts work.

The goal is not to make prompts prettier. The goal is to reduce wrong context,
wrong files, wrong test commands, and misleading framework assumptions.

## Status

Prompt Repair is experimental.

What exists today:

- a local benchmark harness in `benchmark/prompt_improvement.py`
- prompt cases in `benchmark/prompt_cases.example.jsonl`
- deterministic repo profiling from files, manifests, symbols, and `.anchor`
  indexes when available
- local Ollama smoke testing for raw prompt vs repaired prompt

What is not implemented yet:

- no compiled `anchor prompt` CLI command yet
- no persistent `.anchor/project_profile.json` yet
- no LLM call in the default repair path
- no patch-level benchmark yet

## Why This Belongs In Anchor

Coding agents often fail before editing code because the first prompt sends them
in the wrong direction. A human might say:

```text
fix the agent locks thing so two ais dont mess up the same file lol
```

A generic agent may invent Python files or test tools. Anchor can repair the task
brief with repo facts:

```text
Likely target areas:
- src/cli/write.rs: write path acquires file/symbol locks
- src/lock/lockd.rs: Rust client talks to anchor-lockd
- lockd/: Go daemon owns cross-process lock state
- tests/test_cli_multi_agent_conflict.rs: multi-agent conflict tests

Suggested checks:
- cargo test
- cargo build --release
- cd lockd && go test ./...
```

That makes Prompt Repair a natural extension of Anchor's core loop:

```text
build index -> repair prompt -> retrieve context -> checked write -> reindex
```

## Terminology

Use **Prompt Repair** or **Prompt Doctor** in user-facing docs.

Avoid calling the product feature "prompt injection" by itself. Prompt injection
usually means adversarial instruction attacks. Anchor should support injection
style risk detection, but the main feature is broader: project-grounded prompt
repair.

## Fast Workflow

Prompt Repair must feel instant. The default path should not call an LLM.

Recommended workflow:

```text
anchor build
  -> .anchor/index/paths.json
  -> .anchor/index/symbols.json
  -> .anchor/index/calls.json
  -> .anchor/project_profile.json

anchor prompt repair "<human prompt>"
  -> tokenize prompt
  -> match symbols/files/tests/frameworks
  -> flag wrong assumptions and prompt risks
  -> render repaired task brief
```

The repair path should be deterministic and local. Optional LLM polishing can be
added later behind a flag:

```bash
anchor prompt repair "..."        # instant deterministic repair
anchor prompt repair --llm "..."  # optional slower rewrite/polish
```

## Planned CLI

```bash
anchor prompt check "fix the lock thing"
anchor prompt repair "fix the lock thing"
anchor prompt repair --format markdown "fix the lock thing"
anchor prompt repair --format json "fix the lock thing"
anchor prompt explain "fix the lock thing"
```

Suggested command behavior:

- `check`: report assumptions, missing context, risky wording, and likely targets.
- `repair`: print the repaired task brief.
- `explain`: show why Anchor changed the prompt.
- `--format json`: machine-readable output for Codex, Claude Code, Cursor, and
  custom agents.

## Project Profile

Prompt Repair should be powered by a cached profile generated during
`anchor build` or a future `anchor learn`.

Proposed file:

```text
.anchor/project_profile.json
```

Suggested fields:

```json
{
  "profile_version": 1,
  "source_hash": "sha256-of-profile-inputs",
  "languages": ["Rust", "Go"],
  "manifests": ["Cargo.toml", "lockd/go.mod"],
  "test_commands": ["cargo test", "cd lockd && go test ./..."],
  "top_modules": ["src/cli", "src/lock", "src/storage", "lockd"],
  "frameworks_present": [],
  "frameworks_absent": ["express", "jest", "react", "next"],
  "entrypoints": ["src/bin/cli.rs", "lockd/main.go"]
}
```

The profile should be rebuilt only when relevant inputs change. Prompt-time work
should be limited to reading this profile plus the existing symbol index.

## Repair Pipeline

### 1. Normalize

- preserve the original prompt
- lowercase/tokenize for matching
- split camelCase, snake_case, paths, and framework names

### 2. Lint

Detect prompt issues before adding context:

- nonexistent framework or test tool
- vague task wording
- destructive scope like "rewrite everything"
- validation avoidance like "skip tests"
- instruction conflict like "ignore repo facts"

### 3. Route

Find likely targets using:

- exact file mentions
- symbol index matches
- BM25-style feature tokens
- manifest/test-command evidence
- optional call-neighborhood hints from `calls.json`

Each target should carry an evidence label:

```text
verified: exact file/symbol exists
inferred: token match or related module
not_found: prompt referenced something absent
needs_confirmation: weak match
```

### 4. Repair

Render a task brief with:

- original prompt
- what Anchor changed
- verified project facts
- likely files/symbols
- assumptions to avoid
- checks to run
- short instructions for the downstream coding agent

### 5. Cache

Cache repairs by:

```text
hash(project_profile + symbol_index + original_prompt + render_options)
```

The same prompt in the same project should return immediately.

## Benchmark Workflow

Current smoke benchmark:

```bash
python3 benchmark/prompt_improvement.py \
  --model qwen2.5-coder:7b \
  --cases benchmark/prompt_cases.example.jsonl \
  --out benchmark/results.prompt_smoke.jsonl
```

Default behavior runs one case. Use `--limit 0` to dry-run all cases:

```bash
python3 benchmark/prompt_improvement.py --dry-run --limit 0
```

The benchmark reports two early signals:

- `brief_quality`: whether the repaired prompt adds repo-grounded targets,
  warnings, checks, and avoids excessive bloat.
- downstream plan score: whether the local model selects hidden expected
  files/checks, stays grounded in real repo paths, avoids wrong frameworks, and
  avoids hallucinated paths.

Prompt cases should stay small and source-backed:

- prefer short manual paraphrases of public issue text over copied issue bodies
- record source URL and repo license in case provenance
- keep expected targets/checks hidden from the visible human prompt text
- track prompt-copy leakage so downstream score is not just term echoing

This is a smoke benchmark only. The stronger benchmark will compare full agent
runs and measure:

- wrong-file edits
- unnecessary file reads
- tool calls
- token usage
- test pass rate
- patch success rate

## Related Work

- [Aider repo map](https://aider.chat/docs/repomap.html): sends concise repository
  symbols and important definitions so the model can choose relevant files.
- [Continue rules](https://docs.continue.dev/customize/deep-dives/rules): supports
  project-specific instructions and context rules.
- [Repomix](https://repomix.com/): packs codebases into AI-friendly formats with
  token counting and tree-sitter-based compression.
- [promptfoo evaluations](https://www.promptfoo.dev/docs/configuration/guide/):
  useful model/prompt evaluation patterns with test cases and assertions.
- [promptfoo red teaming](https://www.promptfoo.dev/docs/red-team/): relevant if
  Anchor later adds security-focused prompt-injection defense.

Anchor's opportunity is different: repair the user's task brief using local repo
evidence before any coding agent begins.

## Implementation Plan

1. Keep the Python benchmark as a fast experimental harness.
2. Add Rust `src/prompt/` modules:
   - `profile.rs`
   - `lint.rs`
   - `route.rs`
   - `repair.rs`
   - `render.rs`
3. Generate `.anchor/project_profile.json` during `anchor build`.
4. Add `anchor prompt check` and `anchor prompt repair`.
5. Add JSON output for agent integrations.
6. Add patch-level benchmark once the prompt repair is stable.

## Design Rules

- Default repair must be local and deterministic.
- LLM rewrite must be optional, not required for the fast path.
- Every correction should cite repo evidence.
- Do not over-rewrite accurate prompts.
- Prefer "needs confirmation" over fake confidence.
- Never hide the original prompt.

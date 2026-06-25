# Anchor Explained

Anchor is an AI-aware execution layer for coding agents.

The important correction is this: source code is already a filesystem. Anchor
does not win by turning code into files. Anchor wins by making normal code
reads and writes transactional, fresh, scoped, quality-checked, and safe for
one agent or many agents.

Anchor should feel simple to use:

```text
brew install anchor
```

After that, the developer should keep using Codex, Claude Code, Cursor, JCode,
OpenCode, or another coding agent. Anchor sits around the workspace and makes
agent execution better without forcing the user to move into a new agent.

## The Core Idea

Normal agent execution looks like this:

```text
agent searches with rg
agent reads with cat/sed
agent edits files
agent runs tests
human reads a diff later
```

That is cheap to start, but weak for real software work:

- searches are blind
- reads are not remembered as proof
- writes do not prove fresh context
- multiple agents can collide
- tests are often guessed or repeated
- ugly code can still pass visible tests
- the final diff does not explain the execution path

Anchor changes the path:

```text
agent asks for code context
Anchor returns fresh handles and focused code
agent writes through Anchor
Anchor checks freshness, scope, locks, rules, and verification
Anchor accepts or rejects the write
Anchor records a machine-checkable receipt
```

The one-line version:

```text
Anchor is a transactional worktree for AI coding agents.
```

## What Anchor Is Not

Anchor is not just a security product.

Security matters, but it is one part of execution quality. Anchor is trying to
make coding agents better across three connected pillars:

- efficiency: less blind searching, rereading, and repeated verification
- quality: cleaner, smaller, maintainable code with less AI slop
- safety: fresh-context writes, conflict control, and proof before acceptance

Anchor is not a replacement for Git.

Git records accepted source history. Anchor controls agent execution before and
around accepted source history.

Anchor is not a new programming language.

Zev may later change the representation agents read and write, but Anchor must
work on normal source repositories first.

Anchor is not a static source graph.

Parser facts, symbols, chunks, imports, and call relations are useful derived
data. They are not the product. The source repo stays the source of truth.

## The Store Model

`.anchor` should be closer to `.git` than to a search index.

The source tree remains normal:

```text
repo/
  src/
  tests/
  docs/
  pyproject.toml
```

Anchor keeps execution state beside it:

```text
.anchor/
  objects/
    contexts/
    chunks/
    patches/
    checks/
    receipts/
  refs/
    sessions/
    workspaces/
  locks/
  rules/
  context/
    resources/
    memories/
    skills/
  cache/
```

The distinction matters:

```text
source files = truth
objects/receipts = durable execution evidence
cache = disposable derived data
```

If `.anchor/cache` is deleted, Anchor should be able to rebuild it lazily. If
receipts are deleted, execution proof is lost.

## No Build-First Mental Model

`anchor build` should not be the center of the product.

A big upfront build makes Anchor feel like a graph/index product. The better
model is lazy and transactional:

```text
anchor query "deprecation headers"
```

should inspect the live repo state, Git state, existing `.anchor` facts, and
cheap local candidates. Then it should parse or expand only the files/chunks it
needs.

`anchor build` can exist as an optional prewarm command, similar to:

```text
git gc
git update-index
```

Useful for speed, but not required for the core workflow.

## Query

`anchor query` is the read/navigation surface.

It should not verify writes. It should not be a separate planning ceremony. It
should answer: "where should the agent look next, and what exact handles can it
act on?"

Query still verifies its own reads. If Anchor uses cached metadata, it must
check that the cache still matches the live repo. If a handle, file hash, chunk
hash, or derived fact is stale, Anchor should refresh it or block the result and
say the context is stale. The agent should not have to manually notice this.

Example:

```text
anchor query "deprecation response headers"
```

Expected shape:

```text
intent: deprecation response headers

likely_files:
  - fastapi/routing.py
    why: runtime response construction and route handling
  - fastapi/applications.py
    why: public API configuration surface
  - tests/test_response_headers.py
    why: response header assertions

handles:
  - file:fastapi/routing.py
  - chunk:fastapi/routing.py#APIRoute.get_route_handler
  - test:tests/test_response_headers.py

next:
  - anchor view <handle>
  - anchor patch <handle>
```

No token budget should be part of this output. Efficiency is measured by Anchor,
but the agent-facing result should focus on handles, current code facts, likely
tests, and valid next actions.

How query finds candidates:

- cheap text and filename search
- import and package metadata
- Git change history
- previous Anchor receipts and failures
- local project memories
- lightweight parsing of candidate files
- test names and test imports
- optional semantic search as a helper, not the whole system

The query result should be useful even when the repo was never prebuilt.

## View

`anchor view` is the current-code surface.

It should show the exact code around a handle with source hashes and ownership
metadata:

```text
anchor view chunk:fastapi/routing.py#APIRoute.get_route_handler
```

The agent should get:

- current source text
- file hash
- chunk hash
- owner path
- parent context when needed
- related tests when known

View must also verify freshness automatically:

```text
agent asks for handle H
Anchor checks H against the live file
Anchor refreshes derived metadata if needed
Anchor returns current code only if the handle still matches
Anchor blocks with stale-context if the handle no longer points to the same code
```

So the read path is not passive. It is a verified read. The agent sees normal
code context, but Anchor has already checked whether that context is current.

This is how Anchor replaces repeated `sed`, `cat`, and broad file reads with a
smaller, fresher code surface.

## Patch

`anchor patch` is the write surface.

This is where Zero-lang's lesson matters: query is read-only; patch is where
validation happens automatically.

The normal flow should not be:

```text
anchor query
anchor view
anchor patch
anchor verify
```

The normal flow should be:

```text
anchor query/view
anchor patch  # write + automatic verification before accept
```

Patch must be a transaction:

```text
agent submits patch against handle
Anchor checks the handle exists
Anchor checks the old hash is still fresh
Anchor checks locks and ownership
Anchor checks patch scope
Anchor checks project quality rules
Anchor runs or requires the focused verification path
Anchor accepts or rejects the write
Anchor records a receipt
```

Example result:

```text
patch: accepted
fresh-context: ok
lock: ok
scope: ok
quality-rules: ok
verification: ok
receipt: .anchor/objects/receipts/...
```

Verification is automatic in the write path. A separate `anchor check` can
exist for humans, debugging, CI, or broad verification, but agents should not
need to call it after every accepted patch just to prove the patch applied.

## Quality Rules

Anchor should treat AI slop as a write-time problem, not only a review problem.

The quality layer should combine Ponytail-style rules with project-local rules:

- no broad rewrite for a narrow behavior change
- no random abstraction without repeated local need
- no unused helper layer
- no duplicated logic when an existing owner exists
- no hidden behavior change outside the requested scope
- no passing test with fragile or unreadable implementation
- code must match project naming, error style, and test style

This does not mean Anchor can perfectly judge all code. It means the write path
should reject or flag obvious low-quality execution before it lands.

## Upper Anchor: Prompt And Context Repair

OpenViking's filesystem paradigm fits here, not in lower code execution.

Memory, resources, and skills are not naturally a filesystem. OpenViking turns
agent context into navigable paths. Anchor can use the same idea for prompt
repair and task framing:

```text
.anchor/context/
  resources/
    architecture.md
    dependencies.md
    testing.md
    style.md
  memories/
    successful_fixes/
    failed_runs/
    recurring_agent_mistakes/
  skills/
    add_api_endpoint.md
    debug_failing_test.md
    refactor_function.md
```

When the user says:

```text
fix deprecation headers
```

Upper Anchor can attach:

- relevant project resources
- prior failures on similar tasks
- known test locations
- reusable workflow skills
- constraints from project rules

This makes the prompt better before the agent starts acting.

## Lower Anchor: Execution Transactions

Lower Anchor controls what happens when the agent touches code.

This is where the mounted workspace or file-boundary idea belongs:

```text
agent
  -> normal code operations
  -> Anchor transaction layer
  -> real repo
```

A filesystem mount is not the invention. Mounts, FUSE, overlayfs, bind mounts,
and Docker workspaces already exist.

The invention is making the boundary agent-aware:

```text
read(path)
  -> check live source against cached facts
  -> refresh or block stale handles
  -> record fresh context hash
  -> return handle-aware current code view

write(path, patch)
  -> require fresh read
  -> check lock/conflict
  -> check scope and quality rules
  -> run focused verification
  -> accept or reject
```

This is the difference between a plain filesystem and Anchor. A normal
filesystem serves bytes. Anchor serves current, checked context and refuses to
pretend stale context is safe.

## Multi-Agent Execution

Anchor should support many agents working on the same repo or related workspaces.

The important units are not only files. They can be:

- path locks
- chunk locks
- symbol locks
- test locks
- session locks
- behavior locks

Example:

```text
agent-a owns chunk:src/auth.py#login
agent-b owns chunk:src/billing.py#create_invoice
agent-c is blocked from writing src/auth.py#login until agent-a releases
```

The goal is not only to prevent conflicts. The goal is to let parallel agents
work safely without broad file-level blocking when their work is actually
independent.

## Receipts

Anchor logs are not enough.

The receipt should be machine-checkable proof:

```text
read this handle at this hash
patched this handle from old hash to new hash
held this lock
followed these project rules
ran these checks
accepted or rejected for these reasons
```

This gives humans and teams a way to inspect the execution, not just the final
diff.

## Difference From OpenViking

OpenViking is a context database for agent memory, resources, and skills.

Anchor can borrow the path-based context idea for upper prompt repair:

```text
resources / memories / skills
```

But code is already a filesystem. Anchor should not claim it invented code
navigation as files.

The difference:

```text
OpenViking = filesystem-like memory/context for agents
Anchor = transactional execution layer for code changes
```

## Difference From Zero

Zero makes a programming language where the graph is the source of truth and
`.0` is the human projection.

Anchor must not do that to normal repositories.

Anchor's source of truth is still the repo source:

```text
source repo -> Anchor execution database -> checked reads/writes
```

The useful lesson from Zero is the query/patch split:

```text
query/view = read facts and handles
patch = checked write with automatic validation
```

Anchor should copy that discipline, not Zero's source-of-truth model.

## Difference From JCode And Agent Runtimes

JCode, Codex, Claude Code, Cursor, and similar tools own the agent runtime or
developer experience.

Anchor should own the workspace execution substrate.

```text
agent runtime = thinks, chats, plans, calls tools
Anchor substrate = controls how code reads/writes reach the repo
```

That is why Anchor can be neutral. It does not need to be the best agent. It
needs to make every agent's code execution safer, cheaper, and cleaner.

## Difference From Git

Git stores accepted history.

Anchor stores and governs attempted execution.

```text
Git:
  what changed after acceptance

Anchor:
  what was read
  what was attempted
  what was blocked
  what was verified
  why the write was accepted or rejected
```

Git remains the source control system. Anchor is the execution control layer
before Git commits and pull requests.

## Local And Cloud

Local Anchor:

- runs in one developer workspace
- improves one agent session or many local sessions
- stores `.anchor` state locally
- enforces fresh reads, checked writes, locks, and receipts

Cloud/team Anchor:

- coordinates many developers and many agent sessions
- shares locks and receipts
- stores team-visible timelines
- connects to CI/review systems
- gives a central view of agent execution quality

The cloud version is not a GitHub replacement. It is the shared execution layer
for AI agents before changes become PRs or commits.

## Where Zev Fits

Zev is separate from Anchor.

Anchor controls execution on normal code. Zev may later change the representation
agents read and write:

```text
normal source
  -> Zev representation
  -> Anchor read/write transaction
  -> normal source
```

Anchor must be useful without Zev. Zev can make the code surface more compact
and agent-native later.

## Contributor Mental Model

When adding to Anchor, ask:

1. Does this reduce blind agent search/read/write work?
2. Does this force fresh context before writes?
3. Does this make the patch smaller, cleaner, and easier to review?
4. Does this prevent stale or conflicting multi-agent changes?
5. Does verification happen automatically at the write boundary?
6. Does the result create useful execution proof?

If not, it may be normal developer tooling, but it may not belong in Anchor.

## Final Vision

Anchor is an AI-aware transaction layer for software work.

It keeps the source repo normal, keeps agents flexible, and adds the missing
execution rules around code:

```text
better prompt context
focused query/view
fresh checked patch
automatic verification
quality rules
multi-agent locks
machine-checkable receipts
```

That is how Anchor improves efficiency, quality, and safety without becoming a
new programming language, a Git replacement, or just another code search tool.

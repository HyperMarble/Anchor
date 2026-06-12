# Anchor Working Spec

Last updated: 2026-06-12. Branch: `dev`.

This is the agreed plan from the June 10-11 working sessions. It is the only
roadmap. Anything not in this file is out of scope until the items here are
done and measured.

## North star

Three numbers, measured by `benchmark/bench_pillars.py` (the 100-task replay
eval) and, for end-to-end proof, the DeepSWE pair runner:

| Pillar     | Target | Current (100 tasks, 5 repos)              |
|------------|--------|-------------------------------------------|
| Efficiency | 70%+   | +80% on real repos (negative on tiny Mux) |
| Quality    | 70%+   | recall 0.82, top1 0.45, top3 0.66         |
| Safety     | ~100%  | 5/5 mechanisms, receipts now gate default |

Every change in this spec must either move one of these numbers or reduce
code. A change that does neither does not ship.

## Principles

1. **Subtraction over addition.** Prefer deleting a flag to adding one. A
   flag is a decision deferred to the user; the tool should decide.
   Precedent: `gate --require-receipts` was deleted and made the default.
2. **Defaults over modes.** New behavior ships as the default or not at all.
   The only sanctioned mode is `ANCHOR_STRICT` (fail-open dev vs fail-closed
   CI), and it should eventually move into config.
3. **Measured or it didn't happen.** Ranking/retrieval changes are judged
   only by the 100-task eval. No change lands on taste. Precedent: the
   max-pool change was reverted after the eval said no (and will be retried
   fairly, see W3).
4. **The eval is ground truth, the agent run is proof.** Replay eval for
   iteration speed; DeepSWE pair run for the claim that matters.

---

## W1. CLI surface cleanup (subtraction pass)

**Problem.** Commands accumulated optional flags that encode indecision.
Audit findings:

- `Edit` takes four `Option`al mode flags (`--action`, `--pattern`,
  `--symbol`, `--content`) whose valid combinations are implicit. Invalid
  combinations (e.g. `--action replace` without `--pattern`) fail at runtime
  instead of parse time.
- `Context --full` and `--bundle` are presentation toggles; decide whether
  sliced output plus `--full` survives, or slicing is simply the behavior.
- `Protect` takes a positional defaulting to `"status"` — fine, leave it.
- `Task`, `Search`, `Trace` limits are fine (bounded integers with sane
  defaults).

**Change.**

1. Split `Edit` into explicit subcommand forms so clap enforces validity:
   `edit replace <path> --pattern --content`, `edit symbol <path> <name>
   --content`, `edit insert <path> --pattern --content`,
   `edit delete <path> --pattern`. Keep `--expect-hash` on all (it is a real
   safety contract, not indecision).
2. Decide `Context` slicing: slicing stays the default; keep `--full` only
   if the benchmark or agent transcripts show real need; delete `--bundle`
   if unused in practice (grep transcripts/tests first).
3. No new flags anywhere without a line in this spec justifying why it
   cannot be a default.

**Acceptance.** `anchor edit` invalid combinations fail at parse time; flag
count net-down across the CLI; all existing tests pass (update them to the
new forms); README command list updated.

**Size.** ~1 day.

## W2. Recall to ~99% (quality pillar, part 1)

**Problem.** Weighted recall is 0.82. The weak repo is terminal-bench
(0.58): diagnose *why* before changing anything (likely repo shape: large
mixed-language tree, docs/tasks directories, intents that name task ids
rather than code terms).

**Change.**

1. Diagnostic first (same method as the Luna registry diagnosis): print
   truth vs workspace for every terminal-bench miss, categorize the misses,
   then fix the top category only.
2. Recall fixes belong in candidate admission (what enters the workspace),
   not ranking. Ranking is W3's problem.

**Acceptance.** terminal-bench recall ≥ 0.85 without any other repo
regressing; weighted recall ≥ 0.90. Stretch: 0.95+.

**Size.** 1-2 days, mostly diagnosis.

## W3. Retrieval bake-off (quality pillar, part 2)

**Problem.** top1 0.45 / top3 0.66. Lexical scoring is blind to vocabulary
mismatch (intent says "logged out users keep credentials", code says
`rotate_refresh_token`). Two prior ranking tweaks failed because they were
shipped on n=1 diagnosis; this workstream settles ranking with data.

**Change.** Three contenders on the identical 100-task eval:

1. **lexical-only** (current, the baseline)
2. **Semble hybrid**: add `/Volumes/Hak_SSD/semble` (MinishLab, MIT) as a
   retrieval backend — static embeddings (potion-code-16M, CPU, <1s
   full-repo index) + BM25 with path enrichment, RRF fusion, auto-alpha.
   Integration: subprocess first; no Rust port until it wins.
3. **Semble + history leg**: fuse Anchor's history index as a third RRF leg
   (the only leg that fixes the Luna registry-file class).

Notes: Semble max-pools embeddings per file — the max-pooling idea gets its
fair trial here, not as another hand-tuned lexical tweak. Embedding cache
keys off Anchor's existing content hashes for incremental reindex.

**Acceptance.** Winner by weighted top3 (primary) and top1 (secondary) with
no recall regression. If Semble variants don't beat lexical by ≥5 points
top3, keep lexical and close the workstream — do not ship complexity for a
tie. If a variant wins, tier-2 (Rust-native, symbol-level embeddings using
Anchor's symbol index instead of text chunks) becomes a separate decision.

**Size.** 2-3 days including the bake-off runs.

## W4. The `expected` column (surprise, the wm idea collapsed into Anchor)

**Problem.** Anchor records what happened and whether it was checked; it
cannot record what the agent *believed* would happen. Without that, "agent
exploring" and "agent's model of the code is broken" look identical in the
event log.

**Change.** One column, not a product:

1. `anchor check` gains an optional declared expectation:
   `anchor check --expect pass -- pytest tests/test_refund.py`
   (values: `pass` | `fail`). Recorded in the check event meta as
   `expected=pass`.
2. `EventSummary` counts `surprises` (observed != expected) and
   `expected_checks` (checks that carried an expectation).
3. Quality profile: an unresolved surprise docks the score (same family as
   `unresolved_failed_check`); declaring expectations never lowers a score
   (no penalty for honesty).
4. Receipt and `status` surface surprise count.

This is deliberately the *entire* v1. No prediction schemas, no gating SDK,
no forks. If surprise counts prove useful in real runs, the richer
prediction ledger (diff-scope claims, behavior claims) is a future spec.

Exception to the no-new-flags rule: `--expect` is the workstream's whole
point — it is the expected column.

**Acceptance.** A DeepSWE/agent run produces receipts showing
expected/observed/surprise counts; unit tests cover surprise scoring; zero
new commands.

**Size.** ~1 day.

## W5. Niya: forward model of the diff (separate repo, feeds W4 later)

**Problem.** Niya framed as a mini-CWM loses to Meta on resources and
misses the point (CWM exists and is inert — the value is the loop, not the
weights). Reframe: a small *surprise detector* — given a diff, predict
consequences (which tests flip). 70% accuracy is a useless oracle but a
valuable smoke alarm.

**Change.** CPU-only proof of life in the Niya repo:

1. Miner: walk Mux's 239 commits (suite runs in ~2.5s). For each commit:
   checkout parent, run suite, apply diff, rerun, record
   `(diff features, flipped tests)`. Extend to Luna/GitNexus for diversity.
2. Model: Niya's existing hashed-feature + tiny NumPy scorer, repointed at
   "predict flip set / predict any-break probability".
3. Backtest on held-out commits: surprise AUC (does the model flag the
   diffs that actually break things?).

**Acceptance.** AUC meaningfully above chance (>0.70) on held-out commits.
If reached, a later spec wires it as the predictor behind W4's expected
column. If not reached, the reframe failed cheaply — write down why.

**Size.** 2-3 days. No GPU.

## W6. DeepSWE end-to-end proof (the claim that matters)

**Problem.** No graded with-Anchor vs without-Anchor result exists anywhere
(all prior harbor runs died at agent install; codex/claude pair runs never
reached the verifier).

**Change.** Run `benchmark/run_deepswe_codex_pair.py` (codex available now)
and/or `run_deepswe_claude_pair.py` on `python-statemachine-state-data-
scoping`, both modes, through the Docker verifier (Colima). Collect:
verifier reward, tokens, tool calls, duration, receipt quality. Then 2-3
more tasks for a small table.

**Acceptance.** At least one task with verifier rewards recorded for both
baseline and anchor modes. No target number — this is measurement, not
marketing. Whatever it says goes in the README honestly.

**Size.** Half a day of babysitting runs, given codex access.

## Known bugs still open (fix opportunistically, no redesign)

- Raw fs writes outside the anchor library are invisible until `status`
  audit (library path records `write.raw`; direct writes don't). Real fix
  is the receipt gate in CI (shipped); deeper tracking is *not* planned.
- Two lock implementations (in-process `LockManager` + Go lockd) with
  duplicated semantics. Consolidate only when it causes a real bug.
- Tiny-repo overhead (Mux eff -0.56): intake should emit a slimmer packet
  when the repo is under ~20 source files. Low priority; the market is
  large repos.

## Cut list (discussed, deliberately not happening)

- MCP server (CLI + git-gate enforcement instead; revisit only with
  adoption evidence)
- wm as a separate SDK/product (collapsed into W4's expected column)
- Zev integration, lockd load-testing, cloud mode, identity/auth
- Sentrux integration (nice check provider someday; adds no pillar points
  now)
- Any new ranking heuristics outside the W3 bake-off

## Order of work

W1 (cleanup) → W6 (proof run, while codex access is live) → W2 (recall) →
W3 (bake-off) → W4 (expected column) → W5 (Niya, parallel anytime — it's a
different repo).

W6 jumps the queue whenever codex access is available: graded proof is the
scarcest resource.

---

## Cloud shape — decided, not scheduled

Decision (2026-06-12): Anchor Cloud is the GitHub move applied to Anchor's
own data — local CLI stays the product, the cloud hosts the *shared* half of
`.anchor/`. It is not a runtime: no hosted agents, no environments, no
inference, no code hosting. Agents keep running wherever they already run
(Claude Code, Cursor, Ona, codex) and push state to the remote.

Three components, in adoption order:

1. **Gate as a GitHub App** — CI check verifying PR receipts (every changed
   file has a write event, checks ran, score over threshold), posts the
   receipt summary as a PR comment, blocks merge otherwise. The "no receipt,
   no merge" enforcement made mandatory. Smallest piece, the wedge.
2. **Shared lockd** — the existing lockd protocol served across machines:
   cross-developer/cross-agent claims and blocks, visible.
3. **Receipt sync + team dashboard** — sessions push event logs; team sees
   active sessions, claimed symbols, quality scores, surprises.

Business boundary matches the value boundary: local CLI free forever
(single-player wedge), team cloud paid (open-core).

Design obligations this decision creates *now* (cheap, do during normal
work): lockd's persistence format and the receipt/event schema must remain
serializable and versioned, assuming a remote will read them someday.

Build trigger: pillars proven (W2/W3/W6 done) AND a real team asking for
multiplayer. Until both, this section is a decision, not a workstream.

# Graph-Native Code Generation via Typed Diffusion and Execution Feedback

## A Research Paper Draft

*Status: Hypothesis stage with validated sub-claims. Core experiments needed before architecture decisions.*

---

## Abstract

Current code generation models treat source code as flat token sequences, ignoring that programs are fundamentally graphs — call graphs, type graphs, ASTs, data flow graphs. We propose an architecture that makes this graph structure first-class: a typed program graph substrate replaces O(n²) transformer attention with O(n + edges) typed edge message passing, graph diffusion with execution feedback replaces autoregressive token generation, and a hard REPL gate prevents hallucination at the architectural level. The result is a model that reasons about code the way a compiler does — through structure — rather than the way a language model does — through token statistics.

---

##  1. The Core Problem

Transformers achieve code generation through self-attention: every token attends to every other token, O(n²) cost, and the model must learn from token co-occurrence statistics which tokens are structurally related to which other tokens.

This is expensive and redundant. The compiler already knows which tokens are related. The call graph, the type system, the AST — these are pre-computed relevance maps. The model re-learns at O(n²) cost what the program's structure states explicitly.

**The central claim of this paper:** For code, the graph structure pre-computes what Q·K attention computes at runtime. Therefore Q·K is redundant for structural reasoning over programs. Replace it with direct typed edge traversal.

---

## 2. Prior Work and Why It Falls Short


| Architecture              | Complexity   | Problem                                                                                                        |
| ------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------- |
| Transformer (GPT, Claude) | O(n²)        | Relearns structure the compiler already knows. Context window limits.                                          |
| F-Net                     | O(n log n)   | Position-based only. No content-dependent routing. Missing matrix problem.                                     |
| Mamba (SSM)               | O(n)         | State compression loses specific position recall. Can't retrieve "what was the type of variable x on line 47". |
| Wave Field LLM            | O(n log n)   | 1D wave dynamics. Same missing matrix problem as F-Net. GitHub deleted — likely didn't scale.                  |
| GNN (message passing)     | O(n + edges) | Over-smoothing from averaging. Limited long-range propagation.                                                 |
| GraphCodeBERT             | O(n²)        | Transformer + data flow graph as *auxiliary input*. Graph-augmented, not graph-native. Still quadratic.        |
| Code2Vec                  | O(paths)     | AST path aggregation. Good for embedding, not generation.                                                      |
| MOE Transformers          | O(n²/k)      | Learned routing. Still attention. Sparse but not structured.                                                   |


### The Key Distinction

All graph-aware coding models (GraphCodeBERT, Code2Vec, etc.) add graph as auxiliary input to a transformer. They use the graph to *inform* attention, not to *replace* it. The Q·K computation still runs. They're graph-augmented transformers, not graph-native models.

Our proposal: the graph IS the computation substrate. No Q·K. No averaging. Typed edges define information flow completely.

---

## 3. Why Typed Edges Eliminate Averaging

Standard GNN message passing: each node updates by averaging over all neighbors.

```
h_v = Aggregate({h_u : u ∈ N(v)})
```

Why average? Because with homogeneous edges, you don't know which neighbors matter more than others. So you average everyone and hope the model learns to weight them.

Code graphs have **typed edges**. Anchor's graph has 13 distinct edge kinds:

- `Calls` — function A calls function B
- `Defines` — file defines a symbol
- `Imports` — file imports from another
- `Contains` — module/class contains child symbol
- `Implements`, `Extends` — type hierarchy
- `UsesType`, `Parameter`, `Returns`, `References` — type relationships
- `ApiCall` — cross-language HTTP endpoint matching
- `EnvRef` — environment variable reference

When you traverse a `Calls` edge, you know exactly what the relationship means. When you traverse an `Implements` edge, you know exactly what information is relevant. There is no ambiguity requiring averaging.

**Typed edges eliminate the need for averaging.** Instead of:

```
aggregate all neighbors
```

You do:

```
for each edge type:
    follow edges of that type
    apply type-specific transformation
    route to correct destination
```

This is not a learned approximation of relevance. It's deterministic routing along pre-computed structure.

### The Averaging Analogy

If you're at a crossroads and don't know which road leads to your destination, you might walk a little down each road and combine what you see — averaging. But if you have a map showing exactly which road connects to where, you follow the correct road directly. Code graphs are the map.

---

## 4. The Full Architecture

### 4.1 Graph Substrate (KV Cache Layer)

The entire codebase lives as a typed program graph, stored in GPU KV cache rather than loaded into context window.

```
Nodes: symbols (functions, classes, types, constants, modules)
Edges: typed relationships (13 kinds)
Storage: GPU KV cache — O(1) lookup per node by (file, name) key
```

This eliminates the context window problem entirely. A 1M line codebase is ~289 top-level symbols per average module. The graph fits in KV cache. The model never needs to "read files" — it traverses the graph.

Anchor's implementation validates this is practical: graph build on a large Rust codebase takes seconds, queries run in milliseconds on CPU (petgraph DiGraph). On GPU with KV cache, this is sub-millisecond.

### 4.2 Intent Parsing

Natural language query → task graph (what the agent needs to do).

The intent parser is lightweight — not a full transformer. It maps:

- "fix the login bug" → `{task: debug, target: login, scope: auth module}`
- "add pagination to the users endpoint" → `{task: feature, target: users_handler, scope: api layer}`

This produces a structured query that activates specific nodes in the graph.

### 4.3 MOE Routing via Graph Traversal

The task graph identifies entry nodes. From those nodes, we traverse the typed graph — following only relevant edges, activating only relevant symbols.

This is Mixture-of-Experts routing, but:

- **Deterministic**, not learned
- **Structured** by graph topology, not by a router network
- **Causally grounded** — only nodes actually connected to the task activate

For a bug in `login()`:

1. Activate `login` node
2. Follow incoming `Calls` edges → find callers
3. Follow outgoing `Calls` edges → find callees
4. Follow `UsesType` edges → find relevant types
5. Follow `Imports` edges → find dependencies

Only these nodes activate. Everything else stays dormant. Effective computation scales with task scope, not codebase size.

### 4.4 Typed Edge Message Passing

Each activated node aggregates information from its neighbors via type-specific transformations:

```
h_v^(t+1) = ⊕_{k ∈ EdgeKinds} W_k · Aggregate_{u: (u,v) ∈ E_k}(h_u^(t))
```

Where:

- `E_k` = edges of kind k
- `W_k` = learned weight matrix for edge kind k (13 matrices, one per edge type)
- `⊕` = concatenation or gated summation

No averaging across all neighbors. Each edge type has its own learned transformation. Information flows through structure, not through learned attention patterns.

**Complexity**: O(n + edges) for sparse graphs. Anchor's codebase graphs are highly sparse — most symbols connect to 2-5 others, not hundreds.

### 4.5 Graph Diffusion for Generation

Code generation via graph diffusion, not autoregressive token prediction.

**Forward process (training)**: Start from complete, correct program graph. Iteratively remove/corrupt edges and node content. Learn to reverse this process.

**Reverse process (inference)**: Start from task specification (sparse seed graph). Iteratively add structure — new nodes, new edges — until complete program graph emerges. Renderer converts graph to valid code.

```
Seed: {task: "add pagination", entry: users_handler}
Step 1: Add new_node(get_page_params, Function)
Step 2: Add edge(users_handler → get_page_params, Calls)
Step 3: Add new_node(paginate_query, Function)
Step 4: Add edge(get_page_params → paginate_query, Calls)
Step 5: Fill node content (function bodies)
...
```

**Why graph diffusion beats autoregressive generation:**

1. Structure emerges before content — no structurally invalid intermediate states
2. Each step is a graph operation with defined semantics
3. Diffusion is naturally parallel — all nodes denoise simultaneously, not token by token
4. Reversible process provides principled uncertainty quantification

**Prior work on graph diffusion**: GruM (ICML 2024), EDP-GNN, NeurIPS 2024 — all applied to molecular and social graphs. NOT applied to source code generation. This is the gap.

### 4.6 REPL as Hard Architectural Gate

The REPL is not a tool the model can choose to use. It is a hard gate in the architecture.

```
graph_edit → REPL(validate) → { success: continue | failure: revert_and_replan }
```

The model **cannot** emit output that hasn't passed REPL validation. This is architectural, not behavioral. The difference:

- Behavioral constraint: "the model learned to check syntax before outputting" — can be hallucinated away
- Architectural constraint: "output circuit is physically gated on REPL success" — cannot hallucinate through it

At each diffusion step that produces or modifies code:

1. Render the current graph state to code
2. Feed to REPL (compile + typecheck + test)
3. If pass: REPL output feeds back as node embeddings, diffusion continues
4. If fail: error location → specific node/edge in graph → localized revert → re-denoise that region

The REPL output is not discarded — it becomes embedding signal. Parse errors point to specific AST nodes. Type errors point to specific `UsesType` edges. Test failures point to specific callee chains.

**This closes the hallucination loop architecturally.**

### 4.7 Counterfactual Testing

Beyond basic compile/test, the model runs counterfactual tests:

- Remove a node → run tests
- If tests fail: node is causally necessary → high-confidence edge
- If tests pass: node may be dead code → flag for removal

This builds causal understanding of the codebase, not just pattern matching of what usually appears together. The model learns WHY code exists, not just WHAT code exists.

### 4.8 Execution RLHF

Reward signal: does the generated code compile? Do tests pass?

```
reward = w1 * compile_success + w2 * test_pass_rate + w3 * runtime_perf
```

This is:

- **Free** — no human labelers, no preference datasets
- **Ground truth** — the compiler doesn't hallucinate, tests don't lie
- **Dense** — feedback at every step, not just at final output
- **Causally precise** — error traces point to exact graph locations

Anchor generates training trajectories automatically: graph state → query → result → action → outcome. Across thousands of sessions this becomes a labeled dataset with ground truth reward signal.

---

## 5. Architecture Comparison


| Dimension                | Transformer                           | Our Architecture                     |
| ------------------------ | ------------------------------------- | ------------------------------------ |
| Attention complexity     | O(n²)                                 | O(n + edges)                         |
| Context window           | Hard limit (200K tokens max)          | No limit (graph in KV cache)         |
| Structure representation | Learned from token co-occurrence      | Explicit typed graph edges           |
| Routing                  | Learned Q·K                           | Deterministic edge traversal         |
| Generation               | Autoregressive token prediction       | Graph diffusion + renderer           |
| Hallucination            | Behavioral guardrails                 | Hard REPL gate                       |
| Training signal          | Human preference labels               | Compile/test execution               |
| Over-smoothing           | N/A                                   | Eliminated by typed edges            |
| Cross-file reasoning     | Context window must include all files | Graph traversal has no file boundary |
| Multi-language           | Re-trained per language               | Graph edges span language boundaries |


---

## 6. The Graph as Attention — Formal Argument

In transformer self-attention:

```
Attention(Q, K, V) = softmax(QK^T / √d_k) · V
```

The QK^T matrix is learned content-dependent routing — which tokens should attend to which other tokens. This must be learned because in natural language, coreference is ambiguous:

```
"The cat sat on the mat because it was tired"
```

"it" could be cat or mat. The model must learn to resolve this from context.

In code:

```rust
fn login(user: &str) {
    validate_token(user);  // Calls edge: login → validate_token
}
```

The call graph already contains the relevance map. `login → validate_token` is an explicit edge. The compiler resolved it. Learning QK^T to re-discover this edge is redundant computation.

**Claim**: For code, the typed program graph is a pre-computed QK^T matrix. Routing is already solved. Only V (value transformation) needs learning.

What still requires learning:

- What information at each node means semantically (V transformation)
- How meaning transforms as it propagates through typed edges (W_k matrices)
- What the model should generate given the propagated graph state (decoder)

What doesn't require learning:

- Which nodes are relevant to which other nodes (graph structure)
- How to route information (edge types)

---

## 7. Open Problems

### 7.1 Intra-function Data Flow

The call graph doesn't capture data flow within a function body:

```rust
fn process(data: Vec<Item>) {
    for i in 0..data.len() {      // data.len() relevant here
        transform(data[i]);        // no call edge between these lines
    }
}
```

The variable `data` connects `data.len()` and `data[i]` but this connection is in the Data Flow Graph (DFG), not the Call Graph or AST.

**Current state**: Tree-sitter extracts AST but not DFG. LSP partially has DFG. Compilers fully have it (LLVM IR, etc.).

**Options**:

1. Augment with LSP-derived DFG edges (expensive but doable per-codebase)
2. Use AST + call graph and accept intra-function reasoning requires some Q·K (hybrid model)
3. Treat function bodies as opaque blobs, only reason at function-call granularity

**Severity**: HIGH. Intra-function reasoning is a significant fraction of real coding tasks.

### 7.2 Renderer Design

Graph diffusion produces a typed program graph. A renderer must convert this to valid source code. The renderer must:

- Know the grammar of each target language
- Handle the fact that graph structure doesn't fully determine token order
- Produce syntactically valid code even for intermediate diffusion states

Existing work: abstract syntax tree pretty-printers. But these require a complete valid AST — our intermediate diffusion states are incomplete graphs.

**Options**:

1. Grammar-constrained generation: render only when graph is sufficiently complete
2. Tree diffusion: diffuse over ASTs directly (ordered trees, not general graphs)
3. Patch generation: graph edits → text diffs (simpler renderer, less generative power)

**Severity**: MEDIUM-HIGH. Generation interface is not obvious.

### 7.3 Pretraining Objective

What does the model train on before execution feedback?

Options:

- Masked node prediction (like MLM but on graph nodes)
- Edge prediction (link prediction on call graph)
- Trajectory prediction (given graph state + query, predict next graph edit)
- Distillation from existing transformer models on graph-structured inputs

**Severity**: HIGH. Wrong pretraining = model doesn't generalize.

### 7.4 Dynamic Graphs

During code generation, new nodes are being created that don't exist in the graph yet. The graph substrate is pre-computed from existing code. New code doesn't have a node until it's written.

**Solution sketch**: Temporary nodes in working memory (KV cache buffer). Graph is two-tier: permanent (existing codebase) + ephemeral (current generation). REPL validates ephemeral nodes, promotes them to permanent on success.

### 7.5 Multi-language Graphs

Anchor already handles 14 languages with unified node/edge types. But the learned transformations (W_k matrices) may need to be language-aware. A `Calls` edge in Rust has different semantics than a `Calls` edge in Python (ownership vs garbage collection implications).

**Partial solution**: Language as a node/edge attribute. W_k can attend to language attribute.

---

## 8. Validation Experiments (Cheapest to Most Expensive)

### Experiment 1 — Graph as Attention (1-2 GPU-days)

**Question**: Does graph structure replace Q·K for code lookup?

**Setup**: Small GNN on Anchor graph data. Task: given a query (e.g., "find where login is called"), predict the correct node using only graph structure, no Q·K.

**Baseline**: Transformer with same compute budget on same task.

**Interpretation**:

- GNN matches transformer → graph structure IS sufficient to replace Q·K for structural reasoning
- GNN fails → Q·K is doing something the graph doesn't capture

**Data source**: Anchor itself. Run 1000 queries, record graph state + correct answer. Already collected during normal use.

### Experiment 2 — Typed vs Homogeneous Edges (1 GPU-day)

**Question**: Do typed edges outperform homogeneous GNN?

**Setup**: Same task, two models — (a) all edges treated equally (standard GNN message passing), (b) typed edge transformations (W_k per edge type).

**Interpretation**: If (b) > (a), typed edges capture semantically meaningful distinctions.

### Experiment 3 — Over-smoothing Check (1 GPU-day)

**Question**: Is over-smoothing actually a problem at codebase scale?

**Setup**: Standard GNN, vary number of layers (2, 4, 8, 16). Measure performance degradation.

**Interpretation**: If performance degrades after 4 layers → deeper propagation is problematic → wave dynamics or other remedies needed. If stable at 8-10 layers → standard GNN is fine (Anchor graphs have ~6-10 depth).

### Experiment 4 — Graph Diffusion for Code Edits (1-2 GPU-weeks)

**Question**: Can graph diffusion generate correct code edits?

**Setup**: Training data = Anchor write operations (each write is a graph edit). Task: given graph state before + natural language description, predict graph edit.

**Baseline**: Transformer generating text diffs.

**Interpretation**: If graph diffusion matches or beats text diff generation → graph-native generation is viable.

### Experiment 5 — REPL-Gated Training (Full training run)

**Setup**: Full architecture. Train with execution RLHF. Compare to standard RLHF with human preferences.

**Hypothesis**: Execution feedback signal is more precise (points to exact graph location) and more scalable (no human labelers) than preference feedback.

---

## 9. Connection to Anchor

Anchor is not just motivation for this architecture — it's the training data generator and the deployment substrate.

Every Anchor session produces:

```
graph_state → query → graph_result → agent_action → outcome
```

The outcome (compile success, test pass) is ground truth reward signal. Across thousands of sessions, this becomes a labeled trajectory dataset for training the graph-native model.

Anchor's graph representation is the exact graph substrate the model would reason over:

- 289 top-level symbols in Anchor itself (confirmed via `anchor context`)
- 13 edge kinds already defined and populated
- Qualified index enables O(1) per-node lookup — exactly the KV cache operation

The model trained on Anchor trajectories would be deployed as an Anchor-compatible agent — reading the same graph, writing via the same write tool, validated by the same REPL.

**Anchor is both the proof of concept and the training data generator.**

---

## 10. Honest Assessment

### Strong Case FOR

- Graph structure genuinely pre-computes Q·K for structural code reasoning — mathematically sound
- O(n + edges) is real and validated (Anchor: milliseconds on CPU, sparse graphs)
- Typed edges eliminate averaging — not a conjecture, it's just not averaging
- Execution RLHF is free, dense, and ground truth
- Graph diffusion for molecular graphs already works (GruM ICML 2024) — code is a different domain but the mechanism transfers
- No one has done graph-native (not graph-augmented) code generation at scale
- Anchor already generates training trajectories

### Strong Case AGAINST

- Intra-function reasoning may still need Q·K — call graph doesn't have DFG
- Renderer design is unsolved — incomplete graphs don't pretty-print easily
- Two hard problems simultaneously: new architecture + new training paradigm
- GraphCodeBERT (graph-augmented transformer) may already capture most of the gain
- Wave Field LLM failure suggests wave dynamics alone don't scale — we dropped wave dynamics but the general lesson is caution
- Scaling requires GPU infrastructure — Anchor validates the concept on CPU but model training is different

### The Minimum Viable Experiment

GNN on Anchor graph data for code lookup tasks. Don't build the full architecture. Validate the core claim: graph structure replaces Q·K for code reasoning.

If it works → proceed to typed edges, then diffusion.
If it fails → the whole thesis is wrong, cut losses.

**Don't build the wave dynamics. Don't build the renderer. Don't train the diffusion model. Run Experiment 1 first.**

---

## 11. Research Contribution Summary

If validated, this paper contributes:

1. **The graph-as-attention theorem**: formal proof that typed program graphs pre-compute self-attention routing for code, with O complexity analysis
2. **Typed edge message passing for code**: elimination of averaging via semantic edge types, applied to 13 code relationship types
3. **Graph diffusion for code generation**: first application of graph diffusion models to source code (prior work: molecular and social graphs only)
4. **Hard REPL gate**: architectural (not behavioral) prevention of hallucination via execution feedback as a gate
5. **Execution RLHF**: dense, free, ground-truth reward signal from compile/test cycles as a replacement for human preference labeling
6. **Anchor trajectory dataset**: training data generator that produces graph_state → action → outcome trajectories at scale

---

## 12. Related Work to Read

- **GraphCodeBERT** (Guo et al. 2021) — closest prior work. Graph-augmented transformer, not graph-native.
- **GruM** (Jo et al. ICML 2024) — graph diffusion mixture for molecular generation. Primary graph diffusion reference.
- **EDP-GNN** — energy-based graph diffusion. Alternative diffusion formulation.
- **Code2Vec** (Alon et al. 2019) — AST path aggregation. Code representation baseline.
- **Mamba** — why SSMs fail at recall (state compression). Validates we need graph not SSM.
- **F-Net** — confirms missing matrix problem for non-attention mixing.
- **Neural graph signal processing** — wave equations on graphs literature (mostly dropped but cite for context).
- **RLHF from Code Execution** — various, DeepMind, Google. Execution feedback as training signal.

---

*Questions still open: DFG extraction at scale, renderer architecture, pretraining objective, multi-language graph handling, dynamic graph during generation.*

*Minimum experiment: 1-2 GPU-days. Don't commit to full architecture before Experiment 1 results.*
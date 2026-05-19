# SLM — Architecture & Vision

## What it is

A Small Language Model (SLM) coding agent. Separate project from Anchor.

Anchor = infrastructure (harness, locks, cache, context). SLM = the agent that uses Anchor.

---

## Core Architecture

**SLM generates internally in the custom language. Transpiler converts the output to the target.**

```
SLM → [custom language — internal only] → Anchor's transpiler → target output
```

The custom language never surfaces externally. It is purely the model's internal representation — what it thinks in. Anchor's transpiler handles the last mile — converting custom language back to real code in whatever target language is needed.

---

## Why Custom Language Internally

Classic code (Rust, Python) has noise the model doesn't need — semicolons, borrow checker syntax, boilerplate, type annotations. The model parses through all of that to get to the actual logic.

Custom language strips it down to semantic content only. Same information, less friction. Model does less work to understand and generate the same thing.

With solid docs: the model would prefer the custom language over classic code.

---

## Output Targets (Unlimited)

Because the transpiler handles output, the target is unlimited:

- Rust, Python, Go, TypeScript — standard coding agent use case
- Bash scripts — devops, automation
- Assembly, shellcode — cybersecurity use case falls in automatically
- Any language the transpiler supports

One model. One internal format. Transpiler determines the output. SLM is not language-specific.

---

## Training Plan

1. Anchor's transpiler generates (raw_code, custom_lang) training pairs across all supported languages — this is Anchor's job, not SLM's
2. Dev defines the custom language (in progress)
3. Fine-tune SLM on those pairs — model learns to read/write in custom language natively
4. Transpile function baked into the model weights — no external transpiler call needed at inference

---

## Relationship to Anchor

| | Anchor | SLM |
|---|---|---|
| What | Harness + infrastructure | Coding agent |
| Language | Any existing language | Custom language internally |
| Open source | Candidate | No — research project |
| Status | Building | Waiting on custom language |

SLM uses Anchor. Anchor does not depend on SLM.

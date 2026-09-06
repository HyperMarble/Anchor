#!/usr/bin/env python3
"""Benchmark project-aware prompt repair with local Ollama.

This is an early proof harness for Anchor's prompt-repair idea:

1. Read a human, fuzzy coding prompt.
2. Build a lightweight project profile from the current repo.
3. Create an Anchor-improved prompt with verified files, commands, and warnings.
4. Ask a local Ollama coding model for a plan with the raw and improved prompts.
5. Score whether the model mentions expected project facts and avoids wrong ones.

The benchmark is intentionally deterministic around the improvement step. The
LLM is only used as the downstream coding-agent stand-in.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import time
import urllib.error
import urllib.request
from collections import Counter
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Iterable


DEFAULT_MODEL = "qwen2.5-coder:7b"
DEFAULT_CASE_LIMIT = 1
IGNORE_DIRS = {
    ".git",
    ".anchor",
    "target",
    "benchmark/runs",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
}

LANG_BY_EXT = {
    ".rs": "Rust",
    ".go": "Go",
    ".py": "Python",
    ".js": "JavaScript",
    ".jsx": "JavaScript",
    ".ts": "TypeScript",
    ".tsx": "TSX",
    ".java": "Java",
    ".cs": "C#",
    ".rb": "Ruby",
    ".cpp": "C++",
    ".cc": "C++",
    ".cxx": "C++",
    ".hpp": "C++",
    ".h": "C++",
    ".swift": "Swift",
    ".md": "Markdown",
    ".toml": "TOML",
    ".json": "JSON",
    ".sh": "Shell",
}

ASSUMPTION_TERMS = {
    "express": "No Express app was detected.",
    "jest": "No Jest setup was detected.",
    "react": "No React app was detected.",
    "next": "No Next.js app was detected.",
    "python": "Python exists only in benchmark/hypothesis helpers, not the core CLI.",
    "npm": "No package.json workflow was detected.",
    "postgres": "No database layer was detected.",
    "elasticsearch": "No external search service was detected.",
}

ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
PATH_RE = re.compile(r"(?:`([^`]+)`|((?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+))")
PROMPT_CONTROL_PATTERNS = [
    (re.compile(r"\bignore\b.*\b(repo|rules|instructions|tests|context)\b", re.I), "Prompt asks the agent to ignore repo facts, rules, context, or tests."),
    (re.compile(r"\b(no need|dont|don't|skip)\b.*\b(test|check|verify)\b", re.I), "Prompt discourages validation."),
    (re.compile(r"\b(delete|remove|rewrite)\b.*\b(everything|all|whole)\b", re.I), "Prompt may be too destructive or broad."),
    (re.compile(r"\bquick\s*fix\b|\blol\b|\bthing\b", re.I), "Prompt is vague or casual enough to invite wrong assumptions."),
]
CASE_LIST_FIELDS = ("expected_terms", "avoid_terms")
PATH_LIKE_EXTENSIONS = set(LANG_BY_EXT) | {
    ".lock",
    ".yaml",
    ".yml",
    ".txt",
    ".sql",
    ".ini",
}
PATH_LIKE_PREFIXES = (
    "src",
    "test",
    "tests",
    "doc",
    "docs",
    "benchmark",
    "bench",
    "benches",
    "example",
    "examples",
    "script",
    "scripts",
    "crate",
    "crates",
    "package",
    "packages",
    "lockd",
)


@dataclass
class ProjectProfile:
    root: str
    languages: list[str]
    top_dirs: list[str]
    key_files: list[str]
    indexed_files: list[str]
    manifest_files: list[str]
    test_commands: list[str]
    symbols: list[str]
    product_memory: list["ProductMemoryEvidence"]


@dataclass(frozen=True)
class ProductMemoryEvidence:
    source: str
    kind: str
    detail: str


@dataclass
class RunScore:
    case_id: str
    mode: str
    score: int
    hidden_score: int
    visible_score: int
    expected_hits: list[str]
    missing_expected: list[str]
    copied_expected_hits: list[str]
    avoid_hits: list[str]
    valid_paths: list[str]
    valid_checks: list[str]
    actionability_hits: list[str]
    hallucinated_paths: list[str]
    copied_prompt_lines: list[str]
    leakage_penalty: int
    duration_sec: float
    output_chars: int


@dataclass(frozen=True)
class TargetHint:
    path: str
    reason: str


@dataclass
class BriefQuality:
    score: int
    verified_target_count: int
    assumption_warning_count: int
    prompt_risk_count: int
    check_count: int
    product_memory_count: int
    bloat_penalty: int


def validate_provenance(raw: Any, path: Path, line_no: int) -> dict[str, str]:
    source = f"{path}:{line_no}"
    if raw is None:
        return {
            "kind": "manual_curated",
            "source": "Anchor benchmark seed",
            "license": "repo-local example",
            "note": "Small repo-specific prompt written in issue-style language.",
        }
    if not isinstance(raw, dict):
        raise ValueError(f"{source}: field 'provenance' must be an object when provided")

    provenance: dict[str, str] = {}
    for field in ("kind", "source"):
        value = raw.get(field)
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{source}: provenance field {field!r} must be a non-empty string")
        provenance[field] = value

    for field in ("url", "license", "note"):
        value = raw.get(field)
        if value is not None:
            if not isinstance(value, str) or not value.strip():
                raise ValueError(
                    f"{source}: provenance field {field!r} must be a non-empty string"
                )
            provenance[field] = value

    return provenance


def validate_case(raw: Any, path: Path, line_no: int) -> dict[str, Any]:
    source = f"{path}:{line_no}"
    if not isinstance(raw, dict):
        raise ValueError(f"{source}: expected a JSON object, got {type(raw).__name__}")

    case = dict(raw)
    for field in ("id", "human_prompt"):
        value = case.get(field)
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{source}: field {field!r} must be a non-empty string")

    for field in CASE_LIST_FIELDS:
        value = case.get(field, [])
        if value is None:
            value = []
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            raise ValueError(f"{source}: field {field!r} must be a list of strings")
        case[field] = value

    notes = case.get("notes")
    if notes is not None and not isinstance(notes, str):
        raise ValueError(f"{source}: field 'notes' must be a string when provided")

    case["provenance"] = validate_provenance(case.get("provenance"), path, line_no)

    return case


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, start=1):
            line = line.strip()
            if line:
                try:
                    raw = json.loads(line)
                except json.JSONDecodeError as exc:
                    raise ValueError(f"{path}:{line_no}: invalid JSON: {exc.msg}") from exc
                rows.append(validate_case(raw, path, line_no))
    return rows


def write_jsonl(path: Path, rows: Iterable[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=True) + "\n")


def should_skip(path: Path) -> bool:
    parts = path.parts
    for ignored in IGNORE_DIRS:
        ignored_parts = tuple(ignored.split("/"))
        if ignored_parts == parts[: len(ignored_parts)] or ignored in parts:
            return True
    return False


def repo_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for path in root.rglob("*"):
        if path.is_file():
            rel = path.relative_to(root)
            if not should_skip(rel):
                files.append(rel)
    return sorted(files, key=lambda p: p.as_posix())


def detect_test_commands(root: Path) -> list[str]:
    commands: list[str] = []
    if (root / "Cargo.toml").exists():
        commands.extend(["cargo test", "cargo build --release"])
    if (root / "lockd" / "go.mod").exists():
        commands.append("cd lockd && go test ./...")
    if (root / "package.json").exists():
        commands.append("npm test")
    if (root / "pyproject.toml").exists() or (root / "pytest.ini").exists():
        commands.append("pytest")
    if (root / "docs" / "install.sh").exists():
        commands.append("bash -n docs/install.sh docs/uninstall.sh local_install.sh")
    if (root / ".git").exists():
        commands.append("git diff --check")
    return commands


def detect_manifest_files(root: Path) -> list[str]:
    candidates = [
        "Cargo.toml",
        "Cargo.lock",
        "lockd/go.mod",
        "lockd/go.sum",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "pytest.ini",
        "go.mod",
    ]
    return [file for file in candidates if (root / file).exists()]


def load_anchor_index(root: Path) -> tuple[list[str], list[str]]:
    """Use Anchor's generated symbol index when this repo has been built."""
    path = root / ".anchor" / "index" / "symbols.json"
    if not path.exists():
        return [], []
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return [], []

    indexed_files: set[str] = set()
    symbols: list[str] = []
    for item in data.get("symbols", []):
        name = item.get("name")
        file = item.get("path")
        if isinstance(name, str) and isinstance(file, str):
            indexed_files.add(file)
            symbols.append(f"{name} ({file})")
    return sorted(indexed_files), symbols[:120]


def extract_symbols(root: Path, files: list[Path], limit: int = 120) -> list[str]:
    patterns = [
        re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"),
        re.compile(r"^\s*(?:pub\s+)?(?:struct|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)"),
        re.compile(r"^\s*func\s+([A-Za-z_][A-Za-z0-9_]*)"),
        re.compile(r"^\s*(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)"),
        re.compile(r"^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)"),
    ]
    symbols: list[str] = []
    for rel in files:
        if rel.suffix not in {".rs", ".go", ".py", ".js", ".ts", ".tsx"}:
            continue
        try:
            text = (root / rel).read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for line in text.splitlines():
            for pattern in patterns:
                match = pattern.search(line)
                if match:
                    symbols.append(f"{match.group(1)} ({rel.as_posix()})")
                    break
            if len(symbols) >= limit:
                return symbols
    return symbols


def load_product_memory(root: Path) -> list[ProductMemoryEvidence]:
    path = root / ".anchor" / "product_memory.json"
    if not path.exists():
        return []
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return []

    evidence: list[ProductMemoryEvidence] = []
    for item in data.get("facts", []):
        if not isinstance(item, dict):
            continue
        source = item.get("source")
        fact = item.get("fact")
        if (
            isinstance(source, str)
            and isinstance(fact, str)
            and source
            and fact
            and (root / source).exists()
        ):
            evidence.append(ProductMemoryEvidence(source=source, kind="fact", detail=fact))

    for item in data.get("instruction_files", []):
        if not isinstance(item, dict):
            continue
        source = item.get("path")
        kind = item.get("kind")
        note = item.get("note")
        if (
            isinstance(source, str)
            and isinstance(kind, str)
            and isinstance(note, str)
            and source
            and kind
            and note
            and (root / source).exists()
        ):
            evidence.append(ProductMemoryEvidence(source=source, kind=kind, detail=note))

    deduped: dict[tuple[str, str, str], ProductMemoryEvidence] = {}
    for item in evidence:
        deduped.setdefault((item.source, item.kind, item.detail), item)
    ranked = sorted(
        deduped.values(),
        key=lambda item: (item.kind == "fact", item.source, item.detail),
    )
    return ranked[:8]


def instruction_memory_files(profile: ProjectProfile) -> list[str]:
    files = [
        item.source
        for item in profile.product_memory
        if item.kind != "fact" and repo_path_exists(profile, item.source)
    ]
    return sorted(dict.fromkeys(files))


def split_tokens(text: str) -> set[str]:
    tokens: set[str] = set()
    for raw in re.split(r"[^A-Za-z0-9]+", text):
        if len(raw) <= 2:
            continue
        raw = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", raw)
        tokens.update(part.lower() for part in raw.split() if len(part) > 2)
    return tokens


def build_profile(root: Path) -> ProjectProfile:
    files = repo_files(root)
    indexed_files, indexed_symbols = load_anchor_index(root)
    language_counts = Counter(
        LANG_BY_EXT[path.suffix] for path in files if path.suffix in LANG_BY_EXT
    )
    dir_counts = Counter(path.parts[0] for path in files if len(path.parts) > 1)
    key_files = [
        file
        for file in [
            "Cargo.toml",
            "README.md",
            "docs/install.sh",
            "lockd/go.mod",
            "src/bin/cli.rs",
            "src/cli/write.rs",
            "src/storage/anchor.rs",
            "src/storage/bm25.rs",
            "src/cache.rs",
        ]
        if (root / file).exists()
    ]
    return ProjectProfile(
        root=str(root),
        languages=[name for name, _ in language_counts.most_common(8)],
        top_dirs=[name for name, _ in dir_counts.most_common(8)],
        key_files=key_files,
        indexed_files=indexed_files,
        manifest_files=detect_manifest_files(root),
        test_commands=detect_test_commands(root),
        symbols=indexed_symbols or extract_symbols(root, files),
        product_memory=load_product_memory(root),
    )


def repo_path_exists(profile: ProjectProfile, path: str) -> bool:
    normalized = path.strip().rstrip("/")
    return bool(normalized) and (Path(profile.root) / normalized).exists()


def add_existing_target(
    hits: list[TargetHint], profile: ProjectProfile, path: str, reason: str
) -> None:
    if repo_path_exists(profile, path):
        hits.append(TargetHint(path, f"verified: {reason}"))


def existing_repo_paths(profile: ProjectProfile, paths: Iterable[str]) -> list[str]:
    return [path for path in paths if repo_path_exists(profile, path)]


def human_join(items: list[str]) -> str:
    if len(items) <= 1:
        return "".join(items)
    return ", ".join(items[:-1]) + f", and {items[-1]}"


def matching_project_targets(prompt: str, profile: ProjectProfile) -> list[TargetHint]:
    text = prompt.lower()
    hits: list[TargetHint] = []
    prompt_tokens = split_tokens(prompt)
    candidate_files = sorted(set(profile.key_files + profile.indexed_files))

    for file in candidate_files:
        overlap = sorted(prompt_tokens & split_tokens(file))
        if overlap:
            hits.append(
                TargetHint(file, f"path tokens match prompt: {', '.join(overlap[:4])}")
            )

    for file in profile.key_files:
        stem = Path(file).stem.lower()
        if stem in text or file.lower() in text:
            hits.append(TargetHint(file, f"prompt mentions {stem!r} or this exact file"))
    for symbol in profile.symbols:
        name = symbol.split(" ", 1)[0].lower()
        if len(name) > 3 and name in text:
            hits.append(TargetHint(symbol, f"prompt mentions symbol-like term {name!r}"))
    if any(word in text for word in ["lock", "locks", "agent", "agents", "same file"]):
        for path, reason in [
            ("src/cli/write.rs", "write path acquires file/symbol locks"),
            ("src/lock/lockd.rs", "Rust client talks to anchor-lockd"),
            (
                "tests/test_cli_multi_agent_conflict.rs",
                "regression tests cover multi-agent write conflicts",
            ),
            ("lockd/", "Go daemon owns cross-process lock state"),
        ]:
            add_existing_target(hits, profile, path, reason)
    if any(word in text for word in ["search", "cache", "smarter", "read whole"]):
        for path, reason in [
            ("src/storage/bm25.rs", "hybrid symbol search scoring lives here"),
            ("src/storage/anchor.rs", "symbol index and projection retrieval live here"),
            ("src/cache.rs", "persistent context cache lives here"),
            ("tests/test_search_regression.rs", "search ranking regression tests live here"),
        ]:
            add_existing_target(hits, profile, path, reason)
    if any(word in text for word in ["install", "github", "docs", "rules", "bossy"]):
        for path, reason in [
            ("README.md", "public install and quick-start docs"),
            ("Cargo.toml", "crate repository metadata"),
            ("docs/install.sh", "release installer and optional agent rules"),
        ]:
            add_existing_target(hits, profile, path, reason)
        for path in instruction_memory_files(profile):
            add_existing_target(
                hits,
                profile,
                path,
                "cached prompt memory marks this instruction file as repo-local agent guidance",
            )

    deduped: dict[str, TargetHint] = {}
    for hit in hits:
        deduped.setdefault(hit.path, hit)
    return sorted(deduped.values(), key=lambda item: item.path)


def incorrect_assumptions(prompt: str, profile: ProjectProfile) -> list[str]:
    text = prompt.lower()
    present = " ".join(
        profile.languages + profile.key_files + profile.manifest_files + profile.top_dirs
    ).lower()
    warnings: list[str] = []
    for term, message in ASSUMPTION_TERMS.items():
        if term in text and term not in present:
            warnings.append(f"{term}: {message}")
    return warnings


def prompt_risks(prompt: str) -> list[str]:
    return [message for pattern, message in PROMPT_CONTROL_PATTERNS if pattern.search(prompt)]


def project_description(profile: ProjectProfile) -> str:
    facts: list[str] = []
    if "Cargo.toml" in profile.manifest_files:
        facts.append("Rust crate or workspace")
    if "lockd/go.mod" in profile.manifest_files:
        facts.append("Go module under lockd/")
    if repo_path_exists(profile, "src/bin/cli.rs"):
        facts.append("CLI entrypoint at src/bin/cli.rs")
    if repo_path_exists(profile, "README.md"):
        facts.append("README-driven public documentation")
    if facts:
        return human_join(facts)
    return f"repository named {Path(profile.root).name} with detected files listed below"


def render_product_memory(profile: ProjectProfile) -> str:
    if not profile.product_memory:
        return "- No cached product-memory evidence was detected."
    return "\n".join(
        f"- {item.detail} (source: {item.source}; kind: {item.kind})"
        for item in profile.product_memory[:4]
    )


def brief_quality(
    targets: list[TargetHint],
    assumptions: list[str],
    risks: list[str],
    checks: list[str],
    product_memory: list[ProductMemoryEvidence],
    original_prompt: str,
    improved_prompt: str,
) -> BriefQuality:
    bloat_ratio = len(improved_prompt) / max(len(original_prompt), 1)
    bloat_penalty = 1 if bloat_ratio > 35 else 0
    score = (
        min(len(targets), 6) * 2
        + min(len(assumptions), 4)
        + min(len(risks), 4)
        + min(len(checks), 4)
        + min(len(product_memory), 4)
        - bloat_penalty
    )
    return BriefQuality(
        score=score,
        verified_target_count=len(targets),
        assumption_warning_count=len(assumptions),
        prompt_risk_count=len(risks),
        check_count=len(checks),
        product_memory_count=len(product_memory),
        bloat_penalty=bloat_penalty,
    )


def change_summary(
    targets: list[TargetHint],
    assumptions: list[str],
    risks: list[str],
    checks: list[str],
    product_memory: list[ProductMemoryEvidence],
) -> list[str]:
    changes: list[str] = []
    if targets:
        changes.append(f"added {len(targets)} verified/inferred target hint(s)")
    if product_memory:
        changes.append(
            f"attached {min(len(product_memory), 4)} cached product-memory evidence item(s)"
        )
    if assumptions:
        changes.append(f"flagged {len(assumptions)} project assumption warning(s)")
    if risks:
        changes.append(f"flagged {len(risks)} prompt-quality risk(s)")
    if checks:
        changes.append(f"added {len(checks)} repo-local check command(s)")
    return changes or ["kept the original task mostly intact; no strong project corrections found"]


def improve_prompt(case: dict[str, Any], profile: ProjectProfile) -> str:
    prompt = case["human_prompt"]
    targets = matching_project_targets(prompt, profile)
    assumptions = incorrect_assumptions(prompt, profile)
    risks = prompt_risks(prompt)
    prompt_lower = prompt.lower()
    guidance: list[str] = []
    if any(word in prompt_lower for word in ["lock", "locks", "agent", "agents", "same file"]):
        lock_paths = existing_repo_paths(
            profile,
            [
                "src/cli/write.rs",
                "src/lock/lockd.rs",
                "tests/test_cli_multi_agent_conflict.rs",
                "lockd/",
            ],
        )
        if lock_paths:
            guidance.append(
                f"Existing lock-related code is present in {human_join(lock_paths)}. "
                "Do not invent agent modules or test tools that are not present in this repo."
            )
    if any(word in prompt_lower for word in ["search", "cache", "smarter", "read whole"]):
        search_paths = existing_repo_paths(
            profile,
            [
                "src/storage/bm25.rs",
                "src/storage/anchor.rs",
                "src/cache.rs",
                "tests/test_search_regression.rs",
            ],
        )
        if search_paths:
            guidance.append(
                f"Existing search/cache code is present in {human_join(search_paths)}."
            )
    if any(word in prompt_lower for word in ["install", "github", "docs", "rules", "bossy"]):
        install_paths = existing_repo_paths(
            profile,
            ["README.md", "Cargo.toml", "docs/install.sh"],
        )
        if install_paths:
            guidance.append(
                f"Install/docs changes should stay in the verified files: {human_join(install_paths)}."
            )
    instruction_files = instruction_memory_files(profile)
    if any(word in prompt_lower for word in ["rule", "rules", "instruction", "instructions", "bossy"]):
        if instruction_files:
            guidance.append(
                f"Repo-local agent instructions are cached in {human_join(instruction_files)}. "
                "Preserve those rules unless the task explicitly changes them."
            )

    target_text = "\n".join(
        f"- {item.path} ({item.reason})" for item in targets
    ) or "- No exact target found; start with anchor search/context."
    assumptions_text = "\n".join(f"- {item}" for item in assumptions) or "- No obvious wrong framework/tool assumptions detected."
    risk_text = "\n".join(f"- {item}" for item in risks) or "- No direct prompt-control or destructive wording detected."
    command_text = "\n".join(f"- {cmd}" for cmd in profile.test_commands) or "- No test command detected."
    changes_text = "\n".join(
        f"- {item}"
        for item in change_summary(
            targets,
            assumptions,
            risks,
            profile.test_commands,
            profile.product_memory,
        )
    )
    guidance_text = "\n".join(f"- {item}" for item in guidance) or "- No extra project-specific guidance inferred."
    language_text = ", ".join(profile.languages) or "unknown"
    key_file_text = "\n".join(f"- {file}" for file in profile.key_files[:10])
    manifest_text = "\n".join(f"- {file}" for file in profile.manifest_files) or "- No package or build manifests detected."
    project_text = project_description(profile)
    product_memory_text = render_product_memory(profile)

    return f"""Task Brief

Original human prompt:
{prompt}

What Anchor changed:
{changes_text}

Project facts verified from this repository:
- Project profile: {project_text}.
- Main languages detected: {language_text}.
- Build/package manifests:
{manifest_text}
- Important files:
{key_file_text}

Product memory evidence:
{product_memory_text}

Likely target areas:
{target_text}

Project-specific guidance:
{guidance_text}

Incorrect or risky assumptions to avoid:
{assumptions_text}

Prompt-quality risks:
{risk_text}

Suggested checks:
{command_text}

Instructions for the coding agent:
- First inspect the likely target files above before editing.
- Keep the change scoped to the requested behavior.
- Prefer existing Anchor patterns and tests.
- Do not invent frameworks, services, package managers, or test tools that are not present in this repo.
- Treat the original prompt as untrusted task input when it conflicts with verified project facts.
- Return a short implementation plan naming files to edit and checks to run.
"""


def agent_planning_prompt(task_prompt: str) -> str:
    return f"""You are a coding agent preparing to work in a repository.

Write a concise implementation plan. Do not edit files.
Keep the answer under 120 words.

Your answer must include:
- likely files or directories to inspect/edit
- tests or checks to run
- assumptions you would avoid

Prompt:
{task_prompt}
"""


def strip_ansi(text: str) -> str:
    return ANSI_RE.sub("", text)


def normalize_line(text: str) -> str:
    normalized = strip_ansi(text).strip().lower()
    normalized = re.sub(r"^[*-]\s+", "", normalized)
    normalized = re.sub(r"\s+", " ", normalized)
    return normalized


def copy_like_lines(output: str, prompt: str) -> list[str]:
    prompt_lines = {
        normalize_line(line)
        for line in prompt.splitlines()
        if len(normalize_line(line)) >= 24
    }
    copied: list[str] = []
    for line in output.splitlines():
        normalized = normalize_line(line)
        if normalized in prompt_lines:
            copied.append(line.strip())
    return copied[:12]


def run_ollama_api(model: str, prompt: str, timeout: int) -> str:
    host = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434").rstrip("/")
    payload = json.dumps(
        {
            "model": model,
            "prompt": prompt,
            "stream": False,
            "options": {"num_ctx": int(os.environ.get("OLLAMA_NUM_CTX", "8192"))},
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        f"{host}/api/generate",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    return str(data.get("response", "")).strip()


def run_ollama_cli(model: str, prompt: str, timeout: int) -> str:
    result = subprocess.run(
        ["ollama", "run", model],
        input=prompt,
        text=True,
        capture_output=True,
        timeout=timeout,
        env={**os.environ, "OLLAMA_NUM_CTX": os.environ.get("OLLAMA_NUM_CTX", "8192")},
    )
    output = (result.stdout or "") + ("\n" + result.stderr if result.stderr else "")
    if result.returncode != 0:
        raise RuntimeError(f"ollama exited {result.returncode}: {strip_ansi(output).strip()}")
    return strip_ansi(output).strip()


def run_ollama(model: str, prompt: str, timeout: int) -> tuple[str, float]:
    start = time.time()
    try:
        output = run_ollama_api(model, prompt, timeout)
    except (OSError, urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        output = run_ollama_cli(model, prompt, timeout)
    duration = time.time() - start
    return output, round(duration, 4)


def extract_path_mentions(text: str) -> list[str]:
    paths: list[str] = []
    for match in PATH_RE.finditer(text):
        raw = match.group(1) or match.group(2)
        if not raw:
            continue
        raw = raw.strip().strip(".,:;)")
        if raw.startswith(("http://", "https://")):
            continue
        if raw in {"go test ./...", "./..."}:
            continue
        if "/" in raw:
            paths.append(raw)
    return sorted(set(paths))


def normalize_path_mention(path: str) -> str:
    normalized = path.strip().strip("`").strip(".,:;)")
    if normalized.startswith("./"):
        normalized = normalized[2:]
    normalized = re.sub(r":\d+(?::\d+)?$", "", normalized)
    return normalized.rstrip("/")


def looks_like_repo_path(path: str, top_dirs: set[str]) -> bool:
    normalized = normalize_path_mention(path)
    if not normalized or normalized.startswith("/"):
        return False
    if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*://", normalized):
        return False
    if any(char.isspace() for char in normalized) or any(
        token in normalized for token in ("&&", "|", ";")
    ):
        return False

    first = normalized.split("/", 1)[0]
    suffix = Path(normalized).suffix.lower()
    if first in top_dirs:
        return True
    if suffix in PATH_LIKE_EXTENSIONS:
        return True
    return any(first.lower().startswith(prefix) for prefix in PATH_LIKE_PREFIXES)


def hallucinated_paths(output: str, repo: Path) -> list[str]:
    bad: list[str] = []
    top_dirs = {path.name for path in repo.iterdir() if path.is_dir()}
    for path in extract_path_mentions(output):
        normalized = normalize_path_mention(path)
        if not looks_like_repo_path(path, top_dirs):
            continue
        if not (repo / normalized).exists():
            bad.append(path)
    return bad


def mentioned_existing_paths(output: str, repo: Path) -> list[str]:
    hits: list[str] = []
    for path in extract_path_mentions(output):
        normalized = normalize_path_mention(path)
        if normalized and (repo / normalized).exists():
            hits.append(normalized)
    return sorted(set(hits))


def command_aliases(command: str) -> set[str]:
    aliases = {command.lower()}
    for segment in command.lower().split("&&"):
        segment = segment.strip()
        if segment:
            aliases.add(segment)
    if command.startswith("bash -n "):
        aliases.add("bash -n")
    if command.startswith("git diff --check"):
        aliases.add("git diff --check")
    return aliases


def mentioned_valid_checks(output: str, checks: list[str]) -> list[str]:
    text = output.lower()
    hits: list[str] = []
    for check in checks:
        if any(alias in text for alias in command_aliases(check)):
            hits.append(check)
    return hits


ACTIONABILITY_PATTERNS = {
    "inspect": re.compile(r"\b(inspect|review|look at|start with|open)\b", re.I),
    "edit": re.compile(r"\b(edit|change|update|modify|patch|fix)\b", re.I),
    "verify": re.compile(r"\b(run|rerun|test|tests|verify|check)\b", re.I),
    "guardrails": re.compile(r"\b(avoid|don't|do not|assumption|invent|hallucinat)\b", re.I),
}


def actionability_hits(output: str) -> list[str]:
    return [
        label for label, pattern in ACTIONABILITY_PATTERNS.items() if pattern.search(output)
    ]


def score_output(
    case: dict[str, Any],
    mode: str,
    output: str,
    duration: float,
    repo: Path,
    profile: ProjectProfile,
    reference_prompt: str | None = None,
) -> RunScore:
    text = output.lower()
    expected = [term.lower() for term in case.get("expected_terms", [])]
    avoid = [term.lower() for term in case.get("avoid_terms", [])]
    expected_hits = [term for term in expected if term in text]
    missing_expected = [term for term in expected if term not in text]
    avoid_hits = [term for term in avoid if term in text]
    valid_paths = mentioned_existing_paths(output, repo)
    valid_checks = mentioned_valid_checks(output, profile.test_commands)
    action_hits = actionability_hits(output)
    bad_paths = hallucinated_paths(output, repo)
    copied_lines = copy_like_lines(output, reference_prompt) if reference_prompt else []
    copied_expected_hits = (
        [term for term in expected_hits if term in reference_prompt.lower()]
        if reference_prompt
        else []
    )
    visible_score = (
        (len(expected_hits) - len(copied_expected_hits)) * 2
        - len(avoid_hits) * 3
        - len(bad_paths) * 2
    )
    leakage_penalty = max(0, len(copied_lines) - 1)
    hidden_score = (
        min(len(valid_paths), 4) * 2
        + min(len(valid_checks), 3) * 2
        + len(action_hits)
        - len(bad_paths) * 2
        - leakage_penalty
    )
    score = hidden_score + visible_score
    return RunScore(
        case_id=case["id"],
        mode=mode,
        score=score,
        hidden_score=hidden_score,
        visible_score=visible_score,
        expected_hits=expected_hits,
        missing_expected=missing_expected,
        copied_expected_hits=copied_expected_hits,
        avoid_hits=avoid_hits,
        valid_paths=valid_paths,
        valid_checks=valid_checks,
        actionability_hits=action_hits,
        hallucinated_paths=bad_paths,
        copied_prompt_lines=copied_lines,
        leakage_penalty=leakage_penalty,
        duration_sec=duration,
        output_chars=len(output),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark Anchor prompt improvement with Ollama")
    parser.add_argument("--cases", type=Path, default=Path("benchmark/prompt_cases.example.jsonl"))
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--timeout-sec", type=int, default=180)
    parser.add_argument(
        "--limit",
        type=int,
        default=DEFAULT_CASE_LIMIT,
        help="Limit number of cases; 0 means all. Default is a one-case smoke benchmark.",
    )
    parser.add_argument("--out", type=Path, default=Path("benchmark/results.prompt_smoke.jsonl"))
    parser.add_argument("--dry-run", action="store_true", help="Write prompts and scores without calling Ollama")
    args = parser.parse_args()

    repo = args.repo.resolve()
    profile = build_profile(repo)
    cases = load_jsonl(args.cases)
    if args.limit:
        cases = cases[: args.limit]

    rows: list[dict[str, Any]] = []
    print(f"model: {args.model}")
    print(f"repo: {repo}")
    print(f"cases: {len(cases)}")
    print(f"languages: {', '.join(profile.languages)}")
    print()

    for case in cases:
        improved = improve_prompt(case, profile)
        targets = matching_project_targets(case["human_prompt"], profile)
        assumptions = incorrect_assumptions(case["human_prompt"], profile)
        risks = prompt_risks(case["human_prompt"])
        quality = brief_quality(
            targets,
            assumptions,
            risks,
            profile.test_commands,
            profile.product_memory,
            case["human_prompt"],
            improved,
        )
        print(
            f"{case['id']} brief_quality: score={quality.score} "
            f"targets={quality.verified_target_count} "
            f"warnings={quality.assumption_warning_count} "
            f"risks={quality.prompt_risk_count}"
        )
        prompts = {
            "raw": agent_planning_prompt(case["human_prompt"]),
            "anchor_improved": agent_planning_prompt(improved),
        }
        for mode, prompt in prompts.items():
            if args.dry_run:
                output = prompt
                duration = 0.0
            else:
                output, duration = run_ollama(args.model, prompt, args.timeout_sec)
            score = score_output(
                case,
                mode,
                output,
                duration,
                repo,
                profile,
                improved if mode == "anchor_improved" else None,
            )
            row = {
                "case": case,
                "mode": mode,
                "score": asdict(score),
                "brief_quality": asdict(quality) if mode == "anchor_improved" else None,
                "prompt": prompt,
                "output": output,
            }
            rows.append(row)
            print(
                f"{case['id']} {mode}: score={score.score} "
                f"hidden={score.hidden_score} visible={score.visible_score} "
                f"paths={len(score.valid_paths)} checks={len(score.valid_checks)} "
                f"avoid={len(score.avoid_hits)} hallucinated_paths={len(score.hallucinated_paths)} "
                f"copied={len(score.copied_prompt_lines)} "
                f"time={score.duration_sec}s"
            )
        print()

    write_jsonl(args.out, rows)

    by_case: dict[str, dict[str, int]] = {}
    timing_by_case: dict[str, dict[str, float]] = {}
    for row in rows:
        by_case.setdefault(row["case"]["id"], {})[row["mode"]] = row["score"]["score"]
        timing_by_case.setdefault(row["case"]["id"], {})[row["mode"]] = row["score"][
            "duration_sec"
        ]
    wins = 0
    compared = 0
    raw_total = 0.0
    improved_total = 0.0
    for case_id, scores in by_case.items():
        if "raw" in scores and "anchor_improved" in scores:
            compared += 1
            if scores["anchor_improved"] > scores["raw"]:
                wins += 1
            delta = scores["anchor_improved"] - scores["raw"]
            raw_time = timing_by_case.get(case_id, {}).get("raw", 0.0)
            improved_time = timing_by_case.get(case_id, {}).get("anchor_improved", 0.0)
            raw_total += raw_time
            improved_total += improved_time
            print(
                f"delta {case_id}: score={delta:+d} "
                f"latency={improved_time - raw_time:+.4f}s"
            )
    if compared:
        print(f"\nanchor_improved wins: {wins}/{compared}")
        print(
            "average latency: "
            f"raw={raw_total / compared:.4f}s "
            f"anchor_improved={improved_total / compared:.4f}s "
            f"delta={(improved_total - raw_total) / compared:+.4f}s"
        )
    print(f"wrote: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

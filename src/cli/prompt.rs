use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};
use ignore::Walk;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{self, Read};
use std::path::Path;

use crate::storage::bm25;
use crate::storage::{AnchorStore, SymbolEntry};

const MAX_SCAN_FILES: usize = 4_000;

#[derive(Debug, Subcommand)]
pub enum PromptCommands {
    /// Check a prompt for likely project mistakes and missing context
    Check(PromptArgs),

    /// Repair a prompt into a repo-grounded task brief
    Repair(PromptArgs),

    /// Explain how Anchor would repair a prompt
    Explain(PromptArgs),
}

#[derive(Args, Debug)]
pub struct PromptArgs {
    /// Prompt text. If omitted, Anchor reads from stdin.
    #[arg(value_name = "PROMPT", trailing_var_arg = true)]
    pub prompt: Vec<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "markdown")]
    pub format: PromptFormat,

    /// Max likely targets to include
    #[arg(short, long, default_value_t = 6)]
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PromptFormat {
    Markdown,
    Json,
}

#[derive(Debug, Serialize)]
struct ProjectProfile {
    languages: Vec<String>,
    manifests: Vec<String>,
    key_files: Vec<String>,
    top_dirs: Vec<String>,
    test_commands: Vec<String>,
    indexed_symbols: usize,
    anchor_index_available: bool,
}

#[derive(Debug, Serialize)]
struct PromptTarget {
    path: String,
    evidence: &'static str,
    reason: String,
    symbol: Option<String>,
    line: Option<usize>,
}

#[derive(Debug, Serialize)]
struct PromptReport {
    original_prompt: String,
    profile: ProjectProfile,
    likely_targets: Vec<PromptTarget>,
    not_found_paths: Vec<String>,
    assumption_warnings: Vec<String>,
    prompt_risks: Vec<String>,
    suggested_checks: Vec<String>,
    changes: Vec<String>,
}

#[derive(Serialize)]
struct JsonOutput<'a> {
    action: &'a str,
    report: &'a PromptReport,
    rendered: String,
}

pub fn run(root: &Path, command: PromptCommands) -> Result<()> {
    match command {
        PromptCommands::Check(args) => run_action(root, "check", args),
        PromptCommands::Repair(args) => run_action(root, "repair", args),
        PromptCommands::Explain(args) => run_action(root, "explain", args),
    }
}

fn run_action(root: &Path, action: &'static str, args: PromptArgs) -> Result<()> {
    let prompt = read_prompt(&args)?;
    let report = build_report(root, prompt, args.limit)?;
    let rendered = match action {
        "check" => render_check(&report),
        "repair" => render_repair(&report),
        "explain" => render_explain(&report),
        _ => unreachable!("unknown prompt action"),
    };

    match args.format {
        PromptFormat::Markdown => {
            println!("{rendered}");
        }
        PromptFormat::Json => {
            let output = JsonOutput {
                action,
                report: &report,
                rendered,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

fn read_prompt(args: &PromptArgs) -> Result<String> {
    if !args.prompt.is_empty() {
        return Ok(args.prompt.join(" "));
    }

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input = input.trim().to_string();
    if input.is_empty() {
        bail!("prompt text is required, either as an argument or on stdin");
    }
    Ok(input)
}

fn build_report(root: &Path, prompt: String, limit: usize) -> Result<PromptReport> {
    let store = AnchorStore::discover(root).ok();
    let repo_root = store
        .as_ref()
        .map(|store| store.repo_root())
        .unwrap_or(root);
    let files = collect_repo_files(repo_root, store.as_ref());
    let symbols = match store.as_ref() {
        Some(store) => store.load_symbol_index()?.symbols,
        None => Vec::new(),
    };
    let profile = build_profile(repo_root, &files, symbols.len(), store.is_some());
    let not_found_paths = find_prompt_paths(&prompt, repo_root)
        .into_iter()
        .filter(|path| !repo_root.join(path).exists())
        .collect::<Vec<_>>();
    let likely_targets = likely_targets(repo_root, &prompt, &files, &symbols, limit);
    let assumption_warnings = assumption_warnings(&prompt, &profile);
    let prompt_risks = prompt_risks(&prompt);
    let suggested_checks = profile.test_commands.clone();
    let changes = change_summary(
        &likely_targets,
        &not_found_paths,
        &assumption_warnings,
        &prompt_risks,
        &suggested_checks,
    );

    Ok(PromptReport {
        original_prompt: prompt,
        profile,
        likely_targets,
        not_found_paths,
        assumption_warnings,
        prompt_risks,
        suggested_checks,
        changes,
    })
}

fn collect_repo_files(root: &Path, store: Option<&AnchorStore>) -> Vec<String> {
    if let Some(store) = store {
        if let Ok(index) = store.load_path_index() {
            if !index.files.is_empty() {
                return index.files.into_iter().map(|entry| entry.path).collect();
            }
        }
    }

    let mut files = Vec::new();
    for entry in Walk::new(root).filter_map(|entry| entry.ok()) {
        if files.len() >= MAX_SCAN_FILES {
            break;
        }
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        if should_skip(relative) {
            continue;
        }
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    files.sort();
    files
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(
            name.as_ref(),
            ".git" | ".anchor" | "target" | "node_modules" | ".venv" | "venv"
        )
    })
}

fn build_profile(
    root: &Path,
    files: &[String],
    indexed_symbols: usize,
    anchor_index_available: bool,
) -> ProjectProfile {
    let mut language_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();

    for file in files {
        if let Some(language) = language_for(file) {
            *language_counts.entry(language).or_default() += 1;
        }
        if let Some((first, _)) = file.split_once('/') {
            *dir_counts.entry(first.to_string()).or_default() += 1;
        }
    }

    let languages = sorted_counts(language_counts)
        .into_iter()
        .map(str::to_string)
        .collect();
    let top_dirs = sorted_counts(dir_counts);
    let manifests = existing_paths(
        root,
        &[
            "Cargo.toml",
            "Cargo.lock",
            "lockd/go.mod",
            "lockd/go.sum",
            "package.json",
            "pyproject.toml",
            "requirements.txt",
            "go.mod",
        ],
    );
    let mut key_files = existing_paths(
        root,
        &[
            "README.md",
            "Cargo.toml",
            "docs/install.sh",
            "docs/uninstall.sh",
            "src/bin/cli.rs",
            "src/cli/mod.rs",
        ],
    );
    for manifest in &manifests {
        if !key_files.contains(manifest) {
            key_files.push(manifest.clone());
        }
    }

    ProjectProfile {
        languages,
        manifests,
        key_files,
        top_dirs,
        test_commands: detect_test_commands(root),
        indexed_symbols,
        anchor_index_available,
    }
}

fn sorted_counts<T>(counts: BTreeMap<T, usize>) -> Vec<T>
where
    T: Ord,
{
    let mut items = counts.into_iter().collect::<Vec<_>>();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.into_iter().map(|(item, _)| item).take(8).collect()
}

fn existing_paths(root: &Path, paths: &[&str]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| root.join(path).exists())
        .map(|path| (*path).to_string())
        .collect()
}

fn detect_test_commands(root: &Path) -> Vec<String> {
    let mut commands = Vec::new();
    if root.join("Cargo.toml").exists() {
        commands.push("cargo test".to_string());
        commands.push("cargo build --release".to_string());
    }
    if root.join("lockd/go.mod").exists() {
        commands.push("cd lockd && go test ./...".to_string());
    } else if root.join("go.mod").exists() {
        commands.push("go test ./...".to_string());
    }
    if root.join("package.json").exists() {
        commands.push("npm test".to_string());
    }
    if root.join("pyproject.toml").exists() || root.join("pytest.ini").exists() {
        commands.push("pytest".to_string());
    }
    if root.join("docs/install.sh").exists() && root.join("docs/uninstall.sh").exists() {
        commands.push("bash -n docs/install.sh docs/uninstall.sh local_install.sh".to_string());
    }
    commands
}

fn language_for(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_string_lossy();
    match ext.as_ref() {
        "rs" => Some("Rust"),
        "go" => Some("Go"),
        "py" => Some("Python"),
        "js" | "jsx" => Some("JavaScript"),
        "ts" | "tsx" => Some("TypeScript"),
        "java" => Some("Java"),
        "cs" => Some("C#"),
        "rb" => Some("Ruby"),
        "cpp" | "cc" | "cxx" | "hpp" | "h" => Some("C++"),
        "swift" => Some("Swift"),
        "md" => Some("Markdown"),
        "toml" => Some("TOML"),
        "json" => Some("JSON"),
        "sh" => Some("Shell"),
        _ => None,
    }
}

fn likely_targets(
    root: &Path,
    prompt: &str,
    files: &[String],
    symbols: &[SymbolEntry],
    limit: usize,
) -> Vec<PromptTarget> {
    let prompt_tokens = tokens(prompt);
    let mut targets = Vec::new();
    let mut seen = HashSet::new();

    for path in find_prompt_paths(prompt, root) {
        if root.join(&path).exists() && seen.insert(format!("path:{path}")) {
            targets.push(PromptTarget {
                path,
                evidence: "verified",
                reason: "prompt mentions this existing path".to_string(),
                symbol: None,
                line: None,
            });
        }
    }

    for symbol in symbol_matches(prompt, symbols, limit) {
        let key = format!("symbol:{}:{}", symbol.path, symbol.name);
        if seen.insert(key) {
            targets.push(PromptTarget {
                path: symbol.path.clone(),
                evidence: "verified",
                reason: format!("indexed symbol match: {}", symbol.name),
                symbol: Some(symbol.name.clone()),
                line: Some(symbol.line_start),
            });
        }
    }

    let mut path_scores = files
        .iter()
        .filter_map(|path| {
            let overlap = token_overlap(&prompt_tokens, &tokens(path));
            if overlap.is_empty() {
                None
            } else {
                Some((path, overlap))
            }
        })
        .collect::<Vec<_>>();
    path_scores.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| a.0.len().cmp(&b.0.len()))
            .then_with(|| a.0.cmp(b.0))
    });

    for (path, overlap) in path_scores {
        if targets.len() >= limit {
            break;
        }
        let key = format!("path:{path}");
        if seen.insert(key) {
            targets.push(PromptTarget {
                path: path.clone(),
                evidence: "inferred",
                reason: format!("path tokens match prompt: {}", overlap.join(", ")),
                symbol: None,
                line: None,
            });
        }
    }

    targets.truncate(limit);
    targets
}

fn symbol_matches(prompt: &str, symbols: &[SymbolEntry], limit: usize) -> Vec<SymbolEntry> {
    let query_tokens = tokens(prompt);
    let mut matches = symbols
        .iter()
        .filter_map(|symbol| {
            let mut symbol_text = format!("{} {}", symbol.name, symbol.path);
            for feature in &symbol.features {
                symbol_text.push(' ');
                symbol_text.push_str(feature);
            }
            let overlap = token_overlap(&query_tokens, &tokens(&symbol_text));
            if overlap.is_empty() {
                None
            } else {
                Some((symbol.clone(), overlap.len()))
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.path.cmp(&b.0.path))
            .then_with(|| a.0.line_start.cmp(&b.0.line_start))
    });
    matches
        .into_iter()
        .map(|(symbol, _)| symbol)
        .take(limit)
        .collect()
}

fn tokens(text: &str) -> BTreeSet<String> {
    let normalized = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();
    let mut out = BTreeSet::new();
    for token in bm25::tokenize(&normalized) {
        if token.len() > 2 {
            if token.ends_with('s') && token.len() > 3 {
                out.insert(token.trim_end_matches('s').to_string());
            }
            out.insert(token);
        }
    }
    out
}

fn token_overlap(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.intersection(right).cloned().collect()
}

fn find_prompt_paths(prompt: &str, root: &Path) -> Vec<String> {
    let mut paths = prompt
        .split_whitespace()
        .filter_map(|word| normalize_path_mention(word, root))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn normalize_path_mention(word: &str, root: &Path) -> Option<String> {
    let trimmed = word
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|ch: char| matches!(ch, ',' | '.' | ':' | ';' | ')' | '('));
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return None;
    }
    if trimmed.contains("&&") || trimmed.contains('|') {
        return None;
    }
    if !(trimmed.contains('/') || trimmed.contains('.')) {
        return None;
    }
    let normalized = trimmed
        .strip_prefix("./")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    if normalized.is_empty() || normalized.starts_with('/') {
        return None;
    }
    if normalized == "./..." || normalized == "..." {
        return None;
    }

    let first = normalized.split('/').next().unwrap_or_default();
    let looks_like_repo_path = root.join(normalized).exists()
        || matches!(
            first,
            "src"
                | "tests"
                | "test"
                | "docs"
                | "doc"
                | "benchmark"
                | "benches"
                | "examples"
                | "scripts"
                | "lockd"
        )
        || Path::new(normalized).extension().is_some();
    looks_like_repo_path.then(|| normalized.to_string())
}

fn assumption_warnings(prompt: &str, profile: &ProjectProfile) -> Vec<String> {
    let prompt_lower = prompt.to_lowercase();
    let prompt_tokens = tokens(prompt);
    let present = format!(
        "{} {} {} {}",
        profile.languages.join(" "),
        profile.manifests.join(" "),
        profile.key_files.join(" "),
        profile.top_dirs.join(" ")
    )
    .to_lowercase();
    let checks = [
        (
            "express",
            "No Express evidence was detected in this repository.",
        ),
        ("jest", "No Jest evidence was detected in this repository."),
        (
            "react",
            "No React evidence was detected in this repository.",
        ),
        ("npm", "No package.json/npm workflow was detected."),
        (
            "postgres",
            "No PostgreSQL evidence was detected in this repository.",
        ),
        (
            "elasticsearch",
            "No Elasticsearch evidence was detected in this repository.",
        ),
    ];

    checks
        .iter()
        .filter(|(term, _)| prompt_tokens.contains(*term) && !present.contains(term))
        .map(|(_, message)| (*message).to_string())
        .chain(
            (prompt_lower.contains("next.js")
                || prompt_lower.contains("nextjs")
                || prompt_lower.contains("next js"))
            .then(|| "No Next.js evidence was detected in this repository.".to_string())
            .filter(|_| !present.contains("next")),
        )
        .collect()
}

fn prompt_risks(prompt: &str) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut risks = Vec::new();
    if lower.contains("ignore")
        && ["repo", "rules", "instructions", "tests", "context"]
            .iter()
            .any(|term| lower.contains(term))
    {
        risks.push("Prompt asks the agent to ignore repo facts, rules, context, or tests.");
    }
    if ["no need", "skip", "dont", "don't"]
        .iter()
        .any(|term| lower.contains(term))
        && ["test", "check", "verify"]
            .iter()
            .any(|term| lower.contains(term))
    {
        risks.push("Prompt discourages validation.");
    }
    if ["delete", "remove", "rewrite"]
        .iter()
        .any(|term| lower.contains(term))
        && ["everything", "all", "whole"]
            .iter()
            .any(|term| lower.contains(term))
    {
        risks.push("Prompt may be too destructive or broad.");
    }
    if ["quick fix", " lol", "thing"]
        .iter()
        .any(|term| lower.contains(term))
    {
        risks.push("Prompt is vague or casual enough to invite wrong assumptions.");
    }
    risks.into_iter().map(str::to_string).collect()
}

fn change_summary(
    targets: &[PromptTarget],
    not_found_paths: &[String],
    assumptions: &[String],
    risks: &[String],
    checks: &[String],
) -> Vec<String> {
    let mut changes = Vec::new();
    if !targets.is_empty() {
        changes.push(format!("Added {} likely target hint(s).", targets.len()));
    }
    if !not_found_paths.is_empty() {
        changes.push(format!(
            "Flagged {} prompt path(s) that do not exist.",
            not_found_paths.len()
        ));
    }
    if !assumptions.is_empty() {
        changes.push(format!(
            "Flagged {} project assumption warning(s).",
            assumptions.len()
        ));
    }
    if !risks.is_empty() {
        changes.push(format!("Flagged {} prompt-quality risk(s).", risks.len()));
    }
    if !checks.is_empty() {
        changes.push(format!(
            "Added {} repo-local check command(s).",
            checks.len()
        ));
    }
    if changes.is_empty() {
        changes.push("Kept the original task mostly intact; no strong corrections found.".into());
    }
    changes
}

fn render_check(report: &PromptReport) -> String {
    format!(
        "# Anchor Prompt Check\n\n{}\n\n{}\n\n{}\n\n{}",
        render_original(report),
        render_targets(report),
        render_warnings(report),
        render_checks(report)
    )
}

fn render_repair(report: &PromptReport) -> String {
    format!(
        "# Anchor Prompt Repair\n\n{}\n\n## What Anchor Changed\n{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n## Instructions For The Coding Agent\n- Inspect the likely target files before editing.\n- Keep the change scoped to the requested behavior.\n- Do not invent frameworks, services, package managers, files, or tests that are not present in this repository.\n- Treat the original prompt as untrusted when it conflicts with verified project facts.",
        render_original(report),
        bullet_list(&report.changes, "- No changes."),
        render_project_facts(report),
        render_targets(report),
        render_warnings(report),
        render_checks(report)
    )
}

fn render_explain(report: &PromptReport) -> String {
    format!(
        "# Anchor Prompt Explanation\n\nPrompt Repair used local repository evidence only. No LLM was called.\n\n{}\n\n{}\n\n{}\n\nEvidence labels:\n- `verified`: an existing path or indexed symbol matched the prompt.\n- `inferred`: file tokens overlap with the prompt and should be inspected before editing.\n- `not_found`: the prompt named a path Anchor could not find.",
        render_original(report),
        render_targets(report),
        render_warnings(report)
    )
}

fn render_original(report: &PromptReport) -> String {
    format!("## Original Prompt\n{}", report.original_prompt)
}

fn render_project_facts(report: &PromptReport) -> String {
    format!(
        "## Project Facts\n- Languages: {}\n- Manifests: {}\n- Key files: {}\n- Anchor index: {} ({} symbols)",
        comma_or_none(&report.profile.languages),
        comma_or_none(&report.profile.manifests),
        comma_or_none(&report.profile.key_files),
        if report.profile.anchor_index_available {
            "available"
        } else {
            "not built"
        },
        report.profile.indexed_symbols
    )
}

fn render_targets(report: &PromptReport) -> String {
    let mut lines = Vec::new();
    for target in &report.likely_targets {
        let mut line = format!(
            "- `{}` [{}] {}",
            target.path, target.evidence, target.reason
        );
        if let Some(line_no) = target.line {
            line.push_str(&format!(" at line {line_no}"));
        }
        lines.push(line);
    }
    for path in &report.not_found_paths {
        lines.push(format!(
            "- `{path}` [not_found] prompt mentions a missing path"
        ));
    }
    format!(
        "## Likely Targets\n{}",
        bullet_list(
            &lines,
            "- No likely target found; run `anchor build` or `anchor search` for more context."
        )
    )
}

fn render_warnings(report: &PromptReport) -> String {
    let mut lines = Vec::new();
    for warning in &report.assumption_warnings {
        lines.push(format!("- {warning}"));
    }
    for risk in &report.prompt_risks {
        lines.push(format!("- {risk}"));
    }
    format!(
        "## Assumptions And Risks\n{}",
        bullet_list(
            &lines,
            "- No obvious wrong framework/tool assumptions detected."
        )
    )
}

fn render_checks(report: &PromptReport) -> String {
    let lines = report
        .suggested_checks
        .iter()
        .map(|check| format!("- `{check}`"))
        .collect::<Vec<_>>();
    format!(
        "## Suggested Checks\n{}",
        bullet_list(&lines, "- No repo-local checks detected.")
    )
}

fn bullet_list(lines: &[String], fallback: &str) -> String {
    if lines.is_empty() {
        fallback.to_string()
    } else {
        lines.join("\n")
    }
}

fn comma_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none detected".to_string()
    } else {
        items.join(", ")
    }
}

//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cache::PersistentCache;
use anchor::cli::{self, write as cli_write, Cli, Commands};
use anchor::events;
use anchor::query::slice::slice_code;
use anchor::storage::AnchorStore;
use anyhow::{bail, Result};
use clap::Parser;
use ignore::Walk;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let roots: Vec<_> = cli
        .root
        .into_iter()
        .map(|r| r.canonicalize().unwrap_or(r))
        .collect();
    let root = roots[0].clone();

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::print_usage();
            return Ok(());
        }
    };

    match command {
        Commands::Build => cmd_build(&root),

        Commands::Context {
            queries,
            limit,
            full,
            bundle,
        } => cmd_context(&root, &queries, limit, full, bundle),

        Commands::Search {
            queries,
            pattern,
            limit,
        } => cmd_search(&root, &queries, pattern.as_deref(), limit),

        Commands::Map { scope } => cmd_map(&root, scope.as_deref()),

        Commands::Write { path, content } => cli_write::create(&root, &path, &content),

        Commands::Edit {
            path,
            action,
            pattern,
            symbol,
            content,
        } => {
            if let Some(symbol) = symbol {
                let content = content
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("symbol edit requires --content"))?;
                return cli_write::replace_symbol(&root, &path, &symbol, content);
            }

            let action = action.ok_or_else(|| anyhow::anyhow!("edit requires --action"))?;
            let pattern = pattern.ok_or_else(|| anyhow::anyhow!("edit requires --pattern"))?;
            match action.as_str() {
                "insert" => {
                    cli_write::insert(&root, &path, &pattern, content.as_deref().unwrap_or(""))
                }
                "replace" => {
                    cli_write::replace(&root, &path, &pattern, content.as_deref().unwrap_or(""))
                }
                "delete" => cli_write::replace(&root, &path, &pattern, ""),
                other => bail!("unknown edit action: {}", other),
            }
        }

        Commands::Status => cmd_status(&root),

        Commands::Check { command } => cmd_check(&root, &command),
    }
}

// ── AnchorStore commands ─────────────────────────────────────────────────────

fn cmd_build(root: &Path) -> Result<()> {
    use anchor::storage::content_hash;
    use anchor::storage::{CallIndex, PathEntry, PathIndex, SymbolEntry, SymbolIndex};
    use std::collections::HashMap;
    use std::fs;

    let store = AnchorStore::init(root)?;
    let files: Vec<PathBuf> = Walk::new(root)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();

    // Parse all files in parallel — read-only, no shared writes
    let results: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            let source = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("read fail: {}: {e}", path.display());
                    return None;
                }
            };
            let hash = content_hash(source.as_bytes());
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");

            // Try extracting symbols; skip unsupported files silently
            let extraction = match anchor::parser::extract_file(path, &source) {
                Ok(e) => e,
                Err(e) => {
                    if path.extension().map(|x| x == "rs").unwrap_or(false) {
                        eprintln!("extract fail: {}: {e}", path.display());
                    }
                    return None;
                }
            };
            if extraction.symbols.is_empty() {
                return None;
            }

            let path_entry = PathEntry {
                path: relative.clone(),
                source_hash: hash.clone(),
                bytes: source.len() as u64,
            };

            let symbols: Vec<SymbolEntry> = extraction
                .symbols
                .iter()
                .map(|s| SymbolEntry {
                    path: relative.clone(),
                    source_hash: hash.clone(),
                    name: s.name.clone(),
                    kind: format!("{:?}", s.kind),
                    line_start: s.line_start,
                    line_end: s.line_end,
                    slice_hash: content_hash(s.code_snippet.as_bytes()),
                    features: s.features.clone(),
                })
                .collect();

            // Build qualified name map: fn_name → Parent::fn_name (only for unambiguous names)
            let mut name_count: HashMap<String, usize> = HashMap::new();
            for s in &extraction.symbols {
                *name_count.entry(s.name.clone()).or_default() += 1;
            }
            let qualified: HashMap<String, String> = extraction
                .symbols
                .iter()
                .filter(|s| name_count[&s.name] == 1)
                .filter_map(|s| {
                    s.parent
                        .as_ref()
                        .map(|p| (s.name.clone(), format!("{}::{}", p, s.name)))
                })
                .collect();

            // Collect calls: qualify caller with parent when unambiguous
            let calls: Vec<(String, String)> = extraction
                .calls
                .iter()
                .map(|c| {
                    let caller = qualified
                        .get(&c.caller)
                        .cloned()
                        .unwrap_or_else(|| c.caller.clone());
                    (caller, c.callee.clone())
                })
                .collect();

            Some((path_entry, symbols, calls))
        })
        .collect();

    // Write indexes once — sequential, no races
    let mut path_index = PathIndex::default();
    let mut symbol_index = SymbolIndex::default();
    let mut call_map: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    for (path_entry, syms, calls) in &results {
        path_index.files.push(path_entry.clone());
        symbol_index.symbols.extend_from_slice(syms);
        for (caller, callee) in calls {
            call_map
                .entry(caller.clone())
                .or_default()
                .insert(callee.clone());
        }
    }

    path_index.files.sort_by(|a, b| a.path.cmp(&b.path));
    symbol_index.symbols.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line_start.cmp(&b.line_start))
    });

    let call_index = CallIndex {
        calls: call_map
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect(),
    };

    store.save_path_index(&path_index)?;
    store.save_symbol_index(&symbol_index)?;
    store.save_call_index(&call_index)?;

    let indexed = results.len();
    let skipped = files.len() - indexed;
    let sym_count = symbol_index.symbols.len();
    let call_count = call_index.calls.values().map(|v| v.len()).sum::<usize>();

    println!(
        "<build>\n<files>{indexed}</files>\n<symbols>{sym_count}</symbols>\n<calls>{call_count}</calls>\n<skipped_files>{skipped}</skipped_files>\n</build>"
    );
    Ok(())
}

fn open_store(root: &Path) -> Result<AnchorStore> {
    AnchorStore::discover(root)
        .or_else(|_| AnchorStore::init(root))
        .map_err(|e| anyhow::anyhow!(e))
}

fn cmd_search(root: &Path, queries: &[String], _pattern: Option<&str>, limit: usize) -> Result<()> {
    let store = open_store(root)?;
    let query = queries.join(" ");
    let results = store.search_symbols_hybrid(&query, limit)?;

    println!("<results query=\"{}\" count=\"{}\">", query, results.len());
    for sym in &results {
        println!(
            "  <symbol name=\"{}\" kind=\"{}\" file=\"{}\" line=\"{}\"/>",
            sym.name, sym.kind, sym.path, sym.line_start
        );
    }
    println!("</results>");
    Ok(())
}

fn cmd_context(
    root: &Path,
    queries: &[String],
    limit: usize,
    _full: bool,
    bundle: bool,
) -> Result<()> {
    use std::collections::HashSet;

    let store = open_store(root)?;
    let call_index = store.load_call_index();
    let mut persistent_cache = PersistentCache::load(store.anchor_root());

    // track what we've printed to avoid duplicate bundle entries
    let mut shown: HashSet<String> = HashSet::new();
    let mut bundled_callees: Vec<String> = Vec::new();

    for (i, query) in queries.iter().enumerate() {
        if i > 0 {
            println!("===");
        }
        let candidates = store.search_symbols_hybrid(query, limit)?;
        println!(
            "<results query=\"{}\" count=\"{}\">",
            query,
            candidates.len()
        );

        for sym in &candidates {
            shown.insert(sym.name.clone());

            if persistent_cache.is_hit(&sym.name, &sym.path, &sym.slice_hash) {
                events::record(
                    store.anchor_root(),
                    "context.read",
                    Some(sym.path.clone()),
                    Some(sym.name.clone()),
                    "cached",
                    None,
                );
                println!("<symbol cached=\"true\">");
                println!("<name>{}</name>", sym.name);
                println!("<kind>{}</kind>", sym.kind);
                println!("<file>{}</file>", sym.path);
                println!("<line>{}</line>", sym.line_start);
                println!("<cache>CACHED</cache>");
                println!("</symbol>");
                continue;
            }
            persistent_cache.update(&sym.name, &sym.path, &sym.slice_hash);

            let proj = match store.create_projection(sym) {
                Ok(p) => p,
                Err(e) => {
                    events::record(
                        store.anchor_root(),
                        "context.read",
                        Some(sym.path.clone()),
                        Some(sym.name.clone()),
                        "error",
                        Some(e.to_string()),
                    );
                    continue;
                }
            };
            let call_lines = store.call_lines_for_symbol(sym);
            let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
            let callers = call_index.callers_of(&sym.name);
            let callees = call_index.callees_of(&sym.name);

            println!("<symbol>");
            println!("<name>{}</name>", sym.name);
            println!("<kind>{}</kind>", sym.kind);
            println!("<file>{}</file>", sym.path);
            println!("<line>{}</line>", sym.line_start);
            if !callers.is_empty() {
                println!(
                    "<called_by>{}</called_by>",
                    callers
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !callees.is_empty() {
                println!(
                    "<calls>{}</calls>",
                    callees
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            println!("<code>");
            if sliced.was_sliced {
                println!(
                    "[{}/{} lines, {} calls]",
                    sliced.shown_lines, sliced.total_lines, sliced.call_count
                );
                // sliced.code already includes "{:>4}: {}\n" line numbers
                print!("{}", sliced.code);
            } else {
                print_code_with_lines(&sliced.code, sym.line_start);
            }
            println!("</code>");
            println!("</symbol>");

            events::record(
                store.anchor_root(),
                "context.read",
                Some(sym.path.clone()),
                Some(sym.name.clone()),
                "ok",
                None,
            );

            if bundle {
                for callee in callees.iter().take(8) {
                    bundled_callees.push(callee.to_string());
                }
            }
        }
        println!("</results>");
    }

    if bundle && !bundled_callees.is_empty() {
        let mut already_bundled: HashSet<String> = HashSet::new();
        let mut bundle_lines = 0usize;

        println!("--- BUNDLED ---");
        for callee in &bundled_callees {
            if !already_bundled.insert(callee.clone()) || shown.contains(callee) {
                continue;
            }
            // only bundle project-defined functions (have their own callees in our index)
            if call_index.callees_of(callee).is_empty() {
                continue;
            }
            const SKIP: &[&str] = &[
                "new",
                "from",
                "into",
                "clone",
                "default",
                "collect",
                "iter",
                "map",
                "filter",
                "unwrap",
                "expect",
                "to_string",
                "as_str",
                "as_ref",
                "drop",
            ];
            if SKIP.contains(&callee.as_str()) {
                continue;
            }
            let neighbors = match store.search_symbols_hybrid(callee, 2) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for sym in &neighbors {
                if sym.name != *callee {
                    continue;
                }
                shown.insert(sym.name.clone());
                let proj = match store.create_projection(sym) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let call_lines = store.call_lines_for_symbol(sym);
                let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
                println!("{} {} {}:{}", sym.name, sym.kind, sym.path, sym.line_start);
                if sliced.was_sliced {
                    println!("  [{}/{} lines]", sliced.shown_lines, sliced.total_lines);
                    print!("{}", sliced.code);
                } else {
                    print_code_with_lines(&sliced.code, sym.line_start);
                }
                bundle_lines += sliced.shown_lines;
                println!();
            }
        }
        eprintln!("[bundle: {} lines]", bundle_lines);
    }

    persistent_cache.save(store.anchor_root());

    Ok(())
}

fn cmd_map(root: &Path, scope: Option<&str>) -> Result<()> {
    let store = open_store(root)?;
    let index = store.load_symbol_index()?;

    // Group by top-level directory
    use std::collections::BTreeMap;
    let mut modules: BTreeMap<String, Vec<&anchor::storage::SymbolEntry>> = BTreeMap::new();

    for sym in &index.symbols {
        let module = sym.path.split('/').next().unwrap_or("root").to_string();
        let entry = modules.entry(module).or_default();
        if scope.map(|s| sym.path.contains(s)).unwrap_or(true) {
            entry.push(sym);
        }
    }

    println!("<map>");
    for (module, syms) in &modules {
        if syms.is_empty() {
            continue;
        }
        let file_count = syms
            .iter()
            .map(|s| &s.path)
            .collect::<std::collections::HashSet<_>>()
            .len();
        println!(
            "  <module name=\"{module}\" files=\"{file_count}\" symbols=\"{}\">",
            syms.len()
        );
        // Top 5 symbols by name length (proxy for importance/complexity)
        let mut top: Vec<_> = syms.iter().take(5).collect();
        top.sort_by_key(|s| s.name.len());
        for sym in top.iter().rev() {
            println!(
                "    <symbol name=\"{}\" kind=\"{}\" file=\"{}\"/>",
                sym.name, sym.kind, sym.path
            );
        }
        println!("  </module>");
    }
    println!("</map>");
    Ok(())
}

fn cmd_status(root: &Path) -> Result<()> {
    use std::collections::BTreeSet;

    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;

    let mut context_reads = 0usize;
    let mut cache_hits = 0usize;
    let mut edits_ok = 0usize;
    let mut writes_ok = 0usize;
    let mut checks_ok = 0usize;
    let mut checks_failed = 0usize;
    let mut errors = 0usize;
    let mut lock_blocks = 0usize;
    let mut paths = BTreeSet::new();
    let mut symbols = BTreeSet::new();

    for event in &events {
        if let Some(path) = &event.path {
            paths.insert(path.clone());
        }
        if let Some(symbol) = &event.symbol {
            symbols.insert(symbol.clone());
        }
        if event.status == "error" {
            errors += 1;
        }

        match (event.event_type.as_str(), event.status.as_str()) {
            ("context.read", "ok") => context_reads += 1,
            ("context.read", "cached") => {
                context_reads += 1;
                cache_hits += 1;
            }
            ("edit.apply", "ok") => edits_ok += 1,
            ("write.apply", "ok") => writes_ok += 1,
            ("check.run", "ok") => checks_ok += 1,
            ("check.run", "error") => checks_failed += 1,
            ("lock.acquire", "blocked") => lock_blocks += 1,
            _ => {}
        }
    }

    println!("<status>");
    println!("<events>{}</events>", events.len());
    println!("<context_reads>{context_reads}</context_reads>");
    println!("<cache_hits>{cache_hits}</cache_hits>");
    println!("<edits>{edits_ok}</edits>");
    println!("<writes>{writes_ok}</writes>");
    println!("<checks_ok>{checks_ok}</checks_ok>");
    println!("<checks_failed>{checks_failed}</checks_failed>");
    println!("<lock_blocks>{lock_blocks}</lock_blocks>");
    println!("<errors>{errors}</errors>");
    println!("<paths>{}</paths>", paths.len());
    for path in &paths {
        println!("  <path>{path}</path>");
    }
    println!("<symbols>{}</symbols>", symbols.len());
    for symbol in &symbols {
        println!("  <symbol>{symbol}</symbol>");
    }
    println!("<signals>");
    println!(
        "  <signal name=\"context_used\" status=\"{}\"/>",
        if context_reads > 0 { "ok" } else { "missing" }
    );
    println!(
        "  <signal name=\"edits_applied\" status=\"{}\"/>",
        if edits_ok + writes_ok > 0 {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  <signal name=\"lock_conflicts\" status=\"{}\" count=\"{}\"/>",
        if lock_blocks == 0 { "ok" } else { "blocked" },
        lock_blocks
    );
    println!(
        "  <signal name=\"checks\" status=\"{}\" passed=\"{}\" failed=\"{}\"/>",
        if checks_failed == 0 { "ok" } else { "failed" },
        checks_ok,
        checks_failed
    );
    println!(
        "  <signal name=\"errors\" status=\"{}\" count=\"{}\"/>",
        if errors == 0 { "ok" } else { "failed" },
        errors
    );
    println!("</signals>");
    println!("</status>");

    Ok(())
}

fn cmd_check(root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("check requires a command");
    }

    let store = open_store(root)?;
    let output = std::process::Command::new(&command[0])
        .args(&command[1..])
        .current_dir(root)
        .output()?;
    let code = output.status.code().unwrap_or(-1);
    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
    let command_text = command.join(" ");

    events::record(
        store.anchor_root(),
        "check.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
    );

    println!("<check>");
    println!("<command>{command_text}</command>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    println!("</check>");

    if output.status.success() {
        Ok(())
    } else {
        bail!("check failed with exit code {code}")
    }
}

fn print_code_with_lines(code: &str, start_line: usize) {
    for (i, line) in code.lines().enumerate() {
        println!(" {:>3}: {}", start_line + i, line);
    }
}

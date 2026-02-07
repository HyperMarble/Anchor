//! Anchor CLI - Code Intelligence for AI Agents.
//!
//! Read/Search:
//!   anchor search <query>            Find symbols
//!   anchor read <symbol>             Full context
//!   anchor context <query>           Search + Read combined
//!
//! Write:
//!   anchor write <path> <content>    Create/overwrite file
//!   anchor edit <path> ...           Edit existing file
//!
//! Parallel:
//!   anchor plan <plan.json>          Parallel operations
//!
//! System:
//!   anchor build                     Build graph
//!   anchor stats                     Show stats
//!   anchor daemon [start|stop]       Manage daemon

use anchor::cli::{self, read as cli_read, Cli, Commands};
use anchor::graph::{build_graph, CodeGraph};
use anchor::updater;
use anyhow::Result;
use clap::Parser;
use std::path::Path;

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or(cli.root);
    let cache_path = root.join(".anchor/graph.bin");

    // No command = show usage only (no banner)
    if cli.command.is_none() {
        println!("anchor v{}", updater::VERSION);
        println!();
        cli::print_usage();
        return Ok(());
    }

    match cli.command.unwrap() {
        // ─── Read/Search Commands ─────────────────────────────────
        Commands::Search { query, pattern, limit } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::search(&graph, &query, pattern.as_deref(), limit)
        }

        Commands::Read { symbol } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::read(&graph, &symbol)
        }

        Commands::Context { query, limit } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::context(&graph, &query, limit)
        }

        // ─── Write Commands (TODO: ACI-based) ─────────────────────
        Commands::Write { path, content } => {
            let full_path = root.join(&path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, &content)?;
            println!(r#"{{"status": "created", "path": "{}"}}"#, path);
            Ok(())
        }

        Commands::Edit { path, action, pattern, content } => {
            let full_path = root.join(&path);
            match action.as_str() {
                "insert" => {
                    let c = content.ok_or_else(|| anyhow::anyhow!("Content required for insert"))?;
                    anchor::insert_after(&full_path, &pattern, &c)?;
                    println!(r#"{{"status": "inserted", "path": "{}"}}"#, path);
                }
                "replace" => {
                    let c = content.ok_or_else(|| anyhow::anyhow!("Content required for replace"))?;
                    anchor::replace_all(&full_path, &pattern, &c)?;
                    println!(r#"{{"status": "replaced", "path": "{}"}}"#, path);
                }
                "delete" => {
                    anchor::replace_all(&full_path, &pattern, "")?;
                    println!(r#"{{"status": "deleted", "path": "{}"}}"#, path);
                }
                _ => return Err(anyhow::anyhow!("Unknown action: {}", action)),
            }
            Ok(())
        }

        // ─── Parallel Command ─────────────────────────────────────
        Commands::Plan { file } => {
            cli::plan::execute(&root, &file)
        }

        // ─── System Commands ──────────────────────────────────────
        Commands::Build => {
            cli_read::build(&root, &cache_path)
        }

        Commands::Stats => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::stats(&graph)
        }

        Commands::Daemon { action } => {
            cli::daemon::handle(&root, action.as_ref())
        }

        Commands::Update => {
            updater::update()
        }

        Commands::Uninstall => {
            uninstall()
        }

        Commands::Version => {
            println!("v{}", updater::VERSION);
            Ok(())
        }
    }
}

/// Uninstall anchor
fn uninstall() -> Result<()> {
    let exe_path = std::env::current_exe()?;

    println!("Uninstalling anchor...");

    // Remove .anchor directory in home if exists
    if let Ok(home) = std::env::var("HOME") {
        let anchor_dir = std::path::Path::new(&home).join(".anchor");
        if anchor_dir.exists() {
            let _ = std::fs::remove_dir_all(&anchor_dir);
            println!("Removed ~/.anchor/");
        }
    }

    // Remove the binary last
    std::fs::remove_file(&exe_path)?;

    println!("Anchor uninstalled.");
    Ok(())
}

/// Load graph from cache or build if not exists
fn load_or_build_graph(root: &Path, cache_path: &Path) -> Result<CodeGraph> {
    if cache_path.exists() {
        match CodeGraph::load(cache_path) {
            Ok(graph) => return Ok(graph),
            Err(e) => {
                eprintln!("Warning: Failed to load cache, rebuilding: {}", e);
            }
        }
    }

    // Build and cache
    let graph = build_graph(root);
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = graph.save(cache_path);
    Ok(graph)
}

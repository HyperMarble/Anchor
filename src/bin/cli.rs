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

    // No command = show help
    if cli.command.is_none() {
        cli::print_usage();
        return Ok(());
    }

    match cli.command.unwrap() {
        // ─── Query Commands ───────────────────────────────────────
        Commands::Context { query, limit } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::context(&graph, &query, limit)
        }

        Commands::Search { query, pattern, limit } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::search(&graph, &query, pattern.as_deref(), limit)
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

        Commands::Edit { path, action, pattern: _, content: _ } => {
            // TODO: Write operations not finalized yet
            println!(r#"{{"status": "error", "message": "Write operations not yet finalized", "path": "{}", "action": "{}"}}"#, path, action);
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

        Commands::Map { scope } => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::map(&graph, scope.as_deref())
        }

        Commands::Overview => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::overview(&graph)
        }

        Commands::Files => {
            let graph = load_or_build_graph(&root, &cache_path)?;
            cli_read::files(&graph)
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

/// Uninstall anchor (runs shell script)
fn uninstall() -> Result<()> {
    use std::process::Command;

    let script = r#"
        INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
        if [ -w "$INSTALL_DIR" ]; then
            rm -f "$INSTALL_DIR/anchor"
        else
            sudo rm -f "$INSTALL_DIR/anchor"
        fi
        rm -rf ~/.anchor
        echo "Anchor uninstalled."
    "#;

    Command::new("sh")
        .arg("-c")
        .arg(script)
        .status()?;

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

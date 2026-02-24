//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cli::{self, read as cli_read, Cli, Commands};
use anchor::graph::{build_graph, CodeGraph};
use anchor::updater;
use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize tracing — control with RUST_LOG env var (e.g. RUST_LOG=debug)
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
    let root = roots[0].clone(); // primary root for cache/daemon
    let cache_path = root.join(".anchor/graph.bin");

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::print_usage();
            return Ok(());
        }
    };

    match command {
        // ─── Query Commands ───────────────────────────────────────
        Commands::Context {
            queries,
            limit,
            full,
        } => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::context(&graph, &queries, limit, full)
        }

        Commands::Search {
            queries,
            pattern,
            limit,
        } => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::search(&graph, &queries, pattern.as_deref(), limit)
        }

        // ─── Write Commands (TODO: ACI-based) ─────────────────────
        Commands::Write { path, content } => {
            let full_path = root.join(&path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, &content)?;
            println!("<result>");
            println!("<path>{}</path>", path);
            println!("<status>created</status>");
            println!("<bytes>{}</bytes>", content.len());
            println!("</result>");
            Ok(())
        }

        Commands::Edit {
            path,
            action,
            pattern,
            content,
        } => {
            let full_path = root.join(&path);
            match action.as_str() {
                "insert" => {
                    let c =
                        content.ok_or_else(|| anyhow::anyhow!("Content required for insert"))?;
                    anchor::insert_after(&full_path, &pattern, &c)?;
                    println!("<result>");
                    println!("<path>{}</path>", path);
                    println!("<status>inserted</status>");
                    println!("<pattern>{}</pattern>", pattern);
                    println!("</result>");
                }
                "replace" => {
                    let c =
                        content.ok_or_else(|| anyhow::anyhow!("Content required for replace"))?;
                    anchor::replace_all(&full_path, &pattern, &c)?;
                    println!("<result>");
                    println!("<path>{}</path>", path);
                    println!("<status>replaced</status>");
                    println!("<pattern>{}</pattern>", pattern);
                    println!("</result>");
                }
                "delete" => {
                    anchor::replace_all(&full_path, &pattern, "")?;
                    println!("<result>");
                    println!("<path>{}</path>", path);
                    println!("<status>deleted</status>");
                    println!("<pattern>{}</pattern>", pattern);
                    println!("</result>");
                }
                _ => return Err(anyhow::anyhow!("Unknown action: {}", action)),
            }
            Ok(())
        }

        // ─── System Commands ──────────────────────────────────────
        Commands::Init => cli::init::init(&root),

        Commands::Build => {
            cli_read::build(&roots, &cache_path)?;
            // Auto-start daemon for file watching
            if !anchor::daemon::is_daemon_running(&root) {
                cli::daemon::start_background(&roots)?;
            }
            Ok(())
        }

        Commands::Map { scope } => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::map(&graph, scope.as_deref())
        }

        Commands::Overview => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::overview(&graph)
        }

        Commands::Files => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::files(&graph)
        }

        Commands::Stats => {
            let graph = load_or_build_graph(&roots, &cache_path)?;
            cli_read::stats(&graph)
        }

        Commands::Mcp => tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime")
            .block_on(anchor::mcp::run(roots)),

        Commands::Daemon { action } => cli::daemon::handle(&roots, action.as_ref()),

        Commands::Update => updater::update(),

        Commands::Uninstall => uninstall(),

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

    Command::new("sh").arg("-c").arg(script).status()?;

    Ok(())
}

/// Load graph from cache or build if not exists
fn load_or_build_graph(roots: &[PathBuf], cache_path: &Path) -> Result<CodeGraph> {
    if cache_path.exists() {
        match CodeGraph::load(cache_path) {
            Ok(graph) => return Ok(graph),
            Err(e) => {
                eprintln!("Warning: Failed to load cache, rebuilding: {}", e);
            }
        }
    }

    // Build and cache
    let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
    let graph = build_graph(&root_refs);
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = graph.save(cache_path);
    Ok(graph)
}

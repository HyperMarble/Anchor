//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cli::{self, read as cli_read, write as cli_write, Cli, Commands};
use anchor::graph::build_graph;
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

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::print_usage();
            return Ok(());
        }
    };

    match command {
        Commands::Context {
            queries,
            limit,
            full,
        } => {
            let graph = build_fresh_legacy_graph(&roots);
            cli_read::context(&graph, &queries, limit, full)
        }

        Commands::Search {
            queries,
            pattern,
            limit,
        } => {
            let graph = build_fresh_legacy_graph(&roots);
            cli_read::search(&graph, &queries, pattern.as_deref(), limit)
        }

        Commands::Map { scope } => {
            let graph = build_fresh_legacy_graph(&roots);
            cli_read::map(&graph, scope.as_deref())
        }

        Commands::Write { path, content } => cli_write::create(&path, &content),

        Commands::Edit {
            path,
            action,
            pattern,
            content,
        } => match action.as_str() {
            "insert" => cli_write::insert(&path, &pattern, content.as_deref().unwrap_or("")),
            "replace" => {
                cli_write::replace(&root, &path, &pattern, content.as_deref().unwrap_or(""))
            }
            "delete" => cli_write::replace(&root, &path, &pattern, ""),
            other => anyhow::bail!("unknown edit action: {}", other),
        },

        Commands::Mcp => tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime")
            .block_on(anchor::mcp::run(roots)),
    }
}

/// Temporary bridge while the CLI moves from CodeGraph to `.anchor` indexes.
/// It deliberately avoids `.anchor/graph.bin` so legacy graph data cannot go stale.
fn build_fresh_legacy_graph(roots: &[PathBuf]) -> anchor::graph::CodeGraph {
    let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
    build_graph(&root_refs)
}

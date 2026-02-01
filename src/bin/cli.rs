//! Anchor CLI - Code intelligence for AI agents.
//!
//! Usage:
//!   anchor                       # Show banner and help
//!   anchor overview              # Codebase overview
//!   anchor search <query>        # Search symbols/files
//!   anchor context <query>       # Get full context
//!   anchor deps <symbol>         # Dependencies
//!   anchor stats                 # Graph statistics
//!   anchor build                 # Rebuild graph

use anchor::{build_graph, get_context, graph_search, anchor_dependencies, anchor_stats, CodeGraph};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Anchor - LSP for AI Agents", long_about = None)]
struct Cli {
    /// Project root directory (default: current directory)
    #[arg(short, long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show codebase overview - files, key symbols, entry points
    Overview,

    /// Search for symbols or files
    Search {
        /// Query string (symbol name or file path)
        query: String,

        /// How many hops to traverse in the graph
        #[arg(short, long, default_value = "1")]
        depth: usize,
    },

    /// Get full context for a symbol (code + dependencies + dependents)
    Context {
        /// Symbol name or file path
        query: String,

        /// Intent: find, understand, modify, refactor, overview
        #[arg(short, long, default_value = "understand")]
        intent: String,
    },

    /// Show what depends on a symbol and what it depends on
    Deps {
        /// Symbol name
        symbol: String,
    },

    /// Show graph statistics
    Stats,

    /// Rebuild the code graph from scratch
    Build,
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Print the ASCII banner
fn print_banner() {
    println!(r#"
    _    _   _  ____ _   _  ___  ____
   / \  | \ | |/ ___| | | |/ _ \|  _ \
  / _ \ |  \| | |   | |_| | | | | |_) |
 / ___ \| |\  | |___|  _  | |_| |  _ <
/_/   \_\_| \_|\____|_| |_|\___/|_| \_\

        LSP for AI Agents
"#);
}

fn run(cli: Cli) -> Result<()> {
    let root = cli.root.canonicalize().unwrap_or(cli.root);
    let cache_path = root.join(".anchor").join("graph.bin");

    // No command = show banner and usage
    if cli.command.is_none() {
        print_banner();
        println!("Usage: anchor <COMMAND>");
        println!();
        println!("Commands:");
        println!("  overview  Show codebase overview");
        println!("  search    Search for symbols or files");
        println!("  context   Get full context for a symbol");
        println!("  deps      Show dependencies");
        println!("  stats     Show graph statistics");
        println!("  build     Rebuild the code graph");
        println!();
        println!("Run 'anchor --help' for more info.");
        return Ok(());
    }

    // Handle build command separately (doesn't need existing graph)
    if let Some(Commands::Build) = &cli.command {
        return cmd_build(&root, &cache_path);
    }

    // For other commands, load the existing graph
    let graph = if cache_path.exists() {
        CodeGraph::load(&cache_path)?
    } else {
        eprintln!("Building graph (first run)...");
        let graph = build_graph(&root);
        std::fs::create_dir_all(cache_path.parent().unwrap())?;
        graph.save(&cache_path)?;
        graph
    };

    match cli.command {
        Some(Commands::Overview) => cmd_overview(&graph),
        Some(Commands::Search { query, depth }) => cmd_search(&graph, &query, depth),
        Some(Commands::Context { query, intent }) => cmd_context(&graph, &query, &intent),
        Some(Commands::Deps { symbol }) => cmd_deps(&graph, &symbol),
        Some(Commands::Stats) => cmd_stats(&graph),
        Some(Commands::Build) => unreachable!(), // Handled above
        None => unreachable!(), // Handled above
    }
}

/// Build/rebuild the code graph
fn cmd_build(root: &PathBuf, cache_path: &PathBuf) -> Result<()> {
    println!("Rebuilding graph...");
    let graph = build_graph(root);
    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    graph.save(cache_path)?;

    let stats = graph.stats();
    println!("Graph built successfully!");
    println!("  Files:   {}", stats.file_count);
    println!("  Symbols: {}", stats.symbol_count);
    println!("  Edges:   {}", stats.total_edges);
    Ok(())
}

/// Show codebase overview
fn cmd_overview(graph: &CodeGraph) -> Result<()> {
    print_banner();

    let stats = graph.stats();
    println!("Files:   {}", stats.file_count);
    println!("Symbols: {}", stats.symbol_count);
    println!("Edges:   {}", stats.total_edges);
    println!();

    // Show top-level structure
    let result = graph_search(graph, "src/", 0);
    if !result.matched_files.is_empty() {
        println!("Structure:");
        for file in result.matched_files.iter().take(15) {
            println!("  {}", file.display());
        }
        if result.matched_files.len() > 15 {
            println!("  ... and {} more", result.matched_files.len() - 15);
        }
    }
    println!();

    // Show entry points (main functions)
    let mains = graph_search(graph, "main", 0);
    if !mains.symbols.is_empty() {
        println!("Entry points:");
        for sym in mains.symbols.iter().filter(|s| s.name == "main") {
            println!("  {} in {}", sym.name, sym.file.display());
        }
    }
    Ok(())
}

/// Search for symbols or files
fn cmd_search(graph: &CodeGraph, query: &str, depth: usize) -> Result<()> {
    let result = graph_search(graph, query, depth);

    if result.symbols.is_empty() && result.matched_files.is_empty() {
        println!("No results for '{}'", query);
        return Ok(());
    }

    println!("Search: '{}' (depth={})", query, depth);
    println!();

    if !result.matched_files.is_empty() {
        println!("Files:");
        for file in &result.matched_files {
            println!("  {}", file.display());
        }
        println!();
    }

    if !result.symbols.is_empty() {
        println!("Symbols:");
        for sym in &result.symbols {
            println!("  {} ({}) - {}:{}", sym.name, sym.kind, sym.file.display(), sym.line);
        }
        println!();
    }

    if !result.connections.is_empty() {
        println!("Connections:");
        for conn in result.connections.iter().take(20) {
            println!("  {} --[{}]-> {}", conn.from, conn.relationship, conn.to);
        }
        if result.connections.len() > 20 {
            println!("  ... and {} more", result.connections.len() - 20);
        }
    }
    Ok(())
}

/// Get full context for a symbol
fn cmd_context(graph: &CodeGraph, query: &str, intent: &str) -> Result<()> {
    let result = get_context(graph, query, intent);
    let json = serde_json::to_string_pretty(&result)?;
    println!("{}", json);
    Ok(())
}

/// Show dependencies for a symbol
fn cmd_deps(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let result = anchor_dependencies(graph, symbol);
    let json = serde_json::to_string_pretty(&result)?;
    println!("{}", json);
    Ok(())
}

/// Show graph statistics
fn cmd_stats(graph: &CodeGraph) -> Result<()> {
    let result = anchor_stats(graph);
    let json = serde_json::to_string_pretty(&result)?;
    println!("{}", json);
    Ok(())
}

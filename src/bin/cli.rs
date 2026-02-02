//! Anchor CLI - Code intelligence for AI agents.
//!
//! Usage:
//!   anchor                       # Show banner and help
//!   anchor daemon                # Start daemon (foreground)
//!   anchor daemon start          # Start daemon (background)
//!   anchor daemon stop           # Stop daemon
//!   anchor search <query>        # Search symbols/files
//!   anchor context <query>       # Get full context
//!   anchor deps <symbol>         # Dependencies
//!   anchor stats                 # Graph statistics
//!   anchor build                 # Force rebuild graph
//!   anchor update                # Update to latest version

use anchor::daemon::{is_daemon_running, send_request, start_daemon, Request, Response};
use anchor::updater;
use anchor::{build_graph, get_context, graph_search, anchor_dependencies, anchor_stats, CodeGraph};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Start daemon in background (silent)
fn start_daemon_background(root: &PathBuf) -> Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(&exe)
        .arg("--root")
        .arg(root)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

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
    /// Manage the anchor daemon
    Daemon {
        #[command(subcommand)]
        action: Option<DaemonAction>,
    },

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

    /// Update anchor to the latest version
    Update,

    /// Show current version
    Version,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start daemon in background
    Start,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
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

    // No command = show banner and usage
    if cli.command.is_none() {
        print_banner();
        println!("v{}", updater::VERSION);
        println!();
        println!("Usage: anchor <COMMAND>");
        println!();
        println!("Commands:");
        println!("  daemon    Manage the daemon (start, stop, status)");
        println!("  overview  Show codebase overview");
        println!("  search    Search for symbols or files");
        println!("  context   Get full context for a symbol");
        println!("  deps      Show dependencies");
        println!("  stats     Show graph statistics");
        println!("  build     Rebuild the code graph");
        println!("  update    Update to latest version");
        println!("  version   Show current version");
        println!();
        println!("Run 'anchor --help' for more info.");

        // Check for updates in background
        updater::notify_if_update_available();
        std::thread::sleep(std::time::Duration::from_millis(100));

        return Ok(());
    }

    // Handle commands that don't need daemon
    match &cli.command {
        Some(Commands::Update) => {
            return updater::update();
        }
        Some(Commands::Version) => {
            println!("anchor v{}", updater::VERSION);
            if let Some(latest) = updater::check_for_update() {
                println!("Update available: {}", latest);
            }
            return Ok(());
        }
        Some(Commands::Daemon { action }) => {
            return handle_daemon_command(&root, action.as_ref());
        }
        _ => {}
    }

    // Auto-start daemon if not running (silently)
    if !is_daemon_running(&root) {
        let _ = start_daemon_background(&root);

        // Wait for daemon to be ready (up to 10 seconds)
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if is_daemon_running(&root) && send_request(&root, Request::Ping).is_ok() {
                break;
            }
        }
    }

    run_via_daemon(&root, cli.command.unwrap())
}

/// Handle daemon management commands
fn handle_daemon_command(root: &PathBuf, action: Option<&DaemonAction>) -> Result<()> {
    match action {
        None => {
            // Run daemon in foreground
            println!("Starting daemon in foreground (Ctrl+C to stop)...");
            start_daemon(root)?;
            Ok(())
        }
        Some(DaemonAction::Start) => {
            if is_daemon_running(root) {
                println!("Daemon is already running.");
                return Ok(());
            }

            // Start daemon in background
            let exe = std::env::current_exe()?;
            let child = Command::new(exe)
                .arg("--root")
                .arg(root)
                .arg("daemon")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            println!("Daemon started (PID: {})", child.id());
            Ok(())
        }
        Some(DaemonAction::Stop) => {
            if !is_daemon_running(root) {
                println!("Daemon is not running.");
                return Ok(());
            }

            match send_request(root, Request::Shutdown) {
                Ok(Response::Goodbye) => println!("Daemon stopped."),
                Ok(_) => println!("Unexpected response from daemon."),
                Err(e) => println!("Failed to stop daemon: {}", e),
            }
            Ok(())
        }
        Some(DaemonAction::Status) => {
            if is_daemon_running(root) {
                // Ping the daemon
                match send_request(root, Request::Ping) {
                    Ok(Response::Pong) => println!("Daemon is running and responsive."),
                    Ok(_) => println!("Daemon is running but gave unexpected response."),
                    Err(e) => println!("Daemon process exists but not responding: {}", e),
                }
            } else {
                println!("Daemon is not running.");
            }
            Ok(())
        }
    }
}

/// Run command via daemon (fast path)
fn run_via_daemon(root: &PathBuf, command: Commands) -> Result<()> {
    let request = match &command {
        Commands::Overview => Request::Overview,
        Commands::Search { query, depth } => Request::Search {
            query: query.clone(),
            depth: *depth,
        },
        Commands::Context { query, intent } => Request::Context {
            query: query.clone(),
            intent: intent.clone(),
        },
        Commands::Deps { symbol } => Request::Deps {
            symbol: symbol.clone(),
        },
        Commands::Stats => Request::Stats,
        Commands::Build => Request::Rebuild,
        Commands::Daemon { .. } | Commands::Update | Commands::Version => unreachable!(),
    };

    match send_request(root, request) {
        Ok(Response::Ok { data }) => {
            // Format output based on command type
            match command {
                Commands::Overview => {
                    print_banner();
                    if let Some(stats) = data.get("stats") {
                        println!("Files:   {}", stats.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0));
                        println!("Symbols: {}", stats.get("symbol_count").and_then(|v| v.as_u64()).unwrap_or(0));
                        println!("Edges:   {}", stats.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0));
                    }
                    println!();
                    if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
                        println!("Structure:");
                        for file in files.iter().take(15) {
                            if let Some(path) = file.as_str() {
                                println!("  {}", path);
                            }
                        }
                        if files.len() > 15 {
                            println!("  ... and {} more", files.len() - 15);
                        }
                    }
                }
                Commands::Search { query, depth } => {
                    println!("Search: '{}' (depth={}) [via daemon]", query, depth);
                    println!();
                    println!("{}", serde_json::to_string_pretty(&data)?);
                }
                _ => {
                    println!("{}", serde_json::to_string_pretty(&data)?);
                }
            }
            Ok(())
        }
        Ok(Response::Error { message }) => {
            eprintln!("Daemon error: {}", message);
            Ok(())
        }
        Ok(_) => {
            eprintln!("Unexpected response from daemon");
            Ok(())
        }
        Err(_) => {
            // Silently fall back to local mode
            run_local(root, command)
        }
    }
}

/// Run command locally (loads graph from disk)
fn run_local(root: &Path, command: Commands) -> Result<()> {
    let cache_path = root.join(".anchor").join("graph.bin");

    // Handle build command separately
    if let Commands::Build = command {
        return cmd_build(root, &cache_path);
    }

    // Load graph
    let graph = if cache_path.exists() {
        CodeGraph::load(&cache_path)?
    } else {
        eprintln!("Building graph (first run)...");
        let graph = build_graph(root);
        std::fs::create_dir_all(cache_path.parent().unwrap())?;
        graph.save(&cache_path)?;
        graph
    };

    match command {
        Commands::Overview => cmd_overview(&graph),
        Commands::Search { query, depth } => cmd_search(&graph, &query, depth),
        Commands::Context { query, intent } => cmd_context(&graph, &query, &intent),
        Commands::Deps { symbol } => cmd_deps(&graph, &symbol),
        Commands::Stats => cmd_stats(&graph),
        Commands::Build | Commands::Daemon { .. } | Commands::Update | Commands::Version => unreachable!(),
    }
}

/// Build/rebuild the code graph
fn cmd_build(root: &Path, cache_path: &Path) -> Result<()> {
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

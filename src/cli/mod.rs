//! CLI module for Anchor.
//!
//! Commands:
//! - Read/Search: search, read, context
//! - Write: write, edit (TODO: ACI-based)
//! - Parallel: plan
//! - System: build, stats, daemon

pub mod daemon;
pub mod plan;
pub mod read;
pub mod write;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Anchor - Code Intelligence for AI Agents", long_about = None)]
pub struct Cli {
    /// Project root directory (default: current directory)
    #[arg(short, long, default_value = ".")]
    pub root: PathBuf,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    // ─── Read/Search (3 commands) ─────────────────────────────────
    /// Search for symbols (lightweight: names, files, lines)
    Search {
        /// Symbol name to search for
        query: String,

        /// Regex pattern (Brzozowski derivatives - ReDoS safe)
        #[arg(short, long)]
        pattern: Option<String>,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Read full context for a symbol (code + callers + callees)
    Read {
        /// Symbol name
        symbol: String,
    },

    /// Search + Read combined (find + full context)
    Context {
        /// Query (symbol name or file path)
        query: String,

        /// Max results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    // ─── Write (2 commands) - TODO: ACI-based ─────────────────────
    /// Create or overwrite a file
    Write {
        /// File path
        path: String,

        /// File content (or - for stdin)
        content: String,
    },

    /// Edit an existing file (insert, replace, delete)
    Edit {
        /// File path
        path: String,

        /// Action: insert, replace, delete
        #[arg(short, long)]
        action: String,

        /// Pattern to find
        #[arg(short, long)]
        pattern: String,

        /// Content (for insert/replace)
        #[arg(short, long)]
        content: Option<String>,
    },

    // ─── Parallel (1 command) ─────────────────────────────────────
    /// Execute parallel operations from plan.json
    Plan {
        /// Path to plan JSON file
        file: String,
    },

    // ─── Overview ─────────────────────────────────────────────────
    /// Show codebase overview (files, structure, key symbols)
    Overview,

    /// List all indexed files as tree
    Files,

    // ─── System ───────────────────────────────────────────────────
    /// Build/rebuild the code graph
    Build,

    /// Show graph statistics
    Stats,

    /// Manage the anchor daemon
    Daemon {
        #[command(subcommand)]
        action: Option<daemon::DaemonAction>,
    },

    /// Update anchor to latest version
    Update,

    /// Uninstall anchor (runs shell script)
    Uninstall,

    /// Show version
    Version,
}

/// Print the ASCII banner (only for install/update)
pub fn print_banner() {
    println!(
        r#"
 █████╗ ███╗   ██╗ ██████╗██╗  ██╗ ██████╗ ██████╗
██╔══██╗████╗  ██║██╔════╝██║  ██║██╔═══██╗██╔══██╗
███████║██╔██╗ ██║██║     ███████║██║   ██║██████╔╝
██╔══██║██║╚██╗██║██║     ██╔══██║██║   ██║██╔══██╗
██║  ██║██║ ╚████║╚██████╗██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝

        Code Intelligence for AI Agents
"#
    );
}

/// Print usage help
pub fn print_usage() {
    println!("Start here:");
    println!("  build                 Index codebase (auto-starts watcher)");
    println!("  overview              Files + symbol counts by directory");
    println!("  files                 List all indexed files");
    println!();
    println!("Query (use context first):");
    println!("  context <query>       Search + code + callers + callees");
    println!("  search <query>        Find symbols (NAME KIND FILE:LINE)");
    println!("  search -p <regex>     Find by pattern");
    println!("  read <symbol>         Full context for exact symbol");
    println!();
    println!("Output format:");
    println!("  NAME KIND FILE:LINE");
    println!("  > callers");
    println!("  < callees");
    println!("  --- code");
    println!();
    println!("Write:");
    println!("  write <path> <content>");
    println!("  edit <path> -a insert|replace|delete -p <pattern> [-c content]");
}

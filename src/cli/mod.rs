//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod daemon;
pub mod read;
pub mod write;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Infrastructure for Coding AI agents")]
#[command(override_help = HELP_TEXT)]
pub struct Cli {
    /// Project root directory (default: current directory)
    #[arg(short, long, default_value = ".")]
    pub root: PathBuf,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

const HELP_TEXT: &str = "
  █████╗ ███╗   ██╗ ██████╗██╗  ██╗ ██████╗ ██████╗
 ██╔══██╗████╗  ██║██╔════╝██║  ██║██╔═══██╗██╔══██╗
 ███████║██╔██╗ ██║██║     ███████║██║   ██║██████╔╝
 ██╔══██║██║╚██╗██║██║     ██╔══██║██║   ██║██╔══██╗
 ██║  ██║██║ ╚████║╚██████╗██║  ██║╚██████╔╝██║  ██║
 ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
    Infrastructure for Coding AI agents

Start here:
  build                 Index codebase
  map                   Codebase map (modules + top symbols)
  map <scope>           Zoom into module

Query:
  context <sym> [sym2…]  Code + callers + callees
  search <q> [q2…]      Find symbols

Write:
  write <path> <content>      Create/overwrite file
  edit <path> -a <action> -p <pattern> [-c <content>]

Other:
  overview              Files + symbol counts
  stats                 Graph statistics

Options:
  -r, --root <PATH>     Project root (default: .)
";

#[derive(Subcommand)]
pub enum Commands {
    // ─── Query Commands ─────────────────────────────────────────────
    /// Get symbol context (code + callers + callees)
    Context {
        /// Symbol names to query (supports multiple)
        queries: Vec<String>,

        /// Max results per symbol
        #[arg(short, long, default_value = "5")]
        limit: usize,

        /// Show full unsliced code (disable graph slicing)
        #[arg(short = 'F', long)]
        full: bool,
    },

    /// Search for symbols (lightweight: names, files, lines)
    Search {
        /// Symbol names to search for (supports multiple)
        queries: Vec<String>,

        /// Regex pattern (Brzozowski derivatives - ReDoS safe)
        #[arg(short, long)]
        pattern: Option<String>,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    // ─── Write Commands ──────────────────────────────────────────
    /// Create or overwrite a file
    Write { path: String, content: String },

    /// Edit a file (insert, replace, delete)
    Edit {
        path: String,
        #[arg(short, long)]
        action: String,
        #[arg(short, long)]
        pattern: String,
        #[arg(short, long)]
        content: Option<String>,
    },

    // ─── Overview ─────────────────────────────────────────────────
    /// Compact codebase map for AI agents
    Map {
        /// Optional scope: zoom into specific module/directory
        scope: Option<String>,
    },

    /// Show codebase overview (files, structure, key symbols)
    Overview,

    // ─── System ───────────────────────────────────────────────────
    /// Build/rebuild the code graph
    Build,

    /// Show graph statistics
    Stats,

    // ─── Hidden Commands ─────────────────────────────────────────
    /// List all indexed files
    #[command(hide = true)]
    Files,

    /// Start MCP server (Model Context Protocol) on stdio
    Mcp,

    /// Manage the anchor daemon
    #[command(hide = true)]
    Daemon {
        #[command(subcommand)]
        action: Option<daemon::DaemonAction>,
    },

    /// Update anchor to latest version
    #[command(hide = true)]
    Update,

    /// Uninstall anchor (runs shell script)
    #[command(hide = true)]
    Uninstall,

    /// Show version
    #[command(hide = true)]
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

    Infrastructure for Coding AI agents
"#
    );
}

/// Print usage help
pub fn print_usage() {
    print!("{}", HELP_TEXT);
}

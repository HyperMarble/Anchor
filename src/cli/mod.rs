//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod write;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Coding infrastructure for AI agents — faster, cheaper, multi-agent safe")]
#[command(override_help = HELP_TEXT)]
pub struct Cli {
    /// Project root directories (can specify multiple: -r ./backend -r ./frontend)
    #[arg(short, long, default_value = ".")]
    pub root: Vec<PathBuf>,

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
  context <sym> [sym2…]  Code + callers + callees
  status                  Session quality/provenance signals
  trace                   Recent execution events
  receipt                 Machine-readable execution receipt
  check -- <cmd>          Run and record a verification command
  search <q> [q2…]      Find symbols
  map [scope]           Codebase map / zoom into module
  write <path> <content> Create/overwrite file
  edit <path> ...        Insert/replace/delete text

Options:
  -r, --root <PATH>     Project root (default: .)
";

#[derive(Subcommand)]
pub enum Commands {
    /// Get symbol context (code + callers + callees)
    Context {
        /// Symbol names to query (supports multiple)
        queries: Vec<String>,

        /// Max results per symbol
        #[arg(short, long, default_value = "5")]
        limit: usize,

        /// Show full unsliced code
        #[arg(short = 'F', long)]
        full: bool,

        /// Bundle call-index neighbors not yet shown in this output
        #[arg(short = 'b', long)]
        bundle: bool,
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

    /// Compact codebase map for AI agents
    Map {
        /// Optional scope: zoom into specific module/directory
        scope: Option<String>,
    },

    /// Create or overwrite a file
    Write {
        /// File path
        path: String,

        /// File content
        content: String,
    },

    /// Edit a file by pattern or indexed symbol
    Edit {
        /// File path
        path: String,

        /// Action: insert, replace, delete
        #[arg(short, long)]
        action: Option<String>,

        /// Pattern to match
        #[arg(short, long)]
        pattern: Option<String>,

        /// Indexed symbol to replace
        #[arg(short, long)]
        symbol: Option<String>,

        /// Content for insert/replace
        #[arg(short, long)]
        content: Option<String>,
    },

    /// Show compact execution/provenance status
    Status,

    /// Show recent execution/provenance events
    Trace {
        /// Max events to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Print a machine-readable execution receipt
    Receipt,

    /// Run a verification command and record the result
    Check {
        /// Command and arguments to run after `--`
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Index the codebase into .anchor/ store
    Build,
}

/// Print usage help
pub fn print_usage() {
    print!("{}", HELP_TEXT);
}

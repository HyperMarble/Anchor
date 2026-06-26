//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod protect;
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
  task <intent>           One-shot task intake for agent work
  context <sym> [sym2…]  Code + callers + callees
  status                  Session quality/provenance signals
  trace                   Recent execution events
  receipt                 Machine-readable execution receipt
  gate                    Enforce quality score threshold
  protect                 Make source files writable only through Anchor
  check -- <cmd>          Run and record a verification command
  run -- <cmd>            Run terminal command with raw-write audit
  search <q> [q2…]      Find symbols
  map [scope]           Codebase map / zoom into module
  write <path> <content> Create file / overwrite non-source file
  edit <path> ...        Insert/replace/delete text

Options:
  -r, --root <PATH>     Project root (default: .)
";

#[derive(Subcommand)]
pub enum Commands {
    /// Build one compact task intake: symbols, code slices, related files, and test hints
    Task {
        /// Natural-language task intent
        intent: Vec<String>,

        /// Max ranked symbols to inspect
        #[arg(short, long, default_value = "8")]
        limit: usize,

        /// Max symbols to include with code in the packet
        #[arg(short = 'c', long, default_value = "4")]
        context_limit: usize,
    },

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

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Compact codebase map for AI agents
    Map {
        /// Optional scope: zoom into specific module/directory
        scope: Option<String>,
    },

    /// Create a file or overwrite a non-source file
    Write {
        /// File path
        path: String,

        /// File content
        content: String,

        /// Required current file hash before writing. Use "missing" for new files.
        #[arg(long)]
        expect_hash: Option<String>,
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

        /// Required current file hash before editing. Use "missing" for new files.
        #[arg(long)]
        expect_hash: Option<String>,
    },

    /// Protect source files from raw writes: on, off, or status
    Protect {
        /// Action: on, off, status
        #[arg(default_value = "status")]
        action: String,
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

    /// Enforce a minimum execution quality score
    Gate {
        /// Minimum acceptable quality score
        #[arg(long, default_value = "85")]
        min_score: u8,
    },

    /// Run a verification command and record the result
    Check {
        /// Command and arguments to run after `--`
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Run a terminal command and fail if it mutates files outside Anchor writes
    Run {
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

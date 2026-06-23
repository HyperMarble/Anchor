//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod prompt;
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
  search <q> [q2…]      Find symbols
  prompt repair <text>   Repair a prompt into a repo-grounded task brief
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

    /// Check, repair, or explain project-aware prompts
    Prompt {
        #[command(subcommand)]
        command: prompt::PromptCommands,
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

    /// Index the codebase into .anchor/ store
    Build,
}

/// Print usage help
pub fn print_usage() {
    print!("{}", HELP_TEXT);
}

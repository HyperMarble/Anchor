//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod protect;

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
  status                  Session quality/provenance signals
  trace                   Recent execution events
  receipt                 Machine-readable execution receipt
  gate                    Enforce quality score threshold
  protect                 Make source files writable only through Anchor
  check -- <cmd>          Run and record a verification command
  run -- <cmd>            Run terminal command with raw-write audit

Options:
  -r, --root <PATH>     Project root (default: .)
";

#[derive(Subcommand)]
pub enum Commands {
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

}

/// Print usage help
pub fn print_usage() {
    print!("{}", HELP_TEXT);
}

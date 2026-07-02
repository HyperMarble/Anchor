//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cli::{self, protect as cli_protect, Cli, Commands};
use anchor::events;
use anchor::lock::lockd;
use anchor::storage::AnchorStore;
use anyhow::{bail, Result};
use clap::Parser;
use std::path::Path;
use tracing_subscriber::EnvFilter;

fn main() {
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
    let root = roots[0].clone();
    lockd::set_workspace(&root);

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::print_usage();
            return Ok(());
        }
    };

    match command {
        Commands::Protect { action } => cli_protect::run(&root, &action),

        Commands::Status => cmd_status(&root),

        Commands::Trace { limit } => cmd_trace(&root, limit),

        Commands::Receipt => cmd_receipt(&root),

        Commands::Gate { min_score } => cmd_gate(&root, min_score),

        Commands::Check { command } => cmd_check(&root, &command),

        Commands::Run { command } => cmd_run(&root, &command),
    }
}

include!("cli_parts/history_store.rs");
include!("cli_parts/repo_audit.rs");
include!("cli_parts/status.rs");
include!("cli_parts/gate.rs");
include!("cli_parts/check.rs");
include!("cli_parts/run.rs");
include!("cli_parts/execroot.rs");
include!("cli_parts/trace_print.rs");

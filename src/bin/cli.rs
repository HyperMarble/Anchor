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
use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::Path;
#[cfg(windows)]
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
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
    let roots: Vec<_> = cli.root.into_iter().map(canonicalize_cli_root).collect();
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

fn canonicalize_cli_root(root: std::path::PathBuf) -> std::path::PathBuf {
    let canonical = root.canonicalize().unwrap_or(root);
    normalize_windows_verbatim_path(canonical)
}

#[cfg(not(windows))]
fn normalize_windows_verbatim_path(path: std::path::PathBuf) -> std::path::PathBuf {
    path
}

#[cfg(windows)]
fn normalize_windows_verbatim_path(path: PathBuf) -> PathBuf {
    use std::os::windows::ffi::OsStrExt;

    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];

    let encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
    if let Some(rest) = encoded.strip_prefix(VERBATIM_UNC_PREFIX) {
        let mut normalized = vec![b'\\' as u16, b'\\' as u16];
        normalized.extend_from_slice(rest);
        return PathBuf::from(OsString::from_wide(&normalized));
    }
    if let Some(rest) = encoded.strip_prefix(VERBATIM_PREFIX) {
        return PathBuf::from(OsString::from_wide(rest));
    }
    path
}

include!("cli_parts/history_store.rs");
include!("cli_parts/status.rs");
include!("cli_parts/gate.rs");
include!("cli_parts/check.rs");
include!("cli_parts/run.rs");
include!("cli_parts/execroot.rs");
include!("cli_parts/trace_print.rs");
include!("cli_parts/repo_audit.rs");

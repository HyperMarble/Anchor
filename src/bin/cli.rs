//
//  cli.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anchor::cache::PersistentCache;
use anchor::cli::{self, protect as cli_protect, write as cli_write, Cli, Commands};
use anchor::events;
use anchor::lock::lockd;
use anchor::parser::language::is_indexable_text_path;
use anchor::query::slice::slice_code;
use anchor::storage::{
    content_hash, AnchorStore, CallIndex, HistoryIndex, PathIndex, ProjectProfile, SymbolEntry,
    SymbolIndex,
};
use anyhow::{bail, Result};
use clap::Parser;
use ignore::Walk;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

const TASK_PACKET_SCHEMA: &str = "anchor.task_packet";
const TASK_WORKSPACE_CURRENT: &str = "current.json";

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
        Commands::Build => cmd_build(&root),

        Commands::Task {
            intent,
            limit,
            context_limit,
        } => cmd_task(&root, &intent, limit, context_limit),

        Commands::Context {
            queries,
            limit,
            full,
            bundle,
        } => cmd_context(&root, &queries, limit, full, bundle),

        Commands::Search { queries, limit } => cmd_search(&root, &queries, limit),

        Commands::Map { scope } => cmd_map(&root, scope.as_deref()),

        Commands::Write {
            path,
            content,
            expect_hash,
        } => cli_write::create(&root, &path, &content, expect_hash.as_deref()),

        Commands::Edit {
            path,
            action,
            pattern,
            symbol,
            content,
            expect_hash,
        } => {
            if let Some(symbol) = symbol {
                let content = content
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("symbol edit requires --content"))?;
                return cli_write::replace_symbol(
                    &root,
                    &path,
                    &symbol,
                    content,
                    expect_hash.as_deref(),
                );
            }

            let action = action.ok_or_else(|| anyhow::anyhow!("edit requires --action"))?;
            let pattern = pattern.ok_or_else(|| anyhow::anyhow!("edit requires --pattern"))?;
            match action.as_str() {
                "insert" => cli_write::insert(
                    &root,
                    &path,
                    &pattern,
                    content.as_deref().unwrap_or(""),
                    expect_hash.as_deref(),
                ),
                "replace" => cli_write::replace(
                    &root,
                    &path,
                    &pattern,
                    content.as_deref().unwrap_or(""),
                    expect_hash.as_deref(),
                ),
                "delete" => cli_write::replace(&root, &path, &pattern, "", expect_hash.as_deref()),
                other => bail!("unknown edit action: {}", other),
            }
        }

        Commands::Protect { action } => cli_protect::run(&root, &action),

        Commands::Status => cmd_status(&root),

        Commands::Trace { limit } => cmd_trace(&root, limit),

        Commands::Receipt => cmd_receipt(&root),

        Commands::Gate { min_score } => cmd_gate(&root, min_score),

        Commands::Check { command } => cmd_check(&root, &command),

        Commands::Run { command } => cmd_run(&root, &command),
    }
}

include!("cli_parts/build.rs");
include!("cli_parts/history_store.rs");
include!("cli_parts/task_index.rs");
include!("cli_parts/task_index_helpers.rs");
include!("cli_parts/context_packet.rs");
include!("cli_parts/context.rs");
include!("cli_parts/task_command.rs");
include!("cli_parts/task_intake_output.rs");
include!("cli_parts/task_intake_sections.rs");
include!("cli_parts/map_git.rs");
include!("cli_parts/status.rs");
include!("cli_parts/gate.rs");
include!("cli_parts/check.rs");
include!("cli_parts/run.rs");
include!("cli_parts/execroot.rs");
include!("cli_parts/trace_print.rs");
include!("cli_parts/task_tokens.rs");
include!("cli_parts/task_symbol_rank.rs");
include!("cli_parts/task_slices.rs");
include!("cli_parts/task_path_helpers.rs");
include!("cli_parts/task_packet.rs");
include!("cli_parts/task_selection.rs");
include!("cli_parts/task_print.rs");
include!("cli_parts/verification_plan.rs");

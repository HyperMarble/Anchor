//! Daemon management: start, stop, status
//! TODO: Daemon functionality not finalized yet.

use anyhow::Result;
use clap::Subcommand;
use std::path::Path;

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start daemon in background
    Start,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
}

/// Handle daemon management commands (not finalized)
pub fn handle(_root: &Path, action: Option<&DaemonAction>) -> Result<()> {
    match action {
        None => {
            println!("Daemon functionality not yet finalized.");
            println!("Use 'anchor build' to index, then query with 'anchor search/context'.");
            Ok(())
        }
        Some(DaemonAction::Start) => {
            println!("Daemon functionality not yet finalized.");
            Ok(())
        }
        Some(DaemonAction::Stop) => {
            println!("Daemon functionality not yet finalized.");
            Ok(())
        }
        Some(DaemonAction::Status) => {
            println!("Daemon functionality not yet finalized.");
            Ok(())
        }
    }
}

/// Start daemon in background (not finalized)
pub fn start_background(_root: &Path) -> Result<()> {
    Ok(())
}

/// Wait for daemon to be ready (not finalized)
pub fn wait_for_ready(_root: &Path) {
    // No-op
}

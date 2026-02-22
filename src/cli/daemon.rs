//
//  daemon.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::daemon::{is_daemon_running, send_request, start_daemon, Request, Response};

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start daemon in background
    Start,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
}

/// Handle daemon management commands
pub fn handle(roots: &[PathBuf], action: Option<&DaemonAction>) -> Result<()> {
    let root = &roots[0];
    match action {
        None => {
            // Run daemon in foreground
            println!("Starting daemon in foreground (Ctrl+C to stop)...");
            start_daemon(roots)?;
            Ok(())
        }
        Some(DaemonAction::Start) => {
            if is_daemon_running(root) {
                println!("Daemon is already running.");
                return Ok(());
            }
            let exe = std::env::current_exe()?;
            let mut cmd = Command::new(exe);
            for r in roots {
                cmd.arg("--root").arg(r);
            }
            let child = cmd
                .arg("daemon")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            println!("Daemon started (PID: {})", child.id());
            Ok(())
        }
        Some(DaemonAction::Stop) => {
            if !is_daemon_running(root) {
                println!("Daemon is not running.");
                return Ok(());
            }
            match send_request(root, Request::Shutdown) {
                Ok(Response::Goodbye) => println!("Daemon stopped."),
                Ok(_) => println!("Unexpected response from daemon."),
                Err(e) => println!("Failed to stop daemon: {}", e),
            }
            Ok(())
        }
        Some(DaemonAction::Status) => {
            if is_daemon_running(root) {
                match send_request(root, Request::Ping) {
                    Ok(Response::Pong) => println!("Daemon is running and responsive."),
                    Ok(_) => println!("Daemon is running but gave unexpected response."),
                    Err(e) => println!("Daemon process exists but not responding: {}", e),
                }
            } else {
                println!("Daemon is not running.");
            }
            Ok(())
        }
    }
}

/// Start daemon in background (silent)
pub fn start_background(roots: &[PathBuf]) -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(&exe);
    for r in roots {
        cmd.arg("--root").arg(r);
    }
    cmd.arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// Wait for daemon to be ready
pub fn wait_for_ready(root: &Path) {
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if is_daemon_running(root) && send_request(root, Request::Ping).is_ok() {
            break;
        }
    }
}

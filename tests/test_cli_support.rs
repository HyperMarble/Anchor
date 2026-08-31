#![allow(dead_code)]
//! Shared, cfg-aware helpers for the `test_cli_*` integration tests so that
//! shell-driven scenarios behave the same on Unix (`sh -c`) and Windows
//! (`cmd.exe /C`).
//!
//! Included by other test files via:
//! `#[path = "test_cli_support.rs"] mod support;`

use std::process::Command;

/// A portable "run this tiny script" invocation: `program arg0 arg1 ...`.
pub struct ShellCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

impl ShellCommand {
    /// Appends `program` and `args` onto an existing `Command` (e.g. after `--`).
    pub fn apply(&self, cmd: &mut Command) {
        cmd.arg(self.program);
        for arg in &self.args {
            cmd.arg(arg);
        }
    }

    /// Text as anchor records it: `command.join(" ")` over `[program, ...args]`.
    /// Use this to build platform-aware assertions against recorded events.
    pub fn recorded_text(&self) -> String {
        let mut parts = vec![self.program.to_string()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

fn shell(script: impl Into<String>) -> ShellCommand {
    if cfg!(windows) {
        ShellCommand {
            program: "cmd",
            args: vec!["/C".to_string(), script.into()],
        }
    } else {
        ShellCommand {
            program: "sh",
            args: vec!["-c".to_string(), script.into()],
        }
    }
}

/// A no-op command that exits successfully (Unix `true` / Windows `exit /b 0`).
pub fn success() -> ShellCommand {
    if cfg!(windows) {
        shell("exit /b 0")
    } else {
        shell("true")
    }
}

/// A command that exits with the given status code.
pub fn exit_with(code: i32) -> ShellCommand {
    if cfg!(windows) {
        shell(format!("exit /b {code}"))
    } else {
        shell(format!("exit {code}"))
    }
}

/// Prints `text` to stdout with no meaningful separators (Unix `printf` /
/// Windows `echo`). Only use with text containing no shell-special characters.
pub fn print_text(text: &str) -> ShellCommand {
    if cfg!(windows) {
        shell(format!("echo {text}"))
    } else {
        shell(format!("printf {text}"))
    }
}

/// Reads `path` and discards the output (Unix `cat path >/dev/null` /
/// Windows `type path > nul`); used to assert read-only commands are allowed.
pub fn read_file_to_null(path: &str) -> ShellCommand {
    if cfg!(windows) {
        shell(format!("type {path} > nul"))
    } else {
        shell(format!("cat {path} >/dev/null"))
    }
}

/// Overwrites `path` with a single line of text (plus trailing newline).
pub fn write_line(path: &str, line: &str) -> ShellCommand {
    if cfg!(windows) {
        shell(format!("echo {line}> {path}"))
    } else {
        shell(format!("printf '{line}\\n' > {path}"))
    }
}

/// Initializes an isolated git repo with local (non-global) identity config,
/// stages everything, and commits it as the base revision.
pub fn init_git_repo(path: &std::path::Path) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.email")
        .arg("anchor-test@example.invalid")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.name")
        .arg("Anchor Test")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("add")
        .arg("-A")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("commit")
        .arg("-q")
        .arg("-m")
        .arg("base")
        .status()
        .unwrap()
        .success());
}

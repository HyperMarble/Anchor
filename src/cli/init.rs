//
//  init.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use std::path::{Path, PathBuf};

// ─── Types ────────────────────────────────────────────────────────────────────

enum ConfigFormat {
    Json,
    Toml,
}

/// How the agent's hook script blocks tool calls.
enum HookFormat {
    /// JSON `permissionDecision: "deny"` — Claude Code PreToolUse
    ClaudeCode,
    /// exit code 2 + JSON `permission: "deny"` — Cursor beforeShellExecution
    Cursor,
    /// JSON `{"decision": "deny"}` — Gemini CLI BeforeTool
    GeminiCli,
    /// exit code 2 + stderr message — Windsurf pre_run_command
    Windsurf,
    /// TypeScript plugin throwing Error — OpenCode tool.execute.before
    OpenCode,
}

struct HookSetup {
    /// Where to write the hook script (or TS plugin).
    script_path: PathBuf,
    /// Settings file to merge hook config into. None = script only, wire manually.
    settings_path: Option<PathBuf>,
    format: HookFormat,
}

struct Agent {
    name: &'static str,
    /// MCP config file path.
    config_path: PathBuf,
    format: ConfigFormat,
    /// Hook redirect setup. None = agent doesn't support hooks.
    hook: Option<HookSetup>,
}

#[derive(PartialEq)]
enum StepResult {
    Done,
    AlreadyDone,
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn init(root: &Path) -> Result<()> {
    let home = dirs_home();

    println!("<init>");

    setup_global_agent_rules(&home)?;

    let agents = detect_agents(root, &home);

    if agents.is_empty() {
        println!("  <summary configured=\"0\" skipped=\"0\" not_found=\"all\"/>");
        println!("</init>");
        println!("\nNo supported AI agents detected.");
        return Ok(());
    }

    let mut configured = 0u32;
    let mut skipped = 0u32;

    for agent in &agents {
        let mcp = match agent.format {
            ConfigFormat::Json => merge_json_mcp(&agent.config_path),
            ConfigFormat::Toml => merge_toml_mcp(&agent.config_path),
        };

        let hook = agent.hook.as_ref().map(setup_hook).transpose();

        match (mcp, hook) {
            (Ok(mcp_r), Ok(hook_r)) => {
                let hook_status = match hook_r {
                    Some(StepResult::Done) => "hooks=configured",
                    Some(StepResult::AlreadyDone) => "hooks=already-configured",
                    None => "hooks=not-supported",
                };
                let mcp_status = match mcp_r {
                    StepResult::Done => "mcp=configured",
                    StepResult::AlreadyDone => "mcp=already-configured",
                };
                let did_work = mcp_r == StepResult::Done
                    || hook_r
                        .as_ref()
                        .map(|r| r == &StepResult::Done)
                        .unwrap_or(false);

                println!(
                    "  <agent name=\"{}\" {} {} path=\"{}\"/>",
                    agent.name,
                    mcp_status,
                    hook_status,
                    agent.config_path.display()
                );
                if did_work {
                    configured += 1;
                } else {
                    skipped += 1;
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                println!(
                    "  <agent name=\"{}\" status=\"error\" error=\"{}\"/>",
                    agent.name, e
                );
            }
        }
    }

    println!(
        "  <summary configured=\"{}\" skipped=\"{}\"/>",
        configured, skipped
    );
    println!("</init>");

    Ok(())
}

// ─── Detection ────────────────────────────────────────────────────────────────

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"))
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn detect_agents(root: &Path, home: &Path) -> Vec<Agent> {
    let mut agents = Vec::new();

    // Claude Code: `claude` in PATH
    if command_exists("claude") {
        agents.push(Agent {
            name: "claude-code",
            config_path: root.join(".mcp.json"),
            format: ConfigFormat::Json,
            hook: Some(HookSetup {
                script_path: root.join(".claude/hooks/anchor-redirect.sh"),
                settings_path: Some(root.join(".claude/settings.json")),
                format: HookFormat::ClaudeCode,
            }),
        });
    }

    // Cursor: .cursor/ in project or ~/.cursor/
    if root.join(".cursor").is_dir() || home.join(".cursor").is_dir() {
        agents.push(Agent {
            name: "cursor",
            config_path: root.join(".cursor/mcp.json"),
            format: ConfigFormat::Json,
            hook: Some(HookSetup {
                script_path: root.join(".cursor/hooks/anchor-redirect.sh"),
                settings_path: Some(root.join(".cursor/hooks.json")),
                format: HookFormat::Cursor,
            }),
        });
    }

    // Codex: `codex` in PATH (no hooks support yet)
    if command_exists("codex") {
        agents.push(Agent {
            name: "codex",
            config_path: root.join(".codex/config.toml"),
            format: ConfigFormat::Toml,
            hook: None,
        });
    }

    // Gemini CLI: `gemini` in PATH or ~/.gemini/
    if command_exists("gemini") || home.join(".gemini").is_dir() {
        agents.push(Agent {
            name: "gemini-cli",
            config_path: home.join(".gemini/settings.json"),
            format: ConfigFormat::Json,
            hook: Some(HookSetup {
                script_path: home.join(".gemini/hooks/anchor-redirect.sh"),
                settings_path: Some(home.join(".gemini/settings.json")),
                format: HookFormat::GeminiCli,
            }),
        });
    }

    // Windsurf: ~/.codeium/windsurf/
    if home.join(".codeium/windsurf").is_dir() {
        agents.push(Agent {
            name: "windsurf",
            config_path: home.join(".codeium/windsurf/mcp_config.json"),
            format: ConfigFormat::Json,
            hook: Some(HookSetup {
                script_path: home.join(".codeium/windsurf/hooks/anchor-redirect.sh"),
                settings_path: None, // Windsurf hook config path not confirmed — script only
                format: HookFormat::Windsurf,
            }),
        });
    }

    // Kilo Code: .kilocode/ in project (no hooks support confirmed)
    if root.join(".kilocode").is_dir() {
        agents.push(Agent {
            name: "kilo-code",
            config_path: root.join(".kilocode/mcp.json"),
            format: ConfigFormat::Json,
            hook: None,
        });
    }

    // Antigravity: ~/.gemini/antigravity/
    if home.join(".gemini/antigravity").is_dir() {
        agents.push(Agent {
            name: "antigravity",
            config_path: home.join(".gemini/antigravity/mcp_config.json"),
            format: ConfigFormat::Json,
            hook: None,
        });
    }

    // OpenCode: ~/.config/opencode/ or `opencode` in PATH
    if command_exists("opencode") || home.join(".config/opencode").is_dir() {
        agents.push(Agent {
            name: "opencode",
            config_path: home.join(".config/opencode/config.json"),
            format: ConfigFormat::Json,
            hook: Some(HookSetup {
                script_path: home.join(".config/opencode/plugins/anchor-redirect.ts"),
                settings_path: None, // TS plugin loaded automatically from plugins dir
                format: HookFormat::OpenCode,
            }),
        });
    }

    agents
}

// ─── Hook setup ───────────────────────────────────────────────────────────────

fn setup_hook(setup: &HookSetup) -> Result<StepResult> {
    // Write the hook script / plugin file
    let script_written = write_hook_script(&setup.script_path, &setup.format)?;

    // Merge hook config into agent settings (if path is known)
    let config_written = if let Some(settings) = &setup.settings_path {
        merge_hook_config(settings, &setup.format, &setup.script_path)?
    } else {
        StepResult::AlreadyDone // no settings to merge
    };

    if script_written == StepResult::Done || config_written == StepResult::Done {
        Ok(StepResult::Done)
    } else {
        Ok(StepResult::AlreadyDone)
    }
}

fn write_hook_script(path: &Path, format: &HookFormat) -> Result<StepResult> {
    if path.exists() {
        return Ok(StepResult::AlreadyDone);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = hook_script_content(format);
    std::fs::write(path, &content)?;

    // Make shell scripts executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !matches!(format, HookFormat::OpenCode) {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok(StepResult::Done)
}

fn hook_script_content(format: &HookFormat) -> String {
    match format {
        HookFormat::ClaudeCode => r#"#!/bin/bash
# anchor-redirect — Claude Code PreToolUse hook
# Blocks grep/find/cat/rg when anchor graph is built. Falls back if anchor unavailable.

INPUT=$(cat)

# Fallback: allow through if graph not built yet
[ ! -f ".anchor/graph.bin" ] && exit 0

TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# Always redirect Grep and Glob tool calls
if [ "$TOOL" = "Grep" ] || [ "$TOOL" = "Glob" ]; then
  jq -n '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": "Use anchor instead: `anchor context <query>` gives graph-sliced code with callers/callees in one call. `anchor search <query>` for lightweight lookup."
    }
  }'
  exit 0
fi

# Redirect bash search/read commands
if echo "$COMMAND" | grep -qE '^(grep |rg |cat |head |tail |find |fd )'; then
  jq -n '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": "Use anchor instead: `anchor context <query>` gives graph-sliced code with callers/callees in one call. `anchor search <query>` for lightweight lookup."
    }
  }'
fi
"#.to_string(),

        HookFormat::Cursor => r#"#!/bin/bash
# anchor-redirect — Cursor beforeShellExecution hook
# Blocks grep/find/cat/rg when anchor graph is built. Falls back if anchor unavailable.

INPUT=$(cat)

# Fallback: allow through if graph not built yet
[ ! -f ".anchor/graph.bin" ] && exit 0

COMMAND=$(echo "$INPUT" | jq -r '.command // empty')

if echo "$COMMAND" | grep -qE '^(grep |rg |cat |head |tail |find |fd )'; then
  echo "Use anchor instead: \`anchor context <query>\` gives graph-sliced code with callers/callees. \`anchor search <query>\` for lightweight lookup." >&2
  exit 2
fi
"#.to_string(),

        HookFormat::GeminiCli => r#"#!/bin/bash
# anchor-redirect — Gemini CLI BeforeTool hook
# Blocks grep/find/cat/rg when anchor graph is built. Falls back if anchor unavailable.

INPUT=$(cat)

# Fallback: allow through if graph not built yet
[ ! -f ".anchor/graph.bin" ] && exit 0

COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // .command // empty')

if echo "$COMMAND" | grep -qE '^(grep |rg |cat |head |tail |find |fd )'; then
  echo '{"decision": "deny", "reason": "Use anchor instead: `anchor context <query>` gives graph-sliced code with callers/callees. `anchor search <query>` for lightweight lookup."}'
  exit 0
fi
"#.to_string(),

        HookFormat::Windsurf => r#"#!/bin/bash
# anchor-redirect — Windsurf pre_run_command hook
# Blocks grep/find/cat/rg when anchor graph is built. Falls back if anchor unavailable.

INPUT=$(cat)

# Fallback: allow through if graph not built yet
[ ! -f ".anchor/graph.bin" ] && exit 0

COMMAND=$(echo "$INPUT" | jq -r '.tool_info.command_line // .command_line // .command // empty')

if echo "$COMMAND" | grep -qE '^(grep |rg |cat |head |tail |find |fd )'; then
  echo "Use anchor instead: \`anchor context <query>\` gives graph-sliced code with callers/callees. \`anchor search <query>\` for lightweight lookup." >&2
  exit 2
fi
"#.to_string(),

        HookFormat::OpenCode => r#"// anchor-redirect — OpenCode tool.execute.before plugin
// Blocks grep/find/cat/rg when anchor graph is built. Falls back if anchor unavailable.
import { existsSync } from "fs";

export default () => ({
  "tool.execute.before": (_input: unknown, output: { args?: { command?: string; cmd?: string } }) => {
    // Fallback: allow through if graph not built yet
    if (!existsSync(".anchor/graph.bin")) return;

    const cmd = output.args?.command ?? output.args?.cmd ?? "";
    if (/^(grep|rg|cat|head|tail|find|fd)\s/.test(cmd)) {
      throw new Error(
        "Use anchor instead: `anchor context <query>` gives graph-sliced code with callers/callees. `anchor search <query>` for lightweight lookup."
      );
    }
  },
});
"#.to_string(),
    }
}

fn merge_hook_config(
    settings_path: &Path,
    format: &HookFormat,
    script_path: &Path,
) -> Result<StepResult> {
    match format {
        HookFormat::ClaudeCode => merge_claude_hooks(settings_path, script_path),
        HookFormat::Cursor => merge_cursor_hooks(settings_path, script_path),
        HookFormat::GeminiCli => merge_gemini_hooks(settings_path, script_path),
        _ => Ok(StepResult::AlreadyDone),
    }
}

/// Merge anchor hook into Claude Code .claude/settings.json
fn merge_claude_hooks(path: &Path, script_path: &Path) -> Result<StepResult> {
    let mut config: serde_json::Value = load_json(path)?;

    let hooks = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let pre_tool_use = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));

    let arr = pre_tool_use
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("PreToolUse is not an array"))?;

    // Already configured?
    let script_str = script_path.to_string_lossy();
    let already = arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|hs| {
                hs.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("anchor-redirect"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });

    if already {
        return Ok(StepResult::AlreadyDone);
    }

    arr.push(serde_json::json!({
        "matcher": "Bash|Grep|Glob",
        "hooks": [{ "type": "command", "command": script_str }]
    }));

    save_json(path, &config)?;
    Ok(StepResult::Done)
}

/// Merge anchor hook into Cursor .cursor/hooks.json
fn merge_cursor_hooks(path: &Path, script_path: &Path) -> Result<StepResult> {
    let mut config: serde_json::Value = load_json(path)?;

    // Ensure version field
    if config.get("version").is_none() {
        config["version"] = serde_json::json!(1);
    }

    let hooks = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let before_shell = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?
        .entry("beforeShellExecution")
        .or_insert_with(|| serde_json::json!([]));

    let arr = before_shell
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("beforeShellExecution is not an array"))?;

    let script_str = script_path.to_string_lossy();
    let already = arr.iter().any(|entry| {
        entry
            .get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains("anchor-redirect"))
            .unwrap_or(false)
    });

    if already {
        return Ok(StepResult::AlreadyDone);
    }

    arr.push(serde_json::json!({
        "command": script_str,
        "timeout": 5
    }));

    save_json(path, &config)?;
    Ok(StepResult::Done)
}

/// Merge anchor hook into Gemini CLI ~/.gemini/settings.json
fn merge_gemini_hooks(path: &Path, script_path: &Path) -> Result<StepResult> {
    let mut config: serde_json::Value = load_json(path)?;

    let hooks = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let before_tool = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?
        .entry("BeforeTool")
        .or_insert_with(|| serde_json::json!([]));

    let arr = before_tool
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("BeforeTool is not an array"))?;

    let script_str = script_path.to_string_lossy();
    let already = arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|hs| {
                hs.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains("anchor-redirect"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });

    if already {
        return Ok(StepResult::AlreadyDone);
    }

    arr.push(serde_json::json!({
        "hooks": [{
            "name": "anchor-redirect",
            "type": "command",
            "command": script_str
        }]
    }));

    save_json(path, &config)?;
    Ok(StepResult::Done)
}

// ─── MCP config ───────────────────────────────────────────────────────────────

fn merge_json_mcp(path: &Path) -> Result<StepResult> {
    let mut config = load_json(path)?;

    let servers = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Config is not a JSON object"))?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let obj = servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("mcpServers is not an object"))?;

    if obj.contains_key("anchor") {
        return Ok(StepResult::AlreadyDone);
    }

    obj.insert(
        "anchor".to_string(),
        serde_json::json!({ "command": "anchor", "args": ["mcp"] }),
    );

    save_json(path, &config)?;
    Ok(StepResult::Done)
}

fn merge_toml_mcp(path: &Path) -> Result<StepResult> {
    let mut table: toml::value::Table = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            toml::value::Table::new()
        } else {
            toml::from_str(&content)?
        }
    } else {
        toml::value::Table::new()
    };

    let mcp_servers = table
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));

    let mcp_table = mcp_servers
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("mcp_servers is not a table"))?;

    if mcp_table.contains_key("anchor") {
        return Ok(StepResult::AlreadyDone);
    }

    let mut anchor_table = toml::value::Table::new();
    anchor_table.insert(
        "command".to_string(),
        toml::Value::String("anchor".to_string()),
    );
    anchor_table.insert(
        "args".to_string(),
        toml::Value::Array(vec![toml::Value::String("mcp".to_string())]),
    );
    mcp_table.insert("anchor".to_string(), toml::Value::Table(anchor_table));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml::to_string_pretty(&table)?)?;
    Ok(StepResult::Done)
}

// ─── JSON helpers ─────────────────────────────────────────────────────────────

fn load_json(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    Ok(serde_json::from_str(&content)?)
}

fn save_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

// ─── Global agent rules ───────────────────────────────────────────────────────

fn setup_global_agent_rules(home: &Path) -> Result<()> {
    let config_dir = home.join(".config").join("opencode");
    std::fs::create_dir_all(&config_dir)?;

    let agents_md_path = config_dir.join("AGENTS.md");
    const BEGIN_MARKER: &str = "<!-- anchor-global-rules:begin -->";
    const END_MARKER: &str = "<!-- anchor-global-rules:end -->";

    let mut rules_block = String::new();
    rules_block.push_str(BEGIN_MARKER);
    rules_block.push('\n');
    rules_block.push_str(
        r#"# Global Rules

## Anchor Commands for Code Intelligence

When working in codebases with `anchor` installed (check for `.anchor/` directory):

**ALWAYS use anchor commands instead of shell commands:**

- `anchor context <query>` - Get symbol code + callers + callees (USE THIS FIRST)
- `anchor search <query>` - Find symbols by name
- `anchor context <symbol> --full` - Single symbol full detail
- `anchor map` - Codebase structure overview

**NEVER use these shell commands for code exploration when anchor is available:**
- `grep`, `rg` - use `anchor search` or `anchor context` instead
- `cat`, `head`, `tail` - use `anchor context` instead
- `find`, `fd` - use `anchor search` or `anchor map` instead
- `sed`, `awk` - not needed for code exploration

**Shell commands are still allowed for:**
- Git operations (`git status`, `git diff`, etc.)
- Package managers (`npm`, `cargo`, `pip`, etc.)
- Docker, file system operations (`mkdir`, `rm`, `mv`, `cp`)
- Running tests, builds, etc.

## Anchor Output Format

Anchor returns structured XML output:
```
<results query="Cli" count="1">
<symbol>
<name>Cli</name>
<kind>struct</kind>
<file>/path/to/file.rs</file>
<line>19</line>
<callers>caller1 caller2</callers>
<callees>callee1 callee2</callees>
<code>
  19: pub struct Cli {
  ...
</code>
</symbol>
</results>
```

Use this structured data for understanding code, making edits, and tracking relationships.
"#,
    );
    rules_block.push('\n');
    rules_block.push_str(END_MARKER);
    rules_block.push('\n');

    let updated = if !agents_md_path.exists() {
        std::fs::write(&agents_md_path, &rules_block)?;
        true
    } else {
        let existing = std::fs::read_to_string(&agents_md_path)?;
        if existing.contains(BEGIN_MARKER) {
            false
        } else {
            let mut merged = existing;
            if !merged.is_empty() && !merged.ends_with('\n') {
                merged.push('\n');
            }
            merged.push('\n');
            merged.push_str(&rules_block);
            std::fs::write(&agents_md_path, merged)?;
            true
        }
    };

    if updated {
        println!("  <global_rules path=\"{}\"/>", agents_md_path.display());
    } else {
        println!("  <global_rules status=\"already_exists\"/>");
    }

    Ok(())
}

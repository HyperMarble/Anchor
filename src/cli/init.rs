//
//  init.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use std::path::{Path, PathBuf};

enum ConfigFormat {
    Json,
    Toml,
}

struct Agent {
    name: &'static str,
    config_path: PathBuf,
    format: ConfigFormat,
}

enum ConfigResult {
    Configured,
    AlreadyConfigured,
}

/// Detect installed agents and configure MCP server for each.
pub fn init(root: &Path) -> Result<()> {
    let home = dirs_home();

    // Setup global agent rules (applies to ALL agents using this machine)
    setup_global_agent_rules(&home)?;

    let agents = detect_agents(root, &home);

    if agents.is_empty() {
        println!("<init>");
        println!("  <summary configured=\"0\" skipped=\"0\" not_found=\"7\"/>");
        println!("</init>");
        println!("\nNo supported AI agents detected.");
        return Ok(());
    }

    println!("<init>");

    let mut configured = 0u32;
    let mut skipped = 0u32;

    for agent in &agents {
        match configure_agent(agent) {
            Ok(ConfigResult::Configured) => {
                println!(
                    "  <agent name=\"{}\" status=\"configured\" path=\"{}\"/>",
                    agent.name,
                    agent.config_path.display()
                );
                configured += 1;
            }
            Ok(ConfigResult::AlreadyConfigured) => {
                println!(
                    "  <agent name=\"{}\" status=\"already-configured\" path=\"{}\"/>",
                    agent.name,
                    agent.config_path.display()
                );
                skipped += 1;
            }
            Err(e) => {
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

/// Get home directory.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"))
}

/// Setup global agent rules that apply to all AI agents on this machine.
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

/// Check if a command exists in PATH.
fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detect which agents are installed. Returns only found agents.
fn detect_agents(root: &Path, home: &Path) -> Vec<Agent> {
    let mut agents = Vec::new();

    // Claude Code: `claude` in PATH
    if command_exists("claude") {
        agents.push(Agent {
            name: "claude-code",
            config_path: root.join(".mcp.json"),
            format: ConfigFormat::Json,
        });
    }

    // Cursor: .cursor/ in project or ~/.cursor/
    if root.join(".cursor").is_dir() || home.join(".cursor").is_dir() {
        agents.push(Agent {
            name: "cursor",
            config_path: root.join(".cursor/mcp.json"),
            format: ConfigFormat::Json,
        });
    }

    // Codex: `codex` in PATH
    if command_exists("codex") {
        agents.push(Agent {
            name: "codex",
            config_path: root.join(".codex/config.toml"),
            format: ConfigFormat::Toml,
        });
    }

    // Gemini CLI: `gemini` in PATH or ~/.gemini/
    if command_exists("gemini") || home.join(".gemini").is_dir() {
        agents.push(Agent {
            name: "gemini-cli",
            config_path: home.join(".gemini/settings.json"),
            format: ConfigFormat::Json,
        });
    }

    // Windsurf: ~/.codeium/windsurf/
    if home.join(".codeium/windsurf").is_dir() {
        agents.push(Agent {
            name: "windsurf",
            config_path: home.join(".codeium/windsurf/mcp_config.json"),
            format: ConfigFormat::Json,
        });
    }

    // Kilo Code: .kilocode/ in project
    if root.join(".kilocode").is_dir() {
        agents.push(Agent {
            name: "kilo-code",
            config_path: root.join(".kilocode/mcp.json"),
            format: ConfigFormat::Json,
        });
    }

    // Antigravity: ~/.gemini/antigravity/
    if home.join(".gemini/antigravity").is_dir() {
        agents.push(Agent {
            name: "antigravity",
            config_path: home.join(".gemini/antigravity/mcp_config.json"),
            format: ConfigFormat::Json,
        });
    }

    agents
}

/// Write MCP config for a single agent.
fn configure_agent(agent: &Agent) -> Result<ConfigResult> {
    match agent.format {
        ConfigFormat::Json => merge_json_config(&agent.config_path),
        ConfigFormat::Toml => merge_toml_config(&agent.config_path),
    }
}

/// Merge anchor MCP entry into a JSON config file.
fn merge_json_config(path: &Path) -> Result<ConfigResult> {
    let mut root: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&content)?
        }
    } else {
        serde_json::json!({})
    };

    let servers = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Config is not a JSON object"))?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let servers_obj = servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("mcpServers is not an object"))?;

    if servers_obj.contains_key("anchor") {
        return Ok(ConfigResult::AlreadyConfigured);
    }

    servers_obj.insert(
        "anchor".to_string(),
        serde_json::json!({
            "command": "anchor",
            "args": ["mcp"]
        }),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let formatted = serde_json::to_string_pretty(&root)?;
    std::fs::write(path, formatted)?;

    Ok(ConfigResult::Configured)
}

/// Merge anchor MCP entry into a TOML config file (Codex).
fn merge_toml_config(path: &Path) -> Result<ConfigResult> {
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
        return Ok(ConfigResult::AlreadyConfigured);
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
    let formatted = toml::to_string_pretty(&table)?;
    std::fs::write(path, formatted)?;

    Ok(ConfigResult::Configured)
}

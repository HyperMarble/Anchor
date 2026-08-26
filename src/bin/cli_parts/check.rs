fn cmd_check(root: &Path, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("check requires a command");
    }

    let store = open_store(root)?;
    let mut cmd = std::process::Command::new(&command[0]);
    cmd.args(&command[1..]).current_dir(root);
    if cli_protect::is_active(root) {
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    }
    let output = cmd.output()?;
    let code = output.status.code().unwrap_or(-1);
    let status = if output.status.success() {
        "ok"
    } else {
        "error"
    };
    let command_text = command.join(" ");
    let check_kind = classify_check_command(command);
    let target_paths = check_target_paths(root, command);
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("command".to_string(), command_text.clone());
    meta.insert("check_kind".to_string(), check_kind.to_string());
    if !target_paths.is_empty() {
        meta.insert("target_paths".to_string(), target_paths.join("\n"));
    }

    events::record_with_meta(
        store.anchor_root(),
        "check.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
        meta,
    );
    let events_after = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events_after)?;
    let handoff = handoff_state(&summary);
    let defer_handoff = is_action_workspace(root);

    println!("<check>");
    println!("<command>{command_text}</command>");
    println!("<kind>{check_kind}</kind>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!("<target_paths>{}</target_paths>", target_paths.len());
    for path in &target_paths {
        println!("  <target_path>{path}</target_path>");
    }
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    if defer_handoff {
        println!("<handoff_gate status=\"deferred\" mode=\"execroot\"/>");
    } else if !handoff.ready {
        println!("<quality_feedback>");
        for blocker in &handoff.blockers {
            println!("  <warning>{}</warning>", escape_xml_text(blocker.message));
        }
        println!("</quality_feedback>");
        for blocker in &handoff.blockers {
            println!(
                "<handoff_gate status=\"blocked\" reason=\"{}\"/>",
                escape_xml_text(blocker.reason)
            );
        }
    } else {
        println!("<handoff_gate status=\"ok\"/>");
    }
    println!("</check>");

    if !output.status.success() {
        bail!("check failed with exit code {code}")
    }
    if !defer_handoff && !handoff.ready {
        bail!("handoff check failed: unresolved blockers remain")
    }
    Ok(())
}

fn is_action_workspace(root: &Path) -> bool {
    root.join(".anchor/action/instruction.md").exists() || root.join("ANCHOR_ACTION.md").exists()
}

fn classify_check_command(command: &[String]) -> &'static str {
    let tokens: Vec<String> = command
        .iter()
        .map(|token| token.to_ascii_lowercase())
        .collect();
    if tokens.is_empty() {
        return "unknown";
    }
    let runner_names = [
        "pytest", "unittest", "cargo", "go", "npm", "pnpm", "yarn", "bun", "mvn", "gradle", "tox",
        "nox", "vitest", "jest", "mocha", "rspec", "mix",
    ];
    for (idx, token) in tokens.iter().enumerate() {
        let name = Path::new(token)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(token);
        if name == "pytest"
            || name == "tox"
            || name == "nox"
            || name == "vitest"
            || name == "jest"
            || name == "mocha"
            || name == "rspec"
        {
            return "test";
        }
        if name == "python" || name == "python3" {
            if idx + 2 < tokens.len() && tokens[idx + 1] == "-m" && tokens[idx + 2] == "pytest" {
                return "test";
            }
            if idx + 2 < tokens.len() && tokens[idx + 1] == "-m" && tokens[idx + 2] == "unittest" {
                return "test";
            }
        }
        if [
            "cargo", "go", "npm", "pnpm", "yarn", "bun", "mvn", "gradle", "mix",
        ]
        .contains(&name)
            && tokens
                .iter()
                .skip(idx + 1)
                .any(|arg| arg == "test" || arg == "tests" || arg == "./...")
        {
            return "test";
        }
        if runner_names.contains(&name) && tokens.iter().any(|arg| arg.contains("test")) {
            return "test";
        }
    }
    "non_test"
}

fn check_target_paths(root: &Path, command: &[String]) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::new();
    for arg in command {
        let cleaned = arg
            .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == ',' || ch == ';')
            .trim();
        if cleaned.is_empty() || cleaned.starts_with('-') {
            continue;
        }
        if cleaned.contains("://") || cleaned.contains('=') {
            continue;
        }
        let candidate = root.join(cleaned);
        if candidate.exists() {
            let relative = candidate
                .strip_prefix(root)
                .unwrap_or(candidate.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if !relative.is_empty() && relative != "." {
                paths.insert(relative);
            }
        }
    }
    paths.into_iter().collect()
}

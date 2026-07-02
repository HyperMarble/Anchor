fn cmd_run_execroot(root: &Path, command: &[String]) -> Result<()> {
    let store = open_store(root)?;
    let command_text = command.join(" ");
    let session_id = format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
        std::process::id()
    );
    let run_root = store.anchor_root().join("runs").join(&session_id);
    let execroot = run_root.join("execroot");
    let patch_path = run_root.join("patch.diff");

    std::fs::create_dir_all(&execroot)?;
    copy_execroot(root, &execroot)?;
    init_execroot_git(&execroot)?;

    let mut cmd = std::process::Command::new(&command[0]);
    cmd.args(&command[1..])
        .current_dir(&execroot)
        .env_remove("ANCHOR_EXECROOT");
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

    let changed_paths = execroot_changed_paths(&execroot)?;
    let patch = execroot_patch(&execroot)?;
    std::fs::write(&patch_path, patch)?;

    let mut meta = std::collections::BTreeMap::new();
    meta.insert("command".to_string(), command_text.clone());
    meta.insert(
        "execroot".to_string(),
        execroot.to_string_lossy().to_string(),
    );
    meta.insert(
        "patch_path".to_string(),
        patch_path.to_string_lossy().to_string(),
    );
    if !changed_paths.is_empty() {
        meta.insert("changed_paths".to_string(), changed_paths.join("\n"));
    }
    events::record_with_meta(
        store.anchor_root(),
        "execroot.run",
        None,
        None,
        status,
        Some(format!("exit={code} cmd={command_text}")),
        meta,
    );

    println!("<run>");
    println!("<mode>execroot</mode>");
    println!("<command>{command_text}</command>");
    println!("<status>{status}</status>");
    println!("<exit_code>{code}</exit_code>");
    println!("<execroot>{}</execroot>", execroot.display());
    println!("<patch_path>{}</patch_path>", patch_path.display());
    println!("<changed_files>{}</changed_files>", changed_paths.len());
    for path in &changed_paths {
        println!("  <changed_file>{path}</changed_file>");
    }
    println!("<stdout><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    println!("]]></stdout>");
    println!("<stderr><![CDATA[");
    print!("{}", String::from_utf8_lossy(&output.stderr));
    println!("]]></stderr>");
    println!("</run>");

    if !output.status.success() {
        bail!("execroot command failed with exit code {code}");
    }
    Ok(())
}

fn copy_execroot(root: &Path, execroot: &Path) -> Result<()> {
    fn copy_dir(source: &Path, dest: &Path, base: &Path) -> Result<()> {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap_or(path.as_path());
            if should_skip_execroot_path(rel) {
                continue;
            }
            let target = dest.join(rel);
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                std::fs::create_dir_all(&target)?;
                copy_dir(&path, dest, base)?;
            } else if file_type.is_file() {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&path, &target)?;
            } else if file_type.is_symlink() {
                copy_symlink(&path, &target)?;
            }
        }
        Ok(())
    }

    copy_dir(root, execroot, root)
}

fn should_skip_execroot_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(
            name.as_ref(),
            ".anchor" | ".git" | "target" | "node_modules" | ".venv" | "__pycache__"
        )
    })
}

#[cfg(unix)]
fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let link_target = std::fs::read_link(source)?;
    std::os::unix::fs::symlink(link_target, target)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    let metadata = std::fs::metadata(source)?;
    if metadata.is_file() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
    }
    Ok(())
}

fn init_execroot_git(execroot: &Path) -> Result<()> {
    run_git(execroot, &["init", "-q"])?;
    run_git(execroot, &["config", "user.email", "anchor@example.invalid"])?;
    run_git(execroot, &["config", "user.name", "Anchor Execroot"])?;
    std::fs::write(execroot.join(".git").join("info").join("exclude"), ".anchor/\n")?;
    run_git(execroot, &["add", "-A"])?;
    run_git(execroot, &["commit", "-q", "--allow-empty", "-m", "anchor base"])?;
    Ok(())
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn execroot_changed_paths(execroot: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(execroot)
        .arg("status")
        .arg("--porcelain")
        .arg("--untracked-files=all")
        .output()?;
    if !output.status.success() {
        bail!(
            "git status failed in execroot: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut paths = std::collections::BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        if path.is_empty() || path.starts_with(".anchor/") {
            continue;
        }
        if let Some((_, new_path)) = path.split_once(" -> ") {
            paths.insert(new_path.to_string());
        } else {
            paths.insert(path.to_string());
        }
    }
    Ok(paths.into_iter().collect())
}

fn execroot_patch(execroot: &Path) -> Result<Vec<u8>> {
    run_git(execroot, &["add", "-N", "."])?;
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(execroot)
        .arg("diff")
        .arg("--binary")
        .output()?;
    if !output.status.success() {
        bail!(
            "git diff failed in execroot: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}


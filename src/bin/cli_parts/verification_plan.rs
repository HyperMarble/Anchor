fn build_task_verification_plan(likely_tests: &[(&String, usize)]) -> TaskVerificationPlan {
    let mut hints_with_rank: Vec<(usize, TaskCheckHint)> = Vec::new();

    if let Some((rank, command)) = python_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "python_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = go_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "go_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = rust_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "rust_tests".to_string(),
                command,
            },
        ));
    }
    if let Some((rank, command)) = js_test_command(likely_tests) {
        hints_with_rank.push((
            rank,
            TaskCheckHint {
                kind: "javascript_tests".to_string(),
                command,
            },
        ));
    }

    hints_with_rank.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.kind.cmp(&b.1.kind)));
    let preferred_check = hints_with_rank
        .first()
        .map(|(_, hint)| hint.command.clone());
    let check_hints = hints_with_rank.into_iter().map(|(_, hint)| hint).collect();

    TaskVerificationPlan {
        steps: vec![
            "Run at least one focused test-like command through anchor check before handoff."
                .to_string(),
            "If a check fails, fix the cause and rerun that same command successfully.".to_string(),
        ],
        preferred_check,
        check_hints,
    }
}

fn python_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            let path = path.as_str();
            if path.ends_with(".py") && is_runnable_python_test_path(path) {
                Some((rank, format!("python -m pytest {path}")))
            } else {
                None
            }
        })
}

fn go_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            if path.ends_with("_test.go") {
                Some((rank, format!("go test {}", test_package_arg(path))))
            } else {
                None
            }
        })
}

fn rust_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            if !path.starts_with("tests/") || !path.ends_with(".rs") {
                return None;
            }
            let stem = Path::new(path).file_stem().and_then(|stem| stem.to_str())?;
            Some((rank, format!("cargo test --test {stem}")))
        })
}

fn js_test_command(likely_tests: &[(&String, usize)]) -> Option<(usize, String)> {
    likely_tests
        .iter()
        .enumerate()
        .find_map(|(rank, (path, _))| {
            let lower = path.to_ascii_lowercase();
            if is_runnable_javascript_test_path(&lower) {
                Some((rank, format!("npm test -- {}", path.as_str())))
            } else {
                None
            }
        })
}

fn test_package_arg(path: &str) -> String {
    let parent = Path::new(path)
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    if parent.is_empty() {
        ".".to_string()
    } else {
        format!("./{}", parent.replace('\\', "/"))
    }
}

fn print_verification_plan(plan: &TaskVerificationPlan) {
    println!("<verification_plan>");
    for step in &plan.steps {
        println!("  <step>{}</step>", escape_xml_text(step));
    }
    if let Some(command) = &plan.preferred_check {
        println!(
            "  <preferred_check command=\"{}\"/>",
            escape_xml_text(command)
        );
    }
    println!("</verification_plan>");
    println!("<check_hints>");
    for hint in &plan.check_hints {
        println!(
            "  <hint kind=\"{}\" command=\"{}\"/>",
            escape_xml_text(&hint.kind),
            escape_xml_text(&hint.command)
        );
    }
    println!("</check_hints>");
}

fn is_runnable_python_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if file_name == "conftest.py" || file_name == "__init__.py" {
        return false;
    }
    file_name.starts_with("test_") || file_name.ends_with("_test.py")
}

fn is_runnable_javascript_test_path(lower_path: &str) -> bool {
    lower_path.ends_with(".test.ts")
        || lower_path.ends_with(".spec.ts")
        || lower_path.ends_with(".test.tsx")
        || lower_path.ends_with(".spec.tsx")
        || lower_path.ends_with(".test.js")
        || lower_path.ends_with(".spec.js")
        || lower_path.ends_with(".test.jsx")
        || lower_path.ends_with(".spec.jsx")
}

fn add_path_score(
    scores: &mut std::collections::BTreeMap<String, usize>,
    path: String,
    score: usize,
) {
    let entry = scores.entry(path).or_default();
    *entry += score;
}

fn path_signal_tokens(path: &str) -> std::collections::BTreeSet<String> {
    const GENERIC_PATH_TOKENS: &[&str] = &[
        "app", "bin", "core", "lib", "main", "mod", "package", "packages", "python", "src", "test",
        "tests",
    ];

    path.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
        .filter(|part| !GENERIC_PATH_TOKENS.contains(&part.as_str()))
        .collect()
}

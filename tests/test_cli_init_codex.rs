use std::fs;
use std::process::Command;

#[test]
fn cli_init_writes_idempotent_project_agent_rules() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let first = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("init")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first init failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let agents_path = dir.path().join("AGENTS.md");
    let first_rules = fs::read_to_string(&agents_path).unwrap();
    assert!(first_rules.contains("<!-- anchor-cli-rules:begin -->"));
    assert!(first_rules.contains("anchor context <symbol>"));
    assert!(first_rules.contains("anchor write"));

    let second = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("init")
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second init failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let second_rules = fs::read_to_string(&agents_path).unwrap();
    assert_eq!(first_rules, second_rules);
    assert_eq!(
        second_rules
            .matches("<!-- anchor-cli-rules:begin -->")
            .count(),
        1
    );
}

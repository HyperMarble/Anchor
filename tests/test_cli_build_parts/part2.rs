#[test]
fn cli_task_intake_includes_constructor_context_for_ranked_classes() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/state.py"),
        "class State:\n    def __init__(self, data=None):\n        self.data = data\n\n    def snapshot(self):\n        return dict(self.data or {})\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("add scoped state data defaults")
        .arg("--limit")
        .arg("3")
        .arg("--context-limit")
        .arg("1")
        .output()
        .unwrap();

    assert!(
        task.status.success(),
        "task failed: {}\n{}",
        String::from_utf8_lossy(&task.stderr),
        String::from_utf8_lossy(&task.stdout)
    );

    let stdout = String::from_utf8_lossy(&task.stdout);
    assert!(stdout.contains("<name>State</name>"), "{stdout}");
    assert!(
        stdout.contains("<child_context role=\"constructor\" name=\"__init__\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("def __init__(self, data=None):"),
        "{stdout}"
    );
    assert!(stdout.contains("self.data = data"), "{stdout}");
}

#[test]
fn cli_task_intake_uses_git_history_for_related_tests() {
    let dir = tempfile::tempdir().unwrap();
    run_git(dir.path(), &["init", "-q"]);
    run_git(
        dir.path(),
        &["config", "user.email", "anchor-test@example.invalid"],
    );
    run_git(dir.path(), &["config", "user.name", "Anchor Test"]);

    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("src/payments.py"),
        "def refund_payment(order):\n    return order\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/audit_log.py"),
        "def write_entry(message):\n    return message\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/test_refund.py"),
        "def test_refund_payment():\n    assert True\n",
    )
    .unwrap();
    run_git(
        dir.path(),
        &[
            "add",
            "src/payments.py",
            "src/audit_log.py",
            "tests/test_refund.py",
        ],
    );
    run_git(dir.path(), &["commit", "-q", "-m", "add refund flow"]);

    fs::write(
        dir.path().join("src/payments.py"),
        "def refund_payment(order):\n    return payment_lock(order)\n\n\ndef payment_lock(order):\n    return order\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/audit_log.py"),
        "def write_entry(message):\n    return {\"message\": message}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/test_refund.py"),
        "def test_refund_payment():\n    assert payment_lock_fixture()\n\n\ndef payment_lock_fixture():\n    return True\n",
    )
    .unwrap();
    run_git(
        dir.path(),
        &[
            "add",
            "src/payments.py",
            "src/audit_log.py",
            "tests/test_refund.py",
        ],
    );
    run_git(dir.path(), &["commit", "-q", "-m", "fix refund locking"]);

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("fix refund payment locking")
        .arg("--limit")
        .arg("8")
        .arg("--context-limit")
        .arg("2")
        .output()
        .unwrap();

    assert!(
        task.status.success(),
        "task failed: {}\n{}",
        String::from_utf8_lossy(&task.stderr),
        String::from_utf8_lossy(&task.stdout)
    );
    let stdout = String::from_utf8_lossy(&task.stdout);
    assert!(stdout.contains("<historical_files"), "{stdout}");
    assert!(
        stdout.contains("<historical_tests count=\"1\">"),
        "{stdout}"
    );
    assert!(stdout.contains("tests/test_refund.py"), "{stdout}");

    let history_path = dir.path().join(".anchor/index/history.json");
    let history: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&history_path).unwrap()).unwrap();
    assert_eq!(history["schema"], "anchor.history_index.v2");
    let neighbors = history["adjacency"]["src/payments.py"]
        .as_array()
        .unwrap_or_else(|| panic!("missing adjacency in {}", history));
    let refund_test = neighbors
        .iter()
        .find(|neighbor| neighbor["related_path"] == "tests/test_refund.py")
        .unwrap_or_else(|| panic!("missing historical neighbor in {}", history));
    assert_eq!(refund_test["commits"], 2);
    assert!(refund_test["score"].as_u64().unwrap() >= 2, "{}", history);

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert!(
        workspace["related_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "src/audit_log.py" && file["reason"] == "related+history"),
        "{workspace}"
    );
}

#[test]
fn cli_task_intake_ranks_behavior_owners_above_generic_verbs() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("statemachine/engines")).unwrap();
    fs::write(
        dir.path().join("statemachine/state.py"),
        r#"
class State:
    def __init__(self, initial=False, data=None):
        self.initial = initial
        self.data = data
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("statemachine/events.py"),
        r#"
def add(events):
    return events
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("statemachine/statemachine.py"),
        r#"
class StateChart:
    def __init__(self):
        self.configuration = []
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("statemachine/engines/base.py"),
        r#"
class BaseEngine:
    def _enter_states(self):
        pass

    def _exit_states(self):
        pass
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("add scoped per-state data lifecycle")
        .arg("--limit")
        .arg("8")
        .arg("--context-limit")
        .arg("3")
        .output()
        .unwrap();

    assert!(
        task.status.success(),
        "task failed: {}\n{}",
        String::from_utf8_lossy(&task.stderr),
        String::from_utf8_lossy(&task.stdout)
    );
    let stdout = String::from_utf8_lossy(&task.stdout);
    let state_rank = stdout
        .find("name=\"State\"")
        .unwrap_or_else(|| panic!("State should be ranked in task intake:\n{stdout}"));
    if let Some(add_rank) = stdout.find("name=\"add\"") {
        assert!(
            state_rank < add_rank,
            "State should rank before generic add():\n{stdout}"
        );
    }
    assert!(stdout.contains("statemachine/state.py"), "{stdout}");
    assert!(stdout.contains("statemachine/engines/base.py"), "{stdout}");
}


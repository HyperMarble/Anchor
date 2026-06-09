use std::fs;
use std::process::Command;

fn run_git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {:?} failed to start: {e}", args));
    assert!(
        output.status.success(),
        "git {:?} failed: {}\n{}",
        args,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_search_auto_builds_once_before_reads() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("src.py"), "def login():\n    return True\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let search = Command::new(anchor)
        .env("ANCHOR_AGENT_ID", "agent-auto-build-test")
        .env("ANCHOR_SESSION_ID", "session-auto-build-test")
        .arg("--root")
        .arg(dir.path())
        .arg("search")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "search failed: {}\n{}",
        String::from_utf8_lossy(&search.stderr),
        String::from_utf8_lossy(&search.stdout)
    );
    assert!(
        String::from_utf8_lossy(&search.stderr).contains("auto-built index"),
        "first read should auto-build once: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(
        String::from_utf8_lossy(&search.stdout).contains("login"),
        "{}",
        String::from_utf8_lossy(&search.stdout)
    );

    let context = Command::new(anchor)
        .env("ANCHOR_AGENT_ID", "agent-auto-build-test")
        .env("ANCHOR_SESSION_ID", "session-auto-build-test")
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        context.status.success(),
        "context failed: {}\n{}",
        String::from_utf8_lossy(&context.stderr),
        String::from_utf8_lossy(&context.stdout)
    );
    assert!(
        !String::from_utf8_lossy(&context.stderr).contains("auto-built index"),
        "second read should use the existing index: {}",
        String::from_utf8_lossy(&context.stderr)
    );

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));
    let auto_builds = raw_events
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .map(|event| event["event_type"] == "index.build")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(auto_builds, 1, "{raw_events}");
}

#[test]
fn cli_task_intake_auto_builds_and_records_one_intake_event() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("src/payments.py"),
        "def refund_payment(order):\n    return payment_lock(order)\n\n\ndef payment_lock(order):\n    return order\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/test_refund.py"),
        "def test_refund_payment():\n    assert True\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .env("ANCHOR_AGENT_ID", "agent-task-packet-test")
        .env("ANCHOR_SESSION_ID", "session-task-packet-test")
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
    assert!(
        String::from_utf8_lossy(&task.stderr).contains("auto-built index"),
        "task should auto-build on first intake: {}",
        String::from_utf8_lossy(&task.stderr)
    );

    let stdout = String::from_utf8_lossy(&task.stdout);
    assert!(stdout.contains("<task_intake>"), "{stdout}");
    assert!(stdout.contains("refund_payment"), "{stdout}");
    assert!(stdout.contains("payment_lock"), "{stdout}");
    assert!(stdout.contains("tests/test_refund.py"), "{stdout}");

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));
    let task_intakes = raw_events
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .map(|event| event["event_type"] == "task.intake")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(task_intakes, 1, "{raw_events}");
}

#[test]
fn cli_task_intake_prefers_symbol_whose_source_owns_intent_terms() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/configuration.py"),
        "class Configuration:\n    def states(self):\n        return []\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/callbacks.py"),
        "class CallbackSpec:\n    def call(self):\n        return None\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/state.py"),
        "class State:\n    def __init__(self, data=None):\n        self.data = data\n\n    def state_data_snapshot(self):\n        return dict(self.data or {})\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("add scoped state data defaults callbacks snapshots")
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
    let state_pos = stdout.find("symbol name=\"State\"").unwrap_or(usize::MAX);
    let config_pos = stdout
        .find("symbol name=\"Configuration\"")
        .unwrap_or(usize::MAX);
    let callback_pos = stdout
        .find("symbol name=\"CallbackSpec\"")
        .unwrap_or(usize::MAX);

    assert!(state_pos < config_pos, "{stdout}");
    assert!(state_pos < callback_pos, "{stdout}");
    assert!(stdout.contains("<name>State</name>"), "{stdout}");
}

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
        dir.path().join("tests/test_refund.py"),
        "def test_refund_payment():\n    assert True\n",
    )
    .unwrap();
    run_git(
        dir.path(),
        &["add", "src/payments.py", "tests/test_refund.py"],
    );
    run_git(dir.path(), &["commit", "-q", "-m", "add refund flow"]);

    fs::write(
        dir.path().join("src/payments.py"),
        "def refund_payment(order):\n    return payment_lock(order)\n\n\ndef payment_lock(order):\n    return order\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/test_refund.py"),
        "def test_refund_payment():\n    assert payment_lock_fixture()\n\n\ndef payment_lock_fixture():\n    return True\n",
    )
    .unwrap();
    run_git(
        dir.path(),
        &["add", "src/payments.py", "tests/test_refund.py"],
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

#[test]
fn cli_task_intake_ranks_tests_related_to_source_paths() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("statemachine/io/scxml")).unwrap();
    fs::create_dir_all(dir.path().join("tests/scxml")).unwrap();
    fs::create_dir_all(dir.path().join("tests/examples")).unwrap();
    fs::write(
        dir.path().join("statemachine/io/scxml/schema.py"),
        "class State:\n    datamodel = None\n    data = None\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("statemachine/io/scxml/parser.py"),
        "def parse_state_data(node):\n    return node\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/scxml/test_scxml_cases.py"),
        "def test_scxml_cases():\n    assert True\n",
    )
    .unwrap();
    fs::write(
        dir.path()
            .join("tests/examples/statechart_history_machine.py"),
        "class HistoryMachine:\n    pass\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("add scoped state data lifecycle")
        .arg("--limit")
        .arg("4")
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
    let scxml_test = stdout
        .find("tests/scxml/test_scxml_cases.py")
        .unwrap_or_else(|| panic!("SCXML test should be listed:\n{stdout}"));
    let unrelated_example = stdout
        .find("tests/examples/statechart_history_machine.py")
        .unwrap_or(usize::MAX);
    assert!(scxml_test < unrelated_example, "{stdout}");
    assert!(stdout.contains("<verification_plan>"), "{stdout}");
    assert!(stdout.contains("<check_hints>"), "{stdout}");
    assert!(
        stdout.contains("tests/scxml/test_scxml_cases.py"),
        "{stdout}"
    );
}

#[test]
fn cli_build_skips_binary_assets_before_utf8_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/images")).unwrap();
    fs::write(dir.path().join("src.py"), "def login():\n    return True\n").unwrap();
    fs::write(dir.path().join("docs/images/logo.png"), [0, 159, 146, 150]).unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let build = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(
        build.status.success(),
        "build failed: {}\n{}",
        String::from_utf8_lossy(&build.stderr),
        String::from_utf8_lossy(&build.stdout)
    );
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(!stderr.contains("read fail"), "{stderr}");

    let stdout = String::from_utf8_lossy(&build.stdout);
    assert!(stdout.contains("<files>1</files>"), "{stdout}");
}

#[test]
fn cli_context_truncates_large_default_symbol_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("def noisy():\n");
    for i in 0..180 {
        source.push_str(&format!("    print({i})\n"));
    }
    fs::write(dir.path().join("src.py"), source).unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let build = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\n{}",
        String::from_utf8_lossy(&build.stderr),
        String::from_utf8_lossy(&build.stdout)
    );

    let context = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("noisy")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        context.status.success(),
        "context failed: {}\n{}",
        String::from_utf8_lossy(&context.stderr),
        String::from_utf8_lossy(&context.stdout)
    );

    let stdout = String::from_utf8_lossy(&context.stdout);
    assert!(stdout.contains("context truncated"), "{stdout}");
    assert!(!stdout.contains("print(179)"), "{stdout}");
}

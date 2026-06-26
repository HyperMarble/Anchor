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
fn cli_task_intake_scopes_fresh_repo_without_full_auto_build() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("src/payments.py"),
        "def refund_payment(order):\n    return payment_lock(order)\n\n\ndef payment_lock(order):\n    return order\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/catalog.py"),
        "def reserve_stock(item):\n    return {\"item\": item, \"reserved\": True}\n",
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
        !String::from_utf8_lossy(&task.stderr).contains("auto-built index"),
        "task intake should use scoped indexing instead of full auto-build: {}",
        String::from_utf8_lossy(&task.stderr)
    );

    let stdout = String::from_utf8_lossy(&task.stdout);
    assert!(stdout.contains("<task_intake>"), "{stdout}");
    assert!(stdout.contains("<scoped_files>"), "{stdout}");
    assert!(stdout.contains("refund_payment"), "{stdout}");
    assert!(stdout.contains("payment_lock"), "{stdout}");
    assert!(stdout.contains("tests/test_refund.py"), "{stdout}");

    let symbols_path = dir.path().join(".anchor/index/symbols.json");
    let symbols = fs::read_to_string(&symbols_path)
        .unwrap_or_else(|e| panic!("missing scoped symbols {}: {e}", symbols_path.display()));
    assert!(symbols.contains("src/payments.py"), "{symbols}");
    assert!(
        !symbols.contains("src/catalog.py"),
        "task intake should not parse unrelated source files in a fresh repo: {symbols}"
    );

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));
    let full_builds = raw_events
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .map(|event| event["event_type"] == "index.build")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(full_builds, 0, "{raw_events}");
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


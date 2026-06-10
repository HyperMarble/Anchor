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
fn cli_task_intake_creates_active_workspace_with_exact_slices() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::create_dir_all(dir.path().join("src/catalog")).unwrap();
    fs::create_dir_all(dir.path().join("tests/auth")).unwrap();
    fs::write(
        dir.path().join("src/auth/session.py"),
        r#"
class SessionManager:
    def issue_session(self, user):
        return {"user": user, "active": True}

    def rotate_refresh_token(self, credential):
        revoked = self.revoke_refresh_credential(credential)
        return {"rotated": revoked}

    def revoke_refresh_credential(self, credential):
        credential["revoked"] = True
        return credential
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("src/catalog/inventory.py"),
        r#"
class InventoryCounter:
    def reserve_stock(self, item):
        return {"item": item, "reserved": True}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/auth/test_session_rotation.py"),
        "def test_refresh_token_rotation_revokes_old_credential():\n    assert True\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("fix logged in users keeping old refresh credentials after rotation")
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
    assert!(stdout.contains("<task_workspace"), "{stdout}");
    assert!(stdout.contains("<exact_slices"), "{stdout}");
    assert!(stdout.contains("src/auth/session.py"), "{stdout}");
    assert!(stdout.contains("rotate_refresh_token"), "{stdout}");
    assert!(
        stdout.contains("tests/auth/test_session_rotation.py"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("reserve_stock"),
        "unrelated inventory implementation should not be part of exact slices:\n{stdout}"
    );

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert_eq!(workspace["schema"], "anchor.task_workspace.v1");
    assert_eq!(
        workspace["active_paths"][0]["path"], "src/auth/session.py",
        "{workspace}"
    );
    let slices = workspace["exact_slices"].as_array().unwrap();
    assert!(
        slices
            .iter()
            .any(|slice| slice["symbol"] == "rotate_refresh_token"
                && slice["path"] == "src/auth/session.py"),
        "{workspace}"
    );
    assert!(
        workspace["likely_tests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|test| test["path"] == "tests/auth/test_session_rotation.py"),
        "{workspace}"
    );
    assert_eq!(
        workspace["verification_plan"]["preferred_check"],
        "python -m pytest tests/auth/test_session_rotation.py",
        "{workspace}"
    );
    assert!(
        workspace["verification_plan"]["check_hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint["kind"] == "python_tests"
                && hint["command"] == "python -m pytest tests/auth/test_session_rotation.py"),
        "{workspace}"
    );
}

#[test]
fn cli_task_intake_prefers_same_stem_package_tests() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("tracing")).unwrap();
    fs::create_dir_all(dir.path().join("discovery")).unwrap();
    fs::write(
        dir.path().join("tracing/tracing.go"),
        r#"
package tracing

type Manager struct{}

func (m *Manager) ApplyConfig() error {
    return buildTracerProvider()
}

func buildTracerProvider() error {
    return nil
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("tracing/tracing_test.go"),
        r#"
package tracing

func TestReinstallingTracerProvider(t *testing.T) {}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("discovery/manager.go"),
        r#"
package discovery

type Manager struct{}

func (m *Manager) ApplyConfig() error {
    return nil
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("discovery/manager_test.go"),
        r#"
package discovery

func TestManagerApplyConfig(t *testing.T) {}
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("tracing manager apply config reinstall tracer provider")
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

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert_eq!(
        workspace["active_paths"][0]["path"],
        "tracing/tracing.go",
        "{}",
        String::from_utf8_lossy(&task.stdout)
    );
    assert_eq!(
        workspace["likely_tests"][0]["path"],
        "tracing/tracing_test.go",
        "{}",
        String::from_utf8_lossy(&task.stdout)
    );
    assert_eq!(
        workspace["verification_plan"]["preferred_check"], "go test ./tracing",
        "{workspace}"
    );
    assert!(
        workspace["verification_plan"]["check_hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint["kind"] == "go_tests" && hint["command"] == "go test ./tracing"),
        "{workspace}"
    );
}

#[test]
fn cli_task_intake_references_workspace_slice_instead_of_duplicating_context_code() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/telemetry.py"),
        r#"
def record_runtime_action(payload):
    payload["recorded"] = True
    return payload
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("record runtime action telemetry payload")
        .arg("--limit")
        .arg("4")
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
    assert!(stdout.contains("<workspace_slice_ref"), "{stdout}");
    assert_eq!(
        stdout.matches("return payload").count(),
        1,
        "context should refer to the exact workspace slice instead of duplicating its code:\n{stdout}"
    );
}

#[test]
fn cli_task_intake_prefers_exact_slices_over_large_owner_classes() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();

    let mut large_owner = String::from("class RuntimeCoordinator:\n");
    for idx in 0..120 {
        large_owner.push_str(&format!(
            "    def noisy_{idx}(self):\n        return 'state data default lifecycle callback snapshot'\n"
        ));
    }
    fs::write(dir.path().join("src/runtime.py"), large_owner).unwrap();
    fs::write(
        dir.path().join("src/state_data.py"),
        r#"
class DataVar:
    def __init__(self, default=None, factory=None, type=None):
        self.default = default
        self.factory = factory
        self.type = type

def validate_data_declaration(data):
    if not isinstance(data, dict):
        raise ValueError("data requires dict")
    return data
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("state accepts data defaults lifecycle callbacks snapshots DataVar validates declarations and factories")
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

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    let slices = workspace["exact_slices"].as_array().unwrap();
    assert!(
        slices
            .iter()
            .any(|slice| slice["symbol"] == "validate_data_declaration"),
        "{workspace}"
    );
    assert!(
        !slices
            .iter()
            .any(|slice| slice["symbol"] == "RuntimeCoordinator"),
        "large owner classes should not dominate exact task slices: {workspace}"
    );
}

#[test]
fn cli_task_intake_prefers_direct_module_over_noisy_large_function() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/ui")).unwrap();
    fs::create_dir_all(dir.path().join("src/runtime")).unwrap();
    fs::create_dir_all(dir.path().join("tests/runtime")).unwrap();

    let mut noisy_ui = String::from("export function App() {\n");
    for idx in 0..150 {
        noisy_ui.push_str(&format!(
            "  const row{idx} = 'telemetry event runtime action recorded policy dashboard';\n"
        ));
    }
    noisy_ui.push_str("  return null;\n}\n");
    fs::write(dir.path().join("src/ui/App.jsx"), noisy_ui).unwrap();

    fs::write(
        dir.path().join("src/runtime/telemetry.py"),
        r#"
def record_runtime_action(event, action):
    return {"event": event, "action": action, "recorded": True}

def flush_telemetry_events(outbox):
    return list(outbox)
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/runtime/test_telemetry.py"),
        "def test_record_runtime_action():\n    assert True\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("how telemetry events are recorded for runtime actions")
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

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert_eq!(
        workspace["active_paths"][0]["path"], "src/runtime/telemetry.py",
        "{workspace}"
    );
    assert!(
        workspace["exact_slices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slice| {
                slice["path"] == "src/runtime/telemetry.py"
                    && slice["symbol"] == "record_runtime_action"
            }),
        "{workspace}"
    );
    assert!(
        !workspace["exact_slices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slice| slice["symbol"] == "App"),
        "large noisy UI function should not be an exact task slice: {workspace}"
    );
    assert!(
        workspace["likely_tests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|test| test["path"] == "tests/runtime/test_telemetry.py"),
        "{workspace}"
    );
}

#[test]
fn cli_task_intake_builds_rust_integration_test_hint() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("src/storage.rs"),
        r#"
pub struct ObjectStore;

impl ObjectStore {
    pub fn content_addressed_object_path(hash: &str) -> String {
        format!("objects/{}/{}", &hash[..2], hash)
    }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/storage_test.rs"),
        r#"
#[test]
fn content_addressed_object_path_uses_hash_prefix_directory() {}
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let task = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("task")
        .arg("content addressed object path hash prefix directory")
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

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert!(
        workspace["likely_tests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|test| test["path"] == "tests/storage_test.rs"),
        "{workspace}"
    );
    assert_eq!(
        workspace["verification_plan"]["preferred_check"], "cargo test --test storage_test",
        "{workspace}"
    );
    assert!(
        workspace["verification_plan"]["check_hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint["kind"] == "rust_tests"
                && hint["command"] == "cargo test --test storage_test"),
        "{workspace}"
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

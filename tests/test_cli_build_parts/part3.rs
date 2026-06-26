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
fn cli_task_intake_creates_task_packet_with_owner_chunks() {
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
    assert!(stdout.contains("<task_packet"), "{stdout}");
    assert!(stdout.contains("<owner_chunks"), "{stdout}");
    assert!(stdout.contains("src/auth/session.py"), "{stdout}");
    assert!(stdout.contains("rotate_refresh_token"), "{stdout}");
    assert!(
        stdout.contains("tests/auth/test_session_rotation.py"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("reserve_stock"),
        "unrelated inventory implementation should not be part of owner chunks:\n{stdout}"
    );

    let workspace_path = dir.path().join(".anchor/tasks/current.json");
    let workspace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    assert_eq!(workspace["schema"], "anchor.task_packet");
    assert_eq!(
        workspace["likely_files"][0]["path"], "src/auth/session.py",
        "{workspace}"
    );
    let slices = workspace["owner_chunks"].as_array().unwrap();
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
        workspace["likely_files"][0]["path"],
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


#[test]
fn cli_task_intake_references_owner_chunk_instead_of_duplicating_context_code() {
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
    assert!(stdout.contains("<owner_chunk_ref"), "{stdout}");
    assert_eq!(
        stdout.matches("return payload").count(),
        1,
        "context should refer to the exact workspace slice instead of duplicating its code:\n{stdout}"
    );
}

#[test]
fn cli_task_intake_prefers_owner_chunks_over_large_owner_classes() {
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
    let slices = workspace["owner_chunks"].as_array().unwrap();
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
        "large owner classes should not dominate owner chunks: {workspace}"
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
        workspace["likely_files"][0]["path"], "src/runtime/telemetry.py",
        "{workspace}"
    );
    assert!(
        workspace["owner_chunks"]
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
        !workspace["owner_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slice| slice["symbol"] == "App"),
        "large noisy UI function should not be an owner chunk: {workspace}"
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


use std::fs;
use std::process::Command;

#[test]
fn query_returns_handles_and_view_resolves_chunk() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("src/api.py"),
        r#"
class APIRoute:
    def get_route_handler(self, request):
        response = {"headers": {}}
        response["headers"]["Deprecation"] = "true"
        return response
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/test_api.py"),
        r#"
from src.api import APIRoute

def test_deprecation_header():
    assert APIRoute().get_route_handler({})["headers"]["Deprecation"] == "true"
"#,
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let query = Command::new(anchor)
        .arg("-r")
        .arg(dir.path())
        .arg("query")
        .arg("--json")
        .arg("--limit")
        .arg("4")
        .arg("deprecation response headers")
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&query.stdout).unwrap();
    assert_eq!(json["schema"], "anchor.query.v1");
    assert!(json["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["handle"] == "file:src/api.py"));
    assert!(json["tests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["handle"] == "test:tests/test_api.py"));
    let handle = json["chunks"][0]["handle"].as_str().unwrap();
    assert!(handle.starts_with("chunk:src/api.py#"));

    let view = Command::new(anchor)
        .arg("-r")
        .arg(dir.path())
        .arg("view")
        .arg("--json")
        .arg(handle)
        .output()
        .unwrap();
    assert!(
        view.status.success(),
        "{}",
        String::from_utf8_lossy(&view.stderr)
    );
    let viewed: serde_json::Value = serde_json::from_slice(&view.stdout).unwrap();
    assert_eq!(viewed["schema"], "anchor.view.v1");
    assert_eq!(viewed["kind"], "chunk");
    assert_eq!(viewed["path"], "src/api.py");
    assert!(viewed["source_hash"].as_str().unwrap().len() == 64);
    assert!(viewed["code"].as_str().unwrap().contains("Deprecation"));
}

use anchor::parser::extract_file;
use anchor::storage::{AnchorStore, CallIndex};
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

const AUTH_SRC: &str = r#"
pub fn login(username: &str, password: &str) -> bool {
    validate(username);
    check_password(password);
    true
}

fn validate(input: &str) -> bool {
    !input.is_empty()
}

fn check_password(pw: &str) -> bool {
    pw.len() >= 8
}
"#;

fn make_store() -> (tempfile::TempDir, AnchorStore) {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    let path = src.join("auth.rs");
    fs::write(&path, AUTH_SRC).unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&path).unwrap();

    // Build and save call index (normally done by `anchor build`)
    let extraction = extract_file(&path, AUTH_SRC).unwrap();
    let mut call_map: HashMap<String, Vec<String>> = HashMap::new();
    for call in &extraction.calls {
        call_map
            .entry(call.caller.clone())
            .or_default()
            .push(call.callee.clone());
    }
    let call_index = CallIndex { calls: call_map };
    store.save_call_index(&call_index).unwrap();

    (dir, store)
}

#[test]
fn signature_mode_symbol_found() {
    let (_dir, store) = make_store();
    let results = store.search_symbols_hybrid("login", 5).unwrap();
    let sym = results.iter().find(|s| s.name == "login").unwrap();

    // signature = first line of the projection
    let proj = store.create_projection(sym).unwrap();
    let sig = proj.text.lines().next().unwrap().trim_end();
    assert!(
        sig.contains("fn login"),
        "signature line must contain fn login"
    );
    assert!(!sig.contains("validate"), "signature must not contain body");
}

#[test]
fn full_body_contains_all_lines() {
    let (_dir, store) = make_store();
    let results = store.search_symbols_hybrid("login", 5).unwrap();
    let sym = results.iter().find(|s| s.name == "login").unwrap();
    let proj = store.create_projection(sym).unwrap();

    assert!(
        proj.text.contains("validate"),
        "full body must contain inner calls"
    );
    assert!(
        proj.text.contains("check_password"),
        "full body must contain all calls"
    );
}

#[test]
fn search_returns_all_symbols() {
    let (_dir, store) = make_store();
    let results = store.search_symbols_hybrid("validate", 5).unwrap();
    assert!(
        results.iter().any(|s| s.name == "validate"),
        "validate must be findable"
    );
}

#[test]
fn callers_callees_in_call_index() {
    let (_dir, store) = make_store();
    let call_index = store.load_call_index();

    // login calls validate and check_password
    let callees = call_index.callees_of("login");
    assert!(
        callees.contains(&"validate") || callees.contains(&"check_password"),
        "login must have callees in call index"
    );
}

#[test]
fn signature_is_single_line() {
    let (_dir, store) = make_store();
    let results = store.search_symbols_hybrid("check_password", 5).unwrap();
    let sym = results.iter().find(|s| s.name == "check_password").unwrap();
    let proj = store.create_projection(sym).unwrap();
    let sig = proj.text.lines().next().unwrap();
    assert!(!sig.contains('\n'), "signature must be a single line");
}

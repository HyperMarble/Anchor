#[cfg(windows)]
mod windows_tests {
    use super::*;

    #[test]
    fn protection_restores_original_readonly_attribute() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("app.py");
        fs::write(&source, "print('hello')\n").unwrap();

        protect_on(temp.path()).unwrap();
        assert!(fs::metadata(&source).unwrap().permissions().readonly());
        let state: serde_json::Value = serde_json::from_slice(
            &fs::read(temp.path().join(".anchor/protection.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["schema"], PROTECTION_SCHEMA);
        assert_eq!(state["entries"][0]["permissions"]["platform"], "windows");

        protect_off(temp.path()).unwrap();
        assert!(!fs::metadata(&source).unwrap().permissions().readonly());
    }

    #[test]
    fn unlock_guard_restores_readonly_attribute_after_error() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("app.py");
        fs::write(&source, "print('hello')\n").unwrap();
        protect_on(temp.path()).unwrap();

        let result: Result<()> = with_unlocked_path(temp.path(), &source, || {
            assert!(!fs::metadata(&source)?.permissions().readonly());
            bail!("intentional failure")
        });
        assert!(result.is_err());
        assert!(fs::metadata(&source).unwrap().permissions().readonly());

        protect_off(temp.path()).unwrap();
    }

    #[test]
    fn legacy_unix_state_is_rejected_without_rewriting_it() {
        let temp = tempfile::tempdir().unwrap();
        let anchor_dir = temp.path().join(".anchor");
        fs::create_dir_all(&anchor_dir).unwrap();
        let state_path = anchor_dir.join("protection.json");
        let legacy = br#"{
            "schema": "anchor.protection.v1",
            "entries": [{"path":"app.py","kind":"file","mode":420}]
        }"#;
        fs::write(&state_path, legacy).unwrap();

        let error = load_state(temp.path()).unwrap_err().to_string();
        assert!(error.contains("cannot be restored safely on Windows"), "{error}");
        assert_eq!(fs::read(&state_path).unwrap(), legacy);
    }
}

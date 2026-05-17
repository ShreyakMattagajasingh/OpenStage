use serde_json::Value;

#[test]
fn agent_capture_writes_latest_editor_mode_json() {
    qa::run_avatar_desktop(&["--agent-capture", "--deterministic"])
        .expect("avatar_desktop --agent-capture");
    let path = qa::workspace_root()
        .join("user_data")
        .join("debug_screenshots")
        .join("latest_editor_mode.json");
    assert!(
        path.exists(),
        "expected {} to exist after deterministic capture",
        path.display()
    );
    let body = std::fs::read_to_string(&path).expect("read editor mode json");
    let parsed: Value = serde_json::from_str(&body).expect("valid editor mode json");
    assert_eq!(
        parsed
            .get("current_mode")
            .and_then(Value::as_str)
            .expect("current_mode"),
        "character"
    );
    let modes = parsed
        .get("available_modes")
        .and_then(Value::as_array)
        .expect("available_modes");
    assert_eq!(modes.len(), 11);
    assert!(modes.iter().any(|mode| mode.as_str() == Some("asset")));
}

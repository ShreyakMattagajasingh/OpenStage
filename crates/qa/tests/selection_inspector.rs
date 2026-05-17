//! Stage 20 integration test: agent capture writes a selection JSON
//! alongside the scene-graph dumps, and the deterministic capture
//! contains the auto-selected `avatar_001`.
//!
//! Requires `cargo build --bin avatar_desktop` to have been run first.

use serde_json::Value;

#[test]
fn agent_capture_writes_latest_selection_json() {
    qa::run_avatar_desktop(&["--agent-capture", "--deterministic"])
        .expect("avatar_desktop --agent-capture");
    let selection_path = qa::workspace_root()
        .join("user_data")
        .join("debug_screenshots")
        .join("latest_selection.json");
    assert!(
        selection_path.exists(),
        "expected {} to exist after deterministic capture",
        selection_path.display()
    );
    let body = std::fs::read_to_string(&selection_path).expect("read selection json");
    let parsed: Value = serde_json::from_str(&body).expect("valid selection json");
    let active = parsed
        .get("active_object")
        .and_then(Value::as_str)
        .expect("active_object field");
    assert_eq!(active, "avatar_001");
    let selected = parsed
        .get("selected_objects")
        .and_then(Value::as_array)
        .expect("selected_objects array");
    assert!(
        selected.iter().any(|v| v.as_str() == Some("avatar_001")),
        "selected_objects should include avatar_001"
    );
}

//! Phase 16 integration test: `--agent-capture --deterministic` outputs
//! must SSIM-match the committed `tests/golden/*.png` baselines.
//!
//! Requires `cargo build --bin avatar_desktop` to have been run first
//! (CI does this as a separate step before `cargo test`).

#[test]
fn agent_full_body_capture_matches_golden() {
    qa::run_avatar_desktop(&["--agent-capture", "--deterministic"])
        .expect("avatar_desktop --agent-capture");
    let actual = qa::workspace_root().join("user_data/debug_screenshots/agent_full_body.png");
    qa::assert_matches_golden(&actual, "full_body.png");
}

#[test]
fn agent_portrait_capture_matches_golden() {
    qa::run_avatar_desktop(&["--agent-capture", "--deterministic"])
        .expect("avatar_desktop --agent-capture");
    let actual = qa::workspace_root().join("user_data/debug_screenshots/agent_portrait.png");
    qa::assert_matches_golden(&actual, "portrait.png");
}

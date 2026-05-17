//! Phase 16 integration test: `asset_builder import` round-trips
//! GLB → copied model + metadata JSON + DB row + thumbnail.
//!
//! Requires `cargo build --bin asset_builder` to have been run first.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn asset_builder_bin() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "asset_builder.exe"
    } else {
        "asset_builder"
    };
    let root = qa::workspace_root();
    for profile in &["debug", "release"] {
        let candidate = root.join("target").join(profile).join(exe_name);
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "asset_builder binary not found under target/. \
         Run `cargo build --bin asset_builder` first."
    );
}

fn tmp_workspace() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!(
        "qa_asset_builder_{}_{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn import_copies_glb_writes_metadata_upserts_db() {
    let src_glb = qa::workspace_root().join("assets/processed/avatars/bodies/phase4_rig.glb");
    assert!(src_glb.exists(), "phase4_rig.glb fixture missing");

    let tmp = tmp_workspace();
    let staging = tmp.join("input.glb");
    std::fs::copy(&src_glb, &staging).expect("stage input glb");

    let bin = asset_builder_bin();
    let status = Command::new(&bin)
        .args([
            "import",
            staging.to_str().unwrap(),
            "--id",
            "qa_body_imported_001",
            "--category",
            "body",
            "--display-name",
            "QA Imported Body",
            "--workspace",
        ])
        .arg(&tmp)
        .status()
        .expect("spawn asset_builder import");
    assert!(status.success(), "asset_builder import failed");

    let model = tmp.join("assets/processed/avatars/bodies/qa_body_imported_001.glb");
    let meta = tmp.join("assets/processed/metadata/qa_body_imported_001.json");
    let db = tmp.join("user_data/asset_catalog.sqlite");
    assert!(model.exists(), "model not copied to {}", model.display());
    assert!(meta.exists(), "metadata not written to {}", meta.display());
    assert!(db.exists(), "catalog DB not created at {}", db.display());

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta).unwrap()).unwrap();
    assert_eq!(json["id"], "qa_body_imported_001");
    assert_eq!(json["displayName"], "QA Imported Body");
    assert_eq!(json["category"], "body");
}

#[test]
fn list_includes_imported_asset() {
    let src_glb = qa::workspace_root().join("assets/processed/avatars/bodies/phase4_rig.glb");
    let tmp = tmp_workspace();
    let staging = tmp.join("input.glb");
    std::fs::copy(&src_glb, &staging).unwrap();
    let bin = asset_builder_bin();

    let status = Command::new(&bin)
        .args([
            "import",
            staging.to_str().unwrap(),
            "--id",
            "qa_body_list_001",
            "--category",
            "body",
            "--workspace",
        ])
        .arg(&tmp)
        .status()
        .unwrap();
    assert!(status.success());

    let out = Command::new(&bin)
        .args(["list", "--category", "body", "--workspace"])
        .arg(&tmp)
        .output()
        .expect("spawn list");
    assert!(out.status.success(), "list failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("qa_body_list_001"),
        "imported id missing from list output:\n{stdout}"
    );
}

#[test]
fn reimport_without_force_fails() {
    let src_glb = qa::workspace_root().join("assets/processed/avatars/bodies/phase4_rig.glb");
    let tmp = tmp_workspace();
    let staging = tmp.join("input.glb");
    std::fs::copy(&src_glb, &staging).unwrap();
    let bin = asset_builder_bin();

    // First import succeeds.
    let status = Command::new(&bin)
        .args([
            "import",
            staging.to_str().unwrap(),
            "--id",
            "qa_body_dup_001",
            "--category",
            "body",
            "--workspace",
        ])
        .arg(&tmp)
        .status()
        .unwrap();
    assert!(status.success());

    // Second import without --force must fail.
    let status = Command::new(&bin)
        .args([
            "import",
            staging.to_str().unwrap(),
            "--id",
            "qa_body_dup_001",
            "--category",
            "body",
            "--workspace",
        ])
        .arg(&tmp)
        .status()
        .unwrap();
    assert!(
        !status.success(),
        "duplicate id should have failed without --force"
    );
}

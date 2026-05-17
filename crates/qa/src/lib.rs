//! Shared helpers for the Phase 16 integration tests.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

/// Workspace root resolved from `CARGO_MANIFEST_DIR`.
/// `crates/qa` is two levels deep, so go up twice.
pub fn workspace_root() -> PathBuf {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set during cargo test");
    PathBuf::from(manifest)
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize workspace root")
}

/// Directory where committed golden PNGs live.
pub fn golden_dir() -> PathBuf {
    workspace_root().join("tests/golden")
}

/// Directory where committed perf baselines live.
pub fn baseline_dir() -> PathBuf {
    workspace_root().join("tests/baselines")
}

/// Path to the built `avatar_desktop` binary. Tries the test-target dir
/// (`CARGO_TARGET_TMPDIR`) first, then standard `target/debug` and
/// `target/release`.
pub fn avatar_desktop_bin() -> Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        "avatar_desktop.exe"
    } else {
        "avatar_desktop"
    };
    let root = workspace_root();
    for profile in &["debug", "release"] {
        let candidate = root.join("target").join(profile).join(exe_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!(
        "avatar_desktop binary not found under target/. \
         Run `cargo build --bin avatar_desktop` first."
    )
}

/// Spawn the `avatar_desktop` binary with the given args and wait for
/// exit. CWD is the workspace root so the binary sees `assets/` next to
/// it like a developer run does.
pub fn run_avatar_desktop(args: &[&str]) -> Result<()> {
    let root = workspace_root();
    let build_status = Command::new("cargo")
        .args(["build", "--bin", "avatar_desktop"])
        .current_dir(&root)
        .status()
        .context("build avatar_desktop binary")?;
    if !build_status.success() {
        bail!(
            "cargo build --bin avatar_desktop exited with status {:?}",
            build_status.code()
        );
    }
    let bin = avatar_desktop_bin()?;
    let status = Command::new(&bin)
        .args(args)
        .current_dir(&root)
        .status()
        .with_context(|| format!("spawn {}", bin.display()))?;
    if !status.success() {
        bail!(
            "{} {:?} exited with status {:?}",
            bin.display(),
            args,
            status.code()
        );
    }
    Ok(())
}

/// Assert that `actual` matches the committed golden `<name>` within
/// the Phase 16 SSIM threshold (>= 0.99) and max-diff fraction (<= 1%).
pub fn assert_matches_golden(actual_path: &Path, golden_name: &str) {
    let actual = image::open(actual_path)
        .unwrap_or_else(|e| panic!("open actual {}: {}", actual_path.display(), e))
        .to_rgba8();
    let golden_path = golden_dir().join(golden_name);
    let golden = image::open(&golden_path)
        .unwrap_or_else(|e| panic!("open golden {}: {}", golden_path.display(), e))
        .to_rgba8();
    let report = renderer::diff::compare_rgba(&actual, &golden, 8);
    let ssim_min = 0.99;
    let max_pct = 0.01;
    assert!(
        renderer::diff::passes(&report, ssim_min, max_pct),
        "golden drift on {}:\n  SSIM = {:.4} (min {:.2})\n  diff pixels = {} / {} ({:.2}%) > {:.2}%\n  max channel diff = {}",
        golden_name,
        report.ssim,
        ssim_min,
        report.diff_pixel_count,
        report.total_pixels,
        report.diff_fraction() * 100.0,
        max_pct * 100.0,
        report.max_channel_diff
    );
}

/// Load a PerfReport from JSON at `path`.
pub fn read_perf_report(path: &Path) -> Result<engine_core::PerfReport> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow!("parse perf report: {e}"))
}

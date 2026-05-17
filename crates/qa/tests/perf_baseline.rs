//! Phase 16 integration test: a fresh `--agent-perf --deterministic`
//! run must stay within tolerance of the committed
//! `tests/baselines/perf_baseline.json`.

use std::path::PathBuf;

use engine_core::PerfReport;

const FPS_REGRESSION_RATIO: f32 = 0.9; // tolerate down to 90% of baseline
const FRAME_MS_REGRESSION_RATIO: f32 = 1.2; // tolerate up to 120% of baseline

fn user_data_perf_json() -> PathBuf {
    qa::workspace_root()
        .join("user_data")
        .join("perf")
        .join("latest_perf_report.json")
}

#[test]
fn current_perf_does_not_regress_against_baseline() {
    let baseline_path = qa::baseline_dir().join("perf_baseline.json");
    let baseline: PerfReport = qa::read_perf_report(&baseline_path).expect("read baseline json");
    qa::run_avatar_desktop(&["--agent-perf", "--perf-frames", "300", "--deterministic"])
        .expect("avatar_desktop --agent-perf");

    let current_path = user_data_perf_json();
    let current: PerfReport = qa::read_perf_report(&current_path).expect("read current json");

    let fps_floor = baseline.average_fps * FPS_REGRESSION_RATIO;
    let p99_ceiling = baseline.p99_frame_ms * FRAME_MS_REGRESSION_RATIO;

    assert!(
        current.passed,
        "current run's PerfReport.passed = false (avg_fps {:.2}, p99 {:.2} ms)",
        current.average_fps, current.p99_frame_ms
    );
    assert!(
        current.average_fps >= fps_floor,
        "average_fps regressed: {:.2} < {:.2} (90% of baseline {:.2})",
        current.average_fps,
        fps_floor,
        baseline.average_fps
    );
    assert!(
        current.p99_frame_ms <= p99_ceiling,
        "p99_frame_ms regressed: {:.2} > {:.2} (120% of baseline {:.2})",
        current.p99_frame_ms,
        p99_ceiling,
        baseline.p99_frame_ms
    );
}

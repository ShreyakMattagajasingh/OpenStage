# Testing — Phase 16

This page documents the QA infrastructure added in Phase 16: the
integration-test layout, how to update goldens / perf baselines, and
how to run the full suite locally.

## Test layers

```
crates/qa/tests/
├── golden_capture.rs          (2) SSIM compare full_body + portrait
├── save_load_round_trip.rs    (3) AvatarSave through CharacterStore
├── asset_builder_pipeline.rs  (3) import + list + force-reimport
├── soak.rs                    (1) 500-frame headless render
└── perf_baseline.rs           (1) fps + p99 vs committed baseline

tests/golden/
├── full_body.png              committed v1.0 baseline
└── portrait.png               committed v1.0 baseline

tests/baselines/
└── perf_baseline.json         committed perf baseline
```

Per-crate unit tests still live in `#[cfg(test)] mod tests` blocks
inside each crate's `src/`. The new `crates/qa` package only exists
for cross-crate integration tests that need to shell out to a built
binary or compare against committed assets.

## Running locally

```powershell
# Build the binaries first - integration tests shell out to them.
cargo build --workspace --bins

# Full QA suite (matches CI):
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Just the integration suite:
cargo test -p qa

# A single test:
cargo test -p qa --test golden_capture
cargo test -p qa --test soak --release   # use release for soak
cargo test -p qa --test perf_baseline --release
```

The soak and perf tests are CPU-sensitive; run them with `--release`
when you want representative numbers. The default `cargo test
--workspace` invocation runs them in `dev` profile and the tolerances
(33 ms / 0.9× FPS) are wide enough that they still pass.

## Updating the golden images

The `--agent-capture --deterministic` flag pair freezes the
animation player at t=0 and uses stable filenames (no timestamp
suffix) so the captured PNGs are byte-identical across runs on the
same hardware.

When a real rendering change lands and the SSIM gate trips, capture
fresh goldens and commit them in the same PR:

```powershell
cargo run --bin avatar_desktop -- --agent-capture --deterministic
copy user_data/debug_screenshots/agent_full_body.png tests/golden/full_body.png
copy user_data/debug_screenshots/agent_portrait.png tests/golden/portrait.png
```

Then re-run `cargo test -p qa --test golden_capture` to confirm. A
golden update should always be a deliberate review item — never the
fix for a flaky CI.

### Cross-machine goldens

The committed PNGs come from the dev machine's GPU (Intel Iris Xe via
Vulkan). GitHub Actions runs on a different GPU stack (WARP /
DirectX software fallback on `windows-latest`), so the very first CI
run may exceed the SSIM tolerance. The plan was to absorb that with
the 0.99 SSIM threshold + 1 % pixel-diff allowance, but if a CI run
fails on the dev-machine goldens, the fix is to capture goldens on a
clean CI worker once and commit those instead.

## Updating the perf baseline

```powershell
cargo run --release --bin avatar_desktop -- --agent-perf --perf-frames 300 --deterministic
copy user_data/perf/latest_perf_report.json tests/baselines/perf_baseline.json
```

The regression test allows:
- `averageFps ≥ baseline × 0.9`
- `p99FrameMs ≤ baseline × 1.2`

This is intentionally generous so a busy runner doesn't false-alarm,
while still catching a genuine 10 %+ regression.

## Determinism details

`--deterministic` flips three switches:

1. `FrameClock::pin_dt(1/60s)` — the frame clock returns a fixed
   `dt` regardless of wall time, so animation samples are stable.
2. After body install, the animation player is paused and seeked to
   `t = 0`, so the captured pose is the bind pose / first keyframe.
3. `run_agent_capture()` uses stable filenames
   (`agent_full_body.png`, `agent_portrait.png`,
   `latest_agent_capture.json`) without timestamp suffixes.

What `--deterministic` doesn't control: wgpu tessellation, depth
rounding, GPU-driver-specific texture filtering. SSIM ≥ 0.99 is
chosen to absorb that variation.

## CI

`.github/workflows/ci.yml` runs the full suite on every push and PR
on `windows-latest`:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo build --workspace --bins` (integration tests need
   `target/debug/*.exe`)
4. `cargo test --workspace`
5. Upload `user_data/debug_screenshots/`, `user_data/perf/`,
   `tests/golden/`, `tests/baselines/` as artifacts so a human can
   eyeball drift even on green runs.

No matrix, no cross-platform — Windows-first per spec.

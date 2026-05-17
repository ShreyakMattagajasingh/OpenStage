//! Phase 16 short soak test: render 500 headless frames of the Phase 4
//! rig + Phase 7 top, assert no panics and that average CPU frame time
//! stays under a generous 33 ms (30 FPS floor — wide enough to survive
//! a busy CI runner).

use std::path::Path;
use std::time::Instant;

use animation::SkinningPalette;
use engine_core::{FrameSample, FrameStats};
use glam::Mat4;
use renderer::{
    gltf_loader::load_glb, scene::SceneRenderer, Light, OrbitCamera, Renderer, SceneInstance,
};

const SOAK_FRAMES: usize = 500;
const WARMUP_FRAMES: usize = 50;
const TARGET_AVG_MS: f32 = 33.0; // 30 FPS floor

fn process_rss_bytes() -> Option<u64> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let pid = Pid::from(std::process::id() as usize);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]));
    sys.process(pid).map(|p| p.memory())
}

#[test]
fn five_hundred_frame_headless_soak() {
    let mut renderer = match Renderer::new_headless([0.118, 0.118, 0.180, 1.0]) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping soak: no wgpu adapter ({e})");
            return;
        }
    };
    let format = renderer.gpu.config.format;
    let scene = SceneRenderer::new(&renderer.gpu.device, format);
    let glb = qa::workspace_root().join("assets/processed/avatars/bodies/phase4_rig.glb");
    let loaded = load_glb(&renderer.gpu.device, Path::new(&glb)).expect("load phase4_rig.glb");
    let skeleton = loaded.skeleton.as_ref().expect("phase4_rig has skeleton");
    let palette = SkinningPalette::bind_pose(skeleton).expect("bind pose");
    let material = scene.make_material(
        &renderer.gpu.device,
        &renderer.gpu.queue,
        loaded.base_color_image.as_ref(),
        [1.0, 1.0, 1.0, 1.0],
        "soak",
    );

    let mut camera = OrbitCamera::new(1.0);
    camera.preset_full_body();
    let light = Light::default();
    let mut stats = FrameStats::new(SOAK_FRAMES);

    let rss_at_warmup = run_n_frames(
        &mut renderer,
        &scene,
        &loaded,
        &material,
        &palette,
        &camera,
        &light,
        &mut stats,
        WARMUP_FRAMES,
    );

    let rss_after_soak = run_n_frames(
        &mut renderer,
        &scene,
        &loaded,
        &material,
        &palette,
        &camera,
        &light,
        &mut stats,
        SOAK_FRAMES - WARMUP_FRAMES,
    );

    let avg_ms = stats.average_frame_ms();
    let max_ms = stats.max_frame_ms();
    println!(
        "soak: {} frames | avg {:.2} ms | max {:.2} ms | rss warmup {:?} -> {:?}",
        SOAK_FRAMES, avg_ms, max_ms, rss_at_warmup, rss_after_soak
    );

    assert!(
        avg_ms < TARGET_AVG_MS,
        "soak avg frame time {avg_ms:.2} ms exceeds 30 FPS floor {TARGET_AVG_MS:.2} ms"
    );

    if let (Some(start), Some(end)) = (rss_at_warmup, rss_after_soak) {
        // Allow up to 1.5x growth. wgpu lazily allocates so the warmup
        // figure is the steady-state baseline.
        assert!(
            end <= start + start / 2,
            "process RSS grew {start} -> {end} bytes (>1.5x); possible leak"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_n_frames(
    renderer: &mut Renderer,
    scene: &SceneRenderer,
    loaded: &renderer::gltf_loader::LoadedGlb,
    material: &renderer::Material,
    palette: &SkinningPalette,
    camera: &OrbitCamera,
    light: &Light,
    stats: &mut FrameStats,
    n: usize,
) -> Option<u64> {
    for _ in 0..n {
        let t0 = Instant::now();
        let _ = renderer
            .capture_rgba(
                64,
                64,
                wgpu::Color {
                    r: 0.118,
                    g: 0.118,
                    b: 0.180,
                    a: 1.0,
                },
                |fc| {
                    let instance = SceneInstance {
                        mesh: &loaded.mesh,
                        model: Mat4::IDENTITY,
                        material,
                        skinning_palette: Some(palette),
                    };
                    scene.draw(fc, [0, 0, 64, 64], camera, light, &[instance]);
                },
            )
            .expect("capture frame");
        stats.push(FrameSample {
            total_ms: t0.elapsed().as_secs_f32() * 1000.0,
            ..Default::default()
        });
    }
    process_rss_bytes()
}

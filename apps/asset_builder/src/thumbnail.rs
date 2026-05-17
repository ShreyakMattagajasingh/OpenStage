//! Render a thumbnail PNG of a GLB by spinning up a headless renderer.

use std::path::Path;

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use renderer::{gltf_loader::load_glb, scene::SceneRenderer, Light, OrbitCamera, Renderer};
use tracing::info;

/// Render `glb_path` to a square thumbnail at `width`x`height` pixels and
/// write it as a PNG at `out_png`.
pub fn render_thumbnail(glb_path: &Path, width: u32, height: u32, out_png: &Path) -> Result<()> {
    let width = width.max(16);
    let height = height.max(16);

    // 1. Headless renderer + device.
    let mut renderer =
        Renderer::new_headless([0.15, 0.16, 0.20, 1.0]).context("Renderer::new_headless")?;

    // 2. Load the GLB into a GPU mesh.
    let loaded = load_glb(&renderer.gpu.device, glb_path)
        .with_context(|| format!("load_glb {}", glb_path.display()))?;

    // 3. Build the scene pipeline against the headless target format.
    let format = renderer.gpu.config.format;
    let scene = SceneRenderer::new(&renderer.gpu.device, format);
    let material = scene.make_material(
        &renderer.gpu.device,
        &renderer.gpu.queue,
        loaded.base_color_image.as_ref(),
        [1.0, 1.0, 1.0, 1.0],
        "thumbnail",
    );

    // 4. Skinning palette (bind pose) if mesh is skinned.
    let palette = if loaded.is_skinned {
        match &loaded.skeleton {
            Some(skel) => Some(
                animation::SkinningPalette::bind_pose(skel)
                    .context("SkinningPalette::bind_pose")?,
            ),
            None => None,
        }
    } else {
        None
    };

    // 5. Camera framed on the mesh AABB.
    let aspect = width as f32 / height as f32;
    let mut camera = OrbitCamera::new(aspect);
    let bbox = loaded.mesh.bbox;
    let center = bbox.center();
    let longest = bbox.longest_axis().max(0.1);
    camera.target = Vec3::new(0.0, center.y, 0.0);
    camera.yaw = 0.0;
    camera.pitch = (-10.0_f32).to_radians();
    // Fit longest extent into ~80% of frame height.
    let half_fov = camera.fov_y_rad * 0.5;
    let dist_fit = longest / (2.0 * half_fov.tan());
    camera.distance = (dist_fit * 1.5 + longest * 0.3).clamp(0.6, 9.5);

    let light = Light::default();

    // 6. Encode the offscreen frame and read back RGBA pixels.
    let clear = wgpu::Color {
        r: 0.15,
        g: 0.16,
        b: 0.20,
        a: 1.0,
    };
    let shot = renderer
        .capture_rgba(width, height, clear, |fc| {
            let instance = renderer::SceneInstance {
                mesh: &loaded.mesh,
                model: Mat4::IDENTITY,
                material: &material,
                skinning_palette: palette.as_ref(),
            };
            scene.draw(fc, [0, 0, width, height], &camera, &light, &[instance]);
        })
        .context("capture_rgba")?;

    // 7. Save as PNG. (Reusing image::save_buffer rather than export crate
    // because that crate restricts widths to 512/1024/2048 for user exports.)
    if let Some(parent) = out_png.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    image::save_buffer(
        out_png,
        &shot.pixels,
        shot.width,
        shot.height,
        image::ColorType::Rgba8,
    )
    .with_context(|| format!("save thumbnail {}", out_png.display()))?;

    info!(
        glb = %glb_path.display(),
        png = %out_png.display(),
        width,
        height,
        skinned = loaded.is_skinned,
        "thumbnail written"
    );
    Ok(())
}

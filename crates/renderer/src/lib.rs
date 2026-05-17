//! Renderer crate.
//!
//! Phase 2 surface:
//!   `Renderer::new` → `Renderer::resize` → `Renderer::render(|fc| { ... })`.
//! Inside the callback the caller composes the frame:
//!   `scene.draw(fc, &camera, &light, &[(&mesh, model, material)])`
//!   then egui's paint pass.
//! Clear pass (color + depth) is handled by `Renderer` before the callback.

pub mod renderer;

pub mod camera;
pub mod color;
pub mod debug_lines;
pub mod diff;
pub mod light;
pub mod material;
pub mod mesh;
pub mod scene;
pub mod shader;

// Future-plug-in modules — kept as stubs so module paths are stable.
pub mod gltf_loader;
pub mod screenshot;
pub mod texture;
pub mod timer;

pub use camera::OrbitCamera;
pub use debug_lines::DebugLineRenderer;
pub use gltf_loader::{load_glb, LoadedGlb};
pub use light::Light;
pub use material::Material;
pub use mesh::{Aabb, Mesh, Vertex};
pub use renderer::{FrameCtx, Renderer, DEPTH_FORMAT};
pub use scene::{SceneInstance, SceneRenderer};
pub use screenshot::RgbaScreenshot;
pub use texture::Texture;

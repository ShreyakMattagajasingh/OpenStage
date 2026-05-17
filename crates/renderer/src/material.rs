//! Per-mesh material. Owns the base-color texture + bind group.

use crate::texture::Texture;

pub struct Material {
    pub base_color: [f32; 4],
    pub texture: Texture,
    pub bind_group: wgpu::BindGroup,
}

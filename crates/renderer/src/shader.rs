//! WGSL shader loaders.

pub fn load_mesh(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"))
}

pub fn load_debug_lines(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::include_wgsl!("shaders/debug_lines.wgsl"))
}

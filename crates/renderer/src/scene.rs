//! Mesh pipeline + scene draw call.
//!
//! Four bind groups:
//!   group 0: per-frame uniforms (camera + light)
//!   group 1: per-draw instance uniforms (model + base color tint)
//!   group 2: per-material (base color texture + sampler)
//!   group 3: per-draw skinning palette
//!
//! The instance uniform buffer is **rewritten between draws** — fine for the
//! tiny instance counts in Phase 3. Phase 7 (many slots) moves to dynamic offsets.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use image::DynamicImage;

use animation::{SkinningPalette, MAX_BONES};

/// Max number of slots/instances drawn per frame. 16 leaves room for the 9
/// Phase-7 slots and a handful of debug overlays later.
const MAX_SCENE_INSTANCES: u64 = 16;
/// Per-instance stride in the dynamic-offset uniform buffer. Must be ≥
/// `min_uniform_buffer_offset_alignment` (256 on every desktop GPU we care
/// about) and ≥ `size_of::<InstanceUniforms>` (96 B as of Phase 7).
const INSTANCE_UBO_STRIDE: u64 = 256;

use crate::{
    camera::OrbitCamera,
    light::Light,
    material::Material,
    mesh::{Mesh, Vertex},
    renderer::{FrameCtx, DEPTH_FORMAT},
    shader,
    texture::Texture,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct InstanceUniforms {
    model: [[f32; 4]; 4],
    base_color: [f32; 4],
    skinning: [f32; 4],
}

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    instance_buf: wgpu::Buffer,
    instance_bg: wgpu::BindGroup,
    material_bgl: wgpu::BindGroupLayout,
    skin_buf: wgpu::Buffer,
    skin_bg: wgpu::BindGroup,
}

#[derive(Clone, Copy)]
pub struct SceneInstance<'a> {
    pub mesh: &'a Mesh,
    pub model: Mat4,
    pub material: &'a Material,
    pub skinning_palette: Option<&'a SkinningPalette>,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.frame.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<FrameUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });
        let instance_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.instance.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    // Dynamic offset so multiple slots can share one buffer
                    // without their writes clobbering each other before any
                    // pass runs. (queue.write_buffer is queue-level — every
                    // pass sees the *last* write into a non-offset binding.)
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<InstanceUniforms>() as u64,
                    ),
                },
                count: None,
            }],
        });
        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.material.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let skin_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.skin.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_BONES) as u64,
                    ),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene.pipeline.layout"),
            bind_group_layouts: &[&frame_bgl, &instance_bgl, &material_bgl, &skin_bgl],
            push_constant_ranges: &[],
        });

        let shader_module = shader::load_mesh(device);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene.frame.ubo"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene.frame.bg"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buf.as_entire_binding(),
            }],
        });

        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene.instance.ubo"),
            size: MAX_SCENE_INSTANCES * INSTANCE_UBO_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene.instance.bg"),
            layout: &instance_bgl,
            // Bind a window of exactly one InstanceUniforms; dynamic offsets
            // slide that window over the buffer per draw.
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &instance_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<InstanceUniforms>() as u64),
                }),
            }],
        });
        let skin_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene.skin.ubo"),
            size: (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_BONES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let skin_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene.skin.bg"),
            layout: &skin_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: skin_buf.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            frame_buf,
            frame_bg,
            instance_buf,
            instance_bg,
            material_bgl,
            skin_buf,
            skin_bg,
        }
    }

    /// Build a `Material`. If `image` is None, uses a 1×1 white texture.
    pub fn make_material(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: Option<&DynamicImage>,
        tint: [f32; 4],
        label: &str,
    ) -> Material {
        let texture = match image {
            Some(img) => Texture::from_dynamic_image(device, queue, img, label),
            None => Texture::white_1x1(device, queue),
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label}.material.bg")),
            layout: &self.material_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });
        Material {
            base_color: tint,
            texture,
            bind_group,
        }
    }

    /// Swap a `Material`'s texture in place. Used by Phase 9 to flip the
    /// face quad's expression without rebuilding the whole material chain.
    pub fn rebuild_material_with_texture(
        &self,
        device: &wgpu::Device,
        material: &mut Material,
        texture: Texture,
        label: &str,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label}.material.bg")),
            layout: &self.material_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });
        material.texture = texture;
        material.bind_group = bind_group;
    }

    /// Draw `instances` (mesh, model matrix, material) using `camera` and `light`.
    /// `viewport_px = [x, y, w, h]` in framebuffer pixels — the scene is drawn
    /// only within this rectangle so the caller can carve out the side panel.
    /// Must be called inside the `Renderer::render` callback.
    pub fn draw<'a>(
        &'a self,
        fc: &mut FrameCtx<'_>,
        viewport_px: [u32; 4],
        camera: &OrbitCamera,
        light: &Light,
        instances: &[SceneInstance<'a>],
    ) {
        let light_dir = light.direction.normalize_or_zero();
        let light_color = light.color * light.intensity;
        let frame_u = FrameUniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
            camera_pos: Vec4::from((camera.eye(), 1.0)).to_array(),
            light_dir: Vec4::from((light_dir, 0.0)).to_array(),
            light_color: Vec4::from((light_color, 1.0)).to_array(),
            ambient: Vec4::from((light.ambient, 1.0)).to_array(),
        };
        fc.queue
            .write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&frame_u));

        // --- 1. Write all per-instance uniforms upfront ---------------------
        // queue.write_buffer flushes BEFORE encoder commands run, so each
        // instance gets its own offset slice. Skin palette is shared across
        // every slot in a frame (single skeleton) — write once.
        let max = MAX_SCENE_INSTANCES as usize;
        if instances.len() > max {
            tracing::warn!(
                count = instances.len(),
                max,
                "scene received more instances than MAX_SCENE_INSTANCES; tail dropped"
            );
        }
        let used_count = instances.len().min(max);
        for (i, instance) in instances.iter().enumerate().take(used_count) {
            let skinning_enabled = instance.skinning_palette.is_some() && instance.mesh.is_skinned;
            let inst_u = InstanceUniforms {
                model: instance.model.to_cols_array_2d(),
                base_color: instance.material.base_color,
                skinning: [if skinning_enabled { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            };
            fc.queue.write_buffer(
                &self.instance_buf,
                (i as u64) * INSTANCE_UBO_STRIDE,
                bytemuck::bytes_of(&inst_u),
            );
        }
        // Pick a palette to use for the whole frame — the first instance that
        // claims one wins. Wearables and bodies in avatar mode share the same
        // body skeleton, so a single write is correct.
        if let Some(skin_matrices) = instances
            .iter()
            .take(used_count)
            .find_map(|inst| inst.skinning_palette.map(|p| p.padded_cols_array_2d()))
        {
            fc.queue
                .write_buffer(&self.skin_buf, 0, bytemuck::cast_slice(&skin_matrices));
        }

        // --- 2. Encode one render pass per draw, varying the dynamic offset.
        for (i, instance) in instances.iter().enumerate().take(used_count) {
            let offset = ((i as u64) * INSTANCE_UBO_STRIDE) as u32;

            let mut pass = fc.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: fc.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: fc.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_viewport(
                viewport_px[0] as f32,
                viewport_px[1] as f32,
                viewport_px[2].max(1) as f32,
                viewport_px[3].max(1) as f32,
                0.0,
                1.0,
            );
            pass.set_bind_group(0, &self.frame_bg, &[]);
            pass.set_bind_group(1, &self.instance_bg, &[offset]);
            pass.set_bind_group(2, &instance.material.bind_group, &[]);
            pass.set_bind_group(3, &self.skin_bg, &[]);
            pass.set_vertex_buffer(0, instance.mesh.vbuf.slice(..));
            pass.set_index_buffer(instance.mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..instance.mesh.index_count, 0, 0..1);
        }
    }
}

//! Debug line rendering for bind-pose skeleton visualization.

use animation::Skeleton;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::{
    camera::OrbitCamera,
    renderer::{FrameCtx, DEPTH_FORMAT},
    shader,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct LineVertex {
    pos: [f32; 3],
    color: [f32; 4],
}

impl LineVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x4,
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
}

pub struct DebugLineRenderer {
    pipeline: wgpu::RenderPipeline,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
}

impl DebugLineRenderer {
    pub const SKELETON_COLOR: [f32; 4] = [0.15, 0.9, 1.0, 1.0];
    pub const AXIS_X_COLOR: [f32; 4] = [0.95, 0.3, 0.3, 1.0];
    pub const AXIS_Y_COLOR: [f32; 4] = [0.3, 0.95, 0.4, 1.0];
    pub const AXIS_Z_COLOR: [f32; 4] = [0.3, 0.55, 1.0, 1.0];

    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("debug-lines.frame.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("debug-lines.pipeline.layout"),
            bind_group_layouts: &[&frame_bgl],
            push_constant_ranges: &[],
        });

        let shader = shader::load_debug_lines(device);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("debug-lines.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[LineVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                // Debug bones should be inspectable even when they run through
                // the inside of an opaque bind-pose mesh.
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug-lines.frame.ubo"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("debug-lines.frame.bg"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buf.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            frame_buf,
            frame_bg,
        }
    }

    pub fn draw_skeleton(
        &self,
        fc: &mut FrameCtx<'_>,
        viewport_px: [u32; 4],
        camera: &OrbitCamera,
        skeleton: &Skeleton,
        world_transforms: &[Mat4],
        model: Mat4,
    ) {
        let vertices: Vec<LineVertex> = skeleton
            .bones
            .iter()
            .enumerate()
            .filter_map(|(child_idx, child)| {
                child.parent.map(|parent_idx| (parent_idx.0, child_idx))
            })
            .flat_map(|(parent_idx, child_idx)| {
                let parent_world = world_transforms
                    .get(parent_idx)
                    .copied()
                    .unwrap_or(skeleton.bones[parent_idx].world_bind_transform);
                let child_world = world_transforms
                    .get(child_idx)
                    .copied()
                    .unwrap_or(skeleton.bones[child_idx].world_bind_transform);
                let a = model.transform_point3(parent_world.transform_point3(glam::Vec3::ZERO));
                let b = model.transform_point3(child_world.transform_point3(glam::Vec3::ZERO));
                [
                    LineVertex {
                        pos: a.to_array(),
                        color: Self::SKELETON_COLOR,
                    },
                    LineVertex {
                        pos: b.to_array(),
                        color: Self::SKELETON_COLOR,
                    },
                ]
            })
            .collect();

        if vertices.is_empty() {
            return;
        }

        let frame_u = FrameUniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
        };
        fc.queue
            .write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&frame_u));

        let vbuf = fc
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("debug-lines.skeleton.vbuf"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut pass = fc.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("debug-lines"),
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
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    pub fn draw_axes(
        &self,
        fc: &mut FrameCtx<'_>,
        viewport_px: [u32; 4],
        camera: &OrbitCamera,
        world: Mat4,
        length_m: f32,
    ) {
        let origin = world.transform_point3(glam::Vec3::ZERO);
        let x = world.transform_vector3(glam::Vec3::X).normalize_or_zero() * length_m;
        let y = world.transform_vector3(glam::Vec3::Y).normalize_or_zero() * length_m;
        let z = world.transform_vector3(glam::Vec3::Z).normalize_or_zero() * length_m;
        let vertices = [
            LineVertex {
                pos: origin.to_array(),
                color: Self::AXIS_X_COLOR,
            },
            LineVertex {
                pos: (origin + x).to_array(),
                color: Self::AXIS_X_COLOR,
            },
            LineVertex {
                pos: origin.to_array(),
                color: Self::AXIS_Y_COLOR,
            },
            LineVertex {
                pos: (origin + y).to_array(),
                color: Self::AXIS_Y_COLOR,
            },
            LineVertex {
                pos: origin.to_array(),
                color: Self::AXIS_Z_COLOR,
            },
            LineVertex {
                pos: (origin + z).to_array(),
                color: Self::AXIS_Z_COLOR,
            },
        ];

        let frame_u = FrameUniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
        };
        fc.queue
            .write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&frame_u));

        let vbuf = fc
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("debug-lines.axes.vbuf"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut pass = fc.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("debug-lines.axes"),
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
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
}

//! Mesh data and GPU buffers.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

/// Axis-aligned bounding box in mesh-local space (before any model matrix).
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_vertices(verts: &[Vertex]) -> Self {
        // Degenerate fallback for an empty mesh — collapses to origin.
        if verts.is_empty() {
            return Self {
                min: Vec3::ZERO,
                max: Vec3::ZERO,
            };
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in verts {
            let p = Vec3::from(v.pos);
            min = min.min(p);
            max = max.max(p);
        }
        Self { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
    pub fn longest_axis(&self) -> f32 {
        let s = self.size();
        s.x.max(s.y).max(s.z)
    }
}

impl Vertex {
    pub const fn new(pos: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self::skinned(pos, normal, uv, [0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0])
    }

    pub const fn skinned(
        pos: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        joints: [u32; 4],
        weights: [f32; 4],
    ) -> Self {
        Self {
            pos,
            normal,
            uv,
            joints,
            weights,
        }
    }

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x3, // pos
            1 => Float32x3, // normal
            2 => Float32x2, // uv
            3 => Uint32x4,  // joints
            4 => Float32x4, // weights
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

pub struct Mesh {
    pub vbuf: wgpu::Buffer,
    pub ibuf: wgpu::Buffer,
    pub index_count: u32,
    pub is_skinned: bool,
    /// Local-space bounding box. Used by callers to fit/center the mesh into
    /// the scene without baking a normalization transform into the geometry.
    pub bbox: Aabb,
}

impl Mesh {
    pub fn from_data(
        device: &wgpu::Device,
        label: &str,
        vertices: &[Vertex],
        indices: &[u32],
        is_skinned: bool,
    ) -> Self {
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label}.vbuf")),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label}.ibuf")),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let bbox = Aabb::from_vertices(vertices);
        Self {
            vbuf,
            ibuf,
            index_count: indices.len() as u32,
            is_skinned,
            bbox,
        }
    }

    /// Axis-aligned cube of edge `size`, centered at origin. Per-face normals
    /// (24 vertices total — corners are NOT shared across faces, so each face
    /// gets a flat normal).
    pub fn cube(device: &wgpu::Device, size: f32) -> Self {
        let h = size * 0.5;

        // Per-face vertex order: bottom-left, bottom-right, top-right, top-left,
        // CCW when viewed from outside (front face for `FrontFace::Ccw`).
        // UVs map each face to [0..1]^2 — same order: BL(0,1), BR(1,1), TR(1,0), TL(0,0).
        let v = [
            // +X face
            Vertex::new([h, -h, h], [1.0, 0.0, 0.0], [0.0, 1.0]),
            Vertex::new([h, -h, -h], [1.0, 0.0, 0.0], [1.0, 1.0]),
            Vertex::new([h, h, -h], [1.0, 0.0, 0.0], [1.0, 0.0]),
            Vertex::new([h, h, h], [1.0, 0.0, 0.0], [0.0, 0.0]),
            // -X face
            Vertex::new([-h, -h, -h], [-1.0, 0.0, 0.0], [0.0, 1.0]),
            Vertex::new([-h, -h, h], [-1.0, 0.0, 0.0], [1.0, 1.0]),
            Vertex::new([-h, h, h], [-1.0, 0.0, 0.0], [1.0, 0.0]),
            Vertex::new([-h, h, -h], [-1.0, 0.0, 0.0], [0.0, 0.0]),
            // +Y face (top)
            Vertex::new([-h, h, h], [0.0, 1.0, 0.0], [0.0, 1.0]),
            Vertex::new([h, h, h], [0.0, 1.0, 0.0], [1.0, 1.0]),
            Vertex::new([h, h, -h], [0.0, 1.0, 0.0], [1.0, 0.0]),
            Vertex::new([-h, h, -h], [0.0, 1.0, 0.0], [0.0, 0.0]),
            // -Y face (bottom)
            Vertex::new([-h, -h, -h], [0.0, -1.0, 0.0], [0.0, 1.0]),
            Vertex::new([h, -h, -h], [0.0, -1.0, 0.0], [1.0, 1.0]),
            Vertex::new([h, -h, h], [0.0, -1.0, 0.0], [1.0, 0.0]),
            Vertex::new([-h, -h, h], [0.0, -1.0, 0.0], [0.0, 0.0]),
            // +Z face (front)
            Vertex::new([-h, -h, h], [0.0, 0.0, 1.0], [0.0, 1.0]),
            Vertex::new([h, -h, h], [0.0, 0.0, 1.0], [1.0, 1.0]),
            Vertex::new([h, h, h], [0.0, 0.0, 1.0], [1.0, 0.0]),
            Vertex::new([-h, h, h], [0.0, 0.0, 1.0], [0.0, 0.0]),
            // -Z face (back)
            Vertex::new([h, -h, -h], [0.0, 0.0, -1.0], [0.0, 1.0]),
            Vertex::new([-h, -h, -h], [0.0, 0.0, -1.0], [1.0, 1.0]),
            Vertex::new([-h, h, -h], [0.0, 0.0, -1.0], [1.0, 0.0]),
            Vertex::new([h, h, -h], [0.0, 0.0, -1.0], [0.0, 0.0]),
        ];

        // Two triangles per face: (0,1,2) and (0,2,3) — CCW from outside.
        let mut indices: Vec<u32> = Vec::with_capacity(36);
        for face in 0..6u32 {
            let b = face * 4;
            indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }

        Self::from_data(device, "cube", &v, &indices, false)
    }
}

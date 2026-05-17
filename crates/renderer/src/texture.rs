//! GPU textures.
//!
//! Phase 3c MVP: RGBA8 upload from `image::DynamicImage`, plus a 1×1 white
//! fallback for meshes/materials without a baseColor texture. No mip
//! generation yet — added when Phase 14 needs filtering quality.

use image::DynamicImage;

pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    /// Upload `img` as `Rgba8UnormSrgb` so the GPU does sRGB→linear on sample.
    /// Matches the linear-lit shading we do in the fragment shader.
    pub fn from_dynamic_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &DynamicImage,
        label: &str,
    ) -> Self {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        upload_rgba8_srgb(device, queue, &rgba, w, h, label)
    }

    /// 1×1 white pixel. Used as the default base color texture so the
    /// fragment shader always has something to sample.
    pub fn white_1x1(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let pixel: [u8; 4] = [255, 255, 255, 255];
        upload_rgba8_srgb(device, queue, &pixel, 1, 1, "white-1x1")
    }
}

fn upload_rgba8_srgb(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: &[u8],
    width: u32,
    height: u32,
    label: &str,
) -> Texture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(&format!("{label}.sampler")),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    Texture {
        texture,
        view,
        sampler,
    }
}

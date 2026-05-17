//! wgpu device + surface bring-up and the per-frame render entry point.
//!
//! Phase 2 scope: initialize wgpu, configure a swapchain on the winit window,
//! own the depth texture, and run the clear pass each frame. The caller (the
//! desktop app) receives a [`FrameCtx`] inside the render callback so it can
//! encode additional passes — the scene pass and egui draw into this. Both
//! get the color and depth views via the context.

use std::sync::{mpsc, Arc};

use tracing::{debug, info, warn};
use winit::{dpi::PhysicalSize, window::Window};

use crate::screenshot::RgbaScreenshot;
use crate::timer::GpuTimer;

/// Depth format used by every pass that touches depth.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Everything the caller might need to encode additional draw work for a frame.
pub struct FrameCtx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// Current swapchain texture's view. Color was cleared by the renderer;
    /// later passes should use `LoadOp::Load` on this attachment.
    pub view: &'a wgpu::TextureView,
    /// Depth view (already cleared to 1.0). Later passes should use
    /// `LoadOp::Load` on it.
    pub depth_view: &'a wgpu::TextureView,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub size: [u32; 2],
    pub surface_format: wgpu::TextureFormat,
}

pub struct GpuContext {
    pub instance: wgpu::Instance,
    /// Present iff the context owns a swapchain (on-screen renderer).
    /// `None` in headless mode — `capture_rgba` still works.
    pub surface: Option<wgpu::Surface<'static>>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    // Keep the window alive for as long as the surface borrows it.
    _window: Option<Arc<Window>>,
}

impl GpuContext {
    pub fn new(window: Arc<Window>, vsync: bool) -> anyhow::Result<Self> {
        let size = window.inner_size();
        // A zero-sized window will make configure() fail later; clamp at config time.
        let initial_w = size.width.max(1);
        let initial_h = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY, // VK, DX12, Metal — skip GL on Windows.
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::default(),
        });

        // SAFETY: window is held by Arc and stored alongside the surface; the
        // 'static surface only outlives `self`, which owns the Arc.
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| anyhow::anyhow!("wgpu create_surface: {e}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .ok_or_else(|| anyhow::anyhow!("no compatible wgpu adapter"))?;

        let adapter_info = adapter.get_info();
        info!(
            backend = ?adapter_info.backend,
            name = %adapter_info.name,
            device_type = ?adapter_info.device_type,
            "selected GPU adapter"
        );

        // Opt in to timestamp queries when the adapter supports both bits.
        // Missing support is fine — `GpuTimer::try_new` will fall back to None.
        let timer_features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let requested_features = adapter.features() & timer_features;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("avatar-studio.device"),
                required_features: requested_features,
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))?;

        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or_else(|| {
                warn!(
                    "no sRGB surface format offered; falling back to {:?}",
                    caps.formats[0]
                );
                caps.formats[0]
            });

        let present_mode = if vsync {
            // Fifo is the vsync mode and guaranteed by the spec.
            wgpu::PresentMode::Fifo
        } else if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            // Mailbox: tear-free, queue-discarding, no frame-rate cap.
            wgpu::PresentMode::Mailbox
        } else if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            // Immediate: may tear but uncapped — fine for benchmarks.
            wgpu::PresentMode::Immediate
        } else {
            warn!("no uncapped present mode supported; falling back to Fifo");
            wgpu::PresentMode::Fifo
        };
        info!(present_mode = ?present_mode, vsync, "selected present mode");

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: initial_w,
            height: initial_h,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        debug!(width = config.width, height = config.height, format = ?config.format, "surface configured");

        Ok(Self {
            instance,
            surface: Some(surface),
            adapter,
            device,
            queue,
            config,
            _window: Some(window),
        })
    }

    /// Bring up a wgpu device with no Surface. Used by the `asset_builder`
    /// CLI for thumbnail rendering. `capture_rgba` works; `render()` does not.
    pub fn new_headless() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::default(),
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or_else(|| anyhow::anyhow!("no compatible wgpu adapter (headless)"))?;

        let adapter_info = adapter.get_info();
        info!(
            backend = ?adapter_info.backend,
            name = %adapter_info.name,
            device_type = ?adapter_info.device_type,
            "selected GPU adapter (headless)"
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("avatar-studio.device.headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))?;

        // Headless callers render via capture_rgba; the config is a stub that
        // only contributes `format` (used by capture_rgba's color texture).
        // Use Rgba8UnormSrgb so the rendered pixels match on-screen sRGB.
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        Ok(Self {
            instance,
            surface: None,
            adapter,
            device,
            queue,
            config,
            _window: None,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let w = size.width.max(1);
        let h = size.height.max(1);
        if (w, h) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        surface.configure(&self.device, &self.config);
    }
}

pub struct Renderer {
    pub gpu: GpuContext,
    clear_color: wgpu::Color,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// GPU-side frame timer. `None` if the adapter doesn't support
    /// `TIMESTAMP_QUERY` + `TIMESTAMP_QUERY_INSIDE_ENCODERS`.
    pub gpu_timer: Option<GpuTimer>,
    // TODO(phase-3): mesh pipeline, material storage, texture cache.
    // TODO(phase-5): skinning pipeline + bone palette buffer.
    // TODO(phase-11): offscreen target for PNG export.
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("avatar-studio.depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl Renderer {
    /// Surface-less constructor for tools like `asset_builder` that only need
    /// `capture_rgba`. `render()` will panic on this renderer.
    pub fn new_headless(clear_color_srgb: [f32; 4]) -> anyhow::Result<Self> {
        let gpu = GpuContext::new_headless()?;
        // Depth texture only exists for parity with the on-screen path's
        // member layout; `capture_rgba` builds its own per-call.
        let (depth_texture, depth_view) = create_depth(&gpu.device, 1, 1);
        let linear = crate::color::srgb_to_linear(clear_color_srgb);
        Ok(Self {
            gpu,
            clear_color: wgpu::Color {
                r: linear[0] as f64,
                g: linear[1] as f64,
                b: linear[2] as f64,
                a: linear[3] as f64,
            },
            depth_texture,
            depth_view,
            gpu_timer: None,
        })
    }

    pub fn new(
        window: Arc<Window>,
        clear_color_srgb: [f32; 4],
        vsync: bool,
    ) -> anyhow::Result<Self> {
        let gpu = GpuContext::new(window, vsync)?;
        let (depth_texture, depth_view) =
            create_depth(&gpu.device, gpu.config.width, gpu.config.height);
        // Config stores sRGB values (the dark slate `#1e1e2e` lives there
        // as 30/255 = 0.118). The sRGB swapchain expects linear input on the
        // GPU side and gamma-encodes on display, so convert here.
        let linear = crate::color::srgb_to_linear(clear_color_srgb);
        let gpu_timer = GpuTimer::try_new(&gpu.device, &gpu.queue, gpu.device.features());
        if gpu_timer.is_some() {
            info!("gpu timestamp queries enabled");
        } else {
            info!("gpu timestamp queries unavailable on this adapter");
        }
        Ok(Self {
            gpu,
            clear_color: wgpu::Color {
                r: linear[0] as f64,
                g: linear[1] as f64,
                b: linear[2] as f64,
                a: linear[3] as f64,
            },
            depth_texture,
            depth_view,
            gpu_timer,
        })
    }

    pub fn last_gpu_ms(&self) -> Option<f32> {
        self.gpu_timer.as_ref().and_then(|t| t.last_ms())
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let prev = (self.gpu.config.width, self.gpu.config.height);
        self.gpu.resize(size);
        let now = (self.gpu.config.width, self.gpu.config.height);
        if prev != now {
            let (t, v) = create_depth(&self.gpu.device, now.0, now.1);
            self.depth_texture = t;
            self.depth_view = v;
        }
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.gpu.config.format
    }

    pub fn size(&self) -> [u32; 2] {
        [self.gpu.config.width, self.gpu.config.height]
    }

    /// Acquire the next surface texture, encode the clear pass, then invoke
    /// `ui` with a [`FrameCtx`] so the caller can append more passes (egui).
    /// Submits and presents the frame on the way out.
    pub fn render<F>(&mut self, ui: F) -> Result<(), wgpu::SurfaceError>
    where
        F: FnOnce(&mut FrameCtx<'_>),
    {
        let surface = self
            .gpu
            .surface
            .as_ref()
            .expect("on-screen renderer required for render()");
        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure with the current size and skip this frame.
                surface.configure(&self.gpu.device, &self.gpu.config);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("avatar-studio.frame-encoder"),
            });

        let timed_frame = self
            .gpu_timer
            .as_mut()
            .map(|t| t.begin(&mut encoder))
            .unwrap_or(false);

        // --- Pass 1: clear color AND depth --------------------------------
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // --- Pass 2+: caller-supplied (scene + egui) ----------------------
        let mut ctx = FrameCtx {
            device: &self.gpu.device,
            queue: &self.gpu.queue,
            view: &view,
            depth_view: &self.depth_view,
            encoder: &mut encoder,
            size: [self.gpu.config.width, self.gpu.config.height],
            surface_format: self.gpu.config.format,
        };
        ui(&mut ctx);

        if timed_frame {
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.end(&mut encoder);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        if timed_frame {
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.after_submit();
            }
        }
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.poll(&self.gpu.device);
        }
        Ok(())
    }

    /// Render a scene into an offscreen texture and return RGBA8 pixels.
    ///
    /// The callback receives a [`FrameCtx`] just like the swapchain render path,
    /// so callers can reuse `SceneRenderer::draw` and debug overlays. Egui is
    /// intentionally not part of this path.
    pub fn capture_rgba<F>(
        &mut self,
        width: u32,
        height: u32,
        clear_color: wgpu::Color,
        draw: F,
    ) -> anyhow::Result<RgbaScreenshot>
    where
        F: FnOnce(&mut FrameCtx<'_>),
    {
        let width = width.max(1);
        let height = height.max(1);
        let format = self.gpu.config.format;
        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let output_size = padded_bytes_per_row as u64 * height as u64;

        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("avatar-studio.capture.color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (_depth, depth_view) = create_depth(&self.gpu.device, width, height);
        let buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("avatar-studio.capture.readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("avatar-studio.capture-encoder"),
            });
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("capture.clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        {
            let mut ctx = FrameCtx {
                device: &self.gpu.device,
                queue: &self.gpu.queue,
                view: &view,
                depth_view: &depth_view,
                encoder: &mut encoder,
                size: [width, height],
                surface_format: format,
            };
            draw(&mut ctx);
        }

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| anyhow::anyhow!("capture readback channel closed: {e}"))?
            .map_err(|e| anyhow::anyhow!("capture readback failed: {e}"))?;

        let data = slice.get_mapped_range();
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height as usize {
            let src_start = y * padded_bytes_per_row as usize;
            let dst_start = y * unpadded_bytes_per_row as usize;
            let src = &data[src_start..src_start + unpadded_bytes_per_row as usize];
            let dst = &mut pixels[dst_start..dst_start + unpadded_bytes_per_row as usize];
            match format {
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                    for (bgra, rgba) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                        rgba[0] = bgra[2];
                        rgba[1] = bgra[1];
                        rgba[2] = bgra[0];
                        rgba[3] = bgra[3];
                    }
                }
                _ => dst.copy_from_slice(src),
            }
        }
        drop(data);
        buffer.unmap();
        RgbaScreenshot::new(width, height, pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_capture_writes_rgba() {
        // Skip on machines with no wgpu adapter (CI containers without a GPU
        // backend). We don't want this to be a hard failure for contributors.
        let mut renderer = match Renderer::new_headless([0.1, 0.2, 0.3, 1.0]) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("skipping headless_capture_writes_rgba: no adapter ({e})");
                return;
            }
        };
        let shot = renderer
            .capture_rgba(
                64,
                32,
                wgpu::Color {
                    r: 0.2,
                    g: 0.5,
                    b: 0.8,
                    a: 1.0,
                },
                |_ctx| {
                    // No additional draw work — clear-pass only.
                },
            )
            .expect("capture succeeds");
        assert_eq!(shot.width, 64);
        assert_eq!(shot.height, 32);
        assert_eq!(shot.pixels.len(), 64 * 32 * 4);
        // At least one pixel must be non-zero (the clear colour).
        assert!(shot.pixels.iter().any(|b| *b != 0));
    }
}

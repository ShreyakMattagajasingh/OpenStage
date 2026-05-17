//! GPU frame timing via wgpu timestamp queries.
//!
//! One [`GpuTimer`] holds a 2-slot timestamp `QuerySet`, a resolve buffer, and
//! a readback buffer. Each frame:
//!  1. `begin(&mut encoder)` writes timestamp 0 before any work.
//!  2. `end(&mut encoder)` writes timestamp 1, resolves into the resolve
//!     buffer, and copies it into a CPU-mappable readback buffer.
//!  3. After submit, `after_submit()` queues an async map of the readback.
//!  4. `poll(&Device)` advances readback; when mapping completes it stores
//!     the last-frame delta. `last_ms()` returns it.
//!
//! Single-buffer protocol: while a readback is in-flight, we skip the
//! begin/end pair on the next frame so we never write to a mapped buffer.
//! Result: ~every other frame produces a sample. Fine for perf logging.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tracing::warn;

const TIMESTAMP_BYTES: u64 = 8;
const QUERY_COUNT: u32 = 2;
const BUFFER_BYTES: u64 = TIMESTAMP_BYTES * QUERY_COUNT as u64;

#[derive(Debug)]
pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    readback_buf: wgpu::Buffer,
    /// Nanoseconds per timestamp tick (from `Queue::get_timestamp_period`).
    ns_per_tick: f32,
    /// True iff `end()` was called this frame and a readback was queued.
    in_flight: bool,
    /// Set by the map_async callback when the buffer is ready to read.
    ready: Arc<AtomicBool>,
    last_ms: Option<f32>,
}

impl GpuTimer {
    /// Build a timer if the device supports both timestamp queries and
    /// `write_timestamp` outside render passes. Returns `None` otherwise so
    /// the caller can gracefully omit GPU timing.
    pub fn try_new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        features: wgpu::Features,
    ) -> Option<Self> {
        if !features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        if !features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
            return None;
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("avatar-studio.gpu-timer.query-set"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERY_COUNT,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("avatar-studio.gpu-timer.resolve"),
            size: BUFFER_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("avatar-studio.gpu-timer.readback"),
            size: BUFFER_BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let ns_per_tick = queue.get_timestamp_period();
        Some(Self {
            query_set,
            resolve_buf,
            readback_buf,
            ns_per_tick,
            in_flight: false,
            ready: Arc::new(AtomicBool::new(false)),
            last_ms: None,
        })
    }

    /// Returns true if this frame will be timed (begin was just called).
    pub fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        if self.in_flight {
            return false;
        }
        encoder.write_timestamp(&self.query_set, 0);
        true
    }

    /// Pair with a `true` return from [`Self::begin`].
    pub fn end(&mut self, encoder: &mut wgpu::CommandEncoder) {
        encoder.write_timestamp(&self.query_set, 1);
        encoder.resolve_query_set(&self.query_set, 0..QUERY_COUNT, &self.resolve_buf, 0);
        encoder.copy_buffer_to_buffer(&self.resolve_buf, 0, &self.readback_buf, 0, BUFFER_BYTES);
    }

    /// Call once per frame *after* the encoder is submitted. Queues the async
    /// map. The mapping resolves later via [`Self::poll`].
    pub fn after_submit(&mut self) {
        let ready = self.ready.clone();
        self.readback_buf
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if let Err(err) = result {
                    warn!(error = ?err, "gpu timer readback failed");
                    ready.store(false, Ordering::Release);
                } else {
                    ready.store(true, Ordering::Release);
                }
            });
        self.in_flight = true;
    }

    /// Advance the device and consume any pending readback. Updates
    /// `last_ms()` when a measurement becomes available.
    pub fn poll(&mut self, device: &wgpu::Device) {
        if !self.in_flight {
            return;
        }
        device.poll(wgpu::Maintain::Poll);
        if !self.ready.swap(false, Ordering::AcqRel) {
            return;
        }
        let slice = self.readback_buf.slice(..);
        let data = slice.get_mapped_range();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&data);
        drop(data);
        self.readback_buf.unmap();
        self.in_flight = false;
        let begin = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let delta_ticks = end.saturating_sub(begin);
        let ns = delta_ticks as f64 * self.ns_per_tick as f64;
        let ms = (ns / 1_000_000.0) as f32;
        self.last_ms = Some(ms);
    }

    pub fn last_ms(&self) -> Option<f32> {
        self.last_ms
    }
}

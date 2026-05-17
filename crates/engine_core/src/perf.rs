//! Lightweight CPU-side frame timing for developer diagnostics and agent runs.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

pub const DEFAULT_ROLLING_CAPACITY: usize = 300;
pub const TARGET_FPS: f32 = 60.0;
pub const TARGET_FRAME_MS: f32 = 1000.0 / TARGET_FPS;

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSample {
    pub total_ms: f32,
    pub egui_ms: f32,
    pub tessellate_ms: f32,
    pub pose_ms: f32,
    pub scene_build_ms: f32,
    pub render_submit_ms: f32,
    /// GPU-side frame duration from timestamp queries, when available.
    /// `None` when the adapter doesn't support timestamp queries, or for
    /// the frames between async readbacks resolving.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_ms: Option<f32>,
    pub instance_count: u32,
}

#[derive(Debug, Clone)]
pub struct FrameStats {
    samples: VecDeque<FrameSample>,
    capacity: usize,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new(DEFAULT_ROLLING_CAPACITY)
    }
}

impl FrameStats {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, sample: FrameSample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn last(&self) -> Option<FrameSample> {
        self.samples.back().copied()
    }

    pub fn average_frame_ms(&self) -> f32 {
        average_ms(self.samples.iter().map(|s| s.total_ms))
    }

    pub fn average_fps(&self) -> f32 {
        fps_from_ms(self.average_frame_ms())
    }

    pub fn min_fps(&self) -> f32 {
        fps_from_ms(self.max_frame_ms())
    }

    pub fn max_frame_ms(&self) -> f32 {
        self.samples
            .iter()
            .map(|s| s.total_ms)
            .fold(0.0_f32, f32::max)
    }

    pub fn p95_frame_ms(&self) -> f32 {
        percentile_ms(self.samples.iter().map(|s| s.total_ms), 0.95)
    }

    pub fn p99_frame_ms(&self) -> f32 {
        percentile_ms(self.samples.iter().map(|s| s.total_ms), 0.99)
    }

    /// Returns the rolling average GPU frame time in ms when at least one
    /// timed sample exists in the window, otherwise `None`.
    pub fn average_gpu_ms(&self) -> Option<f32> {
        let mut sum = 0.0;
        let mut count = 0u32;
        for sample in &self.samples {
            if let Some(ms) = sample.gpu_ms {
                sum += ms;
                count += 1;
            }
        }
        if count == 0 {
            None
        } else {
            Some(sum / count as f32)
        }
    }

    pub fn p95_gpu_ms(&self) -> Option<f32> {
        let values: Vec<f32> = self.samples.iter().filter_map(|s| s.gpu_ms).collect();
        if values.is_empty() {
            None
        } else {
            Some(percentile_ms(values.into_iter(), 0.95))
        }
    }

    pub fn max_gpu_ms(&self) -> Option<f32> {
        self.samples
            .iter()
            .filter_map(|s| s.gpu_ms)
            .fold(None, |acc: Option<f32>, v| {
                Some(acc.map_or(v, |a| a.max(v)))
            })
    }

    pub fn to_report(
        &self,
        captured_at: String,
        window_size: [u32; 2],
        scene_mode: impl Into<String>,
        instance_count: u32,
    ) -> PerfReport {
        let average_frame_ms = self.average_frame_ms();
        let average_fps = self.average_fps();
        PerfReport {
            captured_at,
            frame_count: self.len() as u32,
            average_fps,
            min_fps: self.min_fps(),
            average_frame_ms,
            p95_frame_ms: self.p95_frame_ms(),
            p99_frame_ms: self.p99_frame_ms(),
            max_frame_ms: self.max_frame_ms(),
            average_gpu_ms: self.average_gpu_ms(),
            p95_gpu_ms: self.p95_gpu_ms(),
            max_gpu_ms: self.max_gpu_ms(),
            target_fps: TARGET_FPS,
            target_frame_ms: TARGET_FRAME_MS,
            passed: average_fps >= TARGET_FPS && average_frame_ms <= TARGET_FRAME_MS,
            window_size,
            scene_mode: scene_mode.into(),
            instance_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfReport {
    pub captured_at: String,
    pub frame_count: u32,
    pub average_fps: f32,
    pub min_fps: f32,
    pub average_frame_ms: f32,
    pub p95_frame_ms: f32,
    pub p99_frame_ms: f32,
    pub max_frame_ms: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_gpu_ms: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_gpu_ms: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_gpu_ms: Option<f32>,
    pub target_fps: f32,
    pub target_frame_ms: f32,
    pub passed: bool,
    pub window_size: [u32; 2],
    pub scene_mode: String,
    pub instance_count: u32,
}

fn average_ms(values: impl Iterator<Item = f32>) -> f32 {
    let mut sum = 0.0;
    let mut count = 0;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

fn percentile_ms(values: impl Iterator<Item = f32>, percentile: f32) -> f32 {
    let mut sorted: Vec<f32> = values.filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((sorted.len() - 1) as f32 * percentile.clamp(0.0, 1.0)).ceil() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn fps_from_ms(ms: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        1000.0 / ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(total_ms: f32) -> FrameSample {
        FrameSample {
            total_ms,
            ..Default::default()
        }
    }

    #[test]
    fn computes_average_fps_and_p95() {
        let mut stats = FrameStats::new(8);
        for ms in [10.0, 20.0, 30.0, 40.0] {
            stats.push(sample(ms));
        }
        assert!((stats.average_frame_ms() - 25.0).abs() < 0.001);
        assert!((stats.average_fps() - 40.0).abs() < 0.001);
        assert_eq!(stats.p95_frame_ms(), 40.0);
    }

    #[test]
    fn rolling_window_drops_old_samples() {
        let mut stats = FrameStats::new(2);
        stats.push(sample(10.0));
        stats.push(sample(20.0));
        stats.push(sample(30.0));
        assert_eq!(stats.len(), 2);
        assert!((stats.average_frame_ms() - 25.0).abs() < 0.001);
    }

    #[test]
    fn report_serializes_required_fields() {
        let mut stats = FrameStats::new(4);
        stats.push(sample(16.0));
        let report = stats.to_report("123".into(), [1280, 720], "avatar", 3);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"averageFps\""));
        assert!(json.contains("\"passed\""));
        assert!(json.contains("\"targetFrameMs\""));
    }
}

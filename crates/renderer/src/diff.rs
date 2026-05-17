//! Golden-image diffing for QA snapshot tests.
//!
//! Wraps `image_compare`'s SSIM + max-channel-diff into a single
//! `compare_rgba` helper that returns a report. Used by Phase 16
//! integration tests in `<workspace>/tests/golden_capture.rs`.

use image::RgbaImage;

#[derive(Debug, Clone, Copy)]
pub struct DiffReport {
    /// Structural Similarity Index in `[0.0, 1.0]`; 1.0 = identical.
    pub ssim: f32,
    /// Largest per-channel absolute delta over all pixels.
    pub max_channel_diff: u8,
    /// Pixels whose max channel diff exceeded the threshold passed to
    /// `compare_rgba`.
    pub diff_pixel_count: u32,
    pub total_pixels: u32,
}

impl DiffReport {
    pub fn diff_fraction(&self) -> f32 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.diff_pixel_count as f32 / self.total_pixels as f32
        }
    }
}

/// Compare two RGBA images. The threshold drives `diff_pixel_count` —
/// pixels whose max channel delta exceeds it are counted as "different".
pub fn compare_rgba(actual: &RgbaImage, golden: &RgbaImage, channel_threshold: u8) -> DiffReport {
    assert_eq!(
        actual.dimensions(),
        golden.dimensions(),
        "diff images must share dimensions"
    );
    let w = actual.width();
    let h = actual.height();

    // SSIM via image_compare. The crate's `rgba_hybrid_compare` blends RGB
    // SSIM + alpha L2 — close enough to a pure SSIM for our purposes.
    let ssim = image_compare::rgba_hybrid_compare(actual, golden)
        .map(|r| r.score as f32)
        .unwrap_or(0.0);

    let mut max_diff: u8 = 0;
    let mut diff_pixel_count: u32 = 0;
    for (a, g) in actual.pixels().zip(golden.pixels()) {
        let mut pixel_max = 0u8;
        for c in 0..4 {
            let d = a.0[c].abs_diff(g.0[c]);
            if d > pixel_max {
                pixel_max = d;
            }
        }
        if pixel_max > max_diff {
            max_diff = pixel_max;
        }
        if pixel_max > channel_threshold {
            diff_pixel_count += 1;
        }
    }

    DiffReport {
        ssim,
        max_channel_diff: max_diff,
        diff_pixel_count,
        total_pixels: w * h,
    }
}

/// Pass criteria for a snapshot test:
///   - SSIM at or above `ssim_min`.
///   - Fraction of pixels exceeding `max_diff` is at most `max_pct`.
pub fn passes(report: &DiffReport, ssim_min: f32, max_pct: f32) -> bool {
    report.ssim >= ssim_min && report.diff_fraction() <= max_pct
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn solid(w: u32, h: u32, color: [u8; 4]) -> RgbaImage {
        ImageBuffer::from_pixel(w, h, Rgba(color))
    }

    #[test]
    fn identical_images_score_one() {
        let img = solid(64, 64, [128, 64, 192, 255]);
        let report = compare_rgba(&img, &img, 0);
        assert!(
            (report.ssim - 1.0).abs() < 1e-3,
            "identical SSIM should be ~1.0, got {}",
            report.ssim
        );
        assert_eq!(report.max_channel_diff, 0);
        assert_eq!(report.diff_pixel_count, 0);
        assert!(passes(&report, 0.99, 0.0));
    }

    #[test]
    fn black_vs_white_fails() {
        let a = solid(32, 32, [0, 0, 0, 255]);
        let b = solid(32, 32, [255, 255, 255, 255]);
        let report = compare_rgba(&a, &b, 8);
        assert!(
            report.ssim < 0.5,
            "black-vs-white SSIM should be far from 1.0, got {}",
            report.ssim
        );
        assert_eq!(report.max_channel_diff, 255);
        assert_eq!(report.diff_pixel_count, 32 * 32);
        assert!(!passes(&report, 0.99, 0.01));
    }
}

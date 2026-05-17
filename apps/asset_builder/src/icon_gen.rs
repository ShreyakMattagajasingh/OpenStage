//! Generates the Windows executable / installer icon procedurally.
//!
//! Single 256x256 ICO. Windows scales it down for smaller sizes; multi-res
//! ICOs are nicer but the image crate doesn't expose multi-frame ICO
//! encoding directly, and a clean 256 source is enough for Phase 15.
//!
//! Visual: rounded square in Catppuccin panel (#313244), centered filled
//! circle in Catppuccin accent (#89b4fa), bold white "A" glyph centered.

use std::path::Path;

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgba, RgbaImage};

const SIZE: u32 = 256;
const PANEL: [u8; 4] = [0x31, 0x32, 0x44, 0xff]; // Catppuccin Mocha "surface0"
const ACCENT: [u8; 4] = [0x89, 0xb4, 0xfa, 0xff]; // "blue"
const TEXT: [u8; 4] = [0xcd, 0xd6, 0xf4, 0xff]; // "text"
const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

pub fn generate(out_path: &Path) -> Result<()> {
    let mut img: RgbaImage = ImageBuffer::from_pixel(SIZE, SIZE, Rgba(TRANSPARENT));

    // Background: rounded square covering 8..248 with a 28px corner radius.
    fill_rounded_square(&mut img, 8.0, 248.0, 28.0, Rgba(PANEL));

    // Accent: centered disc.
    fill_disc(&mut img, 128.0, 128.0, 88.0, Rgba(ACCENT));

    // Stylised "A" glyph: two diagonal legs + horizontal crossbar.
    // Glyph is a 96x108 rectangle centered at (128, 132).
    draw_a_glyph(&mut img, 128.0, 132.0, 96.0, 108.0, Rgba(TEXT));

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    img.save_with_format(out_path, image::ImageFormat::Ico)
        .with_context(|| format!("save {}", out_path.display()))?;
    Ok(())
}

fn fill_rounded_square(img: &mut RgbaImage, min: f32, max: f32, radius: f32, color: Rgba<u8>) {
    let w = img.width();
    let h = img.height();
    for y in 0..h {
        for x in 0..w {
            let xf = x as f32 + 0.5;
            let yf = y as f32 + 0.5;
            if !(min..=max).contains(&xf) || !(min..=max).contains(&yf) {
                continue;
            }
            // Distance from the nearest interior corner.
            let dx = clamp_outside(xf, min + radius, max - radius);
            let dy = clamp_outside(yf, min + radius, max - radius);
            if dx * dx + dy * dy <= radius * radius {
                img.put_pixel(x, y, color);
            }
        }
    }
}

fn clamp_outside(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        v - lo
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

fn fill_disc(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, color: Rgba<u8>) {
    let w = img.width();
    let h = img.height();
    let r2 = r * r;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                img.put_pixel(x, y, color);
            }
        }
    }
}

/// Stylised uppercase "A": two slanted bars meeting at the apex, plus a
/// horizontal crossbar at ~60% height. Drawn directly on the image with
/// per-pixel inclusion tests for clarity.
fn draw_a_glyph(img: &mut RgbaImage, cx: f32, cy: f32, width: f32, height: f32, color: Rgba<u8>) {
    let half_w = width * 0.5;
    let half_h = height * 0.5;
    let top_y = cy - half_h;
    let bottom_y = cy + half_h;
    let bar_thickness = width * 0.18;
    let crossbar_y = cy + height * 0.10;
    let crossbar_h = bar_thickness * 0.75;
    let apex_x = cx;
    // Bottom corners (outer edges of the legs).
    let bottom_outer_left = cx - half_w;
    let bottom_outer_right = cx + half_w;

    for y in (top_y.floor() as i32 - 2)..=(bottom_y.ceil() as i32 + 2) {
        if y < 0 || y as u32 >= img.height() {
            continue;
        }
        let yf = y as f32 + 0.5;
        if yf < top_y || yf > bottom_y {
            continue;
        }
        // Slope from apex (at top_y) to outer bottom corners.
        let t = (yf - top_y) / (bottom_y - top_y);
        let outer_left = apex_x + (bottom_outer_left - apex_x) * t;
        let outer_right = apex_x + (bottom_outer_right - apex_x) * t;
        let inner_left = outer_left + bar_thickness;
        let inner_right = outer_right - bar_thickness;

        for x in (outer_left.floor() as i32 - 2)..=(outer_right.ceil() as i32 + 2) {
            if x < 0 || x as u32 >= img.width() {
                continue;
            }
            let xf = x as f32 + 0.5;
            let in_left_leg = xf >= outer_left && xf <= inner_left;
            let in_right_leg = xf >= inner_right && xf <= outer_right;
            let in_crossbar = (yf - crossbar_y).abs() <= crossbar_h * 0.5
                && xf >= outer_left
                && xf <= outer_right;
            if in_left_leg || in_right_leg || in_crossbar {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

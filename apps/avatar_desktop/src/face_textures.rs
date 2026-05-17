//! Procedural face textures for the Phase-9 expression system.
//!
//! Five 128×128 RGBA8 images, drawn with `image` crate primitives. No fonts,
//! no external assets. Each generation is sub-millisecond; the app caches
//! `DynamicImage` per Expression and uploads to GPU on demand.

use avatar::Expression;
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};

const SIZE: u32 = 128;
const SKIN: Rgba<u8> = Rgba([255, 218, 185, 255]); // peachy skin tone
const INK: Rgba<u8> = Rgba([20, 20, 20, 255]); // near-black
const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

/// Build the five face images at once. Returned in `Expression::ALL` order.
pub fn generate_all() -> [(Expression, DynamicImage); 5] {
    [
        (Expression::Neutral, generate(Expression::Neutral)),
        (Expression::Happy, generate(Expression::Happy)),
        (Expression::Sad, generate(Expression::Sad)),
        (Expression::Surprised, generate(Expression::Surprised)),
        (Expression::Angry, generate(Expression::Angry)),
    ]
}

pub fn generate(expression: Expression) -> DynamicImage {
    let mut img: RgbaImage = ImageBuffer::from_pixel(SIZE, SIZE, SKIN);

    // Eye positions: a third in from each side, ~1/3 down from top.
    let eye_y: i32 = 46;
    let eye_l_x: i32 = 42;
    let eye_r_x: i32 = 86;
    let eye_radius = 6.0;

    match expression {
        Expression::Surprised => {
            draw_disk(&mut img, eye_l_x, eye_y, eye_radius * 0.9, INK);
            draw_disk(&mut img, eye_r_x, eye_y, eye_radius * 0.9, INK);
        }
        Expression::Angry => {
            // Slanted slit eyes.
            draw_slit(&mut img, eye_l_x, eye_y, 14, -3, INK);
            draw_slit(&mut img, eye_r_x, eye_y, 14, 3, INK);
            // Eyebrows slanted inward.
            draw_thick_line(
                &mut img,
                eye_l_x - 7,
                eye_y - 14,
                eye_l_x + 9,
                eye_y - 8,
                2,
                INK,
            );
            draw_thick_line(
                &mut img,
                eye_r_x - 9,
                eye_y - 8,
                eye_r_x + 7,
                eye_y - 14,
                2,
                INK,
            );
        }
        Expression::Sad => {
            // Slightly downward eyes — outer corner lower.
            draw_slit(&mut img, eye_l_x, eye_y, 14, 3, INK);
            draw_slit(&mut img, eye_r_x, eye_y, 14, -3, INK);
        }
        _ => {
            draw_disk(&mut img, eye_l_x, eye_y, eye_radius, INK);
            draw_disk(&mut img, eye_r_x, eye_y, eye_radius, INK);
        }
    }

    // Mouth area: vertically below the eyes, horizontally centered.
    let mouth_cx: i32 = (SIZE as i32) / 2;
    let mouth_cy: i32 = 86;
    let mouth_half_w: i32 = 22;

    match expression {
        Expression::Neutral => {
            draw_thick_line(
                &mut img,
                mouth_cx - mouth_half_w,
                mouth_cy,
                mouth_cx + mouth_half_w,
                mouth_cy,
                2,
                INK,
            );
        }
        Expression::Happy => {
            // Upward parabola: y = cy - amplitude * (1 - (x/half)^2).
            let amp = 12.0;
            draw_parabola(&mut img, mouth_cx, mouth_cy + 4, mouth_half_w, -amp, INK);
        }
        Expression::Sad => {
            let amp = 12.0;
            draw_parabola(&mut img, mouth_cx, mouth_cy - 4, mouth_half_w, amp, INK);
        }
        Expression::Surprised => {
            // Small filled circle "O" mouth.
            draw_disk(&mut img, mouth_cx, mouth_cy, 9.0, INK);
            draw_disk(&mut img, mouth_cx, mouth_cy, 6.0, SKIN); // hollow center
        }
        Expression::Angry => {
            // Tight straight line, slightly downward toward the middle.
            draw_thick_line(
                &mut img,
                mouth_cx - mouth_half_w,
                mouth_cy - 2,
                mouth_cx,
                mouth_cy + 2,
                2,
                INK,
            );
            draw_thick_line(
                &mut img,
                mouth_cx,
                mouth_cy + 2,
                mouth_cx + mouth_half_w,
                mouth_cy - 2,
                2,
                INK,
            );
        }
    }

    DynamicImage::ImageRgba8(img)
}

// ---------- pixel helpers --------------------------------------------------

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
        return;
    }
    img.put_pixel(x as u32, y as u32, c);
}

fn draw_disk(img: &mut RgbaImage, cx: i32, cy: i32, r: f32, c: Rgba<u8>) {
    let r2 = r * r;
    let ri = r.ceil() as i32;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let d2 = (dx * dx + dy * dy) as f32;
            if d2 <= r2 {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Filled horizontal-ish slit, length pixels wide, slope = dy across the width.
fn draw_slit(img: &mut RgbaImage, cx: i32, cy: i32, length: i32, slope: i32, c: Rgba<u8>) {
    let half = length / 2;
    for dx in -half..=half {
        let y_off = (slope * dx) / half.max(1);
        for dy in -1..=1 {
            put(img, cx + dx, cy + y_off + dy, c);
        }
    }
}

fn draw_thick_line(
    img: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    c: Rgba<u8>,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let t = thickness / 2;
    loop {
        for ox in -t..=t {
            for oy in -t..=t {
                put(img, x + ox, y + oy, c);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// Sample a parabola y = cy + amp * ((x - cx) / half_w)^2 - amp; plot a thick line.
fn draw_parabola(img: &mut RgbaImage, cx: i32, cy: i32, half_w: i32, amp: f32, c: Rgba<u8>) {
    let mut prev: Option<(i32, i32)> = None;
    for x in (cx - half_w)..=(cx + half_w) {
        let t = (x - cx) as f32 / half_w as f32;
        let y = cy as f32 + amp * (1.0 - t * t);
        let p = (x, y.round() as i32);
        if let Some(q) = prev {
            draw_thick_line(img, q.0, q.1, p.0, p.1, 2, c);
        }
        prev = Some(p);
    }
}

/// Marker so we don't drop the import on test-free builds.
#[allow(dead_code)]
const _MARK: Rgba<u8> = TRANSPARENT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_generate_distinct_pixels() {
        let images = generate_all();
        assert_eq!(images.len(), 5);
        // Compare the first 4KB of each pair to ensure they differ.
        for i in 0..images.len() {
            for j in (i + 1)..images.len() {
                let a = images[i].1.as_rgba8().unwrap().as_raw();
                let b = images[j].1.as_rgba8().unwrap().as_raw();
                let mismatch = a.iter().zip(b.iter()).any(|(x, y)| x != y);
                assert!(
                    mismatch,
                    "expressions {:?} and {:?} produced identical pixels",
                    images[i].0, images[j].0
                );
            }
        }
    }
}

//! sRGB ↔ linear color conversion.
//!
//! The swapchain is `Bgra8UnormSrgb` so the GPU writes color in *linear* and
//! the display step gamma-corrects to sRGB. Designer-facing color values
//! (config, defaults, color pickers) are sRGB; convert at the boundary.

#[inline]
fn srgb_channel_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// RGBA: alpha is left as-is.
pub fn srgb_to_linear(rgba: [f32; 4]) -> [f32; 4] {
    [
        srgb_channel_to_linear(rgba[0]),
        srgb_channel_to_linear(rgba[1]),
        srgb_channel_to_linear(rgba[2]),
        rgba[3],
    ]
}

//! Lighting. One directional light + ambient. Combined with the camera into
//! the per-frame uniform in `scene.rs`.

use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct Light {
    /// World-space direction TO the light source. Will be normalized.
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    pub ambient: Vec3,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            // Up-and-forward — gives a top-front key light, leaves the back in shadow.
            direction: Vec3::new(0.3, 1.0, 0.6),
            color: Vec3::new(1.0, 0.98, 0.92), // very slight warm cast
            intensity: 1.0,
            ambient: Vec3::splat(0.18),
        }
    }
}

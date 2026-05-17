//! Orbit camera. Spherical coordinates around `target`.
//!
//! Conventions match `docs/skeleton_standard.md`: +Y up, +Z forward, right-handed,
//! meters. Pitch is clamped to (-89°, +89°) to prevent gimbal flip.

use glam::{Mat4, Vec3};

const PITCH_LIMIT: f32 = 89.0_f32 * std::f32::consts::PI / 180.0;
const MIN_DISTANCE: f32 = 0.5;
const MAX_DISTANCE: f32 = 10.0;

/// Default sensitivities; the app can tweak by scaling its inputs.
pub const ORBIT_SENS: f32 = 0.005; // rad / pixel
pub const PAN_SENS: f32 = 0.0015; // m / pixel @ distance=1 (scales with distance)
pub const ZOOM_FACTOR: f32 = 0.1; // 10% per wheel notch

#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,   // radians, around +Y
    pub pitch: f32, // radians; 0 = level, positive = looking down
    pub distance: f32,
    pub fov_y_rad: f32,
    pub near: f32,
    pub far: f32,
    pub aspect: f32,
}

impl OrbitCamera {
    pub fn new(aspect: f32) -> Self {
        let mut cam = Self {
            target: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
            pitch: (-15.0_f32).to_radians(),
            distance: 3.0,
            fov_y_rad: 45.0_f32.to_radians(),
            near: 0.05,
            far: 100.0,
            aspect,
        };
        cam.clamp();
        cam
    }

    pub fn set_aspect(&mut self, size: [u32; 2]) {
        let [w, h] = size;
        self.aspect = (w.max(1) as f32) / (h.max(1) as f32);
    }

    /// World-space eye position derived from yaw/pitch/distance/target.
    pub fn eye(&self) -> Vec3 {
        // Forward (from eye to target) in world space:
        //   yaw=0,pitch=0  ⇒ camera sits at target + (0,0,distance), looking toward -Z.
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Eye offset from target = distance * (-forward), where forward is the
        // direction the camera looks. We define forward at yaw=0,pitch=0 as
        // -Z, so eye offset is +Z at rest.
        let offset = Vec3::new(sy * cp, sp, cy * cp) * self.distance;
        self.target + offset
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn proj(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_rad, self.aspect, self.near, self.far)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj() * self.view()
    }

    pub fn orbit(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw -= yaw_delta;
        self.pitch -= pitch_delta;
        self.clamp();
    }

    pub fn zoom(&mut self, scroll_steps: f32) {
        // Multiplicative: each notch scales distance by (1 ± ZOOM_FACTOR).
        let factor = (1.0 - ZOOM_FACTOR).powf(scroll_steps);
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Pan along the camera's right/up axes. Scaling by distance keeps pan
    /// speed feeling consistent at any zoom level.
    pub fn pan(&mut self, dx_px: f32, dy_px: f32) {
        let view = self.view();
        // Inverse view's columns 0 and 1 are right and up in world space.
        let inv = view.inverse();
        let right = inv.x_axis.truncate();
        let up = inv.y_axis.truncate();
        let scale = PAN_SENS * self.distance;
        self.target += -right * dx_px * scale + up * dy_px * scale;
    }

    pub fn focus(&mut self) {
        *self = Self::new(self.aspect);
    }

    pub fn preset_full_body(&mut self) {
        self.target = Vec3::new(0.0, 0.9, 0.0);
        self.yaw = 0.0;
        self.pitch = (-10.0_f32).to_radians();
        self.distance = 3.5;
        self.clamp();
    }

    pub fn preset_face(&mut self) {
        self.target = Vec3::new(0.0, 1.55, 0.0);
        self.yaw = 0.0;
        self.pitch = 0.0;
        self.distance = 0.7;
        self.clamp();
    }

    fn clamp(&mut self) {
        self.pitch = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.distance = self.distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

//! Input state aggregation.
//!
//! TODO(phase-2): collect mouse drag/wheel/keyboard events from winit and expose a
//! camera-agnostic intent struct (orbit yaw/pitch delta, zoom delta, pan delta,
//! focus key, view presets). The renderer's OrbitCamera will consume this.

#![allow(dead_code)]

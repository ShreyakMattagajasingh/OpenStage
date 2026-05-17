//! Engine core: config, errors, frame clock, input.
//!
//! No GPU, no UI. Anything that lives at the "app-wide policy" layer goes here.

pub mod config;
pub mod error;
pub mod input;
pub mod paths;
pub mod perf;
pub mod time;

pub use config::{Config, PathsConfig, RenderConfig, WindowConfig};
pub use error::{CoreError, Result};
pub use paths::{resolve as resolve_paths, PathMode, ResolvedPaths};
pub use perf::{FrameSample, FrameStats, PerfReport};
pub use time::FrameClock;

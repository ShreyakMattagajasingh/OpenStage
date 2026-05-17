//! Animated image export helpers.

use std::fs::File;
use std::path::{Path, PathBuf};

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageError, RgbaImage};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifExportOptions {
    pub size: u32,
    pub frames: u32,
    pub duration_ms: u32,
}

impl Default for GifExportOptions {
    fn default() -> Self {
        Self {
            size: 512,
            frames: 48,
            duration_ms: 2_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum GifExportError {
    #[error("unsupported GIF export size {0}; expected 512 or 1024")]
    UnsupportedSize(u32),
    #[error("unsupported GIF frame count {0}; expected 2..=240")]
    UnsupportedFrameCount(u32),
    #[error("GIF duration must be at least one millisecond")]
    InvalidDuration,
    #[error("pixel buffer length {actual} did not match expected {expected}")]
    PixelLength { actual: usize, expected: usize },
    #[error("could not create parent directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not create GIF {path}: {source}")]
    CreateFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not encode GIF {path}: {source}")]
    Encode {
        path: PathBuf,
        #[source]
        source: ImageError,
    },
}

pub type Result<T> = std::result::Result<T, GifExportError>;

pub fn validate_options(options: GifExportOptions) -> Result<()> {
    match options.size {
        512 | 1024 => {}
        other => return Err(GifExportError::UnsupportedSize(other)),
    }
    if !(2..=240).contains(&options.frames) {
        return Err(GifExportError::UnsupportedFrameCount(options.frames));
    }
    if options.duration_ms == 0 {
        return Err(GifExportError::InvalidDuration);
    }
    Ok(())
}

pub fn write_rgba_gif(
    path: &Path,
    width: u32,
    height: u32,
    frames: &[Vec<u8>],
    delay_ms: u32,
) -> Result<()> {
    if width != height {
        return Err(GifExportError::UnsupportedSize(width.max(height)));
    }
    validate_options(GifExportOptions {
        size: width,
        frames: frames.len() as u32,
        duration_ms: delay_ms.saturating_mul(frames.len() as u32),
    })?;

    let expected = width as usize * height as usize * 4;
    for pixels in frames {
        if pixels.len() != expected {
            return Err(GifExportError::PixelLength {
                actual: pixels.len(),
                expected,
            });
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| GifExportError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file = File::create(path).map_err(|source| GifExportError::CreateFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut encoder = GifEncoder::new(file);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|source| GifExportError::Encode {
            path: path.to_path_buf(),
            source,
        })?;

    let delay = Delay::from_numer_denom_ms(delay_ms.max(1), 1);
    for pixels in frames {
        let img = RgbaImage::from_raw(width, height, pixels.clone()).ok_or(
            GifExportError::PixelLength {
                actual: pixels.len(),
                expected,
            },
        )?;
        let frame = Frame::from_parts(img, 0, 0, delay);
        encoder
            .encode_frame(frame)
            .map_err(|source| GifExportError::Encode {
                path: path.to_path_buf(),
                source,
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_options() {
        assert!(validate_options(GifExportOptions::default()).is_ok());
        assert!(validate_options(GifExportOptions {
            size: 768,
            ..GifExportOptions::default()
        })
        .is_err());
        assert!(validate_options(GifExportOptions {
            frames: 1,
            ..GifExportOptions::default()
        })
        .is_err());
    }

    #[test]
    fn writes_small_gif() {
        let path = std::env::temp_dir().join("avatar_export_video_test.gif");
        let frame = vec![255u8; 512 * 512 * 4];
        write_rgba_gif(&path, 512, 512, &[frame.clone(), frame], 40).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"GIF"));
        let _ = std::fs::remove_file(path);
    }
}

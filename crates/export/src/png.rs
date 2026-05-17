//! PNG export helpers.

use std::path::{Path, PathBuf};

use image::{ImageBuffer, ImageError, Rgba};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportView {
    FullBody,
    Portrait,
}

impl ExportView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullBody => "full_body",
            Self::Portrait => "portrait",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngExportOptions {
    pub size: u32,
    pub transparent_background: bool,
    pub view: ExportView,
}

impl Default for PngExportOptions {
    fn default() -> Self {
        Self {
            size: 1024,
            transparent_background: true,
            view: ExportView::FullBody,
        }
    }
}

#[derive(Debug, Error)]
pub enum PngExportError {
    #[error("unsupported PNG export size {0}; expected 512, 1024, or 2048")]
    UnsupportedSize(u32),
    #[error("pixel buffer length {actual} did not match expected {expected}")]
    PixelLength { actual: usize, expected: usize },
    #[error("could not create parent directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not encode PNG {path}: {source}")]
    Encode {
        path: PathBuf,
        #[source]
        source: ImageError,
    },
}

pub type Result<T> = std::result::Result<T, PngExportError>;

pub fn validate_size(size: u32) -> Result<()> {
    match size {
        512 | 1024 | 2048 => Ok(()),
        other => Err(PngExportError::UnsupportedSize(other)),
    }
}

pub fn write_rgba_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    validate_size(width)?;
    if width != height {
        return Err(PngExportError::UnsupportedSize(width.max(height)));
    }
    let expected = width as usize * height as usize * 4;
    if pixels.len() != expected {
        return Err(PngExportError::PixelLength {
            actual: pixels.len(),
            expected,
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PngExportError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, pixels.to_vec())
        .ok_or(PngExportError::PixelLength {
            actual: pixels.len(),
            expected,
        })?;
    img.save(path).map_err(|source| PngExportError::Encode {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_phase_11_sizes() {
        assert!(validate_size(512).is_ok());
        assert!(validate_size(1024).is_ok());
        assert!(validate_size(2048).is_ok());
        assert!(validate_size(768).is_err());
    }

    #[test]
    fn rejects_wrong_pixel_count() {
        let path = std::env::temp_dir().join("avatar_export_bad.png");
        let err = write_rgba_png(&path, 512, 512, &[0; 4]).unwrap_err();
        assert!(matches!(err, PngExportError::PixelLength { .. }));
    }
}

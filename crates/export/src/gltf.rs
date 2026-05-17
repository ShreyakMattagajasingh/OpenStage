//! GLB/VRM export surface.
//!
//! Phase 17 records the API boundary and the current blocker: runtime meshes
//! are GPU-only after load, so a correct composed-avatar GLB needs CPU mesh
//! retention or a source-asset composition pass. Returning a typed error keeps
//! callers honest until that data path exists.

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarGlbFormat {
    Glb,
    Vrm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarGlbExportOptions {
    pub format: AvatarGlbFormat,
    pub include_animation: bool,
}

impl Default for AvatarGlbExportOptions {
    fn default() -> Self {
        Self {
            format: AvatarGlbFormat::Glb,
            include_animation: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum AvatarGlbExportError {
    #[error(
        "GLB/VRM export needs CPU mesh data; runtime meshes currently retain GPU buffers only"
    )]
    CpuMeshDataUnavailable,
    #[error("unsupported GLB/VRM export destination {path}")]
    UnsupportedDestination { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, AvatarGlbExportError>;

pub fn validate_destination(path: &Path, format: AvatarGlbFormat) -> Result<()> {
    let expected = match format {
        AvatarGlbFormat::Glb => "glb",
        AvatarGlbFormat::Vrm => "vrm",
    };
    let is_expected = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(expected));
    if is_expected {
        Ok(())
    } else {
        Err(AvatarGlbExportError::UnsupportedDestination {
            path: path.to_path_buf(),
        })
    }
}

pub fn export_composed_avatar_placeholder(
    path: &Path,
    options: &AvatarGlbExportOptions,
) -> Result<()> {
    validate_destination(path, options.format)?;
    Err(AvatarGlbExportError::CpuMeshDataUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_extensions() {
        assert!(validate_destination(Path::new("avatar.glb"), AvatarGlbFormat::Glb).is_ok());
        assert!(validate_destination(Path::new("avatar.vrm"), AvatarGlbFormat::Vrm).is_ok());
        assert!(validate_destination(Path::new("avatar.png"), AvatarGlbFormat::Glb).is_err());
    }

    #[test]
    fn placeholder_returns_clear_blocker() {
        let err = export_composed_avatar_placeholder(
            Path::new("avatar.glb"),
            &AvatarGlbExportOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(err, AvatarGlbExportError::CpuMeshDataUnavailable));
    }
}

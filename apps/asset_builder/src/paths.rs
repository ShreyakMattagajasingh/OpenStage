//! Workspace path helpers shared by the Phase 12 subcommands.

use std::path::{Path, PathBuf};

use assets::metadata::AssetCategory;

/// Subdirectory under `assets/processed/avatars/` for a given category. Bodies
/// live at `bodies/`, tops at `tops/`, etc. Existing fixtures use plural forms
/// (`bodies/`, `tops/`); this preserves that.
///
/// Returns `None` for categories that don't represent an on-disk avatar mesh
/// (animation, pose, background, material) — those aren't importable via
/// Phase 12 and the caller bails before reaching here.
pub fn category_dir(cat: AssetCategory) -> Option<&'static str> {
    Some(match cat {
        AssetCategory::Body => "bodies",
        AssetCategory::Head => "heads",
        AssetCategory::Hair => "hairs",
        AssetCategory::Top => "tops",
        AssetCategory::Bottom => "bottoms",
        AssetCategory::Shoes => "shoes",
        AssetCategory::Hat => "hats",
        AssetCategory::Glasses => "glasses",
        AssetCategory::Accessory => "accessories",
        AssetCategory::Animation
        | AssetCategory::Pose
        | AssetCategory::Background
        | AssetCategory::Material => return None,
    })
}

/// `<workspace>/assets/processed/metadata/<id>.json`.
pub fn metadata_path(workspace_root: &Path, id: &str) -> PathBuf {
    workspace_root
        .join("assets/processed/metadata")
        .join(format!("{id}.json"))
}

/// `<workspace>/assets/processed/thumbnails/<id>.png`.
pub fn thumbnail_path(workspace_root: &Path, id: &str) -> PathBuf {
    workspace_root
        .join("assets/processed/thumbnails")
        .join(format!("{id}.png"))
}

/// Path under `model` field inside a metadata JSON. Relative to `assets/processed/`.
pub fn relative_model_path(cat: AssetCategory, id: &str) -> Option<String> {
    let dir = category_dir(cat)?;
    Some(format!("avatars/{dir}/{id}.glb"))
}

/// Same convention for thumbnails: relative to `assets/processed/`.
pub fn relative_thumbnail_path(id: &str) -> String {
    format!("thumbnails/{id}.png")
}

/// `<workspace>/user_data/asset_catalog.sqlite`.
pub fn catalog_db_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("user_data/asset_catalog.sqlite")
}

/// Returns the asset id validated as `^[a-z0-9_]+$`. Rejects empty strings,
/// uppercase, and anything that would make a bad filename.
pub fn validate_asset_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty() {
        anyhow::bail!("asset id is empty");
    }
    for ch in id.chars() {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
            anyhow::bail!(
                "asset id {id:?} contains invalid character {ch:?} (allowed: a-z, 0-9, _)"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_dir_covers_importable_categories() {
        assert_eq!(category_dir(AssetCategory::Body), Some("bodies"));
        assert_eq!(category_dir(AssetCategory::Top), Some("tops"));
        assert_eq!(category_dir(AssetCategory::Animation), None);
    }

    #[test]
    fn validate_asset_id_accepts_snake_lower() {
        assert!(validate_asset_id("body_phase4_rig_001").is_ok());
        assert!(validate_asset_id("").is_err());
        assert!(validate_asset_id("Body").is_err());
        assert!(validate_asset_id("body-1").is_err());
        assert!(validate_asset_id("body 1").is_err());
    }
}

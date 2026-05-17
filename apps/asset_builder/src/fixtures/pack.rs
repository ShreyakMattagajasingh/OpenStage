//! Phase 17 sample fixture pack.
//!
//! These are lightweight catalog fixtures meant to exercise every wearable
//! category in the app. They intentionally reuse the already-skinned Phase 7
//! top mesh so they are guaranteed to bind to the current sample rig; future
//! art passes can replace the GLBs without changing ids.

use std::path::Path;

use anyhow::{Context, Result};
use assets::{AssetCategory, AssetMeta, Catalog};
use tracing::info;

use crate::paths;

const SOURCE_GLB: &str = "assets/processed/avatars/tops/phase7_top.glb";

struct FixtureSpec {
    id: &'static str,
    name: &'static str,
    category: AssetCategory,
    default_color: [f32; 3],
    tags: &'static [&'static str],
}

const FIXTURES: &[FixtureSpec] = &[
    FixtureSpec {
        id: "bottom_phase17_basic_001",
        name: "Phase 17 Bottom",
        category: AssetCategory::Bottom,
        default_color: [0.18, 0.22, 0.32],
        tags: &["sample", "phase17", "bottom"],
    },
    FixtureSpec {
        id: "shoes_phase17_basic_001",
        name: "Phase 17 Shoes",
        category: AssetCategory::Shoes,
        default_color: [0.08, 0.08, 0.10],
        tags: &["sample", "phase17", "shoes"],
    },
    FixtureSpec {
        id: "hair_phase17_basic_001",
        name: "Phase 17 Hair",
        category: AssetCategory::Hair,
        default_color: [0.12, 0.06, 0.03],
        tags: &["sample", "phase17", "hair"],
    },
    FixtureSpec {
        id: "hat_phase17_basic_001",
        name: "Phase 17 Hat",
        category: AssetCategory::Hat,
        default_color: [0.70, 0.12, 0.18],
        tags: &["sample", "phase17", "hat"],
    },
    FixtureSpec {
        id: "glasses_phase17_basic_001",
        name: "Phase 17 Glasses",
        category: AssetCategory::Glasses,
        default_color: [0.05, 0.05, 0.06],
        tags: &["sample", "phase17", "glasses"],
    },
    FixtureSpec {
        id: "accessory_phase17_basic_001",
        name: "Phase 17 Accessory",
        category: AssetCategory::Accessory,
        default_color: [0.90, 0.70, 0.12],
        tags: &["sample", "phase17", "accessory"],
    },
];

pub fn generate(workspace_root: &Path) -> Result<()> {
    let src = workspace_root.join(SOURCE_GLB);
    if !src.exists() {
        super::top::generate(workspace_root)?;
    }

    let mut metas = Vec::new();
    for spec in FIXTURES {
        let model_rel = paths::relative_model_path(spec.category, spec.id)
            .context("fixture category should be importable")?;
        let glb_path = workspace_root.join("assets/processed").join(&model_rel);
        if let Some(parent) = glb_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::copy(&src, &glb_path)
            .with_context(|| format!("copy {} to {}", src.display(), glb_path.display()))?;

        let meta = AssetMeta {
            id: spec.id.to_string(),
            display_name: spec.name.to_string(),
            category: spec.category,
            model: model_rel,
            thumbnail: Some(paths::relative_thumbnail_path(spec.id)),
            supports_color: true,
            default_color: Some(spec.default_color),
            compatible_body_types: vec!["body_phase4_rig_001".to_string()],
            compatible_skeleton: Some(animation::Skeleton::AVATAR_SKELETON_V1.to_string()),
            tags: spec.tags.iter().map(|tag| (*tag).to_string()).collect(),
            version: 1,
        };

        let meta_path = paths::metadata_path(workspace_root, spec.id);
        if let Some(parent) = meta_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&meta)?;
        std::fs::write(&meta_path, json)
            .with_context(|| format!("write {}", meta_path.display()))?;

        info!(id = spec.id, model = %glb_path.display(), "phase17 fixture written");
        metas.push(meta);
    }

    let db_path = paths::catalog_db_path(workspace_root);
    let mut catalog = Catalog::open(&db_path)?;
    catalog.upsert_many(&metas)?;
    Ok(())
}

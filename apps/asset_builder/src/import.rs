//! `import <input.glb>` subcommand: copy a GLB into `assets/processed/`,
//! write the metadata JSON sidecar, optionally render a thumbnail, and upsert
//! the row into the SQLite catalog.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use assets::metadata::{AssetCategory, AssetMeta};
use assets::Catalog;
use tracing::info;

use crate::paths;
use crate::validate::{enforce_category_rules, validate_glb};

#[derive(Debug, Clone, Default)]
pub struct ImportArgs {
    pub input: PathBuf,
    pub workspace_root: PathBuf,
    pub id: Option<String>,
    pub category: Option<AssetCategory>,
    pub display_name: Option<String>,
    pub body_types: Vec<String>,
    pub skeleton: Option<String>,
    pub supports_color: bool,
    pub default_color: Option<[f32; 3]>,
    pub tags: Vec<String>,
    pub meta_path: Option<PathBuf>,
    pub thumb: bool,
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub id: String,
    pub model_path: PathBuf,
    pub metadata_path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
}

pub fn run_import(args: &ImportArgs) -> Result<ImportReport> {
    let input = canonicalise(&args.input)?;
    let workspace = canonicalise(&args.workspace_root)?;

    let mut meta = if let Some(meta_path) = &args.meta_path {
        let text = fs::read_to_string(meta_path)
            .with_context(|| format!("read sidecar {}", meta_path.display()))?;
        serde_json::from_str::<AssetMeta>(&text)
            .with_context(|| format!("parse sidecar {}", meta_path.display()))?
    } else {
        AssetMeta {
            id: String::new(),
            display_name: String::new(),
            category: AssetCategory::Body,
            model: String::new(),
            thumbnail: None,
            supports_color: false,
            default_color: None,
            compatible_body_types: Vec::new(),
            compatible_skeleton: None,
            tags: Vec::new(),
            version: 1,
        }
    };

    if let Some(id) = &args.id {
        meta.id = id.clone();
    }
    if let Some(cat) = args.category {
        meta.category = cat;
    } else if args.meta_path.is_none() {
        return Err(anyhow!("--category is required (no sidecar provided)"));
    }
    if let Some(name) = &args.display_name {
        meta.display_name = name.clone();
    }
    if !args.body_types.is_empty() {
        meta.compatible_body_types = args.body_types.clone();
    }
    if let Some(skel) = &args.skeleton {
        meta.compatible_skeleton = if skel == "null" {
            None
        } else {
            Some(skel.clone())
        };
    }
    if args.supports_color {
        meta.supports_color = true;
    }
    if let Some(c) = args.default_color {
        meta.default_color = Some(c);
    }
    if !args.tags.is_empty() {
        meta.tags = args.tags.clone();
    }
    if meta.id.is_empty() {
        return Err(anyhow!("--id is required (no id in sidecar)"));
    }
    if meta.display_name.is_empty() {
        meta.display_name = meta.id.clone();
    }
    if meta.compatible_skeleton.is_none()
        && matches!(
            meta.category,
            AssetCategory::Top
                | AssetCategory::Bottom
                | AssetCategory::Shoes
                | AssetCategory::Hat
                | AssetCategory::Hair
                | AssetCategory::Glasses
                | AssetCategory::Accessory
                | AssetCategory::Head
        )
    {
        meta.compatible_skeleton = Some(animation::Skeleton::AVATAR_SKELETON_V1.to_string());
    }

    paths::validate_asset_id(&meta.id)?;

    let category_dir = paths::category_dir(meta.category).ok_or_else(|| {
        anyhow!(
            "category {:?} is not importable via Phase 12",
            meta.category
        )
    })?;
    let model_rel = paths::relative_model_path(meta.category, &meta.id)
        .expect("category_dir succeeded so model rel must be Some");
    meta.model = model_rel.clone();

    let report = validate_glb(&input)?;
    enforce_category_rules(&report, meta.category, false)?;

    let db_path = paths::catalog_db_path(&workspace);
    let mut catalog = Catalog::open(&db_path)?;
    if !args.force {
        if let Some(existing) = catalog.find(&meta.id)? {
            return Err(anyhow!(
                "asset id '{}' already exists (model={}); pass --force to overwrite",
                existing.id,
                existing.model
            ));
        }
    }

    let model_path = workspace
        .join("assets/processed/avatars")
        .join(category_dir)
        .join(format!("{}.glb", meta.id));
    if let Some(parent) = model_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    fs::copy(&input, &model_path)
        .with_context(|| format!("copy {} -> {}", input.display(), model_path.display()))?;

    if args.thumb {
        let thumb_abs = paths::thumbnail_path(&workspace, &meta.id);
        crate::thumbnail::render_thumbnail(
            &model_path,
            args.thumb_width.max(16),
            args.thumb_height.max(16),
            &thumb_abs,
        )?;
        meta.thumbnail = Some(paths::relative_thumbnail_path(&meta.id));
    }

    let meta_path = paths::metadata_path(&workspace, &meta.id);
    if let Some(parent) = meta_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&meta)?;
    fs::write(&meta_path, json)
        .with_context(|| format!("write metadata {}", meta_path.display()))?;

    catalog.upsert_many(std::slice::from_ref(&meta))?;

    info!(
        id = %meta.id,
        category = ?meta.category,
        model = %meta.model,
        thumb = meta.thumbnail.is_some(),
        "asset imported"
    );

    let thumbnail_path = if args.thumb {
        Some(paths::thumbnail_path(&workspace, &meta.id))
    } else {
        None
    };
    Ok(ImportReport {
        id: meta.id,
        model_path,
        metadata_path: meta_path,
        thumbnail_path,
    })
}

fn canonicalise(p: &Path) -> Result<PathBuf> {
    p.canonicalize()
        .with_context(|| format!("canonicalize {}", p.display()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use assets::metadata::AssetCategory;

    use super::*;

    fn workspace_root_for_tests() -> PathBuf {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        PathBuf::from(manifest)
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_workspace_with_input(input_src: &Path) -> (PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let id_inc = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "asset_builder_import_{}_{}_{}",
            std::process::id(),
            unique,
            id_inc
        ));
        std::fs::create_dir_all(&root).unwrap();
        let input_dst = root.join("input.glb");
        std::fs::copy(input_src, &input_dst).unwrap();
        (root, input_dst)
    }

    #[test]
    fn import_round_trips_a_body() {
        let workspace_src = workspace_root_for_tests();
        let phase4 = workspace_src.join("assets/processed/avatars/bodies/phase4_rig.glb");
        let (root, input) = temp_workspace_with_input(&phase4);

        let args = ImportArgs {
            input,
            workspace_root: root.clone(),
            id: Some("body_imported_test_001".into()),
            category: Some(AssetCategory::Body),
            display_name: Some("Imported Test Body".into()),
            ..Default::default()
        };

        let report = run_import(&args).expect("import succeeds");

        // GLB copied
        assert!(report.model_path.exists());
        assert!(report
            .model_path
            .ends_with("assets/processed/avatars/bodies/body_imported_test_001.glb"));

        // Metadata JSON written
        assert!(report.metadata_path.exists());
        let text = std::fs::read_to_string(&report.metadata_path).unwrap();
        let parsed: AssetMeta = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.id, "body_imported_test_001");
        assert_eq!(parsed.display_name, "Imported Test Body");
        assert!(matches!(parsed.category, AssetCategory::Body));
        assert_eq!(parsed.model, "avatars/bodies/body_imported_test_001.glb");

        // DB row present
        let db_path = paths::catalog_db_path(&root);
        let catalog = Catalog::open(&db_path).unwrap();
        let row = catalog
            .find("body_imported_test_001")
            .unwrap()
            .expect("row exists");
        assert_eq!(row.model, "avatars/bodies/body_imported_test_001.glb");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn import_with_sidecar_then_flag_override() {
        let workspace_src = workspace_root_for_tests();
        let phase4 = workspace_src.join("assets/processed/avatars/bodies/phase4_rig.glb");
        let (root, input) = temp_workspace_with_input(&phase4);

        let sidecar = AssetMeta {
            id: "body_sidecar_test_001".into(),
            display_name: "Sidecar Name".into(),
            category: AssetCategory::Body,
            model: String::new(),
            thumbnail: None,
            supports_color: false,
            default_color: None,
            compatible_body_types: Vec::new(),
            compatible_skeleton: None,
            tags: vec!["from_sidecar".into()],
            version: 1,
        };
        let sidecar_path = root.join("sidecar.json");
        std::fs::write(&sidecar_path, serde_json::to_string(&sidecar).unwrap()).unwrap();

        let args = ImportArgs {
            input,
            workspace_root: root.clone(),
            display_name: Some("Flag Wins".into()),
            meta_path: Some(sidecar_path),
            ..Default::default()
        };

        let report = run_import(&args).expect("import succeeds");
        let text = std::fs::read_to_string(&report.metadata_path).unwrap();
        let parsed: AssetMeta = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.id, "body_sidecar_test_001");
        assert_eq!(parsed.display_name, "Flag Wins");
        assert_eq!(parsed.tags, vec!["from_sidecar".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }
}

//! Avatar Asset Builder — internal CLI.
//!
//! Phase 7 ships `gen-fixture-top`: procedural skinned wearable for the rig.
//! Phase 17 ships `gen-fixture-pack`: category coverage sample assets.
//! Phase 12 adds `import` / `validate` / `thumbnail` / `list`.

mod fixtures;
mod glb_writer;
mod icon_gen;
mod import;
mod list;
mod paths;
mod thumbnail;
mod validate;

use std::path::PathBuf;

use anyhow::anyhow;
use assets::metadata::AssetCategory;
use assets::Catalog;
use clap::{Parser, Subcommand};

use import::{run_import, ImportArgs};
use list::run_list;
use thumbnail::render_thumbnail;
use validate::run_validate;

#[derive(Parser)]
#[command(
    name = "asset_builder",
    about = "Avatar Studio asset import/validation tool"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Show planned work.
    Plan,
    /// Generate the Phase 7 top fixture GLB + metadata.
    GenFixtureTop {
        /// Override the workspace root. Defaults to the CWD.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Generate the Phase 17 multi-category sample fixture pack.
    GenFixturePack {
        /// Override the workspace root. Defaults to the CWD.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// (Re)generate the Avatar Studio app icon at `assets/icon/avatar_studio.ico`.
    GenIcon {
        /// Override the output path.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Override the workspace root (defaults to CWD).
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Import a GLB into `assets/processed/`, write metadata, optionally
    /// render a thumbnail, and upsert the SQLite catalog row.
    Import {
        /// Path to the input GLB.
        input: PathBuf,
        /// Stable asset id (lowercase alphanumeric + underscore).
        #[arg(long)]
        id: Option<String>,
        /// Asset category: body, top, bottom, shoes, hat, hair, glasses, accessory, head.
        #[arg(long)]
        category: Option<String>,
        /// Human-readable display name.
        #[arg(long = "display-name")]
        display_name: Option<String>,
        /// Compatible body id (repeatable; wearables only).
        #[arg(long = "body-type")]
        body_type: Vec<String>,
        /// Compatible skeleton id. Default for wearables is `avatar_skeleton_v1`.
        /// Pass `null` to clear.
        #[arg(long)]
        skeleton: Option<String>,
        /// Mark as user-colourable.
        #[arg(long = "supports-color")]
        supports_color: bool,
        /// Default color `r,g,b` in sRGB 0..1.
        #[arg(long = "default-color")]
        default_color: Option<String>,
        /// Comma-separated tag list.
        #[arg(long)]
        tags: Option<String>,
        /// Path to a sidecar JSON to read as the starting metadata. Flags override.
        #[arg(long)]
        meta: Option<PathBuf>,
        /// Also render a thumbnail PNG.
        #[arg(long)]
        thumb: bool,
        /// Thumbnail width in pixels.
        #[arg(long = "thumb-width", default_value_t = 256)]
        thumb_width: u32,
        /// Thumbnail height in pixels.
        #[arg(long = "thumb-height", default_value_t = 256)]
        thumb_height: u32,
        /// Workspace root (defaults to CWD).
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Overwrite an existing asset id.
        #[arg(long)]
        force: bool,
    },
    /// Read-only GLB inspection. Optionally enforce category rules.
    Validate {
        input: PathBuf,
        #[arg(long)]
        category: Option<String>,
        #[arg(long = "require-skeleton")]
        require_skeleton: bool,
    },
    /// Render a thumbnail PNG for an asset already in the catalog.
    Thumbnail {
        id: String,
        #[arg(long, default_value_t = 256)]
        width: u32,
        #[arg(long, default_value_t = 256)]
        height: u32,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// List catalog rows. Optional category filter.
    List {
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();
    match cli.cmd {
        None | Some(Cmd::Plan) => {
            println!("asset_builder");
            println!("subcommands:");
            println!("  gen-fixture-top   write phase7_top.glb + phase7_top.json");
            println!("  gen-fixture-pack  write phase17 sample assets across categories");
            println!("  import <glb>      copy + write metadata + upsert DB row");
            println!("  validate <glb>    parse + report mesh/skeleton stats");
            println!("  thumbnail <id>    re-render the thumbnail for an existing asset");
            println!("  list              show catalog rows");
        }
        Some(Cmd::GenFixtureTop { workspace }) => {
            let root = resolve_workspace(workspace);
            fixtures::top::generate(&root)?;
        }
        Some(Cmd::GenFixturePack { workspace }) => {
            let root = resolve_workspace(workspace);
            fixtures::pack::generate(&root)?;
            println!("phase17 fixture pack written");
        }
        Some(Cmd::GenIcon { out, workspace }) => {
            let root = resolve_workspace(workspace);
            let out_path = out.unwrap_or_else(|| root.join("assets/icon/avatar_studio.ico"));
            icon_gen::generate(&out_path)?;
            println!("wrote {}", out_path.display());
        }
        Some(Cmd::Import {
            input,
            id,
            category,
            display_name,
            body_type,
            skeleton,
            supports_color,
            default_color,
            tags,
            meta,
            thumb,
            thumb_width,
            thumb_height,
            workspace,
            force,
        }) => {
            let root = resolve_workspace(workspace);
            let args = ImportArgs {
                input,
                workspace_root: root,
                id,
                category: category.as_deref().map(parse_category).transpose()?,
                display_name,
                body_types: body_type,
                skeleton,
                supports_color,
                default_color: default_color.as_deref().map(parse_color).transpose()?,
                tags: tags
                    .map(|s| {
                        s.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                meta_path: meta,
                thumb,
                thumb_width,
                thumb_height,
                force,
            };
            let report = run_import(&args)?;
            println!(
                "imported id={} model={} metadata={} thumb={}",
                report.id,
                report.model_path.display(),
                report.metadata_path.display(),
                report
                    .thumbnail_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<none>".into()),
            );
        }
        Some(Cmd::Validate {
            input,
            category,
            require_skeleton,
        }) => {
            let cat = category.as_deref().map(parse_category).transpose()?;
            run_validate(&input, cat, require_skeleton)?;
        }
        Some(Cmd::Thumbnail {
            id,
            width,
            height,
            workspace,
        }) => {
            let root = resolve_workspace(workspace);
            let db_path = paths::catalog_db_path(&root);
            let mut catalog = Catalog::open(&db_path)?;
            let mut meta = catalog
                .find(&id)?
                .ok_or_else(|| anyhow!("asset id '{id}' not found in catalog"))?;
            let model_abs = root.join("assets/processed").join(&meta.model);
            let thumb_abs = paths::thumbnail_path(&root, &meta.id);
            render_thumbnail(&model_abs, width, height, &thumb_abs)?;
            meta.thumbnail = Some(paths::relative_thumbnail_path(&meta.id));
            // Update sidecar and DB.
            let meta_path = paths::metadata_path(&root, &meta.id);
            if meta_path.exists() {
                let json = serde_json::to_string_pretty(&meta)?;
                std::fs::write(&meta_path, json)?;
            }
            catalog.upsert_many(std::slice::from_ref(&meta))?;
            println!("thumbnail id={} written={}", meta.id, thumb_abs.display(),);
        }
        Some(Cmd::List {
            category,
            workspace,
        }) => {
            let root = resolve_workspace(workspace);
            let cat = category.as_deref().map(parse_category).transpose()?;
            run_list(&root, cat)?;
        }
    }
    Ok(())
}

fn resolve_workspace(override_path: Option<PathBuf>) -> PathBuf {
    override_path.unwrap_or_else(|| std::env::current_dir().unwrap())
}

fn parse_category(s: &str) -> anyhow::Result<AssetCategory> {
    Ok(match s {
        "body" => AssetCategory::Body,
        "head" => AssetCategory::Head,
        "hair" => AssetCategory::Hair,
        "top" => AssetCategory::Top,
        "bottom" => AssetCategory::Bottom,
        "shoes" => AssetCategory::Shoes,
        "hat" => AssetCategory::Hat,
        "glasses" => AssetCategory::Glasses,
        "accessory" => AssetCategory::Accessory,
        "animation" => AssetCategory::Animation,
        "pose" => AssetCategory::Pose,
        "background" => AssetCategory::Background,
        "material" => AssetCategory::Material,
        other => anyhow::bail!("unknown category: {other}"),
    })
}

fn parse_color(s: &str) -> anyhow::Result<[f32; 3]> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        anyhow::bail!("--default-color must be 'r,g,b'; got {s:?}");
    }
    let r: f32 = parts[0].parse()?;
    let g: f32 = parts[1].parse()?;
    let b: f32 = parts[2].parse()?;
    Ok([r, g, b])
}

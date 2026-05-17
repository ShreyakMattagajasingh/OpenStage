//! GLB validation shared by `validate` and `import` subcommands.
//!
//! Read-only: parses the GLB, computes mesh AABB, and (if a skin is present)
//! runs `Skeleton::from_gltf` to enforce the `avatar_skeleton_v1` convention.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use assets::metadata::AssetCategory;

#[derive(Debug, Clone)]
pub struct ValidateReport {
    pub path: PathBuf,
    pub file_bytes: u64,
    pub mesh_count: u32,
    pub primitive_count: u32,
    pub vertex_count: u32,
    pub animation_count: u32,
    /// `(min, max)` AABB in glTF coordinates. `None` when the file has no
    /// geometry (very rare).
    pub aabb: Option<([f32; 3], [f32; 3])>,
    pub longest_axis_m: f32,
    /// True iff the file contains a glTF skin.
    pub has_skin: bool,
    /// Skeleton name when a skin is present and root node is named.
    pub skeleton_root: Option<String>,
    pub bone_names: Vec<String>,
    /// Captured skeleton parse error (only meaningful when `has_skin` is true).
    pub skeleton_error: Option<String>,
    /// Warnings (mesh AABB out of sensible range, etc.).
    pub warnings: Vec<String>,
}

impl ValidateReport {
    pub fn print(&self) {
        println!("file       {}", self.path.display());
        println!("size       {} bytes", self.file_bytes);
        println!(
            "meshes     {} ({} primitives, {} vertices total)",
            self.mesh_count, self.primitive_count, self.vertex_count
        );
        println!("animations {}", self.animation_count);
        if let Some((mn, mx)) = self.aabb {
            println!(
                "aabb       min=[{:.3}, {:.3}, {:.3}] max=[{:.3}, {:.3}, {:.3}] longest={:.3} m",
                mn[0], mn[1], mn[2], mx[0], mx[1], mx[2], self.longest_axis_m
            );
        } else {
            println!("aabb       <no geometry>");
        }
        if self.has_skin {
            match (&self.skeleton_root, &self.skeleton_error) {
                (_, Some(err)) => println!("skeleton   ERROR — {err}"),
                (Some(name), None) => println!(
                    "skeleton   root={name} bones={} ({})",
                    self.bone_names.len(),
                    summarise_bones(&self.bone_names)
                ),
                (None, None) => println!(
                    "skeleton   <unnamed root> bones={} ({})",
                    self.bone_names.len(),
                    summarise_bones(&self.bone_names)
                ),
            }
        } else {
            println!("skeleton   <no skin>");
        }
        for w in &self.warnings {
            println!("warn       {w}");
        }
    }
}

fn summarise_bones(names: &[String]) -> String {
    if names.len() <= 6 {
        names.join(", ")
    } else {
        let head: Vec<_> = names.iter().take(5).cloned().collect();
        format!("{}, … +{}", head.join(", "), names.len() - 5)
    }
}

pub fn validate_glb(path: &Path) -> Result<ValidateReport> {
    let file_bytes = std::fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len();
    let (document, buffers, _images) =
        gltf::import(path).with_context(|| format!("gltf::import {}", path.display()))?;

    let mut mesh_count: u32 = 0;
    let mut primitive_count: u32 = 0;
    let mut vertex_count: u32 = 0;
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];

    for mesh in document.meshes() {
        mesh_count += 1;
        for prim in mesh.primitives() {
            primitive_count += 1;
            let reader = prim.reader(|buf| Some(&buffers[buf.index()]));
            if let Some(positions) = reader.read_positions() {
                for p in positions {
                    vertex_count += 1;
                    for i in 0..3 {
                        if p[i] < mn[i] {
                            mn[i] = p[i];
                        }
                        if p[i] > mx[i] {
                            mx[i] = p[i];
                        }
                    }
                }
            }
        }
    }

    let aabb = if mn[0].is_finite() && mx[0].is_finite() {
        Some((mn, mx))
    } else {
        None
    };
    let longest_axis_m = aabb
        .map(|(mn, mx)| {
            (mx[0] - mn[0])
                .max(mx[1] - mn[1])
                .max(mx[2] - mn[2])
                .max(0.0)
        })
        .unwrap_or(0.0);

    let mut warnings = Vec::new();
    if let Some((_, _)) = aabb {
        if !(0.05..=5.0).contains(&longest_axis_m) {
            warnings.push(format!(
                "longest axis {longest_axis_m:.3} m is outside sensible range [0.05, 5.0]"
            ));
        }
    }

    let animation_count = document.animations().count() as u32;

    let has_skin = document.skins().next().is_some();
    let mut skeleton_root = None;
    let mut bone_names = Vec::new();
    let mut skeleton_error = None;
    if has_skin {
        if let Some(skin) = document.skins().next() {
            if let Some(root) = skin.skeleton() {
                if let Some(name) = root.name() {
                    skeleton_root = Some(name.to_string());
                }
            }
        }
        match animation::Skeleton::from_gltf(&document, &buffers) {
            Ok(Some(skel)) => {
                bone_names = skel.bones.iter().map(|b| b.name.clone()).collect();
                for w in &skel.warnings {
                    warnings.push(format!("skeleton: {w}"));
                }
            }
            Ok(None) => {
                // has_skin was true but parser said no — keep going.
            }
            Err(e) => {
                skeleton_error = Some(e.to_string());
            }
        }
    }

    Ok(ValidateReport {
        path: path.to_path_buf(),
        file_bytes,
        mesh_count,
        primitive_count,
        vertex_count,
        animation_count,
        aabb,
        longest_axis_m,
        has_skin,
        skeleton_root,
        bone_names,
        skeleton_error,
        warnings,
    })
}

/// Apply category-level rules. Returns Err if the GLB does not match.
pub fn enforce_category_rules(
    report: &ValidateReport,
    category: AssetCategory,
    require_skeleton: bool,
) -> Result<()> {
    if let Some(err) = &report.skeleton_error {
        return Err(anyhow!("skeleton parse failed: {err}"));
    }
    let is_wearable = matches!(
        category,
        AssetCategory::Top
            | AssetCategory::Bottom
            | AssetCategory::Shoes
            | AssetCategory::Hat
            | AssetCategory::Hair
            | AssetCategory::Glasses
            | AssetCategory::Accessory
            | AssetCategory::Head
    );
    if is_wearable {
        if !report.has_skin {
            return Err(anyhow!(
                "wearable category {category:?} requires a skinned mesh, but no skin is present"
            ));
        }
        match report.skeleton_root.as_deref() {
            Some(animation::Skeleton::AVATAR_SKELETON_V1) | None => {}
            Some(other) => {
                return Err(anyhow!(
                    "wearable's skeleton root is '{other}', expected '{}'",
                    animation::Skeleton::AVATAR_SKELETON_V1
                ));
            }
        }
    } else if matches!(category, AssetCategory::Body) && require_skeleton && !report.has_skin {
        return Err(anyhow!(
            "--require-skeleton: body has no skin (use a rigged GLB)"
        ));
    }
    if report.mesh_count == 0 {
        return Err(anyhow!("no meshes in GLB"));
    }
    Ok(())
}

pub fn run_validate(
    glb_path: &Path,
    category: Option<AssetCategory>,
    require_skeleton: bool,
) -> Result<()> {
    let report = validate_glb(glb_path)?;
    report.print();
    if let Some(cat) = category {
        enforce_category_rules(&report, cat, require_skeleton)?;
    } else if require_skeleton && !report.has_skin {
        return Err(anyhow!("--require-skeleton: no skin in GLB"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn workspace_root_for_tests() -> PathBuf {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        PathBuf::from(manifest)
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn validate_rejects_broken_glb() {
        let tmp = std::env::temp_dir().join(format!(
            "asset_builder_validate_broken_{}.glb",
            std::process::id()
        ));
        std::fs::write(&tmp, b"not a real glb").unwrap();
        let err = validate_glb(&tmp).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("gltf") || msg.contains("Glb") || msg.contains("magic"),
            "expected GLB parse error, got: {msg}"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn validate_accepts_skeleton() {
        let workspace = workspace_root_for_tests();
        let glb = workspace.join("assets/processed/avatars/bodies/phase4_rig.glb");
        let report = validate_glb(&glb).expect("validate phase4_rig");
        assert!(report.has_skin, "phase4_rig must have a skin");
        assert!(report.mesh_count >= 1);
        assert!(
            report.bone_names.len() >= 18,
            "phase4_rig must have at least 18 bones, got {}",
            report.bone_names.len()
        );
        assert!(
            report.skeleton_error.is_none(),
            "{:?}",
            report.skeleton_error
        );
        // Root name may be unset on the skin definition itself; what we do
        // require is that the parser accepted the skeleton.
    }
}

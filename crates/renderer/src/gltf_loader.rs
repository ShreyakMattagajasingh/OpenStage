//! Minimal GLB loader.
//!
//! Phase 4 scope: static mesh data plus optional `avatar_skeleton_v1` parsing.
//! Skin weights, animation channels, and per-primitive material sets remain
//! later phases.

use std::path::Path;

use std::collections::HashMap;

use animation::{
    AnimationChannel, AnimationClip, BoneIndex, Interpolation, Keyframe, Skeleton, TransformTrack,
};
use anyhow::{anyhow, bail, Context, Result};
use glam::{Quat, Vec3};
use gltf::animation::util::ReadOutputs;
use image::{DynamicImage, RgbImage, RgbaImage};
use tracing::{debug, info, warn};

use crate::mesh::{Mesh, Vertex};

pub struct LoadedGlb {
    pub mesh: Mesh,
    pub base_color_image: Option<DynamicImage>,
    pub skeleton: Option<Skeleton>,
    pub is_skinned: bool,
    pub animation_clips: Vec<AnimationClip>,
    pub warnings: Vec<String>,
}

/// Load a glTF/GLB file. Returns the merged triangle mesh, optional first
/// base-color texture image, optional skeleton, and load warnings.
pub fn load_glb(device: &wgpu::Device, path: &Path) -> Result<LoadedGlb> {
    let (document, buffers, images) =
        gltf::import(path).with_context(|| format!("import {}", path.display()))?;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut primitive_count = 0usize;
    let mut base_color_image: Option<DynamicImage> = None;
    let mut warnings = Vec::new();
    let skeleton = Skeleton::from_gltf(&document, &buffers)
        .with_context(|| format!("parse skeleton in {}", path.display()))?;
    if let Some(skeleton) = skeleton.as_ref() {
        warnings.extend(skeleton.warnings.iter().cloned());
    }
    let joint_count = skeleton.as_ref().map(|s| s.bones.len()).unwrap_or(0);
    let mut mesh_skinning: Option<bool> = None;
    let mut normalized_any_weights = false;

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                let msg = format!("skipping non-triangle primitive: {:?}", primitive.mode());
                warn!(mode = ?primitive.mode(), "{msg}");
                warnings.push(msg);
                continue;
            }

            let reader = primitive.reader(|b| Some(&buffers[b.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| anyhow!("primitive has no POSITION"))?
                .collect();
            let normals: Vec<[f32; 3]> = match reader.read_normals() {
                Some(it) => it.collect(),
                None => {
                    bail!("primitive has no NORMAL; re-export the GLB with vertex normals enabled")
                }
            };

            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
            let raw_joints = reader
                .read_joints(0)
                .map(|j| j.into_u16().map(|v| v.map(u32::from)).collect::<Vec<_>>());
            let raw_weights = reader
                .read_weights(0)
                .map(|w| w.into_f32().collect::<Vec<_>>());

            let primitive_is_skinned = match (raw_joints.as_ref(), raw_weights.as_ref()) {
                (Some(_), Some(_)) => true,
                (None, None) => false,
                _ => bail!("primitive has only one of JOINTS_0 / WEIGHTS_0"),
            };
            if let Some(previous) = mesh_skinning {
                if previous != primitive_is_skinned {
                    bail!("merged GLB meshes must not mix skinned and static primitives");
                }
            } else {
                mesh_skinning = Some(primitive_is_skinned);
            }
            if primitive_is_skinned && skeleton.is_none() {
                bail!("primitive has skin attributes but the GLB has no valid skeleton");
            }

            if positions.len() != normals.len() || positions.len() != uvs.len() {
                bail!(
                    "attribute count mismatch: pos={} norm={} uv={}",
                    positions.len(),
                    normals.len(),
                    uvs.len()
                );
            }

            let (joints, weights) = if primitive_is_skinned {
                let joints = raw_joints.unwrap();
                let weights = raw_weights.unwrap();
                if joints.len() != positions.len() || weights.len() != positions.len() {
                    bail!(
                        "skin attribute count mismatch: pos={} joints={} weights={}",
                        positions.len(),
                        joints.len(),
                        weights.len()
                    );
                }
                validate_joint_indices(&joints, joint_count)?;
                let mut normalized = Vec::with_capacity(weights.len());
                for weights in weights {
                    let (w, changed) = normalize_skin_weights(weights)
                        .ok_or_else(|| anyhow!("skinned vertex has zero total weight"))?;
                    normalized_any_weights |= changed;
                    normalized.push(w);
                }
                (joints, normalized)
            } else {
                (
                    vec![[0, 0, 0, 0]; positions.len()],
                    vec![[1.0, 0.0, 0.0, 0.0]; positions.len()],
                )
            };

            let vbase = vertices.len() as u32;
            vertices.extend(
                positions
                    .iter()
                    .zip(normals.iter())
                    .zip(uvs.iter())
                    .zip(joints.iter())
                    .zip(weights.iter())
                    .map(|((((p, n), uv), joints), weights)| {
                        Vertex::skinned(*p, *n, *uv, *joints, *weights)
                    }),
            );

            match reader.read_indices() {
                Some(idx) => indices.extend(idx.into_u32().map(|i| i + vbase)),
                None => indices.extend(vbase..vbase + positions.len() as u32),
            }

            if base_color_image.is_none() {
                if let Some(info) = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_texture()
                {
                    let img_idx = info.texture().source().index();
                    if let Some(data) = images.get(img_idx) {
                        match gltf_image_to_dynamic(data) {
                            Some(d) => {
                                debug!(
                                    image_index = img_idx,
                                    width = data.width,
                                    height = data.height,
                                    format = ?data.format,
                                    "base color texture extracted"
                                );
                                base_color_image = Some(d);
                            }
                            None => {
                                let msg = format!(
                                    "unsupported texture pixel format {:?}; rendering untextured",
                                    data.format
                                );
                                warn!(format = ?data.format, "{msg}");
                                warnings.push(msg);
                            }
                        }
                    }
                }
            }
            primitive_count += 1;
        }
    }

    if vertices.is_empty() {
        bail!("GLB contains no triangle geometry: {}", path.display());
    }
    let is_skinned = mesh_skinning.unwrap_or(false);
    if skeleton.is_some() && !is_skinned {
        bail!("GLB has a skeleton but no JOINTS_0 / WEIGHTS_0 skin attributes");
    }
    if normalized_any_weights {
        warnings.push("one or more skin weights were normalized".to_string());
    }
    let animation_clips = if let Some(skeleton) = skeleton.as_ref() {
        parse_animation_clips(&document, &buffers, skeleton, &mut warnings)?
    } else {
        Vec::new()
    };

    let label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("glb")
        .to_string();
    info!(
        path = %path.display(),
        vertices = vertices.len(),
        triangles = indices.len() / 3,
        primitives = primitive_count,
        has_texture = base_color_image.is_some(),
        has_skeleton = skeleton.is_some(),
        is_skinned,
        clips = animation_clips.len(),
        "loaded GLB"
    );

    Ok(LoadedGlb {
        mesh: Mesh::from_data(device, &label, &vertices, &indices, is_skinned),
        base_color_image,
        skeleton,
        is_skinned,
        animation_clips,
        warnings,
    })
}

fn parse_animation_clips(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    skeleton: &Skeleton,
    warnings: &mut Vec<String>,
) -> Result<Vec<AnimationClip>> {
    let bone_by_name: HashMap<&str, BoneIndex> = skeleton
        .bones
        .iter()
        .enumerate()
        .map(|(idx, bone)| (bone.name.as_str(), BoneIndex(idx)))
        .collect();

    let mut clips = Vec::new();
    for (anim_idx, animation) in document.animations().enumerate() {
        let mut by_bone: HashMap<BoneIndex, AnimationChannel> = HashMap::new();
        let mut duration = 0.0_f32;
        let clip_name = animation
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("clip_{anim_idx}"));

        for channel in animation.channels() {
            let target = channel.target();
            let Some(node_name) = target.node().name() else {
                warnings.push(format!(
                    "animation '{clip_name}' targets unnamed node; skipping"
                ));
                continue;
            };
            let Some(&bone) = bone_by_name.get(node_name) else {
                warnings.push(format!(
                    "animation '{clip_name}' targets non-skeleton node '{node_name}'; skipping"
                ));
                continue;
            };
            let interpolation = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::CubicSpline => {
                    bail!("animation '{clip_name}' uses unsupported CUBICSPLINE interpolation");
                }
            };
            let reader = channel.reader(|b| Some(&buffers[b.index()]));
            let times: Vec<f32> = reader
                .read_inputs()
                .ok_or_else(|| anyhow!("animation '{clip_name}' channel has no input times"))?
                .collect();
            if times.is_empty() {
                warnings.push(format!(
                    "animation '{clip_name}' has an empty channel; skipping"
                ));
                continue;
            }
            duration = duration.max(*times.last().unwrap());
            let entry = by_bone.entry(bone).or_insert_with(|| AnimationChannel {
                bone,
                translation: None,
                rotation: None,
                scale: None,
            });

            match reader
                .read_outputs()
                .ok_or_else(|| anyhow!("animation '{clip_name}' channel has no outputs"))?
            {
                ReadOutputs::Translations(values) => {
                    let values: Vec<Vec3> = values.map(Vec3::from).collect();
                    ensure_key_count(&clip_name, "translation", times.len(), values.len())?;
                    entry.translation = Some(TransformTrack {
                        bone,
                        interpolation,
                        keys: times
                            .iter()
                            .copied()
                            .zip(values)
                            .map(|(time, value)| Keyframe { time, value })
                            .collect(),
                    });
                }
                ReadOutputs::Rotations(values) => {
                    let values: Vec<Quat> = values
                        .into_f32()
                        .map(|q| Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize())
                        .collect();
                    ensure_key_count(&clip_name, "rotation", times.len(), values.len())?;
                    entry.rotation = Some(TransformTrack {
                        bone,
                        interpolation,
                        keys: times
                            .iter()
                            .copied()
                            .zip(values)
                            .map(|(time, value)| Keyframe { time, value })
                            .collect(),
                    });
                }
                ReadOutputs::Scales(values) => {
                    let values: Vec<Vec3> = values.map(Vec3::from).collect();
                    ensure_key_count(&clip_name, "scale", times.len(), values.len())?;
                    entry.scale = Some(TransformTrack {
                        bone,
                        interpolation,
                        keys: times
                            .iter()
                            .copied()
                            .zip(values)
                            .map(|(time, value)| Keyframe { time, value })
                            .collect(),
                    });
                }
                ReadOutputs::MorphTargetWeights(_) => {
                    warnings.push(format!(
                        "animation '{clip_name}' contains morph target weights; skipping channel"
                    ));
                }
            }
        }

        let mut channels: Vec<AnimationChannel> = by_bone.into_values().collect();
        channels.sort_by_key(|c| c.bone.0);
        if !channels.is_empty() {
            clips.push(AnimationClip {
                name: clip_name,
                duration,
                channels,
            });
        }
    }

    Ok(clips)
}

fn ensure_key_count(clip: &str, property: &str, inputs: usize, outputs: usize) -> Result<()> {
    if inputs != outputs {
        bail!(
            "animation '{clip}' {property} channel has {inputs} input keys but {outputs} outputs"
        );
    }
    Ok(())
}

pub fn normalize_skin_weights(weights: [f32; 4]) -> Option<([f32; 4], bool)> {
    let sum = weights.iter().sum::<f32>();
    if sum <= 1e-6 {
        return None;
    }
    let changed = (sum - 1.0).abs() > 1e-3;
    if changed {
        Some((
            [
                weights[0] / sum,
                weights[1] / sum,
                weights[2] / sum,
                weights[3] / sum,
            ],
            true,
        ))
    } else {
        Some((weights, false))
    }
}

fn validate_joint_indices(joints: &[[u32; 4]], joint_count: usize) -> Result<()> {
    for joint in joints.iter().flatten() {
        if *joint as usize >= joint_count {
            bail!(
                "JOINTS_0 index {} is outside skeleton joint count {}",
                joint,
                joint_count
            );
        }
    }
    Ok(())
}

fn gltf_image_to_dynamic(data: &gltf::image::Data) -> Option<DynamicImage> {
    match data.format {
        gltf::image::Format::R8G8B8 => {
            RgbImage::from_raw(data.width, data.height, data.pixels.clone())
                .map(DynamicImage::ImageRgb8)
        }
        gltf::image::Format::R8G8B8A8 => {
            RgbaImage::from_raw(data.width, data.height, data.pixels.clone())
                .map(DynamicImage::ImageRgba8)
        }
        gltf::image::Format::R8 => {
            let mut rgb = Vec::with_capacity(data.pixels.len() * 3);
            for &g in &data.pixels {
                rgb.extend_from_slice(&[g, g, g]);
            }
            RgbImage::from_raw(data.width, data.height, rgb).map(DynamicImage::ImageRgb8)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_skin_weights, validate_joint_indices};

    #[test]
    fn normalizes_non_unit_weights() {
        let (weights, changed) = normalize_skin_weights([2.0, 1.0, 1.0, 0.0]).unwrap();
        assert!(changed);
        assert_eq!(weights, [0.5, 0.25, 0.25, 0.0]);
    }

    #[test]
    fn rejects_zero_weight_sum() {
        assert!(normalize_skin_weights([0.0, 0.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn rejects_out_of_range_joint_indices() {
        let err = validate_joint_indices(&[[0, 1, 99, 0]], 18).unwrap_err();
        assert!(err.to_string().contains("outside skeleton joint count"));
    }

    #[test]
    fn phase4_rig_fixture_has_skin_attributes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/processed/avatars/bodies/phase4_rig.glb");
        let (document, buffers, _) = gltf::import(&path).unwrap();
        let primitive = document
            .meshes()
            .next()
            .unwrap()
            .primitives()
            .next()
            .unwrap();
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
        assert!(reader.read_joints(0).is_some());
        assert!(reader.read_weights(0).is_some());
    }

    #[test]
    fn phase4_rig_fixture_has_idle_animation() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/processed/avatars/bodies/phase4_rig.glb");
        let (document, buffers, _) = gltf::import(&path).unwrap();
        let skeleton = animation::Skeleton::from_gltf(&document, &buffers)
            .unwrap()
            .unwrap();
        let clips =
            super::parse_animation_clips(&document, &buffers, &skeleton, &mut Vec::new()).unwrap();
        assert!(clips.iter().any(|clip| clip.name == "idle"));
        let idle = clips.iter().find(|clip| clip.name == "idle").unwrap();
        assert!((idle.duration - 2.0).abs() < 0.01);
    }

    #[test]
    fn duck_fixture_has_no_skin_attributes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/processed/avatars/bodies/duck.glb");
        let (document, buffers, _) = gltf::import(&path).unwrap();
        let primitive = document
            .meshes()
            .next()
            .unwrap()
            .primitives()
            .next()
            .unwrap();
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
        assert!(reader.read_joints(0).is_none());
        assert!(reader.read_weights(0).is_none());
    }
}

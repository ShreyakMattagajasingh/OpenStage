//! `avatar_skeleton_v1` parsing and validation.

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};
use thiserror::Error;

use crate::Pose;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoneIndex(pub usize);

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: Option<BoneIndex>,
    pub local_bind_transform: Mat4,
    pub inverse_bind_matrix: Mat4,
    pub world_bind_transform: Mat4,
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub name: String,
    pub bones: Vec<Bone>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum SkeletonParseError {
    #[error("skeleton root is named '{found}', expected '{expected}'")]
    WrongSkeletonRoot {
        found: String,
        expected: &'static str,
    },
    #[error("skin has a joint without a node name")]
    UnnamedJoint,
    #[error("missing required bones: {0}")]
    MissingRequiredBones(String),
}

impl Skeleton {
    pub const AVATAR_SKELETON_V1: &'static str = "avatar_skeleton_v1";
    pub const MVP_BONE_NAMES: &'static [&'static str] = &[
        "root",
        "hips",
        "spine",
        "chest",
        "neck",
        "head",
        "upperarm_l",
        "lowerarm_l",
        "hand_l",
        "upperarm_r",
        "lowerarm_r",
        "hand_r",
        "upperleg_l",
        "lowerleg_l",
        "foot_l",
        "upperleg_r",
        "lowerleg_r",
        "foot_r",
    ];

    pub fn from_gltf(
        document: &gltf::Document,
        buffers: &[gltf::buffer::Data],
    ) -> Result<Option<Self>, SkeletonParseError> {
        let Some(skin) = document.skins().next() else {
            return Ok(None);
        };

        if let Some(root) = skin.skeleton() {
            if let Some(name) = root.name() {
                if name != Self::AVATAR_SKELETON_V1 {
                    return Err(SkeletonParseError::WrongSkeletonRoot {
                        found: name.to_string(),
                        expected: Self::AVATAR_SKELETON_V1,
                    });
                }
            }
        }

        let parent_by_node = parent_map(document);
        let world_by_node = world_transform_map(document);
        let joints: Vec<_> = skin.joints().collect();
        let joint_node_indices: HashSet<usize> = joints.iter().map(|j| j.index()).collect();
        let node_to_bone: HashMap<usize, BoneIndex> = joints
            .iter()
            .enumerate()
            .map(|(i, node)| (node.index(), BoneIndex(i)))
            .collect();

        let inverse_bind_matrices: Vec<Mat4> = skin
            .reader(|buffer| Some(&buffers[buffer.index()]))
            .read_inverse_bind_matrices()
            .map(|mats| mats.map(|m| Mat4::from_cols_array_2d(&m)).collect())
            .unwrap_or_default();

        let mut warnings = Vec::new();
        if inverse_bind_matrices.len() != joints.len() {
            warnings.push(format!(
                "inverse bind matrix count {} does not match joint count {}; missing entries use identity",
                inverse_bind_matrices.len(),
                joints.len()
            ));
        }

        let mut bones = Vec::with_capacity(joints.len());
        for (idx, node) in joints.iter().enumerate() {
            let Some(name) = node.name() else {
                return Err(SkeletonParseError::UnnamedJoint);
            };
            let parent = nearest_joint_parent(node.index(), &parent_by_node, &joint_node_indices)
                .and_then(|parent_node| node_to_bone.get(&parent_node).copied());
            let local_bind_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
            let world_bind_transform = world_by_node
                .get(&node.index())
                .copied()
                .unwrap_or(local_bind_transform);
            let inverse_bind_matrix = inverse_bind_matrices
                .get(idx)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            bones.push(Bone {
                name: name.to_string(),
                parent,
                local_bind_transform,
                inverse_bind_matrix,
                world_bind_transform,
            });
        }

        validate_bone_names(&bones, &mut warnings)?;

        Ok(Some(Self {
            name: Self::AVATAR_SKELETON_V1.to_string(),
            bones,
            warnings,
        }))
    }

    pub fn bone_index(&self, name: &str) -> Option<BoneIndex> {
        self.bones
            .iter()
            .position(|b| b.name == name)
            .map(BoneIndex)
    }

    pub fn bone(&self, index: BoneIndex) -> Option<&Bone> {
        self.bones.get(index.0)
    }

    pub fn line_segments(&self) -> impl Iterator<Item = (&Bone, &Bone)> {
        self.bones.iter().filter_map(|bone| {
            bone.parent
                .and_then(|parent| self.bone(parent))
                .map(|parent| (parent, bone))
        })
    }

    pub fn bind_world_transforms(&self) -> Vec<Mat4> {
        self.bones.iter().map(|b| b.world_bind_transform).collect()
    }

    pub fn posed_world_transforms(&self, debug_pose: bool) -> Vec<Mat4> {
        if !debug_pose {
            return self.bind_world_transforms();
        }

        let mut out = vec![Mat4::IDENTITY; self.bones.len()];
        for (idx, bone) in self.bones.iter().enumerate() {
            let local = bone.local_bind_transform * debug_rotation(&bone.name);
            out[idx] = match bone.parent {
                Some(parent) => out[parent.0] * local,
                None => local,
            };
        }
        out
    }

    pub fn world_transforms_from_pose(&self, pose: &Pose) -> Vec<Mat4> {
        let mut out = vec![Mat4::IDENTITY; self.bones.len()];
        for (idx, bone) in self.bones.iter().enumerate() {
            let (bind_scale, bind_rotation, bind_translation) =
                bone.local_bind_transform.to_scale_rotation_translation();
            let translation = pose
                .translations
                .get(idx)
                .and_then(|v| *v)
                .unwrap_or(bind_translation);
            let rotation = pose
                .rotations
                .get(idx)
                .and_then(|v| *v)
                .unwrap_or(bind_rotation);
            let scale = pose.scales.get(idx).and_then(|v| *v).unwrap_or(bind_scale);
            let local = Mat4::from_scale_rotation_translation(scale, rotation, translation);
            out[idx] = match bone.parent {
                Some(parent) => out[parent.0] * local,
                None => local,
            };
        }
        out
    }
}

fn debug_rotation(name: &str) -> Mat4 {
    let rot = match name {
        "upperarm_l" => Quat::from_rotation_z(-35.0_f32.to_radians()),
        "lowerarm_l" => Quat::from_rotation_z(-25.0_f32.to_radians()),
        "upperarm_r" => Quat::from_rotation_z(35.0_f32.to_radians()),
        "lowerarm_r" => Quat::from_rotation_z(25.0_f32.to_radians()),
        "upperleg_l" => Quat::from_rotation_x(-18.0_f32.to_radians()),
        "lowerleg_l" => Quat::from_rotation_x(25.0_f32.to_radians()),
        _ => Quat::IDENTITY,
    };
    Mat4::from_scale_rotation_translation(Vec3::ONE, rot, Vec3::ZERO)
}

fn parent_map(document: &gltf::Document) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    for node in document.nodes() {
        for child in node.children() {
            out.insert(child.index(), node.index());
        }
    }
    out
}

fn world_transform_map(document: &gltf::Document) -> HashMap<usize, Mat4> {
    let mut out = HashMap::new();
    for scene in document.scenes() {
        for root in scene.nodes() {
            visit_world(root, Mat4::IDENTITY, &mut out);
        }
    }
    out
}

fn visit_world(node: gltf::Node<'_>, parent_world: Mat4, out: &mut HashMap<usize, Mat4>) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent_world * local;
    out.insert(node.index(), world);
    for child in node.children() {
        visit_world(child, world, out);
    }
}

fn nearest_joint_parent(
    node_index: usize,
    parent_by_node: &HashMap<usize, usize>,
    joint_node_indices: &HashSet<usize>,
) -> Option<usize> {
    let mut cursor = parent_by_node.get(&node_index).copied();
    while let Some(parent) = cursor {
        if joint_node_indices.contains(&parent) {
            return Some(parent);
        }
        cursor = parent_by_node.get(&parent).copied();
    }
    None
}

fn validate_bone_names(
    bones: &[Bone],
    warnings: &mut Vec<String>,
) -> Result<(), SkeletonParseError> {
    let names: HashSet<&str> = bones.iter().map(|b| b.name.as_str()).collect();
    let missing: Vec<&str> = Skeleton::MVP_BONE_NAMES
        .iter()
        .copied()
        .filter(|required| !names.contains(required))
        .collect();
    if !missing.is_empty() {
        return Err(SkeletonParseError::MissingRequiredBones(missing.join(", ")));
    }

    for name in names {
        if !Skeleton::MVP_BONE_NAMES.contains(&name) && !is_reserved_bone(name) {
            warnings.push(format!("unknown non-standard bone '{name}'"));
        }
    }

    Ok(())
}

fn is_reserved_bone(name: &str) -> bool {
    matches!(
        name,
        "jaw" | "eye_l" | "eye_r" | "shoulder_l" | "shoulder_r" | "toe_l" | "toe_r"
    ) || is_reserved_finger_bone(name)
}

fn is_reserved_finger_bone(name: &str) -> bool {
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() != 4 || parts[0] != "finger" {
        return false;
    }
    matches!(parts[1], "thumb" | "index" | "middle" | "ring" | "pinky")
        && matches!(parts[2], "1" | "2" | "3")
        && matches!(parts[3], "l" | "r")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bone(name: &str, parent: Option<usize>) -> Bone {
        Bone {
            name: name.to_string(),
            parent: parent.map(BoneIndex),
            local_bind_transform: Mat4::IDENTITY,
            inverse_bind_matrix: Mat4::IDENTITY,
            world_bind_transform: Mat4::IDENTITY,
        }
    }

    #[test]
    fn mvp_bone_list_matches_expected_order() {
        assert_eq!(Skeleton::MVP_BONE_NAMES[0], "root");
        assert_eq!(Skeleton::MVP_BONE_NAMES[5], "head");
        assert_eq!(Skeleton::MVP_BONE_NAMES[8], "hand_l");
        assert_eq!(Skeleton::MVP_BONE_NAMES[17], "foot_r");
        assert_eq!(Skeleton::MVP_BONE_NAMES.len(), 18);
    }

    #[test]
    fn lookup_finds_representative_bones() {
        let skeleton = Skeleton {
            name: Skeleton::AVATAR_SKELETON_V1.to_string(),
            bones: Skeleton::MVP_BONE_NAMES
                .iter()
                .enumerate()
                .map(|(i, name)| bone(name, i.checked_sub(1)))
                .collect(),
            warnings: Vec::new(),
        };
        assert_eq!(skeleton.bone_index("head"), Some(BoneIndex(5)));
        assert_eq!(skeleton.bone_index("hand_l"), Some(BoneIndex(8)));
        assert_eq!(skeleton.bone_index("foot_r"), Some(BoneIndex(17)));
    }

    #[test]
    fn validation_rejects_missing_required_bones() {
        let mut bones: Vec<_> = Skeleton::MVP_BONE_NAMES
            .iter()
            .copied()
            .filter(|name| *name != "head")
            .map(|name| bone(name, None))
            .collect();
        let err = validate_bone_names(&bones, &mut Vec::new()).unwrap_err();
        assert!(err.to_string().contains("head"));
        bones.push(bone("head", None));
        assert!(validate_bone_names(&bones, &mut Vec::new()).is_ok());
    }

    #[test]
    fn reserved_bones_are_accepted_unknowns_warn() {
        let mut bones: Vec<_> = Skeleton::MVP_BONE_NAMES
            .iter()
            .copied()
            .map(|name| bone(name, None))
            .collect();
        bones.push(bone("jaw", None));
        bones.push(bone("finger_index_1_l", None));
        bones.push(bone("mystery", None));
        let mut warnings = Vec::new();
        validate_bone_names(&bones, &mut warnings).unwrap();
        assert_eq!(warnings, vec!["unknown non-standard bone 'mystery'"]);
    }

    #[test]
    fn phase4_sample_asset_parses_skeleton() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/processed/avatars/bodies/phase4_rig.glb");
        let (document, buffers, _) = gltf::import(&path).unwrap();
        let skeleton = Skeleton::from_gltf(&document, &buffers).unwrap().unwrap();
        assert_eq!(skeleton.name, Skeleton::AVATAR_SKELETON_V1);
        assert!(skeleton.bone_index("head").is_some());
        assert!(skeleton.bone_index("hand_l").is_some());
        assert!(skeleton.bone_index("foot_r").is_some());
    }

    #[test]
    fn duck_asset_remains_static_without_skeleton() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/processed/avatars/bodies/duck.glb");
        let (document, buffers, _) = gltf::import(&path).unwrap();
        assert!(Skeleton::from_gltf(&document, &buffers).unwrap().is_none());
    }
}

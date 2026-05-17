//! Bone palette helpers for GPU skinning.

use glam::Mat4;
use thiserror::Error;

use crate::Skeleton;

pub const MAX_BONES: usize = 64;

#[derive(Debug, Clone)]
pub struct SkinningPalette {
    pub matrices: Vec<Mat4>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SkinningError {
    #[error("skeleton has {bones} bones; Phase 5 supports at most {max}")]
    TooManyBones { bones: usize, max: usize },
    #[error("pose matrix count {pose} does not match skeleton bone count {bones}")]
    PoseCountMismatch { pose: usize, bones: usize },
}

impl SkinningPalette {
    pub fn bind_pose(skeleton: &Skeleton) -> Result<Self, SkinningError> {
        Self::from_world_transforms(skeleton, &skeleton.bind_world_transforms())
    }

    pub fn debug_pose(skeleton: &Skeleton) -> Result<Self, SkinningError> {
        Self::from_world_transforms(skeleton, &skeleton.posed_world_transforms(true))
    }

    pub fn from_world_transforms(
        skeleton: &Skeleton,
        current_world: &[Mat4],
    ) -> Result<Self, SkinningError> {
        if skeleton.bones.len() > MAX_BONES {
            return Err(SkinningError::TooManyBones {
                bones: skeleton.bones.len(),
                max: MAX_BONES,
            });
        }
        if current_world.len() != skeleton.bones.len() {
            return Err(SkinningError::PoseCountMismatch {
                pose: current_world.len(),
                bones: skeleton.bones.len(),
            });
        }
        let matrices = skeleton
            .bones
            .iter()
            .zip(current_world.iter())
            .map(|(bone, world)| *world * bone.inverse_bind_matrix)
            .collect();
        Ok(Self { matrices })
    }

    pub fn padded_cols_array_2d(&self) -> [[[f32; 4]; 4]; MAX_BONES] {
        let mut out = [Mat4::IDENTITY.to_cols_array_2d(); MAX_BONES];
        for (dst, src) in out.iter_mut().zip(self.matrices.iter()) {
            *dst = src.to_cols_array_2d();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use glam::Mat4;

    use super::*;
    use crate::{Bone, BoneIndex};

    fn test_skeleton() -> Skeleton {
        Skeleton {
            name: Skeleton::AVATAR_SKELETON_V1.to_string(),
            bones: vec![
                Bone {
                    name: "root".into(),
                    parent: None,
                    local_bind_transform: Mat4::IDENTITY,
                    inverse_bind_matrix: Mat4::IDENTITY,
                    world_bind_transform: Mat4::IDENTITY,
                },
                Bone {
                    name: "child".into(),
                    parent: Some(BoneIndex(0)),
                    local_bind_transform: Mat4::from_translation(glam::Vec3::Y),
                    inverse_bind_matrix: Mat4::from_translation(-glam::Vec3::Y),
                    world_bind_transform: Mat4::from_translation(glam::Vec3::Y),
                },
            ],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn bind_pose_palette_uses_world_times_inverse_bind() {
        let skeleton = test_skeleton();
        let palette = SkinningPalette::bind_pose(&skeleton).unwrap();
        assert_eq!(palette.matrices.len(), 2);
        assert!(palette.matrices[0].abs_diff_eq(Mat4::IDENTITY, 1e-5));
        assert!(palette.matrices[1].abs_diff_eq(Mat4::IDENTITY, 1e-5));
    }

    #[test]
    fn pose_count_mismatch_is_error() {
        let skeleton = test_skeleton();
        let err = SkinningPalette::from_world_transforms(&skeleton, &[Mat4::IDENTITY]).unwrap_err();
        assert_eq!(err, SkinningError::PoseCountMismatch { pose: 1, bones: 2 });
    }
}

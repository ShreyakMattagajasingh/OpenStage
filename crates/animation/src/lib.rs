//! Skeleton, clips, GPU skinning.
//!
//! TODO(phase-4): `Skeleton` (avatar_skeleton_v1 — see docs/skeleton_standard.md).
//! TODO(phase-5): `SkinnedMesh`, inverse bind matrices, GPU skinning.
//! TODO(phase-6): `Clip`, T/R/S tracks, slerp, `Player` with play/pause/loop.

pub mod clip;
pub mod debug;
pub mod player;
pub mod skeleton;
pub mod skinning;

pub use clip::{AnimationChannel, AnimationClip, Interpolation, Keyframe, Pose, TransformTrack};
pub use player::AnimationPlayer;
pub use skeleton::{Bone, BoneIndex, Skeleton, SkeletonParseError};
pub use skinning::{SkinningError, SkinningPalette, MAX_BONES};

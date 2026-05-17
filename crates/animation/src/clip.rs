//! Animation clip data and sampling.

use glam::{Quat, Vec3};

use crate::BoneIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransformTrack<T> {
    pub bone: BoneIndex,
    pub interpolation: Interpolation,
    pub keys: Vec<Keyframe<T>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationChannel {
    pub bone: BoneIndex,
    pub translation: Option<TransformTrack<Vec3>>,
    pub rotation: Option<TransformTrack<Quat>>,
    pub scale: Option<TransformTrack<Vec3>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Debug, Clone)]
pub struct Pose {
    pub translations: Vec<Option<Vec3>>,
    pub rotations: Vec<Option<Quat>>,
    pub scales: Vec<Option<Vec3>>,
}

impl Pose {
    pub fn new(bone_count: usize) -> Self {
        Self {
            translations: vec![None; bone_count],
            rotations: vec![None; bone_count],
            scales: vec![None; bone_count],
        }
    }
}

impl AnimationClip {
    pub fn sample(&self, time: f32, bone_count: usize) -> Pose {
        let mut pose = Pose::new(bone_count);
        let t = time.clamp(0.0, self.duration.max(0.0));
        for channel in &self.channels {
            let i = channel.bone.0;
            if i >= bone_count {
                continue;
            }
            if let Some(track) = &channel.translation {
                pose.translations[i] = Some(sample_vec3(track, t));
            }
            if let Some(track) = &channel.rotation {
                pose.rotations[i] = Some(sample_quat(track, t));
            }
            if let Some(track) = &channel.scale {
                pose.scales[i] = Some(sample_vec3(track, t));
            }
        }
        pose
    }
}

pub fn sample_vec3(track: &TransformTrack<Vec3>, time: f32) -> Vec3 {
    sample_track(track, time, |a, b, alpha| a.lerp(b, alpha))
}

pub fn sample_quat(track: &TransformTrack<Quat>, time: f32) -> Quat {
    sample_track(track, time, |a, b, alpha| a.slerp(b, alpha).normalize())
}

fn sample_track<T: Copy>(track: &TransformTrack<T>, time: f32, lerp: impl Fn(T, T, f32) -> T) -> T {
    assert!(!track.keys.is_empty(), "animation track has no keys");
    if time <= track.keys[0].time {
        return track.keys[0].value;
    }
    for pair in track.keys.windows(2) {
        let a = pair[0];
        let b = pair[1];
        if time <= b.time {
            return match track.interpolation {
                Interpolation::Step => a.value,
                Interpolation::Linear => {
                    let span = (b.time - a.time).max(1e-6);
                    let alpha = ((time - a.time) / span).clamp(0.0, 1.0);
                    lerp(a.value, b.value, alpha)
                }
            };
        }
    }
    track.keys.last().unwrap().value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_linear_interpolation_returns_midpoint() {
        let track = TransformTrack {
            bone: BoneIndex(0),
            interpolation: Interpolation::Linear,
            keys: vec![
                Keyframe {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                Keyframe {
                    time: 1.0,
                    value: Vec3::new(2.0, 0.0, 0.0),
                },
            ],
        };
        assert_eq!(sample_vec3(&track, 0.5), Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn rotation_uses_slerp_and_normalizes() {
        let track = TransformTrack {
            bone: BoneIndex(0),
            interpolation: Interpolation::Linear,
            keys: vec![
                Keyframe {
                    time: 0.0,
                    value: Quat::IDENTITY,
                },
                Keyframe {
                    time: 1.0,
                    value: Quat::from_rotation_y(std::f32::consts::PI),
                },
            ],
        };
        let q = sample_quat(&track, 0.5);
        assert!((q.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn step_interpolation_holds_previous_key() {
        let track = TransformTrack {
            bone: BoneIndex(0),
            interpolation: Interpolation::Step,
            keys: vec![
                Keyframe {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                Keyframe {
                    time: 1.0,
                    value: Vec3::ONE,
                },
            ],
        };
        assert_eq!(sample_vec3(&track, 0.75), Vec3::ZERO);
    }
}

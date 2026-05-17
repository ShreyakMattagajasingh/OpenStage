//! Animation playback state.

use std::time::Duration;

use crate::{AnimationClip, Pose};

#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub selected_clip: usize,
    pub time: f32,
    pub playing: bool,
    pub looping: bool,
    pub playback_speed: f32,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self {
            selected_clip: 0,
            time: 0.0,
            playing: false,
            looping: true,
            playback_speed: 1.0,
        }
    }
}

impl AnimationPlayer {
    pub fn new_autoplay() -> Self {
        Self {
            playing: true,
            ..Self::default()
        }
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn seek(&mut self, time: f32, clip: &AnimationClip) {
        self.time = time.clamp(0.0, clip.duration.max(0.0));
    }

    pub fn tick(&mut self, dt: Duration, clip: &AnimationClip) {
        if !self.playing {
            return;
        }
        let duration = clip.duration.max(0.0);
        if duration <= 0.0 {
            self.time = 0.0;
            return;
        }

        self.time += dt.as_secs_f32() * self.playback_speed;
        if self.looping {
            self.time = self.time.rem_euclid(duration);
        } else if self.time >= duration {
            self.time = duration;
            self.playing = false;
        } else if self.time < 0.0 {
            self.time = 0.0;
            self.playing = false;
        }
    }

    pub fn sample_pose(&self, clips: &[AnimationClip], bone_count: usize) -> Option<Pose> {
        clips
            .get(self.selected_clip)
            .map(|clip| clip.sample(self.time, bone_count))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::AnimationClip;

    use super::*;

    fn clip() -> AnimationClip {
        AnimationClip {
            name: "idle".into(),
            duration: 2.0,
            channels: Vec::new(),
        }
    }

    #[test]
    fn looping_tick_wraps_time() {
        let mut player = AnimationPlayer::new_autoplay();
        player.time = 1.75;
        player.tick(Duration::from_secs_f32(0.5), &clip());
        assert!((player.time - 0.25).abs() < 1e-5);
        assert!(player.playing);
    }

    #[test]
    fn non_looping_tick_clamps_at_duration() {
        let mut player = AnimationPlayer::new_autoplay();
        player.looping = false;
        player.time = 1.75;
        player.tick(Duration::from_secs_f32(0.5), &clip());
        assert_eq!(player.time, 2.0);
        assert!(!player.playing);
    }

    #[test]
    fn seek_clamps_to_clip_range() {
        let mut player = AnimationPlayer::default();
        let clip = clip();
        player.seek(5.0, &clip);
        assert_eq!(player.time, 2.0);
        player.seek(-1.0, &clip);
        assert_eq!(player.time, 0.0);
    }
}

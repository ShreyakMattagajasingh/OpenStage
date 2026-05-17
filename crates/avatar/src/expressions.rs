//! Facial expression preset selection.
//!
//! Phase 9 ships five hardcoded expressions whose visuals are rendered
//! procedurally by the app (see `apps/avatar_desktop/src/face_textures.rs`).
//! Save format ([docs/save_format.md]) carries the snake-case `as_save_str`
//! representation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Expression {
    #[default]
    Neutral,
    Happy,
    Sad,
    Surprised,
    Angry,
}

impl Expression {
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Happy,
        Self::Sad,
        Self::Surprised,
        Self::Angry,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Neutral => "Neutral",
            Self::Happy => "Happy",
            Self::Sad => "Sad",
            Self::Surprised => "Surprised",
            Self::Angry => "Angry",
        }
    }

    /// Stable identifier used in save files and metadata.
    pub fn as_save_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Happy => "happy",
            Self::Sad => "sad",
            Self::Surprised => "surprised",
            Self::Angry => "angry",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_matches_default_and_count() {
        assert_eq!(Expression::ALL.len(), 5);
        assert_eq!(Expression::default(), Expression::Neutral);
        assert_eq!(Expression::ALL[0], Expression::Neutral);
    }

    #[test]
    fn label_and_save_str_are_distinct_per_variant() {
        let labels: Vec<_> = Expression::ALL.iter().map(|e| e.label()).collect();
        let saves: Vec<_> = Expression::ALL.iter().map(|e| e.as_save_str()).collect();
        // No duplicates.
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
        let mut sorted = saves.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }

    #[test]
    fn serde_round_trips_snake_case() {
        let serialized = serde_json::to_string(&Expression::Surprised).unwrap();
        assert_eq!(serialized, "\"surprised\"");
        let back: Expression = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, Expression::Surprised);
    }
}

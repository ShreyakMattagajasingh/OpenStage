//! Asset metadata schema. One `AssetMeta` per asset; lives on disk as a JSON
//! sidecar under `assets/processed/metadata/<id>.json` and is mirrored into the
//! SQLite catalog by `database::Catalog`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssetMeta {
    /// Stable identifier, unique across the catalog. e.g. `"body_duck_001"`.
    pub id: String,
    /// Human-readable name shown in the UI.
    pub display_name: String,
    pub category: AssetCategory,
    /// Path to the asset file, relative to `assets/processed/`.
    /// e.g. `"avatars/bodies/duck.glb"`.
    pub model: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub supports_color: bool,
    #[serde(default)]
    pub default_color: Option<[f32; 3]>,
    #[serde(default)]
    pub compatible_body_types: Vec<String>,
    #[serde(default)]
    pub compatible_skeleton: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AssetCategory {
    Body,
    Head,
    Hair,
    Top,
    Bottom,
    Shoes,
    Hat,
    Glasses,
    Accessory,
    Animation,
    Pose,
    Background,
    Material,
}

impl AssetCategory {
    /// String form used in SQLite (snake_case, matches `serde(rename_all)`).
    pub fn as_sql_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Head => "head",
            Self::Hair => "hair",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Shoes => "shoes",
            Self::Hat => "hat",
            Self::Glasses => "glasses",
            Self::Accessory => "accessory",
            Self::Animation => "animation",
            Self::Pose => "pose",
            Self::Background => "background",
            Self::Material => "material",
        }
    }

    pub fn from_sql_str(s: &str) -> Option<Self> {
        Some(match s {
            "body" => Self::Body,
            "head" => Self::Head,
            "hair" => Self::Hair,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "shoes" => Self::Shoes,
            "hat" => Self::Hat,
            "glasses" => Self::Glasses,
            "accessory" => Self::Accessory,
            "animation" => Self::Animation,
            "pose" => Self::Pose,
            "background" => Self::Background,
            "material" => Self::Material,
            _ => return None,
        })
    }
}

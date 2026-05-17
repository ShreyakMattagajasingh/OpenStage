//! Avatar slot model.
//!
//! Phase 7 ships nine slots — body + the eight common wearable slots. The
//! `accessory` slot is a catch-all for the long-tail spec slots
//! (earrings/necklace/watch/backpack/handheld), which roll up to one slot for
//! now and split out in a later phase when bone-attached static accessories
//! arrive.

use assets::AssetCategory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Body,
    Head,
    Hair,
    Top,
    Bottom,
    Shoes,
    Hat,
    Glasses,
    Accessory,
}

impl Slot {
    pub const ALL: [Self; 9] = [
        Self::Body,
        Self::Head,
        Self::Hair,
        Self::Top,
        Self::Bottom,
        Self::Shoes,
        Self::Hat,
        Self::Glasses,
        Self::Accessory,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Head => "Head",
            Self::Hair => "Hair",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Shoes => "Shoes",
            Self::Hat => "Hat",
            Self::Glasses => "Glasses",
            Self::Accessory => "Accessory",
        }
    }

    /// Map an asset category to the slot it should populate. Returns `None`
    /// for categories that aren't equippable (`animation`, `pose`,
    /// `background`, `material`).
    pub fn from_asset_category(cat: AssetCategory) -> Option<Self> {
        Some(match cat {
            AssetCategory::Body => Self::Body,
            AssetCategory::Head => Self::Head,
            AssetCategory::Hair => Self::Hair,
            AssetCategory::Top => Self::Top,
            AssetCategory::Bottom => Self::Bottom,
            AssetCategory::Shoes => Self::Shoes,
            AssetCategory::Hat => Self::Hat,
            AssetCategory::Glasses => Self::Glasses,
            AssetCategory::Accessory => Self::Accessory,
            AssetCategory::Animation
            | AssetCategory::Pose
            | AssetCategory::Background
            | AssetCategory::Material => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_to_slot_round_trip() {
        assert_eq!(
            Slot::from_asset_category(AssetCategory::Body),
            Some(Slot::Body)
        );
        assert_eq!(
            Slot::from_asset_category(AssetCategory::Top),
            Some(Slot::Top)
        );
        assert_eq!(
            Slot::from_asset_category(AssetCategory::Accessory),
            Some(Slot::Accessory)
        );
        assert_eq!(Slot::from_asset_category(AssetCategory::Animation), None);
        assert_eq!(Slot::from_asset_category(AssetCategory::Material), None);
    }

    #[test]
    fn slot_all_has_every_variant() {
        assert_eq!(Slot::ALL.len(), 9);
        // Body is always first so iterating ALL renders the body before any wearable.
        assert_eq!(Slot::ALL[0], Slot::Body);
    }
}

//! Top-level avatar aggregate for the current editor session.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::expressions::Expression;
use crate::slots::Slot;

/// Default `body_type` when no body asset is equipped. Wearables with an
/// empty `compatible_body_types` list match any body — including this one.
pub const DEFAULT_BODY_TYPE: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Avatar {
    /// Identifies which body family the current rig belongs to. Wearables
    /// declare `compatible_body_types` against this value.
    pub body_type: String,
    pub skin_tone: [f32; 3],
    /// Slot → asset id of whatever is currently equipped there.
    #[serde(default)]
    pub slots: HashMap<Slot, String>,
    /// Per-slot sRGB tint, user-picked via the color picker. Survives
    /// unequip/re-equip of the same asset within a session; cleared by
    /// [`Avatar::clear_all`].
    #[serde(default)]
    pub slot_colors: HashMap<Slot, [f32; 3]>,
    /// Active facial expression preset. Drives which procedural face
    /// texture is bound to the face quad in avatar mode.
    #[serde(default)]
    pub expression: Expression,
}

impl Default for Avatar {
    fn default() -> Self {
        Self {
            body_type: DEFAULT_BODY_TYPE.to_string(),
            skin_tone: [0.62, 0.41, 0.28],
            slots: HashMap::new(),
            slot_colors: HashMap::new(),
            expression: Expression::default(),
        }
    }
}

impl Avatar {
    /// Equip `asset_id` into `slot`, replacing whatever was there.
    pub fn equip(&mut self, slot: Slot, asset_id: impl Into<String>) {
        self.slots.insert(slot, asset_id.into());
    }

    /// Remove whatever is in `slot`. No-op if the slot is empty.
    pub fn unequip(&mut self, slot: Slot) {
        self.slots.remove(&slot);
    }

    pub fn equipped(&self, slot: Slot) -> Option<&str> {
        self.slots.get(&slot).map(String::as_str)
    }

    /// Iterate equipped slots in [`Slot::ALL`] order so the body comes first.
    pub fn iter(&self) -> impl Iterator<Item = (Slot, &str)> + '_ {
        Slot::ALL
            .iter()
            .filter_map(|&slot| self.slots.get(&slot).map(|id| (slot, id.as_str())))
    }

    /// Drop everything — body, wearables, remembered colors, expression.
    pub fn clear_all(&mut self) {
        self.slots.clear();
        self.slot_colors.clear();
        self.body_type = DEFAULT_BODY_TYPE.to_string();
        self.expression = Expression::default();
    }

    /// Remember the user's color pick for `slot`. Value is sRGB.
    pub fn set_slot_color(&mut self, slot: Slot, color_srgb: [f32; 3]) {
        self.slot_colors.insert(slot, color_srgb);
    }

    /// Look up a previously-picked sRGB color for `slot`.
    pub fn slot_color(&self, slot: Slot) -> Option<[f32; 3]> {
        self.slot_colors.get(&slot).copied()
    }

    // ---- Phase-7 hooks kept around for the old API ---------------------------
    pub fn set_active_body_asset(&mut self, id: impl Into<String>) {
        self.equip(Slot::Body, id);
    }
    pub fn clear_active_body_asset(&mut self) {
        self.unequip(Slot::Body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equip_overwrites_previous_slot_contents() {
        let mut av = Avatar::default();
        av.equip(Slot::Top, "top_a");
        av.equip(Slot::Top, "top_b");
        assert_eq!(av.equipped(Slot::Top), Some("top_b"));
    }

    #[test]
    fn unequip_removes_slot() {
        let mut av = Avatar::default();
        av.equip(Slot::Hair, "hair_001");
        av.unequip(Slot::Hair);
        assert_eq!(av.equipped(Slot::Hair), None);
    }

    #[test]
    fn iter_visits_body_before_wearables() {
        let mut av = Avatar::default();
        av.equip(Slot::Top, "top_001");
        av.equip(Slot::Body, "body_001");
        let order: Vec<Slot> = av.iter().map(|(s, _)| s).collect();
        assert_eq!(order, vec![Slot::Body, Slot::Top]);
    }

    #[test]
    fn clear_all_resets_body_type() {
        let mut av = Avatar {
            body_type: "phase4_rig".into(),
            ..Avatar::default()
        };
        av.equip(Slot::Body, "body_phase4_rig_001");
        av.equip(Slot::Top, "top_a");
        av.clear_all();
        assert_eq!(av.body_type, DEFAULT_BODY_TYPE);
        assert!(av.slots.is_empty());
    }

    #[test]
    fn slot_color_round_trip() {
        let mut av = Avatar::default();
        av.set_slot_color(Slot::Top, [0.9, 0.1, 0.2]);
        assert_eq!(av.slot_color(Slot::Top), Some([0.9, 0.1, 0.2]));
        assert_eq!(av.slot_color(Slot::Hair), None);
    }

    #[test]
    fn clear_all_clears_colors() {
        let mut av = Avatar::default();
        av.set_slot_color(Slot::Body, [0.5, 0.3, 0.2]);
        av.set_slot_color(Slot::Top, [0.0, 0.0, 1.0]);
        av.clear_all();
        assert!(av.slot_colors.is_empty());
    }

    #[test]
    fn slot_color_independent_of_slots_map() {
        let mut av = Avatar::default();
        av.set_slot_color(Slot::Top, [0.5, 0.5, 0.5]);
        // No equip was called; slots map stays empty but color is remembered.
        assert!(av.slots.is_empty());
        assert_eq!(av.slot_color(Slot::Top), Some([0.5, 0.5, 0.5]));
    }
}

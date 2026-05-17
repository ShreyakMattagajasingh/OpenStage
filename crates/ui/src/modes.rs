use serde::{Deserialize, Serialize};

use crate::icons;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    #[default]
    Character,
    Customize,
    Object,
    Rig,
    Pose,
    Animation,
    Expression,
    Material,
    Asset,
    Export,
    Ai,
}

impl EditorMode {
    pub const ALL: [Self; 11] = [
        Self::Character,
        Self::Customize,
        Self::Object,
        Self::Rig,
        Self::Pose,
        Self::Animation,
        Self::Expression,
        Self::Material,
        Self::Asset,
        Self::Export,
        Self::Ai,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Customize => "Customize",
            Self::Object => "Object",
            Self::Rig => "Rig",
            Self::Pose => "Pose",
            Self::Animation => "Animation",
            Self::Expression => "Expression",
            Self::Material => "Material",
            Self::Asset => "Asset",
            Self::Export => "Export",
            Self::Ai => "AI",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Character => icons::USER_CIRCLE,
            Self::Customize => icons::PALETTE,
            Self::Object => icons::CUBE,
            Self::Rig => icons::SPARKLE,
            Self::Pose => icons::USER,
            Self::Animation => icons::SLIDERS,
            Self::Expression => icons::SMILEY,
            Self::Material => icons::PALETTE,
            Self::Asset => icons::IMAGE,
            Self::Export => icons::FILE_ARROW_DOWN,
            Self::Ai => icons::SPARKLE,
        }
    }

    pub fn implemented(self) -> bool {
        matches!(
            self,
            Self::Character
                | Self::Customize
                | Self::Object
                | Self::Material
                | Self::Asset
                | Self::Export
        )
    }

    pub fn coming_soon_stage(self) -> Option<u32> {
        match self {
            Self::Rig => Some(23),
            Self::Pose => Some(24),
            Self::Animation => Some(25),
            Self::Expression => Some(26),
            Self::Ai => Some(32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeftSection {
    Categories,
    AssetList,
    Equipped,
    Gallery,
    Model,
    Export,
    Animation,
    Expression,
    Skinning,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeLayout {
    pub left_sections: &'static [LeftSection],
    pub show_inspector: bool,
    pub show_timeline: bool,
    pub show_mode_bar: bool,
}

impl ModeLayout {
    pub fn has_section(self, section: LeftSection) -> bool {
        self.left_sections.contains(&section)
    }
}

const CHARACTER_SECTIONS: &[LeftSection] = &[
    LeftSection::Categories,
    LeftSection::AssetList,
    LeftSection::Equipped,
    LeftSection::Gallery,
    LeftSection::Model,
    LeftSection::Export,
    LeftSection::Animation,
    LeftSection::Expression,
    LeftSection::Skinning,
    LeftSection::Diagnostics,
];

const CUSTOMIZE_SECTIONS: &[LeftSection] = &[
    LeftSection::Equipped,
    LeftSection::Expression,
    LeftSection::Categories,
    LeftSection::AssetList,
    LeftSection::Skinning,
    LeftSection::Animation,
    LeftSection::Model,
    LeftSection::Gallery,
    LeftSection::Export,
    LeftSection::Diagnostics,
];

const OBJECT_SECTIONS: &[LeftSection] = &[
    LeftSection::Model,
    LeftSection::Equipped,
    LeftSection::Skinning,
    LeftSection::Diagnostics,
];

const MATERIAL_SECTIONS: &[LeftSection] = &[
    LeftSection::Equipped,
    LeftSection::Expression,
    LeftSection::Skinning,
    LeftSection::Diagnostics,
];

const ASSET_SECTIONS: &[LeftSection] = &[
    LeftSection::Categories,
    LeftSection::AssetList,
    LeftSection::Model,
    LeftSection::Diagnostics,
];

const EXPORT_SECTIONS: &[LeftSection] = &[
    LeftSection::Export,
    LeftSection::Model,
    LeftSection::Diagnostics,
];

pub fn mode_layout(mode: EditorMode) -> ModeLayout {
    let left_sections = match mode {
        EditorMode::Character => CHARACTER_SECTIONS,
        EditorMode::Customize => CUSTOMIZE_SECTIONS,
        EditorMode::Object => OBJECT_SECTIONS,
        EditorMode::Material => MATERIAL_SECTIONS,
        EditorMode::Asset => ASSET_SECTIONS,
        EditorMode::Export => EXPORT_SECTIONS,
        EditorMode::Rig
        | EditorMode::Pose
        | EditorMode::Animation
        | EditorMode::Expression
        | EditorMode::Ai => &[],
    };
    ModeLayout {
        left_sections,
        show_inspector: true,
        show_timeline: false,
        show_mode_bar: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_modes_cover_every_variant() {
        assert_eq!(EditorMode::ALL.len(), 11);
        assert_eq!(EditorMode::default(), EditorMode::Character);
        assert!(EditorMode::ALL.contains(&EditorMode::Ai));
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<_> = EditorMode::ALL.iter().map(|mode| mode.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), EditorMode::ALL.len());
    }

    #[test]
    fn implemented_modes_have_sections() {
        for mode in EditorMode::ALL {
            if mode.implemented() {
                assert!(
                    !mode_layout(mode).left_sections.is_empty(),
                    "{:?} should have at least one section",
                    mode
                );
            }
        }
    }
}

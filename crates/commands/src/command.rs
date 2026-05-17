use avatar::Slot;
use serde::{Deserialize, Serialize};
use ui::EditorMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Ui,
    AiPrompt,
    Mcp,
    Script,
    Test,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandName {
    #[serde(rename = "selection.set")]
    SelectionSet,
    #[serde(rename = "selection.clear")]
    SelectionClear,
    #[serde(rename = "avatar.equip_asset")]
    AvatarEquipAsset,
    #[serde(rename = "material.set_color")]
    MaterialSetColor,
    #[serde(rename = "scene.set_visible")]
    SceneSetVisible,
    #[serde(rename = "scene.set_locked")]
    SceneSetLocked,
    #[serde(rename = "transform.set_translation")]
    TransformSetTranslation,
    #[serde(rename = "transform.set_rotation")]
    TransformSetRotation,
    #[serde(rename = "transform.set_scale")]
    TransformSetScale,
    #[serde(rename = "transform.apply_delta")]
    TransformApplyDelta,
    #[serde(rename = "transform.reset")]
    TransformReset,
    #[serde(rename = "editor.set_mode")]
    EditorSetMode,
    #[serde(rename = "history.undo")]
    HistoryUndo,
    #[serde(rename = "history.redo")]
    HistoryRedo,
    #[serde(rename = "legacy.undoable")]
    LegacyUndoable,
}

impl CommandName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelectionSet => "selection.set",
            Self::SelectionClear => "selection.clear",
            Self::AvatarEquipAsset => "avatar.equip_asset",
            Self::MaterialSetColor => "material.set_color",
            Self::SceneSetVisible => "scene.set_visible",
            Self::SceneSetLocked => "scene.set_locked",
            Self::TransformSetTranslation => "transform.set_translation",
            Self::TransformSetRotation => "transform.set_rotation",
            Self::TransformSetScale => "transform.set_scale",
            Self::TransformApplyDelta => "transform.apply_delta",
            Self::TransformReset => "transform.reset",
            Self::EditorSetMode => "editor.set_mode",
            Self::HistoryUndo => "history.undo",
            Self::HistoryRedo => "history.redo",
            Self::LegacyUndoable => "legacy.undoable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    pub id: String,
    pub version: u32,
    pub name: CommandName,
    pub timestamp: String,
    pub source: CommandSource,
    pub payload: CommandPayload,
}

impl CommandEnvelope {
    pub fn new(
        id: impl Into<String>,
        timestamp: impl Into<String>,
        source: CommandSource,
        payload: CommandPayload,
    ) -> Self {
        Self {
            id: id.into(),
            version: 1,
            name: payload.name(),
            timestamp: timestamp.into(),
            source,
            payload,
        }
    }

    pub fn serialize_json(&self) -> Result<String, crate::CommandError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| crate::CommandError::Serialize(e.to_string()))
    }

    pub fn deserialize_json(json: &str) -> Result<Self, crate::CommandError> {
        serde_json::from_str(json).map_err(|e| crate::CommandError::Deserialize(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandPayload {
    #[serde(rename = "selection.set")]
    SelectionSet(SelectionSetPayload),
    #[serde(rename = "selection.clear")]
    SelectionClear(SelectionClearPayload),
    #[serde(rename = "avatar.equip_asset")]
    AvatarEquipAsset(AvatarEquipAssetPayload),
    #[serde(rename = "material.set_color")]
    MaterialSetColor(MaterialSetColorPayload),
    #[serde(rename = "scene.set_visible")]
    SceneSetVisible(SceneSetVisiblePayload),
    #[serde(rename = "scene.set_locked")]
    SceneSetLocked(SceneSetLockedPayload),
    #[serde(rename = "transform.set_translation")]
    TransformSetTranslation(TransformSetTranslationPayload),
    #[serde(rename = "transform.set_rotation")]
    TransformSetRotation(TransformSetRotationPayload),
    #[serde(rename = "transform.set_scale")]
    TransformSetScale(TransformSetScalePayload),
    #[serde(rename = "transform.apply_delta")]
    TransformApplyDelta(TransformApplyDeltaPayload),
    #[serde(rename = "transform.reset")]
    TransformReset(TransformResetPayload),
    #[serde(rename = "editor.set_mode")]
    EditorSetMode(EditorSetModePayload),
    #[serde(rename = "history.undo")]
    HistoryUndo,
    #[serde(rename = "history.redo")]
    HistoryRedo,
    #[serde(rename = "legacy.undoable")]
    LegacyUndoable { label: String },
}

impl CommandPayload {
    pub fn name(&self) -> CommandName {
        match self {
            Self::SelectionSet(_) => CommandName::SelectionSet,
            Self::SelectionClear(_) => CommandName::SelectionClear,
            Self::AvatarEquipAsset(_) => CommandName::AvatarEquipAsset,
            Self::MaterialSetColor(_) => CommandName::MaterialSetColor,
            Self::SceneSetVisible(_) => CommandName::SceneSetVisible,
            Self::SceneSetLocked(_) => CommandName::SceneSetLocked,
            Self::TransformSetTranslation(_) => CommandName::TransformSetTranslation,
            Self::TransformSetRotation(_) => CommandName::TransformSetRotation,
            Self::TransformSetScale(_) => CommandName::TransformSetScale,
            Self::TransformApplyDelta(_) => CommandName::TransformApplyDelta,
            Self::TransformReset(_) => CommandName::TransformReset,
            Self::EditorSetMode(_) => CommandName::EditorSetMode,
            Self::HistoryUndo => CommandName::HistoryUndo,
            Self::HistoryRedo => CommandName::HistoryRedo,
            Self::LegacyUndoable { .. } => CommandName::LegacyUndoable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSetPayload {
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelectionClearPayload {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarEquipAssetPayload {
    pub avatar_id: String,
    pub slot: Slot,
    pub asset_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSetColorPayload {
    pub target: MaterialTarget,
    pub color_srgb: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaterialTarget {
    AvatarSlot { avatar_id: String, slot: Slot },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSetVisiblePayload {
    pub object_id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSetLockedPayload {
    pub object_id: String,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformSetTranslationPayload {
    pub object_id: String,
    pub translation: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformSetRotationPayload {
    pub object_id: String,
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformSetScalePayload {
    pub object_id: String,
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformApplyDeltaPayload {
    pub object_id: String,
    pub delta_translation: Option<[f32; 3]>,
    pub delta_rotation_quat: Option<[f32; 4]>,
    pub delta_scale: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformResetPayload {
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSetModePayload {
    pub mode: EditorMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub command_id: String,
    pub command_name: CommandName,
    pub executed: bool,
    pub dry_run: bool,
    pub message: String,
}

impl CommandResult {
    pub fn new(command: &CommandEnvelope, executed: bool, dry_run: bool, message: String) -> Self {
        Self {
            command_id: command.id.clone(),
            command_name: command.name,
            executed,
            dry_run,
            message,
        }
    }
}

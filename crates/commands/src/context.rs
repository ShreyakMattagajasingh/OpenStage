use crate::{
    AvatarEquipAssetPayload, CommandError, EditorSetModePayload, MaterialSetColorPayload,
    SceneSetLockedPayload, SceneSetVisiblePayload, SelectionSetPayload, TransformApplyDeltaPayload,
    TransformResetPayload, TransformSetRotationPayload, TransformSetScalePayload,
    TransformSetTranslationPayload, ValidationResult,
};

pub trait CommandRuntime {
    type Snapshot: Clone;

    fn snapshot(&self) -> Self::Snapshot;
    fn restore_snapshot(&mut self, snapshot: &Self::Snapshot) -> Result<(), CommandError>;

    fn validate_selection_set(&self, payload: &SelectionSetPayload) -> ValidationResult;
    fn set_selection(&mut self, payload: &SelectionSetPayload) -> Result<(), CommandError>;
    fn clear_selection(&mut self) -> Result<(), CommandError>;

    fn validate_avatar_equip_asset(&self, payload: &AvatarEquipAssetPayload) -> ValidationResult;
    fn equip_asset(&mut self, payload: &AvatarEquipAssetPayload) -> Result<(), CommandError>;

    fn validate_material_set_color(&self, payload: &MaterialSetColorPayload) -> ValidationResult;
    fn set_material_color(&mut self, payload: &MaterialSetColorPayload)
        -> Result<(), CommandError>;

    fn validate_scene_set_visible(&self, payload: &SceneSetVisiblePayload) -> ValidationResult;
    fn scene_set_visible(&mut self, payload: &SceneSetVisiblePayload) -> Result<(), CommandError>;

    fn validate_scene_set_locked(&self, payload: &SceneSetLockedPayload) -> ValidationResult;
    fn scene_set_locked(&mut self, payload: &SceneSetLockedPayload) -> Result<(), CommandError>;

    fn validate_transform_set_translation(
        &self,
        payload: &TransformSetTranslationPayload,
    ) -> ValidationResult;
    fn transform_set_translation(
        &mut self,
        payload: &TransformSetTranslationPayload,
    ) -> Result<(), CommandError>;

    fn validate_transform_set_rotation(
        &self,
        payload: &TransformSetRotationPayload,
    ) -> ValidationResult;
    fn transform_set_rotation(
        &mut self,
        payload: &TransformSetRotationPayload,
    ) -> Result<(), CommandError>;

    fn validate_transform_set_scale(&self, payload: &TransformSetScalePayload) -> ValidationResult;
    fn transform_set_scale(
        &mut self,
        payload: &TransformSetScalePayload,
    ) -> Result<(), CommandError>;

    fn validate_transform_apply_delta(
        &self,
        payload: &TransformApplyDeltaPayload,
    ) -> ValidationResult;
    fn transform_apply_delta(
        &mut self,
        payload: &TransformApplyDeltaPayload,
    ) -> Result<(), CommandError>;

    fn validate_transform_reset(&self, payload: &TransformResetPayload) -> ValidationResult;
    fn transform_reset(&mut self, payload: &TransformResetPayload) -> Result<(), CommandError>;

    fn validate_editor_set_mode(&self, payload: &EditorSetModePayload) -> ValidationResult;
    fn set_editor_mode(&mut self, payload: &EditorSetModePayload) -> Result<(), CommandError>;
}

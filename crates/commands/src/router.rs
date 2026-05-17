use crate::{
    CommandEnvelope, CommandError, CommandHistory, CommandName, CommandPayload, CommandResult,
    CommandRuntime, UndoRecord, ValidationResult,
};

#[derive(Debug, Clone)]
pub struct CommandRouter<S> {
    history: CommandHistory<S>,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use std::collections::HashMap;

    use avatar::Slot;
    use glam::Quat;
    use ui::EditorMode;

    use super::*;
    use crate::{
        AvatarEquipAssetPayload, CommandSource, EditorSetModePayload, MaterialSetColorPayload,
        MaterialTarget, SceneSetLockedPayload, SceneSetVisiblePayload, SelectionClearPayload,
        SelectionSetPayload, TransformApplyDeltaPayload, TransformResetPayload,
        TransformSetRotationPayload, TransformSetScalePayload, TransformSetTranslationPayload,
    };

    #[derive(Debug, Default, Clone, PartialEq)]
    struct MemoryState {
        selection: Vec<String>,
        mode: EditorMode,
        slots: HashMap<Slot, String>,
        colors: HashMap<Slot, [f32; 3]>,
        visible: HashMap<String, bool>,
        locked: HashMap<String, bool>,
        translations: HashMap<String, [f32; 3]>,
        rotations: HashMap<String, [f32; 4]>,
        scales: HashMap<String, [f32; 3]>,
    }

    #[derive(Debug, Default)]
    struct MemoryRuntime {
        state: MemoryState,
        valid_assets: Vec<String>,
    }

    impl MemoryRuntime {
        fn with_assets(ids: &[&str]) -> Self {
            Self {
                valid_assets: ids.iter().map(|id| (*id).to_string()).collect(),
                ..Self::default()
            }
        }
    }

    impl CommandRuntime for MemoryRuntime {
        type Snapshot = MemoryState;

        fn snapshot(&self) -> Self::Snapshot {
            self.state.clone()
        }

        fn restore_snapshot(&mut self, snapshot: &Self::Snapshot) -> Result<(), CommandError> {
            self.state = snapshot.clone();
            Ok(())
        }

        fn validate_selection_set(&self, payload: &SelectionSetPayload) -> ValidationResult {
            if payload.object_ids.iter().any(|id| id.trim().is_empty()) {
                ValidationResult::error("EMPTY_OBJECT_ID", "selection contains an empty object id")
            } else {
                ValidationResult::valid()
            }
        }

        fn set_selection(&mut self, payload: &SelectionSetPayload) -> Result<(), CommandError> {
            self.state.selection = payload.object_ids.clone();
            Ok(())
        }

        fn clear_selection(&mut self) -> Result<(), CommandError> {
            self.state.selection.clear();
            Ok(())
        }

        fn validate_avatar_equip_asset(
            &self,
            payload: &AvatarEquipAssetPayload,
        ) -> ValidationResult {
            if !matches!(payload.avatar_id.as_str(), "current" | "avatar_001") {
                return ValidationResult::error("UNKNOWN_AVATAR", "unknown avatar id");
            }
            if self.valid_assets.contains(&payload.asset_id) {
                ValidationResult::valid()
            } else {
                ValidationResult::error("ASSET_NOT_FOUND", "asset not found")
            }
        }

        fn equip_asset(&mut self, payload: &AvatarEquipAssetPayload) -> Result<(), CommandError> {
            self.state
                .slots
                .insert(payload.slot, payload.asset_id.clone());
            Ok(())
        }

        fn validate_material_set_color(
            &self,
            payload: &MaterialSetColorPayload,
        ) -> ValidationResult {
            let in_range = payload
                .color_srgb
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c));
            if in_range {
                ValidationResult::valid()
            } else {
                ValidationResult::error("COLOR_OUT_OF_RANGE", "color must be finite 0..=1")
            }
        }

        fn set_material_color(
            &mut self,
            payload: &MaterialSetColorPayload,
        ) -> Result<(), CommandError> {
            let MaterialTarget::AvatarSlot { slot, .. } = payload.target;
            self.state.colors.insert(slot, payload.color_srgb);
            Ok(())
        }

        fn validate_scene_set_visible(&self, payload: &SceneSetVisiblePayload) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty")
            } else {
                ValidationResult::valid()
            }
        }

        fn scene_set_visible(
            &mut self,
            payload: &SceneSetVisiblePayload,
        ) -> Result<(), CommandError> {
            self.state
                .visible
                .insert(payload.object_id.clone(), payload.visible);
            Ok(())
        }

        fn validate_scene_set_locked(&self, payload: &SceneSetLockedPayload) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty")
            } else {
                ValidationResult::valid()
            }
        }

        fn scene_set_locked(
            &mut self,
            payload: &SceneSetLockedPayload,
        ) -> Result<(), CommandError> {
            self.state
                .locked
                .insert(payload.object_id.clone(), payload.locked);
            Ok(())
        }

        fn validate_transform_set_translation(
            &self,
            payload: &TransformSetTranslationPayload,
        ) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
            }
            if payload.translation.iter().all(|v| v.is_finite()) {
                ValidationResult::valid()
            } else {
                ValidationResult::error("INVALID_TRANSLATION", "translation must be finite")
            }
        }

        fn transform_set_translation(
            &mut self,
            payload: &TransformSetTranslationPayload,
        ) -> Result<(), CommandError> {
            self.state
                .translations
                .insert(payload.object_id.clone(), payload.translation);
            Ok(())
        }

        fn validate_transform_set_rotation(
            &self,
            payload: &TransformSetRotationPayload,
        ) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
            }
            let length = payload.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
            if !payload.rotation.iter().all(|v| v.is_finite()) {
                ValidationResult::error("INVALID_ROTATION", "rotation must be finite")
            } else if (length - 1.0).abs() > 1e-3 {
                ValidationResult::error("NON_UNIT_ROTATION", "rotation must be unit length")
            } else {
                ValidationResult::valid()
            }
        }

        fn transform_set_rotation(
            &mut self,
            payload: &TransformSetRotationPayload,
        ) -> Result<(), CommandError> {
            self.state
                .rotations
                .insert(payload.object_id.clone(), payload.rotation);
            Ok(())
        }

        fn validate_transform_set_scale(
            &self,
            payload: &TransformSetScalePayload,
        ) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
            }
            if payload.scale.iter().all(|v| v.is_finite() && *v > 0.0) {
                ValidationResult::valid()
            } else {
                ValidationResult::error("INVALID_SCALE", "scale must be finite and positive")
            }
        }

        fn transform_set_scale(
            &mut self,
            payload: &TransformSetScalePayload,
        ) -> Result<(), CommandError> {
            self.state
                .scales
                .insert(payload.object_id.clone(), payload.scale);
            Ok(())
        }

        fn validate_transform_apply_delta(
            &self,
            payload: &TransformApplyDeltaPayload,
        ) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
            }
            if let Some(delta) = payload.delta_translation {
                if !delta.iter().all(|v| v.is_finite()) {
                    return ValidationResult::error(
                        "INVALID_TRANSLATION",
                        "delta translation must be finite",
                    );
                }
            }
            if let Some(delta) = payload.delta_rotation_quat {
                let length = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
                if !delta.iter().all(|v| v.is_finite()) || (length - 1.0).abs() > 1e-3 {
                    return ValidationResult::error(
                        "INVALID_ROTATION",
                        "delta rotation must be a finite unit quaternion",
                    );
                }
            }
            if let Some(delta) = payload.delta_scale {
                if !delta.iter().all(|v| v.is_finite()) {
                    return ValidationResult::error("INVALID_SCALE", "delta scale must be finite");
                }
            }
            ValidationResult::valid()
        }

        fn transform_apply_delta(
            &mut self,
            payload: &TransformApplyDeltaPayload,
        ) -> Result<(), CommandError> {
            if let Some(delta) = payload.delta_translation {
                let current = self
                    .state
                    .translations
                    .entry(payload.object_id.clone())
                    .or_insert([0.0, 0.0, 0.0]);
                for (current, delta) in current.iter_mut().zip(delta) {
                    *current += delta;
                }
            }
            if let Some(delta) = payload.delta_rotation_quat {
                let current = self
                    .state
                    .rotations
                    .entry(payload.object_id.clone())
                    .or_insert([0.0, 0.0, 0.0, 1.0]);
                let next = Quat::from_array(delta) * Quat::from_array(*current);
                *current = next.normalize().to_array();
            }
            if let Some(delta) = payload.delta_scale {
                let current = self
                    .state
                    .scales
                    .entry(payload.object_id.clone())
                    .or_insert([1.0, 1.0, 1.0]);
                for (current, delta) in current.iter_mut().zip(delta) {
                    *current += delta;
                }
            }
            Ok(())
        }

        fn validate_transform_reset(&self, payload: &TransformResetPayload) -> ValidationResult {
            if payload.object_id.trim().is_empty() {
                ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty")
            } else {
                ValidationResult::valid()
            }
        }

        fn transform_reset(&mut self, payload: &TransformResetPayload) -> Result<(), CommandError> {
            self.state.translations.remove(&payload.object_id);
            self.state.rotations.remove(&payload.object_id);
            self.state.scales.remove(&payload.object_id);
            Ok(())
        }

        fn validate_editor_set_mode(&self, _payload: &EditorSetModePayload) -> ValidationResult {
            ValidationResult::valid()
        }

        fn set_editor_mode(&mut self, payload: &EditorSetModePayload) -> Result<(), CommandError> {
            self.state.mode = payload.mode;
            Ok(())
        }
    }

    fn env(payload: CommandPayload) -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_test",
            "2026-05-16T00:00:00Z",
            CommandSource::Test,
            payload,
        )
    }

    #[test]
    fn command_envelope_round_trips_json() {
        let command = env(CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
            avatar_id: "current".into(),
            slot: Slot::Hair,
            asset_id: "hair_001".into(),
        }));
        let json = command.serialize_json().unwrap();
        assert!(json.contains("avatar.equip_asset"));
        assert_eq!(CommandEnvelope::deserialize_json(&json).unwrap(), command);
    }

    #[test]
    fn invalid_payload_validation_fails() {
        let runtime = MemoryRuntime::default();
        let router = CommandRouter::default();
        let command = env(CommandPayload::SelectionSet(SelectionSetPayload {
            object_ids: vec!["".into()],
        }));
        let validation = router.validate(&runtime, &command);
        assert!(!validation.valid);
    }

    #[test]
    fn selection_commands_are_undoable() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::SelectionSet(SelectionSetPayload {
                    object_ids: vec!["avatar_001".into()],
                })),
            )
            .unwrap();
        assert_eq!(runtime.state.selection, vec!["avatar_001"]);
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(runtime.state.selection.is_empty());
        router
            .execute(&mut runtime, env(CommandPayload::HistoryRedo))
            .unwrap();
        assert_eq!(runtime.state.selection, vec!["avatar_001"]);
    }

    #[test]
    fn equip_asset_and_material_color_undo_redo() {
        let mut runtime = MemoryRuntime::with_assets(&["top_001"]);
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
                    avatar_id: "current".into(),
                    slot: Slot::Top,
                    asset_id: "top_001".into(),
                })),
            )
            .unwrap();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::MaterialSetColor(MaterialSetColorPayload {
                    target: MaterialTarget::AvatarSlot {
                        avatar_id: "current".into(),
                        slot: Slot::Top,
                    },
                    color_srgb: [0.2, 0.3, 0.4],
                })),
            )
            .unwrap();
        assert_eq!(runtime.state.slots.get(&Slot::Top).unwrap(), "top_001");
        assert_eq!(runtime.state.colors.get(&Slot::Top), Some(&[0.2, 0.3, 0.4]));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(!runtime.state.colors.contains_key(&Slot::Top));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(!runtime.state.slots.contains_key(&Slot::Top));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryRedo))
            .unwrap();
        assert_eq!(runtime.state.slots.get(&Slot::Top).unwrap(), "top_001");
    }

    #[test]
    fn execute_batch_stops_on_first_error() {
        let mut runtime = MemoryRuntime::with_assets(&["top_001"]);
        let mut router = CommandRouter::default();
        let result = router.execute_batch(
            &mut runtime,
            vec![
                env(CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
                    avatar_id: "current".into(),
                    slot: Slot::Top,
                    asset_id: "top_001".into(),
                })),
                env(CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
                    avatar_id: "current".into(),
                    slot: Slot::Hair,
                    asset_id: "missing".into(),
                })),
            ],
        );
        assert!(result.is_err());
        assert_eq!(router.command_history().len(), 1);
        assert_eq!(runtime.state.slots.get(&Slot::Top).unwrap(), "top_001");
    }

    #[test]
    fn dry_run_does_not_mutate_or_record_history() {
        let runtime = MemoryRuntime::with_assets(&["top_001"]);
        let router = CommandRouter::default();
        let command = env(CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
            avatar_id: "current".into(),
            slot: Slot::Top,
            asset_id: "top_001".into(),
        }));
        let result = router.dry_run(&runtime, &command).unwrap();
        assert!(result.dry_run);
        assert!(runtime.state.slots.is_empty());
        assert_eq!(router.command_history().len(), 0);
    }

    #[test]
    fn selection_clear_serializes_as_empty_payload() {
        let command = env(CommandPayload::SelectionClear(
            SelectionClearPayload::default(),
        ));
        let json = command.serialize_json().unwrap();
        assert!(json.contains("selection.clear"));
        assert_eq!(CommandEnvelope::deserialize_json(&json).unwrap(), command);
    }

    #[test]
    fn scene_set_visible_undoable() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::SceneSetVisible(SceneSetVisiblePayload {
                    object_id: "mesh_top_001".into(),
                    visible: false,
                })),
            )
            .unwrap();
        assert_eq!(runtime.state.visible.get("mesh_top_001"), Some(&false));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(!runtime.state.visible.contains_key("mesh_top_001"));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryRedo))
            .unwrap();
        assert_eq!(runtime.state.visible.get("mesh_top_001"), Some(&false));
    }

    #[test]
    fn scene_set_locked_serializes_as_camel_case() {
        let command = env(CommandPayload::SceneSetLocked(SceneSetLockedPayload {
            object_id: "bone_head".into(),
            locked: true,
        }));
        let json = command.serialize_json().unwrap();
        assert!(json.contains("scene.set_locked"));
        assert!(json.contains("\"objectId\""));
        assert!(json.contains("\"locked\""));
        assert_eq!(CommandEnvelope::deserialize_json(&json).unwrap(), command);
    }

    #[test]
    fn transform_set_translation_is_undoable() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::TransformSetTranslation(
                    TransformSetTranslationPayload {
                        object_id: "mesh_top_001".into(),
                        translation: [0.0, 1.5, 0.0],
                    },
                )),
            )
            .unwrap();
        assert_eq!(
            runtime.state.translations.get("mesh_top_001"),
            Some(&[0.0, 1.5, 0.0])
        );
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(!runtime.state.translations.contains_key("mesh_top_001"));
    }

    #[test]
    fn transform_set_scale_is_undoable() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::TransformSetScale(
                    TransformSetScalePayload {
                        object_id: "mesh_top_001".into(),
                        scale: [1.2, 0.9, 1.1],
                    },
                )),
            )
            .unwrap();
        assert_eq!(
            runtime.state.scales.get("mesh_top_001"),
            Some(&[1.2, 0.9, 1.1])
        );
        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert!(!runtime.state.scales.contains_key("mesh_top_001"));
        router
            .execute(&mut runtime, env(CommandPayload::HistoryRedo))
            .unwrap();
        assert_eq!(
            runtime.state.scales.get("mesh_top_001"),
            Some(&[1.2, 0.9, 1.1])
        );
    }

    #[test]
    fn transform_apply_delta_composes_existing_state() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        runtime
            .state
            .translations
            .insert("bone_head".into(), [1.0, 2.0, 3.0]);
        runtime
            .state
            .rotations
            .insert("bone_head".into(), [0.0, 0.0, 0.0, 1.0]);
        runtime
            .state
            .scales
            .insert("bone_head".into(), [1.0, 1.0, 1.0]);

        router
            .execute(
                &mut runtime,
                env(CommandPayload::TransformApplyDelta(
                    TransformApplyDeltaPayload {
                        object_id: "bone_head".into(),
                        delta_translation: Some([0.5, -0.25, 1.0]),
                        delta_rotation_quat: Some(
                            Quat::from_rotation_y(0.5).normalize().to_array(),
                        ),
                        delta_scale: Some([0.25, 0.0, -0.25]),
                    },
                )),
            )
            .unwrap();

        assert_eq!(
            runtime.state.translations.get("bone_head"),
            Some(&[1.5, 1.75, 4.0])
        );
        assert_eq!(
            runtime.state.scales.get("bone_head"),
            Some(&[1.25, 1.0, 0.75])
        );
        let rotation = runtime.state.rotations.get("bone_head").copied().unwrap();
        assert!((Quat::from_array(rotation).length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn transform_reset_restores_identity_via_undo_redo() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        runtime
            .state
            .translations
            .insert("mesh_top_001".into(), [0.1, 0.2, 0.3]);
        runtime.state.rotations.insert(
            "mesh_top_001".into(),
            Quat::from_rotation_x(0.25).to_array(),
        );
        runtime
            .state
            .scales
            .insert("mesh_top_001".into(), [1.4, 0.8, 1.1]);

        router
            .execute(
                &mut runtime,
                env(CommandPayload::TransformReset(TransformResetPayload {
                    object_id: "mesh_top_001".into(),
                })),
            )
            .unwrap();

        assert!(!runtime.state.translations.contains_key("mesh_top_001"));
        assert!(!runtime.state.rotations.contains_key("mesh_top_001"));
        assert!(!runtime.state.scales.contains_key("mesh_top_001"));

        router
            .execute(&mut runtime, env(CommandPayload::HistoryUndo))
            .unwrap();
        assert_eq!(
            runtime.state.translations.get("mesh_top_001"),
            Some(&[0.1, 0.2, 0.3])
        );
        assert_eq!(
            runtime.state.scales.get("mesh_top_001"),
            Some(&[1.4, 0.8, 1.1])
        );

        router
            .execute(&mut runtime, env(CommandPayload::HistoryRedo))
            .unwrap();
        assert!(!runtime.state.translations.contains_key("mesh_top_001"));
        assert!(!runtime.state.rotations.contains_key("mesh_top_001"));
        assert!(!runtime.state.scales.contains_key("mesh_top_001"));
    }

    #[test]
    fn transform_rotation_serializes_as_camel_case() {
        let command = env(CommandPayload::TransformSetRotation(
            TransformSetRotationPayload {
                object_id: "bone_head".into(),
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
        ));
        let json = command.serialize_json().unwrap();
        assert!(json.contains("transform.set_rotation"));
        assert!(json.contains("\"objectId\""));
        assert!(json.contains("\"rotation\""));
        assert_eq!(CommandEnvelope::deserialize_json(&json).unwrap(), command);
    }

    #[test]
    fn editor_set_mode_executes_without_recording_undo() {
        let mut runtime = MemoryRuntime::default();
        let mut router = CommandRouter::default();
        router
            .execute(
                &mut runtime,
                env(CommandPayload::EditorSetMode(EditorSetModePayload {
                    mode: EditorMode::Object,
                })),
            )
            .unwrap();
        assert_eq!(runtime.state.mode, EditorMode::Object);
        assert_eq!(router.command_history().len(), 0);
    }

    #[test]
    fn editor_set_mode_serializes_as_camel_case() {
        let command = env(CommandPayload::EditorSetMode(EditorSetModePayload {
            mode: EditorMode::Material,
        }));
        let json = command.serialize_json().unwrap();
        assert!(json.contains("editor.set_mode"));
        assert!(json.contains("\"mode\""));
        assert!(json.contains("\"material\""));
        assert_eq!(CommandEnvelope::deserialize_json(&json).unwrap(), command);
    }
}

impl<S> Default for CommandRouter<S> {
    fn default() -> Self {
        Self {
            history: CommandHistory::default(),
        }
    }
}

impl<S: Clone> CommandRouter<S> {
    pub fn validate<R>(&self, runtime: &R, command: &CommandEnvelope) -> ValidationResult
    where
        R: CommandRuntime<Snapshot = S>,
    {
        if command.name != command.payload.name() {
            return ValidationResult::error(
                "NAME_PAYLOAD_MISMATCH",
                format!(
                    "command name {} does not match payload {}",
                    command.name.as_str(),
                    command.payload.name().as_str()
                ),
            );
        }
        match &command.payload {
            CommandPayload::SelectionSet(payload) => runtime.validate_selection_set(payload),
            CommandPayload::SelectionClear(_) => ValidationResult::valid(),
            CommandPayload::AvatarEquipAsset(payload) => {
                runtime.validate_avatar_equip_asset(payload)
            }
            CommandPayload::MaterialSetColor(payload) => {
                runtime.validate_material_set_color(payload)
            }
            CommandPayload::SceneSetVisible(payload) => runtime.validate_scene_set_visible(payload),
            CommandPayload::SceneSetLocked(payload) => runtime.validate_scene_set_locked(payload),
            CommandPayload::TransformSetTranslation(payload) => {
                runtime.validate_transform_set_translation(payload)
            }
            CommandPayload::TransformSetRotation(payload) => {
                runtime.validate_transform_set_rotation(payload)
            }
            CommandPayload::TransformSetScale(payload) => {
                runtime.validate_transform_set_scale(payload)
            }
            CommandPayload::TransformApplyDelta(payload) => {
                runtime.validate_transform_apply_delta(payload)
            }
            CommandPayload::TransformReset(payload) => runtime.validate_transform_reset(payload),
            CommandPayload::EditorSetMode(payload) => runtime.validate_editor_set_mode(payload),
            CommandPayload::HistoryUndo => {
                if self.can_undo() {
                    ValidationResult::valid()
                } else {
                    ValidationResult::error("NOTHING_TO_UNDO", "nothing to undo")
                }
            }
            CommandPayload::HistoryRedo => {
                if self.can_redo() {
                    ValidationResult::valid()
                } else {
                    ValidationResult::error("NOTHING_TO_REDO", "nothing to redo")
                }
            }
            CommandPayload::LegacyUndoable { .. } => ValidationResult::valid(),
        }
    }

    pub fn dry_run<R>(
        &self,
        runtime: &R,
        command: &CommandEnvelope,
    ) -> Result<CommandResult, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        self.require_valid(runtime, command)?;
        Ok(CommandResult::new(
            command,
            false,
            true,
            "dry-run valid".to_string(),
        ))
    }

    pub fn execute<R>(
        &mut self,
        runtime: &mut R,
        command: CommandEnvelope,
    ) -> Result<CommandResult, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        match command.payload {
            CommandPayload::HistoryUndo => self.undo(runtime, &command),
            CommandPayload::HistoryRedo => self.redo(runtime, &command),
            CommandPayload::EditorSetMode(ref payload) => {
                self.require_valid(runtime, &command)?;
                runtime.set_editor_mode(payload)?;
                Ok(CommandResult::new(
                    &command,
                    true,
                    false,
                    "mode changed".to_string(),
                ))
            }
            ref payload => {
                let command = CommandEnvelope {
                    payload: payload.clone(),
                    ..command
                };
                self.require_valid(runtime, &command)?;
                let before = runtime.snapshot();
                match payload {
                    CommandPayload::SelectionSet(payload) => runtime.set_selection(payload)?,
                    CommandPayload::SelectionClear(_) => runtime.clear_selection()?,
                    CommandPayload::AvatarEquipAsset(payload) => runtime.equip_asset(payload)?,
                    CommandPayload::MaterialSetColor(payload) => {
                        runtime.set_material_color(payload)?
                    }
                    CommandPayload::SceneSetVisible(payload) => {
                        runtime.scene_set_visible(payload)?
                    }
                    CommandPayload::SceneSetLocked(payload) => runtime.scene_set_locked(payload)?,
                    CommandPayload::TransformSetTranslation(payload) => {
                        runtime.transform_set_translation(payload)?
                    }
                    CommandPayload::TransformSetRotation(payload) => {
                        runtime.transform_set_rotation(payload)?
                    }
                    CommandPayload::TransformSetScale(payload) => {
                        runtime.transform_set_scale(payload)?
                    }
                    CommandPayload::TransformApplyDelta(payload) => {
                        runtime.transform_apply_delta(payload)?
                    }
                    CommandPayload::TransformReset(payload) => runtime.transform_reset(payload)?,
                    CommandPayload::EditorSetMode(_) => {
                        return Err(CommandError::Unsupported(command.name.as_str().to_string()));
                    }
                    CommandPayload::LegacyUndoable { .. }
                    | CommandPayload::HistoryUndo
                    | CommandPayload::HistoryRedo => {
                        return Err(CommandError::Unsupported(command.name.as_str().to_string()));
                    }
                }
                let after = runtime.snapshot();
                self.history.push(UndoRecord {
                    command: command.clone(),
                    before,
                    after,
                });
                Ok(CommandResult::new(
                    &command,
                    true,
                    false,
                    "command executed".to_string(),
                ))
            }
        }
    }

    pub fn execute_batch<R>(
        &mut self,
        runtime: &mut R,
        commands: Vec<CommandEnvelope>,
    ) -> Result<Vec<CommandResult>, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        let mut out = Vec::with_capacity(commands.len());
        for command in commands {
            out.push(self.execute(runtime, command)?);
        }
        Ok(out)
    }

    pub fn replay<R>(
        &mut self,
        runtime: &mut R,
        commands: Vec<CommandEnvelope>,
    ) -> Result<Vec<CommandResult>, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        self.execute_batch(runtime, commands)
    }

    pub fn record_external(
        &mut self,
        command: CommandEnvelope,
        before: S,
        after: S,
    ) -> Result<(), CommandError> {
        if command.name != CommandName::LegacyUndoable {
            return Err(CommandError::Unsupported(format!(
                "external records must use {}, got {}",
                CommandName::LegacyUndoable.as_str(),
                command.name.as_str()
            )));
        }
        self.history.push(UndoRecord {
            command,
            before,
            after,
        });
        Ok(())
    }

    pub fn undo<R>(
        &mut self,
        runtime: &mut R,
        command: &CommandEnvelope,
    ) -> Result<CommandResult, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        let record = self.history.pop_undo().ok_or(CommandError::NothingToUndo)?;
        runtime.restore_snapshot(&record.before)?;
        self.history.push_redo(record);
        Ok(CommandResult::new(
            command,
            true,
            false,
            "undo complete".to_string(),
        ))
    }

    pub fn redo<R>(
        &mut self,
        runtime: &mut R,
        command: &CommandEnvelope,
    ) -> Result<CommandResult, CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        let record = self.history.pop_redo().ok_or(CommandError::NothingToRedo)?;
        runtime.restore_snapshot(&record.after)?;
        self.history.push_done_without_clearing_redo(record);
        Ok(CommandResult::new(
            command,
            true,
            false,
            "redo complete".to_string(),
        ))
    }

    pub fn command_history(&self) -> &CommandHistory<S> {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn require_valid<R>(&self, runtime: &R, command: &CommandEnvelope) -> Result<(), CommandError>
    where
        R: CommandRuntime<Snapshot = S>,
    {
        let validation = self.validate(runtime, command);
        if validation.valid {
            Ok(())
        } else {
            Err(CommandError::Validation(
                validation
                    .first_error_message()
                    .unwrap_or_else(|| "command is invalid".to_string()),
            ))
        }
    }
}

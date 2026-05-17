//! Serializable command layer for human UI, scripts, and future agent/MCP use.

mod command;
mod context;
mod error;
mod history;
mod router;
mod validation;

pub use command::{
    AvatarEquipAssetPayload, CommandEnvelope, CommandName, CommandPayload, CommandResult,
    CommandSource, EditorSetModePayload, MaterialSetColorPayload, MaterialTarget,
    SceneSetLockedPayload, SceneSetVisiblePayload, SelectionClearPayload, SelectionSetPayload,
    TransformApplyDeltaPayload, TransformResetPayload, TransformSetRotationPayload,
    TransformSetScalePayload, TransformSetTranslationPayload,
};
pub use context::CommandRuntime;
pub use error::CommandError;
pub use history::{CommandHistory, UndoRecord};
pub use router::CommandRouter;
pub use validation::{ValidationResult, ValidationSeverity, ValidationWarning};

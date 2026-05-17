use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CommandError {
    #[error("command validation failed: {0}")]
    Validation(String),
    #[error("command name {name:?} does not match payload {payload:?}")]
    NamePayloadMismatch { name: String, payload: String },
    #[error("unsupported command: {0}")]
    Unsupported(String),
    #[error("runtime rejected command: {0}")]
    Runtime(String),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("could not serialize command: {0}")]
    Serialize(String),
    #[error("could not deserialize command: {0}")]
    Deserialize(String),
}

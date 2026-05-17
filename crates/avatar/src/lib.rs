//! Avatar slot model, customization state, and save/load.
//!
//! Phase 7 owns slot/body compatibility; later phases add color, expression,
//! and schema-v1 save/load.

pub mod avatar;
pub mod colors;
pub mod customization;
pub mod expressions;
pub mod save;
pub mod slots;

pub use avatar::{Avatar, DEFAULT_BODY_TYPE};
pub use expressions::Expression;
pub use save::{new_character_id, AvatarSave, CharacterStore, SavedCharacterSummary};
pub use slots::Slot;

//! Avatar save/load (schema v1). See `docs/save_format.md`.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Avatar, Expression, Slot, DEFAULT_BODY_TYPE};

pub const SAVE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("save schema {found} is not supported; expected {expected}")]
    Schema { found: u32, expected: u32 },
}

pub type Result<T> = std::result::Result<T, SaveError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarSave {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub base_body: String,
    pub body_type: String,
    pub skin_tone: [f32; 3],
    #[serde(default)]
    pub slots: HashMap<Slot, String>,
    #[serde(default)]
    pub colors: HashMap<Slot, [f32; 3]>,
    #[serde(default)]
    pub expression: Expression,
    #[serde(default)]
    pub animation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCharacterSummary {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CharacterStore {
    root: PathBuf,
}

impl AvatarSave {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        avatar: &Avatar,
        animation: Option<String>,
    ) -> Self {
        let now = timestamp_now();
        Self::from_parts(id, name, avatar, animation, now.clone(), now)
    }

    pub fn from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        avatar: &Avatar,
        animation: Option<String>,
        created_at: String,
        updated_at: String,
    ) -> Self {
        let base_body = avatar
            .equipped(Slot::Body)
            .unwrap_or(DEFAULT_BODY_TYPE)
            .to_string();
        let slots = avatar
            .iter()
            .filter(|(slot, _)| *slot != Slot::Body)
            .map(|(slot, asset_id)| (slot, asset_id.to_string()))
            .collect();
        Self {
            schema_version: SAVE_SCHEMA_VERSION,
            id: sanitize_id(id.into()),
            name: clamp_name(name.into()),
            created_at,
            updated_at,
            base_body,
            body_type: avatar.body_type.clone(),
            skin_tone: avatar.skin_tone,
            slots,
            colors: avatar.slot_colors.clone(),
            expression: avatar.expression,
            animation,
        }
    }

    pub fn into_avatar(&self) -> Result<Avatar> {
        self.validate_schema()?;
        let mut avatar = Avatar {
            body_type: self.body_type.clone(),
            skin_tone: self.skin_tone,
            expression: self.expression,
            ..Avatar::default()
        };
        if self.base_body != DEFAULT_BODY_TYPE {
            avatar.equip(Slot::Body, self.base_body.clone());
        }
        for (slot, asset_id) in &self.slots {
            if *slot != Slot::Body {
                avatar.equip(*slot, asset_id.clone());
            }
        }
        avatar.slot_colors = self.colors.clone();
        Ok(avatar)
    }

    pub fn validate_schema(&self) -> Result<()> {
        if self.schema_version != SAVE_SCHEMA_VERSION {
            return Err(SaveError::Schema {
                found: self.schema_version,
                expected: SAVE_SCHEMA_VERSION,
            });
        }
        Ok(())
    }
}

impl CharacterStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self, save: &AvatarSave) -> Result<PathBuf> {
        save.validate_schema()?;
        fs::create_dir_all(&self.root).map_err(|source| SaveError::Io {
            path: self.root.clone(),
            source,
        })?;
        let path = self.path_for(&save.id);
        let text = serde_json::to_string_pretty(save).map_err(|source| SaveError::Json {
            path: path.clone(),
            source,
        })?;
        fs::write(&path, text).map_err(|source| SaveError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    pub fn load(&self, id: &str) -> Result<AvatarSave> {
        self.load_path(&self.path_for(id))
    }

    pub fn load_path(&self, path: &Path) -> Result<AvatarSave> {
        let text = fs::read_to_string(path).map_err(|source| SaveError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let save: AvatarSave = serde_json::from_str(&text).map_err(|source| SaveError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        save.validate_schema()?;
        Ok(save)
    }

    pub fn list(&self) -> Result<Vec<SavedCharacterSummary>> {
        match fs::read_dir(&self.root) {
            Ok(entries) => {
                let mut out = Vec::new();
                for entry in entries {
                    let entry = entry.map_err(|source| SaveError::Io {
                        path: self.root.clone(),
                        source,
                    })?;
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    match self.load_path(&path) {
                        Ok(save) => out.push(SavedCharacterSummary {
                            id: save.id,
                            name: save.name,
                            updated_at: save.updated_at,
                            path,
                        }),
                        Err(_err) => {}
                    }
                }
                out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.name.cmp(&b.name)));
                Ok(out)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(source) => Err(SaveError::Io {
                path: self.root.clone(),
                source,
            }),
        }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root
            .join(format!("{}.json", sanitize_id(id.to_string())))
    }
}

pub fn new_character_id() -> String {
    format!("char_{}", unix_millis())
}

fn timestamp_now() -> String {
    unix_millis().to_string()
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn sanitize_id(raw: String) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    if out.is_empty() {
        new_character_id()
    } else {
        out
    }
}

fn clamp_name(mut name: String) -> String {
    if name.trim().is_empty() {
        name = "Untitled Avatar".to_string();
    }
    name.truncate(64);
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> CharacterStore {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!(
            "avatar_store_test_{}_{}",
            std::process::id(),
            unique
        ));
        CharacterStore::new(root)
    }

    #[test]
    fn save_round_trips_avatar_choices() {
        let store = temp_store();
        let mut avatar = Avatar {
            body_type: "body_phase4_rig_001".into(),
            ..Avatar::default()
        };
        avatar.equip(Slot::Body, "body_phase4_rig_001");
        avatar.equip(Slot::Top, "top_phase7_basic_001");
        avatar.set_slot_color(Slot::Top, [0.1, 0.2, 0.3]);
        avatar.expression = Expression::Happy;

        let save = AvatarSave::from_parts(
            "char_test",
            "Test",
            &avatar,
            Some("idle".into()),
            "1".into(),
            "2".into(),
        );
        store.save(&save).unwrap();
        let loaded = store.load("char_test").unwrap();
        let restored = loaded.into_avatar().unwrap();

        assert_eq!(restored.equipped(Slot::Body), Some("body_phase4_rig_001"));
        assert_eq!(restored.equipped(Slot::Top), Some("top_phase7_basic_001"));
        assert_eq!(restored.slot_color(Slot::Top), Some([0.1, 0.2, 0.3]));
        assert_eq!(restored.expression, Expression::Happy);
    }

    #[test]
    fn gallery_summaries_sort_newest_first() {
        let store = temp_store();
        let avatar = Avatar::default();
        let older = AvatarSave::from_parts("older", "Older", &avatar, None, "1".into(), "1".into());
        let newer = AvatarSave::from_parts("newer", "Newer", &avatar, None, "2".into(), "2".into());
        store.save(&older).unwrap();
        store.save(&newer).unwrap();

        let summaries = store.list().unwrap();
        assert_eq!(summaries[0].id, "newer");
        assert_eq!(summaries[1].id, "older");
    }
}

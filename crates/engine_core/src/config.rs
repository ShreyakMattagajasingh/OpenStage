use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ui::EditorMode;

use crate::error::{CoreError, Result};

pub const CONFIG_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub window: WindowConfig,
    pub render: RenderConfig,
    pub paths: PathsConfig,
    pub editor: EditorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// RGBA in linear-ish 0..1. Surface is sRGB so the GPU does the conversion.
    pub clear_color: [f32; 4],
    /// TODO(phase-14): wire this through the pipeline. Ignored in M1.
    pub msaa_samples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub user_data: PathBuf,
    pub assets: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub last_mode: EditorMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            window: WindowConfig {
                title: "Avatar Studio".into(),
                // Stage 20: widened from 1340 to 1480 to accommodate the
                // new right-side Inspector panel (280 px) while leaving the
                // scene viewport ~900 px wide.
                width: 1480,
                height: 780,
                vsync: true,
            },
            render: RenderConfig {
                // #1e1e2e - a calm dark slate; clearly "rendering happened".
                clear_color: [0.118, 0.118, 0.180, 1.0],
                msaa_samples: 1,
            },
            paths: PathsConfig {
                user_data: PathBuf::from("user_data"),
                assets: PathBuf::from("assets"),
            },
            editor: EditorConfig {
                last_mode: EditorMode::Character,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigV1 {
    pub schema_version: u32,
    pub window: WindowConfig,
    pub render: RenderConfig,
    pub paths: PathsConfig,
}

impl Config {
    /// Load from `path`. If the file doesn't exist, write defaults and return them.
    /// If the file exists but is unreadable/unparseable, surface the error rather
    /// than silently clobbering the user's settings.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                if text.trim_matches(char::from(0)).trim().is_empty() {
                    let cfg = Config::default();
                    cfg.save(path)?;
                    return Ok(cfg);
                }
                let raw: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(raw) => raw,
                    Err(_) => {
                        let cfg = Config::default();
                        cfg.save(path)?;
                        return Ok(cfg);
                    }
                };
                let version = raw
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1) as u32;
                let mut cfg = match version {
                    CONFIG_SCHEMA_VERSION => {
                        serde_json::from_value(raw).map_err(|e| CoreError::json(path, e))?
                    }
                    1 => {
                        let legacy: ConfigV1 =
                            serde_json::from_value(raw).map_err(|e| CoreError::json(path, e))?;
                        Self {
                            schema_version: CONFIG_SCHEMA_VERSION,
                            window: legacy.window,
                            render: legacy.render,
                            paths: legacy.paths,
                            editor: EditorConfig {
                                last_mode: EditorMode::Character,
                            },
                        }
                    }
                    other => {
                        return Err(CoreError::Config(format!(
                            "settings.json schema_version {} unsupported; expected <= {}",
                            other, CONFIG_SCHEMA_VERSION
                        )));
                    }
                };
                if cfg.window.width == 0 || cfg.window.height == 0 {
                    cfg.window.width = 1280;
                    cfg.window.height = 720;
                }
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Config::default();
                cfg.save(path)?;
                Ok(cfg)
            }
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| CoreError::json(path, e))?;
        std::fs::write(path, text).map_err(|e| CoreError::io(path, e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!(
            "avatar_studio_config_{}_{}_{}.json",
            name,
            std::process::id(),
            unique
        ))
    }

    #[test]
    fn schema_version_two_round_trips_editor_mode() {
        let path = temp_path("v2");
        let cfg = Config {
            editor: EditorConfig {
                last_mode: EditorMode::Material,
            },
            ..Config::default()
        };
        cfg.save(&path).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(loaded.editor.last_mode, EditorMode::Material);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migrates_v1_to_character_mode() {
        let path = temp_path("v1");
        let legacy = serde_json::json!({
            "schema_version": 1,
            "window": {
                "title": "Avatar Studio",
                "width": 1480,
                "height": 780,
                "vsync": true
            },
            "render": {
                "clear_color": [0.118, 0.118, 0.180, 1.0],
                "msaa_samples": 1
            },
            "paths": {
                "user_data": "user_data",
                "assets": "assets"
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(loaded.editor.last_mode, EditorMode::Character);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recovers_from_empty_or_corrupt_settings_file() {
        let path = temp_path("corrupt");
        std::fs::write(&path, vec![0u8; 32]).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(loaded.editor.last_mode, EditorMode::Character);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("\"schema_version\": 2"));
        let _ = std::fs::remove_file(path);
    }
}

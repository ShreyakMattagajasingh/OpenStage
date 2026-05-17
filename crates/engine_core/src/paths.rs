//! Runtime path resolution.
//!
//! The same binary needs to run from three different layouts:
//!
//! 1. **`cargo run`** from the workspace root — `assets/` and `user_data/`
//!    live in the workspace CWD; this is the developer path.
//! 2. **Portable** — a self-contained directory the user dragged anywhere;
//!    both `assets/` and `user_data/` sit next to the exe. Triggered by
//!    `AVATAR_STUDIO_PORTABLE=1` or a marker file `avatar_studio.portable`
//!    next to the exe.
//! 3. **Installed** (MSI, Phase 15) — `assets/` ships next to the exe under
//!    `%LOCALAPPDATA%\Programs\AvatarStudio\`; user data goes to
//!    `%LOCALAPPDATA%\AvatarStudio\`.
//!
//! Resolution priority is the same for both roots:
//!     env override > portable marker > workspace fallback > installed default.
//!
//! Cargo-run is detected by inspecting `current_exe()`: if its parent ends in
//! `target/debug` or `target/release`, we are running from the workspace.
//! Otherwise the exe is treated as installed.

use std::path::{Path, PathBuf};

pub const PORTABLE_MARKER: &str = "avatar_studio.portable";
pub const ASSETS_ENV: &str = "AVATAR_STUDIO_ASSETS";
pub const USER_DATA_ENV: &str = "AVATAR_STUDIO_USER_DATA";
pub const PORTABLE_ENV: &str = "AVATAR_STUDIO_PORTABLE";
pub const USER_DATA_DIR_NAME: &str = "AvatarStudio";

/// Snapshot of the runtime layout. Built once via [`resolve`] at startup.
#[derive(Debug, Clone)]
pub struct ResolvedPaths {
    pub assets: PathBuf,
    pub user_data: PathBuf,
    pub mode: PathMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    /// `cargo run` from the workspace — exe lives under `target/...`.
    Workspace,
    /// Portable bundle — exe sits with its own `assets/` and `user_data/`.
    Portable,
    /// Installed — exe ships read-only assets next to it; user data is in LOCALAPPDATA.
    Installed,
}

impl ResolvedPaths {
    pub fn settings(&self) -> PathBuf {
        self.user_data.join("settings.json")
    }
    pub fn catalog_db(&self) -> PathBuf {
        self.user_data.join("asset_catalog.sqlite")
    }
    pub fn characters_dir(&self) -> PathBuf {
        self.user_data.join("characters")
    }
    pub fn exports_dir(&self) -> PathBuf {
        self.user_data.join("exports")
    }
    pub fn debug_screenshots_dir(&self) -> PathBuf {
        self.user_data.join("debug_screenshots")
    }
    pub fn perf_dir(&self) -> PathBuf {
        self.user_data.join("perf")
    }
    pub fn assets_processed(&self) -> PathBuf {
        self.assets.join("processed")
    }
}

/// Resolve runtime paths from real environment + `current_exe()`.
pub fn resolve() -> ResolvedPaths {
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    let env = EnvProbe::from_real_env();
    resolve_with(exe_dir, &env)
}

/// Pure resolver — used by [`resolve`] and tests.
pub fn resolve_with(exe_dir: Option<&Path>, env: &EnvProbe) -> ResolvedPaths {
    let mode = classify_mode(exe_dir, env);

    let assets = if let Some(p) = env.assets.as_ref() {
        PathBuf::from(p)
    } else {
        match mode {
            PathMode::Workspace => PathBuf::from("assets"),
            PathMode::Portable | PathMode::Installed => exe_dir
                .map(|d| d.join("assets"))
                .unwrap_or_else(|| PathBuf::from("assets")),
        }
    };

    let user_data = if let Some(p) = env.user_data.as_ref() {
        PathBuf::from(p)
    } else {
        match mode {
            PathMode::Workspace => PathBuf::from("user_data"),
            PathMode::Portable => exe_dir
                .map(|d| d.join("user_data"))
                .unwrap_or_else(|| PathBuf::from("user_data")),
            PathMode::Installed => env
                .local_app_data
                .clone()
                .map(|d| d.join(USER_DATA_DIR_NAME))
                .unwrap_or_else(|| PathBuf::from("user_data")),
        }
    };

    ResolvedPaths {
        assets,
        user_data,
        mode,
    }
}

fn classify_mode(exe_dir: Option<&Path>, env: &EnvProbe) -> PathMode {
    if env.portable {
        return PathMode::Portable;
    }
    if let Some(dir) = exe_dir {
        if dir.join(PORTABLE_MARKER).exists() {
            return PathMode::Portable;
        }
        if exe_dir_under_target(dir) {
            return PathMode::Workspace;
        }
    }
    PathMode::Installed
}

fn exe_dir_under_target(dir: &Path) -> bool {
    // Match any directory whose final two segments are `target` + `{debug,release,...}`
    // (this covers `target/debug`, `target/release`, and per-profile dirs).
    let mut iter = dir.components().rev();
    let last = iter.next().and_then(|c| c.as_os_str().to_str());
    let parent = iter.next().and_then(|c| c.as_os_str().to_str());
    matches!((parent, last), (Some("target"), Some(_)))
}

/// Environment inputs the resolver consults. Tests build this directly;
/// production code uses [`EnvProbe::from_real_env`].
#[derive(Debug, Clone, Default)]
pub struct EnvProbe {
    pub assets: Option<String>,
    pub user_data: Option<String>,
    pub portable: bool,
    pub local_app_data: Option<PathBuf>,
}

impl EnvProbe {
    pub fn from_real_env() -> Self {
        Self {
            assets: std::env::var(ASSETS_ENV).ok(),
            user_data: std::env::var(USER_DATA_ENV).ok(),
            portable: matches!(
                std::env::var(PORTABLE_ENV).as_deref(),
                Ok("1") | Ok("true") | Ok("TRUE")
            ),
            local_app_data: dirs::data_local_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with_local_appdata(p: &str) -> EnvProbe {
        EnvProbe {
            local_app_data: Some(PathBuf::from(p)),
            ..EnvProbe::default()
        }
    }

    #[test]
    fn assets_root_prefers_env() {
        let env = EnvProbe {
            assets: Some("/tmp/custom-assets".into()),
            local_app_data: Some(PathBuf::from("/tmp/appdata")),
            ..EnvProbe::default()
        };
        let r = resolve_with(Some(Path::new("C:/Program Files/AvatarStudio")), &env);
        assert_eq!(r.assets, PathBuf::from("/tmp/custom-assets"));
    }

    #[test]
    fn user_data_root_portable_marker_triggers_portable_mode() {
        let tmp =
            std::env::temp_dir().join(format!("avatar_studio_paths_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let marker = tmp.join(PORTABLE_MARKER);
        std::fs::write(&marker, "").unwrap();

        let env = env_with_local_appdata("/tmp/appdata");
        let r = resolve_with(Some(&tmp), &env);
        assert_eq!(r.mode, PathMode::Portable);
        assert_eq!(r.user_data, tmp.join("user_data"));
        assert_eq!(r.assets, tmp.join("assets"));

        let _ = std::fs::remove_file(&marker);
        let _ = std::fs::remove_dir(&tmp);
    }

    #[test]
    fn user_data_root_local_appdata_in_installed_mode() {
        // exe path is NOT under target/ and no portable marker → installed.
        let exe_dir = PathBuf::from("C:/Program Files/AvatarStudio");
        let env = env_with_local_appdata("C:/Users/test/AppData/Local");
        let r = resolve_with(Some(&exe_dir), &env);
        assert_eq!(r.mode, PathMode::Installed);
        assert_eq!(
            r.user_data,
            PathBuf::from("C:/Users/test/AppData/Local").join(USER_DATA_DIR_NAME)
        );
        assert_eq!(r.assets, exe_dir.join("assets"));
    }

    #[test]
    fn workspace_fallback_when_exe_in_target_debug() {
        let exe_dir = PathBuf::from("C:/work/avatar-studio/target/debug");
        let env = EnvProbe::default();
        let r = resolve_with(Some(&exe_dir), &env);
        assert_eq!(r.mode, PathMode::Workspace);
        assert_eq!(r.assets, PathBuf::from("assets"));
        assert_eq!(r.user_data, PathBuf::from("user_data"));
    }

    #[test]
    fn portable_env_overrides_target_dir() {
        // Even if the exe sits under target/, AVATAR_STUDIO_PORTABLE=1 wins.
        let exe_dir = PathBuf::from("C:/work/avatar-studio/target/release");
        let env = EnvProbe {
            portable: true,
            ..EnvProbe::default()
        };
        let r = resolve_with(Some(&exe_dir), &env);
        assert_eq!(r.mode, PathMode::Portable);
        assert_eq!(r.assets, exe_dir.join("assets"));
        assert_eq!(r.user_data, exe_dir.join("user_data"));
    }
}

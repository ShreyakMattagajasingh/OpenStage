//! Filesystem scanner: walk `<root>/metadata/*.json` and deserialize each.

use std::path::Path;

use tracing::{debug, warn};

use crate::metadata::AssetMeta;

/// Scan every `*.json` file under `<root>/metadata/` and return the
/// successfully-parsed `AssetMeta` entries. Files that fail to read or parse
/// are logged at WARN and skipped — one bad file doesn't kill the catalog.
pub fn scan_metadata_dir(root: &Path) -> Vec<AssetMeta> {
    let dir = root.join("metadata");
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(e) => {
            warn!(path = %dir.display(), error = %e, "metadata directory unreadable; catalog will be empty");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<AssetMeta>(&text) {
                Ok(meta) => {
                    debug!(id = %meta.id, file = %path.display(), "metadata parsed");
                    out.push(meta);
                }
                Err(e) => {
                    warn!(file = %path.display(), error = %e, "metadata JSON parse failed; skipping");
                }
            },
            Err(e) => {
                warn!(file = %path.display(), error = %e, "metadata file unreadable; skipping");
            }
        }
    }
    out
}

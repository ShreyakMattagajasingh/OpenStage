//! Hand-rolled GLB container writer.
//!
//! The GLB binary container is small enough to emit directly:
//!   * 12-byte header (magic + version + total_length)
//!   * chunk 0: JSON chunk (4-byte aligned, space-padded)
//!   * chunk 1: BIN chunk  (4-byte aligned, zero-padded)
//!
//! See <https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#binary-gltf-layout>.

use std::path::Path;

use anyhow::{Context, Result};
use gltf::json as gj;

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const GLB_VERSION: u32 = 2;
const CHUNK_TYPE_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_TYPE_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Encode `root` + `bin` as a GLB file and write to `path`.
pub fn write_glb(path: &Path, root: &gj::Root, bin: &[u8]) -> Result<()> {
    let mut json_bytes = gj::serialize::to_vec(root).context("serialize gltf JSON")?;
    pad_to_4(&mut json_bytes, b' ');
    let mut bin_padded = bin.to_vec();
    pad_to_4(&mut bin_padded, 0);

    let json_chunk_len = json_bytes.len() as u32;
    let bin_chunk_len = bin_padded.len() as u32;
    // header (12) + json header (8) + json data + bin header (8) + bin data
    let total = 12 + 8 + json_chunk_len + 8 + bin_chunk_len;

    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&GLB_VERSION.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());

    out.extend_from_slice(&json_chunk_len.to_le_bytes());
    out.extend_from_slice(&CHUNK_TYPE_JSON.to_le_bytes());
    out.extend_from_slice(&json_bytes);

    out.extend_from_slice(&bin_chunk_len.to_le_bytes());
    out.extend_from_slice(&CHUNK_TYPE_BIN.to_le_bytes());
    out.extend_from_slice(&bin_padded);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn pad_to_4(buf: &mut Vec<u8>, fill: u8) {
    let rem = buf.len() % 4;
    if rem != 0 {
        buf.extend(std::iter::repeat_n(fill, 4 - rem));
    }
}

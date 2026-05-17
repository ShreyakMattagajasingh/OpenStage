//! SQLite-backed asset catalog. Schema is created on first connection.
//!
//! Tag and compatibility lists are stored as JSON strings — searching inside
//! them is out of scope for Phase 3b. Phase 12 may move them to a join table
//! once we have tag-driven filtering in the UI.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use tracing::debug;

use crate::metadata::{AssetCategory, AssetMeta};

pub struct Catalog {
    conn: Connection,
}

impl Catalog {
    /// Open (or create) the catalog at `path`. Ensures the schema exists.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create_dir_all {}", parent.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("open sqlite at {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("apply catalog schema")?;
        debug!(path = %path.display(), "asset catalog opened");
        Ok(Self { conn })
    }

    /// Upsert every asset; returns the count actually written.
    pub fn upsert_many(&mut self, assets: &[AssetMeta]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut count = 0;
        {
            let mut stmt = tx.prepare(UPSERT_SQL)?;
            for a in assets {
                let (r, g, b) = match a.default_color {
                    Some([r, g, b]) => (Some(r as f64), Some(g as f64), Some(b as f64)),
                    None => (None, None, None),
                };
                let tags_json = serde_json::to_string(&a.tags).unwrap_or_else(|_| "[]".into());
                let compat_body_json =
                    serde_json::to_string(&a.compatible_body_types).unwrap_or_else(|_| "[]".into());

                stmt.execute(params![
                    a.id,
                    a.display_name,
                    a.category.as_sql_str(),
                    a.model,
                    a.thumbnail,
                    a.supports_color as i64,
                    r,
                    g,
                    b,
                    a.compatible_skeleton,
                    a.version as i64,
                    tags_json,
                    compat_body_json,
                ])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn all(&self) -> Result<Vec<AssetMeta>> {
        let mut stmt = self.conn.prepare(SELECT_ALL_SQL)?;
        let rows = stmt.query_map([], row_to_meta)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn by_category(&self, cat: AssetCategory) -> Result<Vec<AssetMeta>> {
        let mut stmt = self.conn.prepare(SELECT_BY_CATEGORY_SQL)?;
        let rows = stmt.query_map(params![cat.as_sql_str()], row_to_meta)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn find(&self, id: &str) -> Result<Option<AssetMeta>> {
        let mut stmt = self.conn.prepare(SELECT_BY_ID_SQL)?;
        Ok(stmt.query_row(params![id], row_to_meta).optional()?)
    }
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetMeta> {
    let cat_str: String = row.get("category")?;
    let category = AssetCategory::from_sql_str(&cat_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown asset category: {cat_str}").into(),
        )
    })?;

    let default_color: Option<[f32; 3]> = match (
        row.get::<_, Option<f64>>("default_color_r")?,
        row.get::<_, Option<f64>>("default_color_g")?,
        row.get::<_, Option<f64>>("default_color_b")?,
    ) {
        (Some(r), Some(g), Some(b)) => Some([r as f32, g as f32, b as f32]),
        _ => None,
    };

    let tags_json: String = row.get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let compat_body_json: String = row.get("compat_body_json")?;
    let compatible_body_types: Vec<String> =
        serde_json::from_str(&compat_body_json).unwrap_or_default();

    Ok(AssetMeta {
        id: row.get("id")?,
        display_name: row.get("display_name")?,
        category,
        model: row.get("model")?,
        thumbnail: row.get("thumbnail")?,
        supports_color: row.get::<_, i64>("supports_color")? != 0,
        default_color,
        compatible_body_types,
        compatible_skeleton: row.get("compatible_skeleton")?,
        tags,
        version: row.get::<_, i64>("version")? as u32,
    })
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS assets (
    id              TEXT PRIMARY KEY NOT NULL,
    display_name    TEXT NOT NULL,
    category        TEXT NOT NULL,
    model           TEXT NOT NULL,
    thumbnail       TEXT,
    supports_color  INTEGER NOT NULL DEFAULT 0,
    default_color_r REAL,
    default_color_g REAL,
    default_color_b REAL,
    compatible_skeleton TEXT,
    version         INTEGER NOT NULL DEFAULT 1,
    tags_json       TEXT NOT NULL DEFAULT '[]',
    compat_body_json TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS idx_assets_category ON assets(category);
"#;

const UPSERT_SQL: &str = r#"
INSERT INTO assets (
    id, display_name, category, model, thumbnail,
    supports_color, default_color_r, default_color_g, default_color_b,
    compatible_skeleton, version, tags_json, compat_body_json
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(id) DO UPDATE SET
    display_name        = excluded.display_name,
    category            = excluded.category,
    model               = excluded.model,
    thumbnail           = excluded.thumbnail,
    supports_color      = excluded.supports_color,
    default_color_r     = excluded.default_color_r,
    default_color_g     = excluded.default_color_g,
    default_color_b     = excluded.default_color_b,
    compatible_skeleton = excluded.compatible_skeleton,
    version             = excluded.version,
    tags_json           = excluded.tags_json,
    compat_body_json    = excluded.compat_body_json
"#;

const SELECT_COLUMNS: &str = "id, display_name, category, model, thumbnail, \
    supports_color, default_color_r, default_color_g, default_color_b, \
    compatible_skeleton, version, tags_json, compat_body_json";

// Use lazy formatting via const strings — these are tiny.
const SELECT_ALL_SQL: &str = "SELECT id, display_name, category, model, thumbnail, \
    supports_color, default_color_r, default_color_g, default_color_b, \
    compatible_skeleton, version, tags_json, compat_body_json \
    FROM assets ORDER BY display_name";

const SELECT_BY_CATEGORY_SQL: &str = "SELECT id, display_name, category, model, thumbnail, \
    supports_color, default_color_r, default_color_g, default_color_b, \
    compatible_skeleton, version, tags_json, compat_body_json \
    FROM assets WHERE category = ?1 ORDER BY display_name";

const SELECT_BY_ID_SQL: &str = "SELECT id, display_name, category, model, thumbnail, \
    supports_color, default_color_r, default_color_g, default_color_b, \
    compatible_skeleton, version, tags_json, compat_body_json \
    FROM assets WHERE id = ?1";

#[allow(dead_code)]
fn _select_columns_consistency_check() {
    // If you grep for unused constants: `SELECT_COLUMNS` is a doc anchor — the
    // three SELECT statements should mirror it exactly. Compiler unused-const
    // warning would mask drift; this stub keeps it alive.
    let _ = SELECT_COLUMNS;
}

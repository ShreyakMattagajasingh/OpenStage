//! `list [--category <cat>]` subcommand: prints catalog rows from SQLite.

use std::path::Path;

use anyhow::{Context, Result};
use assets::metadata::{AssetCategory, AssetMeta};
use assets::Catalog;

use crate::paths;

pub fn run_list(workspace_root: &Path, category: Option<AssetCategory>) -> Result<()> {
    let db_path = paths::catalog_db_path(workspace_root);
    let catalog = Catalog::open(&db_path)
        .with_context(|| format!("open catalog at {}", db_path.display()))?;
    let rows = match category {
        Some(cat) => catalog.by_category(cat)?,
        None => catalog.all()?,
    };
    if rows.is_empty() {
        println!("(no assets in catalog)");
        return Ok(());
    }
    for meta in &rows {
        print_row(meta);
    }
    println!(
        "({} asset{})",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

fn print_row(meta: &AssetMeta) {
    println!(
        "{id:<32}  {cat:<10}  {name:<24}  model={model}  thumb={thumb}",
        id = meta.id,
        cat = meta.category.as_sql_str(),
        name = meta.display_name,
        model = meta.model,
        thumb = if meta.thumbnail.is_some() {
            "yes"
        } else {
            "no"
        },
    );
}

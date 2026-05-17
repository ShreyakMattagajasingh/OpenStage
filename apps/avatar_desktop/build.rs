//! Embed Windows resource (icon + product info) into avatar_desktop.exe.
//!
//! Only runs on Windows targets; no-op everywhere else so the workspace
//! still `cargo check`s on Linux / macOS dev machines.

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let icon = std::path::Path::new("..")
            .join("..")
            .join("assets")
            .join("icon")
            .join("avatar_studio.ico");
        let mut res = winresource::WindowsResource::new();
        if icon.exists() {
            res.set_icon(icon.to_str().expect("icon path utf-8"));
        }
        res.set("ProductName", "Avatar Studio");
        res.set("FileDescription", "Avatar Studio");
        res.set("CompanyName", "Avatar Studio Team");
        res.set("LegalCopyright", "(c) 2026 Avatar Studio Team");
        res.set("OriginalFilename", "avatar_desktop.exe");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=winresource failed: {e}");
        }
    }
}

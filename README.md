# Avatar Studio

In-house desktop avatar creator. Stylized 3D characters, in-house engine, no Unity/Unreal/Godot.

## Stack
Rust · wgpu · winit · egui · glTF · SQLite · serde.

## Build & run (Windows)

```powershell
cd avatar-studio
cargo run --bin avatar_desktop
```

`cargo check` validates the whole workspace.

## Milestone 1 (current)
Window + wgpu clear + egui side panel. See `docs/roadmap.md` for the full phase list.

## Layout

| Path                      | What it is                                    |
| ------------------------- | --------------------------------------------- |
| `apps/avatar_desktop`     | The desktop app shell (binary).               |
| `apps/asset_builder`      | Internal CLI to validate & package GLB assets.|
| `crates/engine_core`      | Config, error types, frame clock, input.      |
| `crates/renderer`         | wgpu device, render passes, camera, meshes.   |
| `crates/avatar`           | Slot model, customization, save/load.         |
| `crates/animation`        | Skeleton, clips, GPU skinning.                |
| `crates/assets`           | Metadata, SQLite catalog, cache.              |
| `crates/ui`               | egui screens (editor, gallery, export).       |
| `crates/export`           | PNG / video / GLB / VRM output.               |
| `assets/`                 | Processed assets shipped with the app.        |
| `user_data/`              | Per-user characters, thumbnails, settings.    |
| `docs/`                   | Architecture, save format, skeleton spec.     |

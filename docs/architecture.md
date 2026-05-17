# Architecture

```
                       ┌─────────────────────────┐
                       │   apps/avatar_desktop   │  ← winit ApplicationHandler
                       │   (the shell + egui)    │
                       └────────┬───────┬────────┘
                                │       │
            ┌───────────────────┘       └───────────────────┐
            │                                               │
     ┌──────▼──────┐                              ┌─────────▼────────┐
     │     ui      │                              │     renderer     │
     │  (egui)     │                              │   (wgpu, glam)   │
     └──────┬──────┘                              └─────────┬────────┘
            │                                               │
            └──────────────┬──────────────┬─────────────────┘
                           │              │
                    ┌──────▼────┐  ┌──────▼─────┐
                    │  avatar   │  │ animation  │
                    └──────┬────┘  └──────┬─────┘
                           │              │
                           └──────┬───────┘
                                  │
                          ┌───────▼───────┐         ┌─────────────┐
                          │    assets     │◄────────│   export    │
                          │ (SQLite + fs) │         │ (PNG/video) │
                          └───────┬───────┘         └─────────────┘
                                  │
                          ┌───────▼───────┐
                          │  engine_core  │  ← config, errors, time, input
                          └───────────────┘

         apps/asset_builder ──► assets crate (validation, ingest)
```

## Crate boundaries

| Crate         | Owns                                                | Does NOT own                       |
| ------------- | --------------------------------------------------- | ---------------------------------- |
| `engine_core` | Config, error types, frame clock, input intents.    | GPU, UI, asset data.               |
| `renderer`    | wgpu device/surface, passes, meshes, materials, camera, screenshot. | UI logic, asset registry. |
| `avatar`      | Slot state, customization, save/load schema.        | GPU buffers.                       |
| `commands`    | Serializable command envelopes, validation, routing, undo/redo history. | Renderer/app internals. |
| `scene`       | Stable-ID logical scene graph, selection state, JSON query API. | GPU resources, viewport picking. |
| `animation`   | Skeleton, clips, GPU skinning data.                 | Render pass orchestration (renderer does that). |
| `assets`      | Metadata schema, SQLite catalog, FS scanner, cache. | GPU upload (renderer does that).   |
| `ui`          | egui screens (editor, gallery, export).             | egui-wgpu wiring (the app does).   |
| `export`      | PNG / video / GLB / VRM encoders.                   | Scene rendering (asks renderer).   |

## Data flow per frame

1. `app::redraw` ticks `FrameClock`, takes egui raw input, runs an egui frame.
2. Renderer acquires the swapchain texture and runs the **clear pass**.
3. Future scene passes (mesh, skinning) render between clear and UI.
4. egui paint pass runs with `LoadOp::Load` and presents.

## Notes

- All inter-crate types are `Send + Sync` unless explicitly noted; the app is single-threaded for M1 but the boundaries leave room for an async asset loader later.
- `engine_core` has no dependency on `wgpu`, `winit`, or `egui`. This keeps "policy" code testable without a GPU.

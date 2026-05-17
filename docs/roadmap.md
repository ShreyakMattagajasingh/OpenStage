# Roadmap

| Phase | Name                                  | Exit criteria                                                    |
| ----- | ------------------------------------- | ---------------------------------------------------------------- |
| 0     | Product definition                    | Spec frozen; this doc + architecture.md merged.                  |
| 1     | Project foundation                    | Workspace builds. `cargo check` green. (Milestone 1)             |
| 2     | Basic renderer                        | Triangle/cube, orbit camera, lighting, depth.                    |
| 3     | GLB asset loading + asset DB          | One static GLB renders. Metadata scanned into SQLite.            |
| 4     | Base avatar + skeleton                | avatar_skeleton_v1 parsed; bones visualised.                     |
| 5     | Skinned mesh rendering                | GPU skinning. Bind pose looks correct.                           |
| 6     | Animation playback                    | Idle clip plays; play/pause/scrub.                               |
| 7     | Slot-based customization              | Hair/top/bottom/shoes swap live.                                 |
| 8     | Color customization                   | Color picker tints material channels.                            |
| 9     | Face and expression system            | 5 expression presets selectable.                                 |
| 10    | Save/load + gallery                   | JSON save round-trips. Gallery grid loads.                       |
| 11    | PNG export                            | 512/1024/2048 + transparent BG, full body & portrait.            |
| 12    | Asset builder CLI                     | Import GLB → metadata + thumbnail + DB row.                      |
| 13    | UI/UX polish                          | Iconography, layouts, animations consistent.                     |
| 14    | Performance optimization              | 60 FPS @ 1080p on mid-range laptop.                              |
| 15    | Packaging and installer               | Signed Windows installer (.msi).                                 |
| 16    | QA and testing                        | Integration tests; soak tests; agent-visible screenshot checks.   |
| 17    | Post-MVP                              | Video/GIF, GLB/VRM, more bodies, undo/redo.                      |

## Cross-cutting requirement: agent visual verification

AI coding agents working on this repo should be able to inspect what a user
would see in the running build. This is separate from user-facing PNG export.

Plan:

1. Add a deterministic dev/QA launch mode for `avatar_desktop` so the same
   sample scene can be opened repeatedly. Implemented in Phase 11 via
   `cargo run --bin avatar_desktop -- --agent-capture`.
2. Add an agent-friendly screenshot path that captures the app window or the
   renderer output and writes timestamped images under `user_data/debug_screenshots/`.
   Implemented in Phase 11 with offscreen renderer captures and
   `latest_agent_capture.json`.
3. Include visual smoke checks in Phase 16: launch app, capture screenshot,
   verify the image is non-empty, and keep the artifact for Codex/Claude/AI
   coding agents to review.
4. For renderer-specific work, prefer an offscreen render target so tests can
   capture frames without requiring manual interaction with the desktop window.

## Out of scope (forever, or until v2)
AI avatar generation · Multiplayer · Marketplace · Cloud accounts · Realistic human rendering · Physics clothing · Motion capture · Advanced body sliders · Mobile.

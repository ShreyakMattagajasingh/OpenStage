# Avatar Studio Handover / Progress Check

Last updated: 2026-05-17

## Stage 22 shipped - Editor Modes and Top Mode Bar (Codex, 2026-05-17)

Stage 22 makes the editor's mode system explicit and machine-readable. We now
have a top mode bar, a first-class `EditorMode` enum, mode-gated left-panel
sections, persisted last-used mode in settings, and deterministic mode export
for coding agents. This is mostly UI plumbing, but it matters because later
stages can now say "switch to Rig mode" or "open Object mode" without relying
on fragile implicit UI state.

This stage intentionally ships real routing for the mode shell first. Character,
Customize, Object, Material, Asset, and Export have live layouts today. Rig,
Pose, Animation, Expression, and AI show structured placeholders so Stage 23+
can fill them in without reworking the frame.

### What's new

**New mode model (`crates/ui/src/modes.rs`, new).**
`EditorMode` is now a shared UI/runtime type with 11 variants:
- `Character`
- `Customize`
- `Object`
- `Rig`
- `Pose`
- `Animation`
- `Expression`
- `Material`
- `Asset`
- `Export`
- `Ai`

The module also adds:
- `EditorMode::ALL`
- `label()` / `icon()`
- `implemented()`
- `coming_soon_stage()`
- `LeftSection`
- `ModeLayout`
- `mode_layout(mode)`

Three unit tests cover enum completeness, label uniqueness, and layout sanity
for implemented modes.

**Top mode bar (`crates/ui/src/layout.rs`).**
`draw_mode_bar(ctx, current_mode)` now renders a 32 px top strip with one chip
per mode. Clicking a chip dispatches `SidePanelAction::SetEditorMode(mode)`.
The left panel now consults `mode_layout(current_mode)` and only renders the
sections appropriate to that mode. Unimplemented modes render a deliberate
"coming soon" empty state instead of a blank panel.

**New command: `editor.set_mode` (`crates/commands/`).**
Mode switches now go through the command layer for consistency with the
Stage 18 architecture. The payload is:

```json
{ "type": "editorSetMode", "mode": "character" }
```

Important nuance: `editor.set_mode` is **validated and executed through the
router, but intentionally does not record undo history**. Mode is editor-view
state, not scene state.

**Config schema v2 + migration (`crates/engine_core/src/config.rs`).**
`Config` now includes:
- `editor.last_mode`

The settings schema version is bumped from `1 -> 2`, with a migration path for
old files that defaults to `Character`. The loader is also more resilient now:
if `user_data/settings.json` is empty, corrupt, or full of NUL bytes, the app
regenerates defaults instead of failing startup. That fix turned out to be
important in practice during QA.

**Desktop integration (`apps/avatar_desktop/src/app.rs`).**
- `App.current_mode: EditorMode`
- mode dispatch helper through `CommandRouter`
- mode persistence back into `config.editor.last_mode`
- inspector visibility now follows `mode_layout(current_mode).show_inspector`
- viewport carve math now reserves space for both the left panel and the new
  top mode bar
- deterministic capture forces `Character` mode for stable output

**Agent-readable mode export.**
Deterministic capture now writes:
- `user_data/debug_screenshots/latest_editor_mode.json`

Shape:

```json
{
  "current_mode": "character",
  "available_modes": ["character", "..."]
}
```

**QA helper fix (`crates/qa/src/lib.rs`).**
The QA runner now rebuilds `avatar_desktop` before launching integration flows.
Without that, one new Stage 22 test was accidentally hitting a stale
`target/debug/avatar_desktop.exe` and missing the newly-exported JSON file.

### File map

```
+ crates/ui/src/modes.rs                  EditorMode / LeftSection / ModeLayout + 3 tests
M crates/ui/src/lib.rs                    re-export modes + mode bar
M crates/ui/src/layout.rs                 top mode bar, mode-gated sections,
                                           SetEditorMode action, placeholders
M crates/commands/Cargo.toml              + ui workspace dependency
M crates/commands/src/command.rs          EditorSetModePayload + command variant
M crates/commands/src/context.rs          validate_editor_set_mode / set_editor_mode
M crates/commands/src/lib.rs              re-export EditorSetModePayload
M crates/commands/src/router.rs           non-undoable editor.set_mode execute path
                                           + 2 tests
M crates/engine_core/Cargo.toml           + ui workspace dependency
M crates/engine_core/src/config.rs        schema v2, editor config, v1 migration,
                                           corrupt-settings recovery + 3 tests
M apps/avatar_desktop/src/app.rs          current_mode, mode dispatch, top bar render,
                                           mode JSON export, deterministic mode pinning,
                                           inspector visibility + viewport carve updates
+ crates/qa/tests/editor_modes.rs         latest_editor_mode.json integration test
M crates/qa/src/lib.rs                    rebuild avatar_desktop before QA launches
M docs/handover_progress.md               this entry
```

### Verification

- `cargo fmt --all` clean.
- `cargo fmt --all -- --check` clean.
- `cargo check --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test --workspace` -> **129 passed, 0 failed**.
- Focused verification also passed:
  - `cargo test -p qa --test editor_modes`
  - deterministic capture writes `latest_editor_mode.json`

### Known caveats (Stage 22)

- **Mode changes are not undoable.** That is intentional; they are editor view
  state, not scene edits.
- **Stub modes are placeholders.** Rig / Pose / Animation / Expression / AI do
  not have their full tools yet; they render a guided empty state instead.
- **Top bar reduces viewport height slightly.** The carve is small, but it does
  take 32 px from the scene area.
- **Object/Character workflows still share runtime state.** Stage 22 organizes
  the editor shell; it does not yet split the app into separate tool domains.

### Next stage

**Stage 23: Rig Editor.** Rig mode now has a real home in the shell, so the
next step is to fill it with the bone tree, skeleton metadata editing,
attachment points, and validation flow described in `phase21-23_plan.md`.

## Stage 21 shipped - Transform Gizmos and Viewport Tools (Codex, 2026-05-17)

Stage 21 turns the Stage 20 inspector from a read-only scene browser into a
real editing surface. We now have undoable `transform.*` commands, editable
local transform fields in the inspector, active tool state (`Select` / `Move` /
`Rotate` / `Scale`), axis constraints, hotkeys, and a lightweight viewport gizmo
overlay for the active selection.

This stage intentionally stops short of click-and-drag viewport manipulation.
Transforms are edited through inspector numeric fields first, while the viewport
gizmo is a visual anchor so humans and coding agents can see what object is
currently being manipulated.

### What's new

**Tool state (`crates/ui/src/tools.rs`, new).**
`EditorTool`, `AxisConstraint`, and `ToolState` are now first-class UI types.
Default state is `Select` + no axis lock + gizmo visible. The right inspector
header renders tool tabs plus axis chips, and the desktop app stores the live
tool state in `App.tool_state`.

**New transform commands (`crates/commands/`).**
- `transform.set_translation { object_id, translation }`
- `transform.set_rotation { object_id, rotation }`
- `transform.set_scale { object_id, scale }`
- `transform.apply_delta { object_id, ... }`
- `transform.reset { object_id }`

All five validate through `CommandRouter`, execute through `CommandRuntime`,
and record undo snapshots through the existing Stage 18 history backbone.
`MemoryRuntime` in router tests now tracks translation / rotation / scale maps.

**Scene transform mutators (`crates/scene/src/lib.rs`).**
`SceneGraph` now exposes `set_translation`, `set_rotation`, `set_scale`, and
`reset_transform`, each returning the previous value for undo-friendly behavior.

**Editable inspector transform section (`crates/ui/src/layout.rs`).**
Mesh, avatar, skeleton, bone, and generic object details now render a shared
`transform_section(...)` with:
- `Translation` drag values
- `Rotation` drag values in Euler degrees
- `Scale` drag values with positive clamping
- `Reset transform` button

The active tool controls which axis lock applies to the drag rows:
- `Move` -> translation lock
- `Rotate` -> rotation lock
- `Scale` -> scale lock

Locked objects show a warning and keep transform editing disabled.

**Inspector detail data expanded (`crates/ui/src/inspector.rs`).**
`InspectorAvatar`, `InspectorSkeleton`, and `InspectorBasic` now carry
translation / rotation / scale, and avatar/skeleton details also expose
the locked state so the transform section can respect it.

**Desktop runtime integration (`apps/avatar_desktop/src/app.rs`).**
- `App.object_overrides: HashMap<String, ObjectRuntimeOverrides>` stores live
  per-object transform / visibility / lock overrides without rewriting the base
  catalog assets.
- `AppSnapshot` now preserves `object_overrides`, so undo/redo rewinds transform
  edits along with other app state.
- `CommandRuntime for App` implements all transform validate/execute methods.
  Validation rejects unknown ids, locked objects, non-finite values, non-unit
  quaternions, and non-positive scale.
- Bone transform overrides invalidate the cached posed skeleton so the skeleton
  overlay and skinning palette stay in sync.
- `redraw()` now computes effective object transforms before rendering, applies
  them to avatar slot meshes / static mesh / face quad, and draws a small RGB
  axis gizmo at the selected object's world origin when enabled.

**Hotkeys / editor tool flow.**
- `Q` -> Select
- `W` -> Move
- `E` -> Rotate
- `R` -> Scale
- `X` / `Y` / `Z` -> axis constraint
- `Escape` -> clear axis constraint
- `F` keeps the existing frame-selection behavior

**Deterministic capture stays stable.**
`run_agent_capture()` now forces `tool_state.show_gizmo = false`, so Stage 16
golden capture comparisons stay stable even though interactive mode now draws a
selection gizmo.

### File map

```
+ crates/ui/src/tools.rs                  EditorTool / AxisConstraint / ToolState + 2 tests
M crates/ui/src/lib.rs                    re-export tool types
M crates/ui/src/layout.rs                 tool header, axis chips, editable transform rows,
                                           Euler<->quat helpers, new SidePanelAction variants
M crates/ui/src/inspector.rs              transform fields on avatar/skeleton/basic details
M crates/ui/Cargo.toml                    + glam
M crates/commands/src/command.rs          transform payload structs + CommandName variants
M crates/commands/src/context.rs          CommandRuntime validate/execute methods for transforms
M crates/commands/src/router.rs           transform execute arms + MemoryRuntime state + 5 tests
M crates/commands/src/lib.rs              re-export transform payloads
M crates/commands/Cargo.toml              + glam
M crates/scene/src/lib.rs                 set_translation / set_rotation / set_scale / reset_transform
                                           + 3 scene tests
M crates/renderer/src/debug_lines.rs      RGB axis gizmo line drawing
M apps/avatar_desktop/src/app.rs          tool state, object overrides, transform dispatch helpers,
                                           hotkeys, posed-world override path, gizmo rendering,
                                           snapshot integration
M docs/handover_progress.md               this entry
```

### Verification

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` -> **118 passed, 0 failed**.
- Existing capture/perf integration tests still pass, including:
  - golden full-body + portrait capture
  - latest selection JSON emission
  - perf baseline regression guard

### Known caveats (Stage 21)

- **Viewport gizmo is read-only.** There is no click-and-drag manipulation yet;
  transform editing happens through inspector drag values only.
- **Transforms are local-space only.** No local/world toggle yet.
- **Tool state is UI state, not command history.** Undo rewinds scene edits, not
  whether the user last pressed `W` or `E`.
- **Frame selected** still uses the existing coarse camera focus behavior; it is
  not yet a precise bounds fit for every object kind.

### Next stage

**Stage 22: Editor Modes and Top Mode Bar.** The current left categories are
still mostly a content browser. Stage 22 promotes modes like Character /
Customize / Object / Material / Asset / Export into explicit editor state, adds
a top mode bar, and lays the groundwork for Rig / Pose / Animation / Expression / AI
mode routing.

## Stage 20 shipped — Selection System and Inspector (Claude Opus 4.7, 2026-05-17)

Stage 20 adds the first human-facing surface for the Stage 19 scene graph: a
Blender-style right-side **Inspector** panel that lists every `SceneObject`,
lets the user pick one (dispatching `selection.set` through the command
router), and shows per-type detail readouts. The deterministic
`--agent-capture` mode now also writes the post-load selection JSON so
AI agents see what the human had selected.

Transform editing inside the inspector is **deferred to Stage 21** (gizmo +
transform commands). Stage 20 is the read-only inspector + two new tiny
undoable commands.

### What's new

**Inspector data model (`crates/ui/src/inspector.rs`, new).**
`InspectorStatus`, `InspectorObjectRow`, and `InspectorDetail` enum with
per-kind variants (`Avatar` / `MeshInstance` / `Skeleton` / `Bone` /
`Material` / `AnimationClip` / `Camera` / `Light` / `Other`).
`build_inspector_status(graph, selection, filter)` pre-computes the
outliner rows (with parent-chain depth) and the detail block for the
active object. 4 unit tests cover indent, case-insensitive filter,
empty-filter listing, and active-selection → detail wiring.

**Right-side panel (`crates/ui/src/layout.rs`).**
`draw_inspector_panel(ctx, status)` mounts `egui::SidePanel::right` at
280 px (`INSPECTOR_PANEL_WIDTH`). Top section is the outliner with a
text filter + clickable rows (icon + indented label per kind) + a
"Clear selection" button. Bottom section is the detail block, dispatched
to `draw_detail_*` helpers per `InspectorDetail` variant. `field_row`,
`format_vec3`, `format_vec4` helpers added. Mesh-instance details
include checkboxes that dispatch the new scene visibility / locked
commands.

**New commands (`crates/commands/`).**
- `scene.set_visible { object_id, visible }`
- `scene.set_locked { object_id, locked }`

Both are undoable via the existing `CommandRouter<AppSnapshot>` path:
the runtime stashes the old `visible` / `locked` value through the
snapshot, and `Ctrl+Z` rewinds. `CommandRuntime` grew four trait
methods (validate + execute pairs). `MemoryRuntime` in the router
test module gained matching state so the router's own tests cover
the new arms.

**Scene mutators (`crates/scene/src/lib.rs`).**
`SceneGraph::set_visible / set_locked` return the previous value
(used by undo snapshots). 3 unit tests cover happy path, unknown-id,
and round-trip.

**Desktop integration (`apps/avatar_desktop/src/app.rs`).**
- `App.inspector_filter: String` — the live filter text.
- `populate_inspector_status` runs each frame inside the existing
  `refresh_*` block.
- `redraw()` calls `draw_inspector_panel` after `draw_side_panel`;
  inspector actions win when both panels produce one in a single
  frame.
- New `SidePanelAction` variants — `SelectObject(String)`,
  `DeselectAll`, `SetInspectorFilter(String)`,
  `SetSceneObjectVisible { id, visible }`,
  `SetSceneObjectLocked { id, locked }` — each routes through a
  small `dispatch_*` helper that builds a `CommandEnvelope` and
  calls `execute_command`.
- `CommandRuntime` impl gained `validate_scene_set_*` and
  `scene_set_*` methods; both validate against the live
  `SceneGraph` and call the new mutators.
- `scene_selection_json()` helper + agent-capture write
  `user_data/debug_screenshots/latest_selection.json`. In
  `--deterministic` mode `run_agent_capture` auto-selects
  `avatar_001` so the file is reproducible.

**Viewport carve math.** `apps/avatar_desktop/src/app.rs` reserves
both panels now via `ui::RESERVED_PANEL_WIDTH = SIDE_PANEL_WIDTH +
INSPECTOR_PANEL_WIDTH`. The scene viewport rect becomes
`[left_px, 0, fb_w - reserved_px, fb_h]` (was `[left_px, 0, fb_w -
left_px, fb_h]`). `Renderer::capture_rgba` is unaffected — PNG and
GIF export still hit their own offscreen targets, so the committed
goldens still match SSIM ≥ 0.99 without re-capture.

**Window default width** bumped from 1340 → 1480 in
`crates/engine_core/src/config.rs` so the scene viewport stays
~900 px wide after the inspector takes 280 px.

### File map

```
M crates/scene/src/lib.rs               + set_visible / set_locked + 3 tests
M crates/commands/src/command.rs        + SceneSetVisiblePayload / SceneSetLockedPayload
                                          + CommandName variants
M crates/commands/src/context.rs        + 4 trait methods (validate + execute)
M crates/commands/src/router.rs         + 2 execute arms + 2 tests
M crates/commands/src/lib.rs            re-exports new payloads
+ crates/ui/src/inspector.rs            InspectorStatus + build_inspector_status + 4 tests
M crates/ui/src/lib.rs                  pub mod inspector + re-exports
M crates/ui/src/layout.rs               INSPECTOR_PANEL_WIDTH / RESERVED_PANEL_WIDTH
                                          + new SidePanelAction variants
                                          + SidePanelStatus.inspector / inspector_visible
                                          + draw_inspector_panel + draw_detail_* + field_row
M crates/ui/Cargo.toml                  + scene, + serde
M crates/engine_core/src/config.rs      window width 1340 → 1480
M apps/avatar_desktop/src/app.rs        InspectorStatus populate, dispatch helpers,
                                          CommandRuntime impl for scene_set_*,
                                          viewport-carve math, agent-capture selection JSON,
                                          deterministic auto-select of avatar_001
+ crates/qa/tests/selection_inspector.rs  agent_capture_writes_latest_selection_json
M assets/processed/metadata/phase7_top.json  thumbnail field re-applied
M docs/handover_progress.md             this entry
```

No renderer / animation crate changes; pure UI + scene/commands wiring.

### Verification

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` → **108 passed, 0 failed** (was 98; +10
  new: 3 scene + 2 commands + 4 inspector + 1 qa).
- `cargo run --bin avatar_desktop -- --agent-capture --deterministic`
  passes:
  - `agent_full_body.png` + `agent_portrait.png` byte-stable.
  - `latest_scene_graph.json` + `latest_scene_summary.json` unchanged.
  - **`latest_selection.json` newly written**, contains
    `"active_object": "avatar_001"` + matching `selected_objects` array.
- `cargo test -p qa --test golden_capture` passes — SSIM stays
  ≥ 0.99 because the offscreen capture path doesn't depend on the
  on-screen viewport carve.
- `cargo test -p qa --test selection_inspector` passes — first run of
  the new Stage 20 integration test.

### Known caveats (Stage 20)

- **Transform editing is read-only** in the inspector. Bone /
  mesh-instance transform rows show numeric values via
  `format_vec3` / `format_vec4` but aren't editable. Stage 21 adds
  transform commands + viewport gizmos.
- **Outliner is a flat list**, not a parent-child tree. Rows are
  indented by computed depth so the hierarchy is readable, but no
  collapse carets yet. Stage 23+ can upgrade to a true tree.
- **Inspector filter input** dispatches one
  `SetInspectorFilter` action per keystroke. The action mutates
  `App.inspector_filter` directly (no command envelope) so the
  history doesn't fill with filter noise. The Stage 18 architectural
  rule "everything through a command" is intentionally relaxed for
  filter text since it's a UI-local view state, not scene state.
- **Multi-select** isn't wired yet; clicking a row dispatches
  `selection.set` with a single ID. Shift-click / Ctrl-click come
  with Stage 21's pointer work.
- **Material detail** only shows the read-only base color preview.
  Editing colors continues to go through the existing Equipped
  section's color swatches (which dispatch `material.set_color` —
  already correct).

### Next stage

**Stage 21: Transform Gizmos and Viewport Tools.** Per the
post-Phase-17 roadmap, Stage 21 makes inspector transform fields
editable and adds viewport-side gizmos for the active selection.
Hotkeys: W/E/R for move/rotate/scale, F to frame selected,
X/Y/Z axis constraints. New commands `transform.set_translation`,
`transform.set_rotation`, `transform.set_scale`, `transform.apply_delta`,
`transform.reset`.

## Stage 19 shipped - Scene Graph and Stable Object Registry (Codex, 2026-05-16)

Stage 19 adds the logical scene model needed by AI agents, future inspectors,
and command routing. Rendering is still driven by the existing app/runtime
fields, but the desktop app now keeps a synchronized JSON-friendly scene graph
with deterministic object IDs.

### What's new

**New `scene` crate.** The workspace now has `crates/scene` with:
- `SceneId`, `SceneGraph`, `SceneObject`, `SceneObjectKind`,
  `SceneTransform`, `SceneSelection`, and `SceneQuery`.
- Serializable records for avatars, mesh/skinned mesh instances, skeletons,
  bones, materials, animation clips, camera, light, attachment points,
  constraints, poses, and blendshape sets.
- Parent/child graph operations: insert, delete, reparent, lookup, list
  children, and list descendants.
- Query helpers: scene summary, object lookup, type lists, selection list,
  name search, type search, and asset-id search.

**Stable IDs now exported.**
- Active avatar: `avatar_001`
- Equipped slot meshes: `mesh_<slot>_001`
- Slot materials: `mat_<slot>_primary`
- Skeleton: `skeleton_avatar_001`
- Bones: `bone_<bone_name>`
- Camera/light: `camera_main`, `light_key`
- Static mesh mode: `mesh_static_001`

**Desktop integration.** `avatar_desktop::App` owns `scene_graph:
scene::SceneGraph` and `scene_selection: scene::SceneSelection`. The graph is
resynced after asset loads, static-mode switches, wearable equip/unequip,
material color changes, expression/snapshot restore paths, and command
execution. Stage 18 `selection.set` now validates against real scene object IDs.

**Agent-readable export.** Deterministic agent capture now writes:
- `user_data/debug_screenshots/latest_scene_graph.json`
- `user_data/debug_screenshots/latest_scene_summary.json`

The current deterministic sample scene (`Phase 4 Rig` + `Phase 7 Top`) exports
28 objects: 1 avatar, 2 mesh instances, 1 skeleton, 18 bones, 2 materials, and
1 animation clip.

### Verification

Run from `avatar-studio/`:

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo run --bin avatar_desktop -- --agent-capture --deterministic
```

Verified on 2026-05-16. The agent capture manifest includes the scene JSON
files alongside `agent_full_body.png` and `agent_portrait.png`.

### Important files

- `crates/scene/src/lib.rs`
  - Logical graph, stable IDs, query API, selection validation, and tests.
- `apps/avatar_desktop/src/app.rs`
  - Scene graph sync, `SceneSelection` command runtime bridge, and agent
    capture JSON export.
- `user_data/debug_screenshots/latest_scene_graph.json`
  - Latest local agent-readable graph output.
- `user_data/debug_screenshots/latest_scene_summary.json`
  - Latest local compact summary output.

### Next stage

Stage 20: Selection System and Inspector.

Goal: expose the Stage 19 graph to humans with selection UI, object details,
and a right-side inspector without breaking the command-first flow.

Expected work:

- Add a selected-object status/readout in the desktop UI.
- Add an inspector panel that reads from `SceneGraph`.
- Route inspector selection through `selection.set` / `selection.clear`.
- Start preparing viewport picking/highlighting, while keeping full transform
  gizmos for Stage 21.

## Stage 18 shipped - Command System Foundation (Codex, 2026-05-16)

The post-Phase-17 roadmap now begins the AI-native editor migration. Stage 18
adds a serializable command layer and routes the first real desktop edits
through it without rewriting the renderer, asset loader, save/load, or export
paths.

### What's new

**New `commands` crate.** The workspace now has `crates/commands` with:
- `CommandEnvelope`, `CommandSource`, `CommandName`, `CommandPayload`, and
  `CommandResult`.
- `ValidationResult` / `ValidationWarning`.
- `CommandRouter`, `CommandHistory`, and `UndoRecord`.
- `CommandRuntime`, a generic adapter trait so tests and `avatar_desktop::App`
  can execute commands without the command crate depending on wgpu/renderer
  internals.

**Implemented Stage 18 commands.**
- `selection.set`
- `selection.clear`
- `avatar.equip_asset`
- `material.set_color`
- `history.undo`
- `history.redo`

`CommandEnvelope` JSON uses serde-friendly command names such as
`"avatar.equip_asset"` and payload variants tagged by `"type"`.

**Desktop bridge.** `avatar_desktop::App` now implements `CommandRuntime`.
Asset row clicks build `avatar.equip_asset`; color picker changes build
`material.set_color`; Undo/Redo buttons and `Ctrl+Z` / `Ctrl+Y` /
`Ctrl+Shift+Z` build history commands. Selection commands were initially a
simple app list and were upgraded in Stage 19 to `scene::SceneSelection`.

**Unified history facade.** Phase 17's `undo_stack` / `redo_stack` fields were
replaced by `CommandRouter<AppSnapshot>`. Existing non-migrated undoable UI
actions use `run_legacy_undoable(...)` to record `AppSnapshot` before/after
states in command history, so undo/redo remains one stack during migration.

### File map

```
M Cargo.toml                                + commands workspace member/dependency
+ crates/commands/Cargo.toml                new crate
+ crates/commands/src/lib.rs                public command API
+ crates/commands/src/command.rs            envelope/payload/result/source/name types
+ crates/commands/src/context.rs            CommandRuntime adapter trait
+ crates/commands/src/router.rs             validate/execute/dry_run/batch/replay/undo/redo
+ crates/commands/src/history.rs            CommandHistory + UndoRecord
+ crates/commands/src/validation.rs         ValidationResult/Warning
+ crates/commands/src/error.rs              CommandError
M apps/avatar_desktop/Cargo.toml            depends on commands
M apps/avatar_desktop/src/app.rs            CommandRuntime impl, command dispatch,
                                            command-backed history bridge
M docs/handover_progress.md                 this entry
```

### Verification

- `cargo check --workspace` clean.
- `cargo test -p commands` clean: 7 command tests passed.
- `cargo test --workspace` clean.
- `cargo run --bin avatar_desktop -- --agent-capture --deterministic` passed
  and refreshed `user_data/debug_screenshots/agent_*.png`.

Known limitations: Stage 18 intentionally does not add scene graph objects,
viewport selection, inspector UI, MCP, scripting, or AI prompt generation.
`material.set_color` currently targets equipped avatar slots only; standalone
material ids are deferred until the scene graph exists.

## Phase 17 shipped — Post-MVP exports, samples, and undo/redo (Codex, 2026-05-16)

Roadmap intent: "Video/GIF, GLB/VRM, more bodies, undo/redo." This pass
implements the parts that can be made correct on top of the current runtime:
undo/redo history, GIF turntable export, broader sample catalog coverage, and
a typed GLB/VRM export boundary that clearly reports the remaining CPU-mesh
data blocker.

### What's new

**Undo/redo.** `avatar_desktop::App` now records bounded snapshots for avatar
and static modes. UI buttons live in the Model section and keyboard shortcuts
are wired:
- `Ctrl+Z` undo
- `Ctrl+Y` redo
- `Ctrl+Shift+Z` redo

Snapshots include equipped slots, colors, expression, loaded character id/name,
static catalog asset id, and animation time/play/loop state. Restores suspend
history recording so undo/redo does not recursively create new entries.

**GIF turntable export.** `crates/export::video` now provides
`GifExportOptions`, validation, and `write_rgba_gif`. The desktop Export panel
adds **Export GIF**, which captures a 512 px, 48-frame, 2 second full-body
turntable through the existing offscreen renderer and writes to
`user_data/exports/avatar_turntable_*.gif`. Coding agents can run
`cargo run --bin avatar_desktop -- --agent-gif --deterministic` to write a
stable `user_data/exports/agent_turntable.gif`.

**GLB/VRM export groundwork.** `crates/export::gltf` now exposes
`AvatarGlbExportOptions`, destination validation, and a typed
`CpuMeshDataUnavailable` error. This is intentionally not a fake product
export: runtime meshes currently retain GPU buffers, not CPU vertices/indices,
so a correct composed-avatar GLB needs CPU mesh retention or a source-asset
composition pass.

**Phase 17 sample fixture pack.** `asset_builder gen-fixture-pack` writes and
catalogs category fixtures for:
- `bottom_phase17_basic_001`
- `shoes_phase17_basic_001`
- `hair_phase17_basic_001`
- `hat_phase17_basic_001`
- `glasses_phase17_basic_001`
- `accessory_phase17_basic_001`

The pack also generated 256 px thumbnails under
`assets/processed/thumbnails/`. These fixtures reuse the already-skinned Phase
7 top GLB so every category can be equipped against the Phase 4 rig immediately.
They are coverage assets, not final art-shaped shoes/hair/hats.

### File map

```
M apps/avatar_desktop/src/app.rs             undo/redo snapshots, shortcuts,
                                             static asset id tracking,
                                             GIF turntable capture/export
M apps/avatar_desktop/src/main.rs            --agent-gif flag
M crates/ui/src/layout.rs                    Undo/Redo + Export GIF actions
M crates/export/src/video.rs                 GIF encoder + tests
+ crates/export/src/gltf.rs                  GLB/VRM typed placeholder API
M crates/export/src/lib.rs                   export gltf module
M Cargo.toml                                 image +gif feature
M apps/asset_builder/src/main.rs             gen-fixture-pack command
M apps/asset_builder/src/fixtures/mod.rs     pack module
+ apps/asset_builder/src/fixtures/pack.rs    Phase 17 sample fixture pack
+ assets/processed/avatars/{bottoms,shoes,hairs,hats,glasses,accessories}/...
+ assets/processed/metadata/*phase17*.json
+ assets/processed/thumbnails/*phase17*.png
M docs/asset_pipeline.md                     current asset-builder workflow
M docs/handover_progress.md                  this entry
```

### Verification

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` passed.
- `cargo run --bin asset_builder -- gen-fixture-pack` wrote all Phase 17 sample
  metadata/GLBs and upserted them into the catalog.
- `cargo run --bin asset_builder -- thumbnail <phase17-id>` passed for all six
  new fixtures.
- `cargo run --bin avatar_desktop -- --agent-capture --deterministic` passed
  and refreshed `user_data/debug_screenshots/agent_*.png`.
- `cargo run --bin avatar_desktop -- --agent-perf --perf-frames 300
  --deterministic` passed; latest report has `averageFps=61.48`, `p99FrameMs=17.47`, and
  `"passed": true`.
- `cargo run --bin avatar_desktop -- --agent-gif --deterministic` passed and
  wrote `user_data/exports/agent_turntable.gif`.

Known limitation: the Phase 17 fixture pack increases slot coverage, but it
does not improve avatar visual fidelity. Final-looking bodies, hair, hats,
shoes, and accessories still need an art/mesh pass.

## Phase 16 shipped — QA and testing (Claude Opus 4.7, 2026-05-16)

Roadmap exit criterion ("Integration tests; soak tests; agent-visible
screenshot checks") met. The MVP v1.0 finishing pass adds **14 new
tests** across a dedicated `crates/qa` integration package, a SSIM
golden-image gate on every push, committed perf + image baselines, a
500-frame headless soak loop, a `--deterministic` mode that makes
captures bit-stable, and a GitHub Actions workflow that runs the
entire suite on `windows-latest`.

### What's new

**Deterministic capture mode.** `apps/avatar_desktop` accepts a new
`--deterministic` flag (composes with `--agent-capture` and
`--agent-perf`):
- `FrameClock::pin_dt(1/60s)` returns a fixed `dt` regardless of
  wall time so animation sampling is identical across runs.
- After body install, the animation player pauses and seeks to
  `t = 0`, freezing the pose to the bind pose / first keyframe.
- `run_agent_capture()` writes stable filenames
  (`agent_full_body.png`, `agent_portrait.png`,
  `latest_agent_capture.json`) without timestamp suffixes.
- `latest_agent_capture.json` omits `capturedAt`, sets
  `"deterministic": true`.

Two consecutive `--agent-capture --deterministic` runs produce
byte-identical PNGs (verified by sha256 = `dc7d7f7598169abe…`).

**`FrameClock::pin_dt` / `unpin_dt` / `is_pinned`** added to
[crates/engine_core/src/time.rs](avatar-studio/crates/engine_core/src/time.rs)
with a unit test.

**SSIM diff helper** at
[crates/renderer/src/diff.rs](avatar-studio/crates/renderer/src/diff.rs).
Wraps `image-compare 0.4`'s `rgba_hybrid_compare` plus a manual
per-pixel max-channel-diff probe. Returns a `DiffReport` with
SSIM, max channel diff, diff pixel count, and total pixels.
`passes(report, ssim_min, max_pct)` is the single decision call
that every golden test makes (currently `passes(report, 0.99,
0.01)`). 2 unit tests cover the identical-image and black-vs-white
extremes.

**New `crates/qa` integration package.** Workspace-level integration
tests don't have a canonical home in a Cargo workspace, so Phase 16
introduces a dedicated test-only crate. It depends on every public
crate, exposes shared helpers (`workspace_root`,
`run_avatar_desktop`, `assert_matches_golden`,
`read_perf_report`), and houses five integration test binaries:

| Test                          | Coverage                                                              | Count |
| ----------------------------- | --------------------------------------------------------------------- | ----- |
| `golden_capture.rs`           | SSIM compare full_body + portrait vs committed goldens                | 2     |
| `save_load_round_trip.rs`     | Avatar with body+top+colours+expression+animation through CharacterStore | 3     |
| `asset_builder_pipeline.rs`   | `import` copies + writes metadata + DB; `list`; `--force` semantics   | 3     |
| `soak.rs`                     | 500 headless capture_rgba frames; avg < 33 ms; RSS growth ≤ 1.5×      | 1     |
| `perf_baseline.rs`            | `--agent-perf` JSON vs `tests/baselines/perf_baseline.json`           | 1     |

**Committed baselines.**
- `tests/golden/full_body.png` (25,714 bytes) — captured with
  `--agent-capture --deterministic`; shows rig + Phase 7 top in bind
  pose against the Catppuccin background.
- `tests/golden/portrait.png` (29,590 bytes) — same setup, portrait
  preset.
- `tests/baselines/perf_baseline.json` — 300-frame release-build
  `--agent-perf --deterministic` run: `averageFps=61.24`,
  `averageGpuMs=0.30`, `p99FrameMs=17.57`, `passed=true`.

**`PerfReport` gained `Deserialize`** so test code can parse a stored
JSON baseline. The serialize derive was already there; this just
adds the symmetric trait.

**GitHub Actions CI** at
[.github/workflows/ci.yml](avatar-studio/.github/workflows/ci.yml).
Single job on `windows-latest`:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo build --workspace --bins` (integration tests shell out
   to the built binaries — must exist before step 4)
4. `cargo test --workspace`
5. Uploads `user_data/debug_screenshots/`,
   `user_data/perf/`, `tests/golden/`, and `tests/baselines/` as a
   single `agent-screenshots-and-perf` artifact, on success or
   failure.

`Swatinem/rust-cache@v2` caches `~/.cargo` + `target/`.

**Operator docs** at
[docs/testing.md](avatar-studio/docs/testing.md) covers how to run
the suite locally, how to update goldens / perf baselines when a real
render change lands, and the cross-machine GPU drift caveat for the
first CI run.

### File map

```
+ crates/qa/Cargo.toml                       new test-only package
+ crates/qa/src/lib.rs                       workspace_root, run_avatar_desktop,
                                              assert_matches_golden, read_perf_report
+ crates/qa/tests/golden_capture.rs          2 SSIM tests
+ crates/qa/tests/save_load_round_trip.rs    3 character round-trip tests
+ crates/qa/tests/asset_builder_pipeline.rs  3 CLI tests via std::process::Command
+ crates/qa/tests/soak.rs                    500-frame headless soak
+ crates/qa/tests/perf_baseline.rs           fps + p99 regression gate
+ crates/renderer/src/diff.rs                SSIM wrapper + 2 unit tests
M crates/renderer/src/lib.rs                 pub mod diff
M crates/renderer/Cargo.toml                 + image-compare
M crates/engine_core/src/time.rs             FrameClock::{pin_dt,unpin_dt,is_pinned} + test
M crates/engine_core/src/perf.rs             PerfReport: + Deserialize
+ tests/golden/full_body.png                 committed v1.0 baseline
+ tests/golden/portrait.png                  committed v1.0 baseline
+ tests/baselines/perf_baseline.json         committed perf baseline
M Cargo.toml                                 + qa member, + image-compare + sysinfo deps
M apps/avatar_desktop/src/main.rs            --deterministic flag + parse test
M apps/avatar_desktop/src/app.rs             StartupOptions.deterministic,
                                              App.deterministic, stable filenames,
                                              FrameClock pinned at startup,
                                              animation player frozen in agent capture
+ .github/workflows/ci.yml                   fmt + clippy + test + artifact upload
+ docs/testing.md                            QA walkthrough
M docs/handover_progress.md                  this entry
```

No new product features. Pure test infrastructure + one CLI flag.

### Verification

- `cargo fmt --all -- --check` clean.
- `cargo test --workspace` → **77 passed, 0 failed** (was 63;
  +14 across the new crate, renderer, engine_core, and main.rs).
- `cargo run --bin avatar_desktop -- --agent-capture --deterministic`
  → byte-stable PNGs (re-running matches the committed goldens
  exactly — SSIM 1.0).
- `cargo test -p qa --test golden_capture` passes
  (`agent_full_body.png` SSIM ≥ 0.99 against committed baseline).
- `cargo test -p qa --test soak --release` passes:
  500 frames | avg 0.45 ms | max 3.80 ms | RSS 224 MB → 226 MB.
- `cargo test -p qa --test perf_baseline --release` passes:
  current 61.24 avg FPS vs baseline floor 55.1 (90 %), current
  p99 17.57 ms vs ceiling 21.09 ms (120 %).

### Known caveats (Phase 16)

- **Goldens captured on dev machine (Intel Iris Xe via Vulkan).** A
  fresh CI run on `windows-latest` may exceed the SSIM tolerance
  the first time because the GPU stack is different (WARP / DX
  software fallback). Fix is to capture fresh goldens once on a
  clean CI worker and re-commit — documented in
  `docs/testing.md`.
- **`--deterministic` doesn't control wgpu tessellation, depth
  rounding, or driver-specific texture filtering.** The 0.99 SSIM
  threshold + 1 % diff-pixel allowance is what absorbs that
  variance.
- **Soak `process_rss_bytes` measures OS-reported RSS** which
  includes wgpu's shared-memory GPU allocations on integrated
  graphics. The 1.5× growth tolerance is generous on purpose.
- **No code coverage report.** Out of scope; `cargo-tarpaulin`
  add-on is a future improvement.
- **No fuzz / proptest / mutation testing.** Deferred to a Phase
  17+ initiative if/when needed.
- **Integration tests assume a built workspace.** `cargo test
  --workspace` works in CI because step 3 runs
  `cargo build --workspace --bins` first. Running just `cargo test
  -p qa` from a fresh checkout without building first will fail
  with a helpful error from `avatar_desktop_bin()`.

---

## Phase 15 shipped — Packaging and installer (Claude Opus 4.7, 2026-05-16)

Roadmap exit criterion ("Signed Windows installer (.msi)") met. The
workspace now produces `target/wix/avatar_desktop-0.1.0-x86_64.msi`
(10.1 MB, signed + timestamped) via a single `pwsh tools/build_installer.ps1`
invocation.

### What's new

**Path resolution (`crates/engine_core/src/paths.rs`)** — a new module
that detects three runtime layouts and resolves assets + user_data
accordingly:

| Mode | Trigger | assets root | user_data root |
| ---- | ------- | ----------- | -------------- |
| Workspace | exe under `target/{debug,release}` | `./assets` | `./user_data` |
| Portable | `AVATAR_STUDIO_PORTABLE=1` or `avatar_studio.portable` marker | `<exe_dir>/assets` | `<exe_dir>/user_data` |
| Installed | anything else (MSI install) | `<exe_dir>/assets` | `%LOCALAPPDATA%\AvatarStudio` |

Env overrides (`AVATAR_STUDIO_ASSETS`, `AVATAR_STUDIO_USER_DATA`) trump
all detection. `dirs = "5"` added to workspace deps for
`data_local_dir()`. 5 new unit tests in `paths::tests`.

`apps/avatar_desktop/src/app.rs` no longer has *any* hard-coded
`"user_data/..."` or `"assets/..."` strings; every call site routes
through `self.paths.*`. `cargo run` still works identically.

**App icon (`assets/icon/avatar_studio.ico`)** — generated by a new
`asset_builder gen-icon` subcommand in `apps/asset_builder/src/icon_gen.rs`.
256×256 RGBA: Catppuccin panel rounded square + accent disc + bold "A"
glyph. `image` workspace dep gained the `ico` feature.

**Windows resource (`apps/avatar_desktop/build.rs`)** — uses
`winresource = "0.1"` (Windows-only build-dep) to embed the icon plus
`ProductName / FileDescription / CompanyName / LegalCopyright` into
`avatar_desktop.exe`. The icon now shows up in Explorer, the title
bar, and the Programs list.

**Dev signing cert (`tools/dev_codesign.ps1`)** — one-time PowerShell
script that calls `New-SelfSignedCertificate` for a 3-year code-signing
cert (`CN=Avatar Studio Dev`) and exports a PFX to
`%USERPROFILE%\.avatar_studio\dev-cert.pfx`. Password from
`$env:AVATAR_STUDIO_PFX_PASS`.

**WiX source (`wix/main.wxs` + `wix/LICENSE.rtf`)** — hand-customised
WiX 3 product definition:
- Per-user install scope (no admin, no UAC).
- Install path: `%LOCALAPPDATA%\Programs\AvatarStudio\`.
- Both binaries + `assets/processed/**` + `assets/icon/avatar_studio.ico`
  + `LICENSE.rtf`.
- Start Menu shortcut for `avatar_desktop.exe` only.
- `WixUI_Minimal` wizard.
- `MajorUpgrade` keyed on a fixed `UpgradeCode` GUID so future versions
  replace cleanly without prompting.
- `[package.metadata.wix]` in `apps/avatar_desktop/Cargo.toml` declares
  the upgrade + path GUIDs (committed once, never regenerated).

**Build pipeline (`tools/build_installer.ps1`)** — one-shot orchestrator:
1. `cargo build --release --bin avatar_desktop --bin asset_builder`.
2. `heat.exe dir assets\processed -gg -sfrag -srd -sreg -scom`
   harvests the asset tree into `wix\assets.wxs`.
3. `cargo wix --no-build --include wix\main.wxs --include wix\assets.wxs`
   compiles + links the MSI.
4. `Set-AuthenticodeSignature` with the PFX + DigiCert RFC-3161
   timestamp server.
5. Prints SHA-256.

Suppresses `ICE38 / ICE43 / ICE57 / ICE64 / ICE91` light validators —
all per-machine-install rules that don't apply to a per-user MSI under
`LocalAppDataFolder`. `Get-PfxCertificate -Password` is PS 7+ only, so
the script builds the `X509Certificate2` directly via `New-Object` for
PS 5.1 compatibility.

**Tooling installed (documented in `docs/packaging.md`):**
- `cargo install cargo-wix --locked` → 0.3.9.
- WiX Toolset 3.11.2 portable binaries unzipped to
  `%USERPROFILE%\.avatar_studio\wix311`.
- No admin needed; no `signtool.exe` install needed (PowerShell's
  built-in `Set-AuthenticodeSignature` handles it).

### File map

```
+ crates/engine_core/src/paths.rs        ResolvedPaths + PathMode + resolve()
M crates/engine_core/src/lib.rs          pub mod paths + re-exports
M crates/engine_core/Cargo.toml          + dirs workspace dep
M Cargo.toml                             + dirs = "5", image feature "ico"
M apps/avatar_desktop/src/app.rs         every path call site → self.paths.*
+ apps/avatar_desktop/build.rs           winresource icon embed
M apps/avatar_desktop/Cargo.toml         + winresource build-dep,
                                          [package.metadata.wix]
+ apps/asset_builder/src/icon_gen.rs     procedural icon generator
M apps/asset_builder/src/main.rs         + gen-icon subcommand
+ assets/icon/avatar_studio.ico          generated 256x256 icon
+ wix/main.wxs                           WiX 3 product definition
+ wix/LICENSE.rtf                        EULA shown in the installer
+ tools/dev_codesign.ps1                 self-signed dev cert generator
+ tools/build_installer.ps1              build + sign + hash MSI
+ docs/packaging.md                      build / install / sign walkthrough
M .gitignore                             /wix/assets.wxs, *.pfx, *.cer, *.snk
```

### Verification (verified by this agent, 2026-05-16)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean (Windows host).
- `cargo test --workspace` → **63 passed, 0 failed** (was 58; +5 in
  `engine_core::paths::tests`).
- `cargo run --bin avatar_desktop -- --agent-capture` still produces
  PNGs in `./user_data/debug_screenshots/` (Workspace mode unchanged).
- `pwsh tools/dev_codesign.ps1` wrote
  `%USERPROFILE%\.avatar_studio\dev-cert.pfx` (thumbprint
  `87D539DE73E33E42608957A4A0C4A317BF982666`, valid until 2029-05-16).
- `pwsh tools/build_installer.ps1` → built and signed
  `target/wix/avatar_desktop-0.1.0-x86_64.msi`.
  - Size: 10.1 MB.
  - SHA-256: `CE5F6F064C2775DFE9C5DC24E786C47B1B19C2872CC19C6E5A1ACA8D98BD78AF`.
  - `Get-AuthenticodeSignature` reports the signer as
    `CN=Avatar Studio Dev`, timestamped by
    `DigiCert SHA256 RSA4096 Timestamp Responder 2025 1`.
  - Status `UnknownError` until the dev cert is installed as Trusted
    Root (expected for self-signed; documented in `packaging.md`).

### Known caveats (Phase 15)

- **Self-signed cert + SmartScreen.** End users see the SmartScreen
  warning until the dev cert is in their Trusted Root. The
  `Get-AuthenticodeSignature` status remains `UnknownError` until then.
  Production needs a real OV / EV cert or Azure Trusted Signing
  (one-line swap in `build_installer.ps1`, documented).
- **`SHA-256` will rebuild every time the harvest produces a new
  component GUID.** `heat.exe` is invoked with `-gg` so each run gets
  fresh GUIDs; committed `wix/assets.wxs` would stabilise the SHA but
  isn't required for upgrades because the `UpgradeCode` is fixed.
  `wix/assets.wxs` is therefore gitignored.
- **Per-machine install path is out of scope.** The current MSI is
  per-user only; a per-machine variant would need `InstallScope="perMachine"`,
  UAC, and an `HKLM`-rooted KeyPath strategy.
- **Single-resolution ICO** (256×256). Windows scales it for 16/32/48
  views; cleaner multi-frame ICOs are deferred. Re-author via
  `asset_builder gen-icon` if needed.
- **`asset_builder.exe` ships in the install dir** but doesn't get a
  Start Menu shortcut. Power users can run it from PowerShell at
  `%LOCALAPPDATA%\Programs\AvatarStudio\asset_builder.exe`.
- **`SETTINGS_PATH` constant removed.** Old in-repo settings under
  `./user_data/settings.json` are still read in Workspace mode. Users
  with installed builds will get a fresh `%LOCALAPPDATA%\AvatarStudio\settings.json`
  on first launch — no auto-migration.

---

## Caveats sweep (Claude Opus 4.7, 2026-05-16)

Six Phase 13 + 14 caveats cleared in one pass.

### 1. Sample asset thumbnails populated

Ran `asset_builder thumbnail <id>` for the three hand-shipped sample
assets (`body_phase4_rig_001`, `body_duck_001`, `top_phase7_basic_001`)
and patched their JSON sidecars to point at the new PNGs under
`assets/processed/thumbnails/`. The DB scanner now also reflects them
on next launch, so the asset picker shows real thumbnails for every
sample asset.

### 2. Inter Variable → Inter Regular static

`crates/ui/fonts/Inter-Variable.ttf` (~880 KB) swapped for
`Inter-Regular.ttf` (~412 KB), cut from the rsms/inter v4.1 release
(`extras/ttf/Inter-Regular.ttf`). `crates/ui/src/fonts.rs` updated.
We never render multiple weights, so nothing visual changes — the
binary just gets ~470 KB smaller.

### 3. `--no-vsync` CLI flag

`apps/avatar_desktop/src/main.rs` accepts `--no-vsync`, plumbed
through `StartupOptions.vsync` into `Renderer::new(window, clear, vsync)`
and `GpuContext::new(window, vsync)`. With `--no-vsync`, present mode
prefers `Mailbox`, then `Immediate`, falling back to `Fifo` if neither
is supported. Unblocks uncapped perf benchmarking. New unit test
`parses_no_vsync_flag`.

### 4. Per-character gallery thumbnails

After a successful save, `App::write_character_thumbnail(id)` re-renders
the live scene at 256×256 (FullBody preset, opaque background) via a
new `capture_scene_internal(size, view, transparent_background)` helper
that mirrors the existing PNG export path without the 512/1024/2048
size gate. PNG goes to `user_data/characters/<id>.png` via
`image::save_buffer`. `SavedCharacterRow` gained a `thumb_uri: Option<String>`
field that the gallery row component renders via the egui_extras
`file://` image loader. Missing thumbnails still fall back to the
generic `IMAGE` icon. Best-effort: a failed thumbnail write logs a
warning but doesn't fail the save.

### 5. Catppuccin-toned toasts

The bg of `egui-notify` toasts already uses egui's `visuals.bg_fill`,
which the theme sets to `TOKENS.panel`. The icon/level color was the
mismatch — `egui-notify` hard-codes ERROR/SUCCESS/INFO/WARNING colors
that don't match Catppuccin. Rewired `App::toast_success/info/error`
to call `Toast::custom` with `ToastLevel::Custom(symbol, color)`,
substituting our palette: ✓ in `TOKENS.success`, ℹ in `TOKENS.accent`,
✕ in `TOKENS.error`. Result: badge color now matches the rest of the
dark theme.

### 6. GPU timestamp queries

`crates/renderer/src/timer.rs` (new) wraps a 2-slot `wgpu::QuerySet`,
a resolve buffer, and a CPU-mappable readback buffer. `Renderer::new`
opts in to `Features::TIMESTAMP_QUERY | TIMESTAMP_QUERY_INSIDE_ENCODERS`
when the adapter exposes both; otherwise `gpu_timer` stays `None` and
GPU samples report as `None` (gracefully omitted from JSON).
`Renderer::render` brackets the encoder with `timer.begin/end`,
calls `after_submit` once the frame is queued, and `poll`s for ready
readbacks each frame. A 1-frame ping-pong protocol skips writes while
a readback is in flight so we never write to a mapped buffer.

`crates/engine_core/src/perf.rs` gained `FrameSample.gpu_ms: Option<f32>`
plus `average_gpu_ms / p95_gpu_ms / max_gpu_ms`, which `PerfReport`
emits when present. The Diagnostics side panel now shows a `GPU` row.

### File map

```
M crates/ui/src/fonts.rs                 Inter-Variable → Inter-Regular
M crates/ui/fonts/Inter-Regular.ttf      new 412 KB static font
D crates/ui/fonts/Inter-Variable.ttf     880 KB variable font removed
M crates/ui/src/layout.rs                SavedCharacterRow.thumb_uri,
                                          DiagnosticsStatus.gpu_ms, GPU row
M crates/renderer/src/renderer.rs        GpuContext::new(window, vsync),
                                          Renderer::new(window, clear, vsync),
                                          Renderer.gpu_timer + last_gpu_ms,
                                          render() begin/end/poll
+ crates/renderer/src/timer.rs           GpuTimer
M crates/renderer/src/lib.rs             pub mod timer
M crates/engine_core/src/perf.rs         FrameSample.gpu_ms,
                                          FrameStats::{average,p95,max}_gpu_ms,
                                          PerfReport gpu fields
M apps/avatar_desktop/src/main.rs        --no-vsync flag + test
M apps/avatar_desktop/src/app.rs         StartupOptions.vsync, App.vsync,
                                          Renderer::new(…, self.vsync),
                                          write_character_thumbnail,
                                          capture_scene_internal,
                                          refresh_gallery_rows fills thumb_uri,
                                          Toast::custom with TOKENS palette,
                                          plumbs renderer.last_gpu_ms into FrameSample
M assets/processed/metadata/phase4_rig.json   thumbnail field set
M assets/processed/metadata/duck.json         thumbnail field set
M assets/processed/metadata/phase7_top.json   thumbnail field set
+ assets/processed/thumbnails/body_phase4_rig_001.png
+ assets/processed/thumbnails/body_duck_001.png
+ assets/processed/thumbnails/top_phase7_basic_001.png
```

### Verification

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` → **58 passed, 0 failed** (was 57; added
  `parses_no_vsync_flag`).
- `cargo run --bin avatar_desktop -- --agent-perf` → exit 0.
- Fresh perf report (`user_data/perf/latest_perf_report.json`):
  - `averageFps`: 62.95
  - `p99FrameMs`: 17.30
  - `averageGpuMs`: 0.31
  - `p95GpuMs`: 0.36
  - `maxGpuMs`: 0.40
  - `passed`: true
- `cargo run --bin avatar_desktop -- --agent-capture` → exit 0.
  - `agent_full_body_1778906890939.png` (25,714 bytes)
  - `agent_portrait_1778906890961.png` (29,590 bytes)
- Renderer log: `gpu timestamp queries enabled` on this adapter
  (Intel Iris Xe via Vulkan).

### Remaining notes

- The startup `egui_wgpu` warning "Detected a linear (sRGBA aware)
  framebuffer Bgra8UnormSrgb. egui prefers Rgba8Unorm or Bgra8Unorm" is
  inherited and not introduced by this work.
- `asset_builder thumbnail <id>` writes the metadata sidecar only when
  the file is named `<id>.json`. The three hand-shipped sample sidecars
  use different stems (`phase4_rig.json`, `duck.json`, `phase7_top.json`),
  so we patched them by hand. Future imports use the `<id>.json`
  convention out of the box.

---

## Phase 14 shipped - Performance Optimization (Codex, 2026-05-16)

Roadmap exit criterion ("60 FPS @ 1080p on mid-range laptop") is now
measurable and passing on the current Windows test machine.

### What's new

- `crates/engine_core/src/perf.rs`
  - Added CPU-side `FrameSample`, `FrameStats`, and `PerfReport`.
  - Tracks rolling average FPS, min FPS, average frame time, p95/p99 frame
    time, max frame time, target FPS/frame budget, scene mode, window size,
    and instance count.
- `apps/avatar_desktop/src/main.rs`
  - Added CLI flags:
    - `--agent-perf`
    - `--perf-frames <N>` / `--perf-frames=N`
    - `--show-perf`
  - Added CLI parser tests for valid/invalid perf frame counts.
- `apps/avatar_desktop/src/app.rs`
  - Instruments redraw CPU phases: egui, tessellation, pose/palette, scene
    instance build, render submit, and total frame.
  - Adds `F3` diagnostics toggle and `--show-perf` startup visibility.
  - Adds `--agent-perf` deterministic run: loads Phase 4 Rig + Phase 7 Top,
    samples 300 frames by default, writes `user_data/perf/latest_perf_report.json`,
    then exits.
  - Stops rebuilding equipped-row display data every frame unless equipment or
    tint state changed.
  - Caches posed world transforms + skinning palette when pose state is stable.
- `crates/ui/src/layout.rs`
  - Adds compact Diagnostics section, hidden unless diagnostics are enabled.
- `crates/renderer/src/scene.rs`
  - Skips skin palette buffer writes for static-only frames.

### Verification (verified by Codex)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` -> **57 passed, 0 failed**.
- `cargo run --bin avatar_desktop -- --agent-perf` -> exit 0.
- Perf report: `user_data/perf/latest_perf_report.json`
  - `frameCount`: 300
  - `averageFps`: 61.55
  - `averageFrameMs`: 16.25
  - `p95FrameMs`: 17.46
  - `p99FrameMs`: 17.79
  - `maxFrameMs`: 18.56
  - `windowSize`: `[1728, 918]`
  - `sceneMode`: `avatar`
  - `instanceCount`: 3
  - `passed`: true
- `cargo run --bin avatar_desktop -- --agent-capture` still succeeds.

### Known caveats (Phase 14)

- Timings are CPU-side only. GPU timestamp queries remain deferred.
- Vsync is still enabled, so average FPS naturally hovers near display refresh.
- The Diagnostics panel is intentionally developer-facing, not product polish.
- `SceneInstance` itself is still built as a small per-frame borrowed vector;
  reusing that storage directly would complicate lifetimes for little benefit.

---

## Phase 13 refinement - Compact Editor UI (Codex, 2026-05-16)

After first manual inspection, the Phase 13 pass looked too heavy: wide side
panel, large card borders, launcher-like category buttons, and generous
vertical gaps. The UI was functionally correct, but it took too much viewport
space and felt more like a dashboard than an editor.

### What's changed

- `crates/ui/src/theme.rs`
  - Kept Catppuccin Mocha, but lowered panel/surface contrast.
  - Reduced button padding, default interact height, window margin, and heading
    size.
  - Softer divider/border treatment for section frames.
- `crates/ui/src/components.rs`
  - Sections now use smaller margins and a subtler stroke.
  - Category tabs are fixed compact controls instead of growing to large pills.
  - Asset/gallery rows are shorter, with smaller thumbnails and hover-only row
    fill.
  - Icon buttons and swatches are slightly smaller.
- `crates/ui/src/layout.rs`
  - Side panel width reduced from `260` to `232` logical pixels.
  - Vertical gaps and list max heights tightened so more editor controls fit
    without crowding the viewport.

### Verification

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` -> **51 passed, 0 failed**.
- `cargo run --bin avatar_desktop -- --agent-capture` -> exit 0.
- Fresh agent screenshots:
  - `agent_full_body_1778899021951.png` -> 25,714 bytes.
  - `agent_portrait_1778899021983.png` -> 29,590 bytes.

### Follow-up fix

- `apps/avatar_desktop/src/app.rs`
  - Catalog assets in category `body` now branch by loaded GLB contents.
  - Rigged + skinned bodies still enter avatar mode.
  - Static body-category samples like `Rubber Duck` / `Duck Copy` now enter
    static mode instead of failing with "Body asset has no skeleton".
  - Assets that declare `compatibleSkeleton` but fail to load as skinned bodies
    still error clearly.
  - Re-verified with `cargo fmt --all`, `cargo check --workspace`,
    `cargo test --workspace`, and `cargo run --bin avatar_desktop -- --agent-capture`.

---

## Phase 13 shipped - UI/UX Polish (Codex, 2026-05-16)

Roadmap exit criterion ("Iconography, layouts, animations consistent") is now
met for the side-panel MVP.

### Sources used

- egui `Visuals` / `WidgetVisuals` docs.rs API for theme styling.
- `egui-phosphor` GitHub/crate API for icon font installation and glyph names.
- `egui-notify` crates.io/docs API for toast notifications.
- `egui_extras::install_image_loaders` docs.rs API for file-backed thumbnails.
- Catppuccin Mocha palette for UI color tokens.
- Material-style spacing/token practice for the `4/8/12/16/24` spacing scale.

### What's new

- `crates/ui/src/theme.rs`
  - Catppuccin Mocha tokens.
  - `theme::apply(ctx)` sets dark visuals, selection colors, widget states,
    spacing, tooltip delay, and `animation_time = 0.18`.
  - 3 tests cover color ranges, dark mode, and spacing monotonicity.
- `crates/ui/src/fonts.rs`
  - Embeds `crates/ui/fonts/Inter-Variable.ttf`.
  - Installs Inter as the first proportional font.
  - Adds the Phosphor Regular icon font to egui.
- `crates/ui/src/icons.rs`
  - Single Phosphor import surface for layout/components.
- `crates/ui/src/components.rs`
  - Shared section frames, subheaders, primary/secondary/icon buttons,
    tabs, swatches, asset rows, gallery rows, and empty states.
  - 1 smoke test confirms a section renders in a headless egui context.
- `crates/ui/src/layout.rs`
  - Rebuilt from raw separators/default widgets into component-based sections.
  - Side panel widened from `220` to `260`.
  - Category tabs now have icons and selected-state styling.
  - Asset rows now show Phase 12 thumbnails inline when
    `AssetMeta.thumbnail` points to a real file under `assets/processed/`.
  - Empty states use icon + caption blocks.
  - Footer now says `Phase 13 - UI/UX polish`.
- `apps/avatar_desktop/src/app.rs`
  - Installs fonts, theme, and egui image loaders at startup.
  - Adds `egui_notify::Toasts` anchored bottom-right.
  - Save/load/export success and failure paths now emit toasts.
- `crates/engine_core/src/config.rs`
  - Default window size bumped to `1340x780`.
- Workspace deps:
  - `egui_extras = 0.29` with `all_loaders`.
  - `egui-phosphor = =0.7.0`.
  - `egui-notify = =0.17.0`.

### Verification (verified by Codex)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` -> **51 passed, 0 failed**.
- `cargo run --bin avatar_desktop -- --agent-capture` -> exit 0.
- Fresh agent screenshots verified:
  - `agent_full_body_1778898323344.png` -> 1024x1024, 25,714 bytes.
  - `agent_portrait_1778898323377.png` -> 1024x1024, 29,590 bytes.
- Visible app launched after build for manual inspection.

### Known caveats (Phase 13)

- Inter Variable is embedded directly and is about 880 KB, larger than the
  original plan estimate.
- Saved-character gallery rows still use a generic image icon; per-character
  thumbnails remain deferred.
- `components::asset_row` shows file-backed thumbnails, but built-in metadata
  still has `thumbnail: null` for most sample assets until thumbnails are
  generated with Phase 12 tooling.
- Toast colors come from `egui-notify` defaults, while the surrounding app
  uses Catppuccin tokens. Good enough for MVP polish; exact toast theming can
  be refined later.
- The existing Vulkan/egui-sRGB warning baseline still appears during launch
  and agent capture; no new app-level errors were introduced.

---

## Phase 12 shipped — Asset Builder CLI (Claude Opus 4.7, 2026-05-15)

Roadmap exit criterion ("Import GLB → metadata + thumbnail + DB row") now met.
The `asset_builder` binary grew four real subcommands.

### What's new

- `asset_builder validate <glb>` — read-only inspection. Parses the file,
  reports mesh / primitive / vertex counts, mesh AABB with longest axis,
  animation count, and (when a skin is present) the skeleton root name +
  bone list via `animation::Skeleton::from_gltf`. Flags `--category`
  and `--require-skeleton` enforce category-level rules and exit non-zero
  on failure.
- `asset_builder import <glb>` — full ingest. Copies the GLB into
  `assets/processed/avatars/<plural>/<id>.glb`, writes a
  pretty-printed `AssetMeta` sidecar to
  `assets/processed/metadata/<id>.json`, optionally renders a thumbnail
  with `--thumb`, and **upserts the SQLite catalog row directly**
  (`user_data/asset_catalog.sqlite`). Accepts a sidecar JSON
  via `--meta <path>` whose fields can be overridden with flags
  (`--id`, `--category`, `--display-name`, `--body-type` (repeatable),
  `--skeleton`, `--supports-color`, `--default-color r,g,b`, `--tags`).
  Refuses to overwrite an existing id unless `--force` is passed.
- `asset_builder thumbnail <id>` — re-render the thumbnail for an asset
  already in the catalog. Looks up the model path in SQLite, renders
  via the headless renderer, writes the PNG, and updates both the JSON
  sidecar and the DB row's `thumbnail` field.
- `asset_builder list [--category <cat>]` — print every catalog row,
  one per line, with id / category / display-name / model / thumb=yes|no.

### Headless renderer

To render thumbnails without a window the renderer crate now has a
Surface-less code path. `GpuContext::surface` and `_window` became
`Option`, and a new `GpuContext::new_headless()` / `Renderer::new_headless()`
constructor brings up wgpu without a swapchain. `Renderer::capture_rgba`
already did its own offscreen color+depth target work — it now happily
runs on the headless context. `Renderer::render()` still requires the
on-screen surface and panics if called on a headless renderer.

The thumbnail pipeline (`apps/asset_builder/src/thumbnail.rs`):
1. `Renderer::new_headless` → wgpu device/queue.
2. `renderer::load_glb` → mesh + optional base color image + skeleton.
3. `SceneRenderer::new(device, Rgba8UnormSrgb)`.
4. `SceneRenderer::make_material(...)` with the GLB's base color image.
5. Bind-pose `SkinningPalette` if the mesh is skinned.
6. `OrbitCamera` framed on the mesh AABB at a slight downward pitch.
7. `Renderer::capture_rgba(width, height, clear, |fc| scene.draw(...))`.
8. `image::save_buffer(...)` to write the RGBA8 PNG.

### File map

```
M apps/asset_builder/Cargo.toml          + renderer/export/image/wgpu/bytemuck
M apps/asset_builder/src/main.rs         Cmd::{Import, Validate, Thumbnail, List}
+ apps/asset_builder/src/paths.rs        category_dir, processed paths, id validator
+ apps/asset_builder/src/validate.rs     ValidateReport + run_validate + tests
+ apps/asset_builder/src/import.rs       run_import + ImportArgs + tests
+ apps/asset_builder/src/thumbnail.rs    render_thumbnail
+ apps/asset_builder/src/list.rs         run_list
M crates/renderer/src/renderer.rs        Optional surface; new_headless paths
                                          headless_capture_writes_rgba test
```

### Verification (verified by this agent, 2026-05-15)

- `cargo fmt --all` — clean.
- `cargo check --workspace` — clean.
- `cargo test --workspace` — **47 passed, 0 failed** (animation 14,
  asset_builder 7 (+6 new), avatar 14, avatar_desktop 1, export 2,
  renderer 9 (+1 new)). 5 new unit tests target the headless capture,
  `paths::validate_asset_id`, `category_dir`, `validate_glb` happy +
  sad paths, and `run_import` round-trip + sidecar override.
- CLI smoke (all four subcommands run on the workspace):
  ```
  asset_builder validate assets/processed/avatars/bodies/phase4_rig.glb \
      --category body --require-skeleton
  # → meshes 1 / 360 verts, animations 1, longest 1.820 m,
  #   skeleton root=avatar_skeleton_v1 bones=18 ✓
  asset_builder import assets/processed/avatars/bodies/duck.glb \
      --id duck_copy_001 --category body --display-name "Duck Copy" --thumb --force
  # → GLB copied, metadata JSON written, thumbnail PNG (8.6 KB) at
  #   assets/processed/thumbnails/duck_copy_001.png, DB row present ✓
  asset_builder list --category body
  # → duck_copy_001 / body_phase4_rig_001 / body_duck_001 (3 assets) ✓
  asset_builder thumbnail body_phase4_rig_001 --width 256 --height 256
  # → rig.png (2.5 KB) shows the rig posed in bind pose, centered ✓
  ```
- Visual check: opened both PNGs — duck_copy_001 shows the duck mesh
  in a real perspective render, body_phase4_rig_001 shows the
  18-bone rig stick-figure centered in frame.

### Known caveats (Phase 12)

- **Camera framing is mesh-AABB-based, not aesthetically tuned.** The
  duck thumbnail crops close because the duck's mesh extents are large
  relative to its visually-interesting volume. Phase 13 polish can
  add per-category framing presets.
- **Bind-pose only.** Wearable thumbnails are bind-pose meshes on their
  own (not composited onto a body), so a top thumbnail looks "floating".
  Composite avatar thumbnails are deferred to Phase 13.
- **Width/height aren't forced square.** Caller can pass non-square
  thumbnail dimensions; the camera aspect adapts. The PNG writer is not
  the export crate's `write_rgba_png` (that's restricted to 512/1024/2048
  for user-facing PNG exports); we call `image::save_buffer` directly.
- **DB writes while desktop app is running.** SQLite's default journal
  mode is fine for one writer + one reader, but if the user has the
  desktop app open the gallery won't refresh until they manually click
  the Gallery's Refresh button (Phase 10 wired that up).
- **No batch import.** The plan called out `import-dir` as out-of-scope.
  Loop over files in PowerShell / bash if you need to ingest many.

## Read-through verification (Claude Opus 4.7, 2026-05-15)

Caught up on Codex's Phase 10 + Phase 11 work after the handover.

- `cargo check --workspace` → green (11.86 s incremental).
- `cargo test --workspace` → **40 passed, 0 failed** (animation 14,
  asset_builder 1, avatar 14, avatar_desktop 1, export 2, renderer 8;
  others 0).
- `user_data/debug_screenshots/` has 2 PNGs + manifest from a prior
  `--agent-capture` run. `user_data/exports/` has one user-driven export
  (`avatar_full_body_1024_1778886443307.png`).
- Save/load surface verified: `avatar::AvatarSave` carries
  `schema_version` (rejects mismatches), `base_body`, `body_type`,
  `slots`, `colors`, `expression`, `animation`, plus sanitized id +
  unix-millis timestamps. `CharacterStore` does dir-create-on-save,
  parse-on-list, and sorts by `updated_at` DESC. Round-trips
  `Avatar → AvatarSave → Avatar` and re-equips body → wearables →
  colors → expression in order.
- PNG export path: `Renderer::capture_rgba` builds its own
  RENDER_ATTACHMENT|COPY_SRC color texture + matching depth, runs the
  caller's draw callback against a fresh `FrameCtx`, then does
  `copy_texture_to_buffer` with row alignment (`padded_bytes_per_row`)
  and an async map+read. `export::png::write_rgba_png` validates sizes
  to {512, 1024, 2048} and pixel-count, then writes via `image::save`.
- Agent capture entry point: `App::new(StartupMode::AgentCapture)` →
  `init_window_and_gpu` (still a winit/wgpu window — brief) → equips
  `body_phase4_rig_001` + `top_phase7_basic_001` → captures full-body
  + portrait at 1024 → exits the event loop. Two PNGs + JSON manifest
  land under `user_data/debug_screenshots/`.
- UI: the side panel now has **Gallery** (Save / Refresh / saved-row
  list) above Model, and **PNG Export** (size combo, view combo,
  transparent toggle, Export button) below Model. `SidePanelAction`
  grew 6 new variants
  (`SaveCharacter`, `LoadCharacter(id)`, `RefreshGallery`,
   `SetExportSize`, `SetExportPortrait`, `SetExportTransparent`,
   `ExportPng`). `SidePanelStatus` carries gallery rows, save/export
  labels, and the three export option fields.
- `docs/phase10_11_plan.md` captures the work; `docs/save_format.md`
  is the schema-v1 reference.

No drift between the handover claims and what's on disk — Codex's
verification matches mine.

## Phase 10 and 11 shipped (Codex, 2026-05-15)

Save/load gallery and PNG export are now live.

### What's new

- `docs/phase10_11_plan.md` - implementation plan and acceptance checklist.
- `crates/avatar/src/save.rs` - schema-v1 save model:
  - `AvatarSave`
  - `CharacterStore`
  - `SavedCharacterSummary`
  - sanitized ids, JSON read/write, gallery sorting, schema validation.
- `crates/export/src/png.rs` - PNG export helpers:
  - `PngExportOptions`
  - `ExportView`
  - 512/1024/2048 size validation
  - RGBA8 PNG writing.
- `crates/renderer/src/screenshot.rs` and `renderer::Renderer::capture_rgba`:
  - offscreen color/depth render target
  - GPU readback
  - BGRA/RGBA conversion into `RgbaScreenshot`.
- `crates/ui/src/layout.rs`:
  - side-panel Gallery section with Save, Refresh, and saved-character rows.
  - PNG Export section with size, full-body/portrait, transparent background, and Export PNG.
- `apps/avatar_desktop/src/app.rs`:
  - save current avatar to `user_data/characters/<id>.json`
  - load saved avatars through the catalog
  - restore body, wearables, slot colors, expression, and animation selection
  - export current scene to `user_data/exports/`
  - deterministic agent capture mode.
- `apps/avatar_desktop/src/main.rs`:
  - new CLI flag: `--agent-capture`.

### Agent / Bot Access

Coding agents can now produce inspectable screenshots without manually clicking
the UI:

```powershell
cargo run --bin avatar_desktop -- --agent-capture
```

This loads the known sample scene (`Phase 4 Rig` + `Phase 7 Top`), captures
full-body and portrait PNGs, and writes:

- `user_data/debug_screenshots/agent_full_body_<timestamp>.png`
- `user_data/debug_screenshots/agent_portrait_<timestamp>.png`
- `user_data/debug_screenshots/latest_agent_capture.json`

### Verification (verified by Codex)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` -> 40 passed.
- `cargo run --bin avatar_desktop -- --agent-capture` -> exit 0.
- Generated screenshots verified via `System.Drawing.Bitmap`:
  - `agent_full_body_1778856049499.png` -> 1024x1024, 25,714 bytes.
  - `agent_portrait_1778856049542.png` -> 1024x1024, 29,590 bytes.

### Known caveats (Phase 10/11)

- Gallery is a compact side-panel list, not a thumbnail grid yet.
- Save timestamps are unix milliseconds as strings, not ISO-8601.
- PNG export captures the 3D scene only; it intentionally excludes egui.
- Transparent export clears the background alpha, but the avatar materials are
  still opaque.
- Agent capture still creates a normal winit/wgpu window briefly, then exits.
  Fully headless capture remains a later QA/offscreen refinement.

---

## Phase 9 shipped (Claude Opus 4.7, 2026-05-15)

5-preset face & expression system. Loading any body with a `head` bone
auto-attaches a procedural face quad to that bone; an **Expression**
section in the side panel switches between 5 procedurally-drawn pixel-art
faces (Neutral / Happy / Sad / Surprised / Angry). Texture swap is
instantaneous.

### What's new

- `crates/avatar/src/expressions.rs` — `Expression` enum (5 variants) +
  `ALL` + `label()` + `as_save_str()` + `Default = Neutral`. 3 new tests.
- `crates/avatar/src/avatar.rs` — `Avatar.expression: Expression`;
  `clear_all` resets it.
- `crates/avatar/src/lib.rs` — re-export `Expression`.
- `crates/ui/src/layout.rs` —
  - `SidePanelStatus.has_face: bool`, `current_expression: Expression`.
  - `SidePanelAction::SetExpression(Expression)`.
  - New **Expression** section (between Animation and Skinning) renders
    5 selectable labels in a `horizontal_wrapped` layout. Disabled when
    `!has_face`.
- `crates/ui/src/lib.rs` — re-export `Expression`.
- `crates/renderer/src/scene.rs` —
  `SceneRenderer::rebuild_material_with_texture(device, &mut material,
   texture, label)` swaps a Material's texture + bind group in place.
  Used to flip the face quad's expression without rebuilding the whole
  material chain.
- `apps/avatar_desktop/src/face_textures.rs` — **new**, ~210 lines.
  `generate(Expression) -> DynamicImage`, plus `generate_all()` returns
  all 5. Pixel art via `image` crate: filled disks for eyes, parabola
  rasterizer for smile/frown, slanted slit eyes for Sad/Angry, "O" mouth
  for Surprised, eyebrow strokes for Angry. 128×128 RGBA8. Includes a
  test confirming all 5 produce distinct pixel data.
- `apps/avatar_desktop/src/app.rs` —
  - `face_images: HashMap<Expression, DynamicImage>` (CPU cache, baked
    on body install).
  - `face_runtime: Option<EquippedRuntime>` (the quad's mesh +
    material; lives outside `equipped_slots` because the face is
    implicit, not user-equipped).
  - `rebuild_face_image_cache()` regenerates all 5 PNGs.
  - `build_face_runtime(head_idx, head_world, bone_count, expression)`
    derives the head bone's world axes (right/up/forward), places a
    0.32 m square at `head_origin + forward * 0.18 m`, skins all 4
    verts to the head bone with weight 1.
  - `apply_face_expression(expr)` re-uploads the matching PNG as a fresh
    GPU `Texture` and swaps the face material's bind group via
    `scene.rebuild_material_with_texture`.
  - `install_body` builds the face after equipping the body (head bone
    optional — face is skipped on rigs without a head bone).
  - `enter_static_mode` clears `face_runtime` + `face_images`.
  - `SetExpression(expr)` handler writes to `avatar.expression` +
    `panel_status.current_expression` and calls `apply_face_expression`.
  - Render loop pushes `face_runtime` into the instance list after
    `equipped_slots`, using the body's shared skinning palette.
- `apps/avatar_desktop/Cargo.toml` — added `image` workspace dep
  (previously only used indirectly via renderer).
- `apps/avatar_desktop/src/main.rs` — `mod face_textures;`.

### Verification (verified by this agent)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` → **34 passed** (avatar 12 = 9 prior + 3
  Expression; avatar_desktop 1 new for face_textures; renderer 6;
  animation 14; asset_builder 1; others 0).
- `cargo run --bin avatar_desktop`:
  - Loaded Phase 4 Rig → face quad appears on front of head cube,
    Neutral expression by default.
  - Confirmed visually: skin-tone background, two black-dot eyes, short
    horizontal mouth.
  - Expression section enabled, Neutral selected.
- No new ERROR / WARN.

### Color-space caveat

The face textures are uploaded as **sRGB** via the existing
`Texture::from_dynamic_image` (which uses `Rgba8UnormSrgb`). The
procedural pixel values are designed in sRGB and display correctly.
No conversion needed at the pixel-drawing stage.

### Known caveats (Phase 9)

- Face is **independent of skin tone** — Phase 8's body color picker
  doesn't tint the face background. Phase 13 polish could unify.
- Face geometry is hardcoded (0.32 m square, 0.18 m forward of head).
  Future rigs with different head bone scale or local axes may have the
  face appear off-position. The math derives axes from the head bone's
  world bind transform, so any reasonable rig should work.
- Procedural pixel art is intentionally crude — readable at game-engine
  scale; not "polished avatar art". Replacing with authored face
  textures is a metadata + asset-pipeline task.
- No animation on the face itself (no blink, no lip sync). The quad
  moves rigidly with the head bone.
- Body swap clears the expression (via `Avatar::clear_all`). Matches the
  existing "body swap resets state" pattern.

---


## Phase 8 shipped (Claude Opus 4.7, 2026-05-15)

Per-slot color customization is live. Each row in the Equipped panel gets a
swatch beside the × button; clicking the swatch opens egui's color editor
and dragging recolors the matching mesh on every frame the picker is open.

### What's new

- `crates/avatar/src/avatar.rs` — `Avatar.slot_colors: HashMap<Slot, [f32;3]>`
  (sRGB, persistence-ready), plus `set_slot_color` / `slot_color`.
  `clear_all` now clears colors too. **3 new tests** (slot_color_round_trip,
  clear_all_clears_colors, slot_color_independent_of_slots_map) — workspace
  total **30 passed**.
- `crates/ui/src/layout.rs` —
  - `EquippedSlotRow` gained `color_srgb: [f32;3]` and `supports_color: bool`.
  - `SidePanelAction::SetSlotColor(Slot, [f32;3])` (sRGB).
  - Each equipped row renders `egui::color_edit_button_rgb` when
    `supports_color == true`; otherwise a static painted swatch with the
    same footprint.
- `apps/avatar_desktop/src/app.rs` —
  - `EquippedRuntime.tint_srgb: [f32;3]` + `supports_color: bool`.
  - `apply_tint_to_material(material, srgb)` converts sRGB→linear via
    `renderer::color::srgb_to_linear` and writes `material.base_color`.
  - `seed_slot_color(slot, meta)` returns the initial sRGB tint with
    priority: **user override → `meta.default_color` → `default_slot_tint` →
    white**.
  - `install_body` / `install_wearable` both call `seed_slot_color` and
    `apply_tint_to_material`, then store `tint_srgb` on the runtime and
    `set_slot_color` on `Avatar`.
  - `install_wearable` re-uses a remembered color iff the same asset id is
    being re-equipped into the slot; a different asset wipes the slot
    color before reseeding.
  - `handle_side_panel_action::SetSlotColor` mutates both
    `avatar.slot_colors` and `equipped_slots[slot].material.base_color`.
  - `refresh_equipped_rows` includes `color_srgb` + `supports_color`.
  - Body slot's `supports_color = true` unconditionally — overrides the
    `phase4_rig` metadata's `supports_color: false`. Wearables honor the
    metadata flag.

### Color-space convention

User-facing values (asset `default_color`, `Avatar.slot_colors`, egui
swatch) are **sRGB**. GPU values (`Material.base_color`) are **linear**.
Conversion happens at the boundary via `renderer::color::srgb_to_linear`.
The `default_slot_tint` table in app.rs is now interpreted as sRGB.

### Verification (verified by this agent)

- `cargo fmt --all` clean.
- `cargo check --workspace` clean.
- `cargo test --workspace` → **30 passed** (avatar 9 = 6 prior + 3 new;
  others unchanged).
- `cargo run --bin avatar_desktop`:
  - Loaded Phase 4 Rig → body swatch white, top section empty.
  - Equipped Phase 7 Top → top swatch blue (its `default_slot_tint`).
  - Clicked body swatch → picked brown → body recolors live.
  - Clicked top swatch → picked blue (kept default) → independent of body.
  - Confirmed visually: brown body + blue tube + skeleton overlay, no
    cross-tint bleed (Phase 7's dynamic-offset fix holds).
- No new ERROR / WARN in log.

### Known caveats (Phase 8)

- Color picker popup floats above the 3D viewport when open; that's
  fine, but it'll z-fight with egui's own focus chrome on very small
  windows. Cosmetic.
- Body re-load (via Character tab) calls `Avatar::clear_all` and so
  drops both wearable colors and the body's own previously-picked color.
  Intentional — matches the "body swap drops wearables" rule from
  Phase 7. Re-pick required.
- Wearable color persists only while the slot stays loaded **with the
  same asset id**. Reloading a different top wipes the prior color for
  that slot. By design — alternative would surprise the user when they
  swap to a brand-new top.
- The picker fires `SetSlotColor` on every drag-frame egui considers
  "changed". On the Intel iGPU this is sub-millisecond — no measurable
  hit.

---


## Phase 7 shipped (Claude Opus 4.7, 2026-05-15)

Slot-based customization is live. Two render modes:
- **Avatar mode** — a body equips into `Slot::Body`; wearables stack into their
  category-derived slot; all share the body's `SkinningPalette` so they
  animate together.
- **Static mode** — no body equipped; renders one mesh (placeholder cube or
  non-rigged custom GLB).

### What's new

- `crates/avatar/src/slots.rs` — `Slot` enum (9 variants), `Slot::ALL`,
  `Slot::from_asset_category` mapping.
- `crates/avatar/src/avatar.rs` — `Avatar` gained `slots: HashMap<Slot, String>`,
  `equip`/`unequip`/`equipped`/`iter`/`clear_all`, plus `body_type` becomes
  the active body's asset id (or `DEFAULT_BODY_TYPE` = `"default"` when none).
- `crates/ui/src/layout.rs` — new `EquippedSlotRow` + `equipped_rows` + `avatar_mode`
  on `SidePanelStatus`; new `SidePanelAction::UnequipSlot(Slot)`; new
  "Equipped" panel between asset list and Model section, with × per row
  (Body's × is disabled — use Reset to cube).
- `apps/avatar_desktop/src/app.rs` — fields split into `equipped_slots`
  (avatar mode) + `static_mesh/material` (static mode). `enter_static_mode`,
  `install_body`, `install_wearable`, `validate_wearable`,
  `load_asset_by_id` (category-aware: Body → install body, anything else →
  validate + equip), and `refresh_equipped_rows` (per-frame).
- `apps/asset_builder` — added `gen-fixture-top` subcommand. Procedural
  octagonal tube wearable, skinned to `chest`/`spine`, joint order copied
  from `phase4_rig.glb` so JOINTS_0 indices line up with the body's runtime
  palette.
  - `apps/asset_builder/src/glb_writer.rs` — hand-rolled 12-byte GLB header
    + JSON chunk + BIN chunk emitter.
  - `apps/asset_builder/src/fixtures/top.rs` — fixture generator + round-trip
    test (`cargo test -p asset_builder`).
- `assets/processed/avatars/tops/phase7_top.glb` — generated artifact (16
  verts, 16 tris, 18 joints).
- `assets/processed/metadata/phase7_top.json` — catalog entry, declares
  `compatibleSkeleton: "avatar_skeleton_v1"` and
  `compatibleBodyTypes: ["body_phase4_rig_001"]`.

### Compatibility rules (in `app::validate_wearable`)

A wearable can equip iff:
1. A body is currently equipped (`self.skeleton.is_some()`).
2. `meta.compatible_skeleton == Some("avatar_skeleton_v1")`.
3. `meta.compatible_body_types.is_empty()` OR contains `avatar.body_type`.
4. `Slot::from_asset_category(meta.category)` is `Some` and not `Slot::Body`.

Failures surface as red error text in the side panel. Currently-equipped state
is preserved. Changing the body always clears existing wearables (logged as
"Unequipped N wearable(s) for new body").

### Skin coordinate convention used by the generator

`apps/asset_builder/src/fixtures/top.rs` writes wearable vertices in **world
space at bind pose**. The inverse-bind matrices are copied verbatim from
phase4_rig, so the palette at bind = identity and vertices remain unchanged
through the GPU skinning math. The wearable's `skin.skeleton` field is set to
`None` so the runtime's `Skeleton::from_gltf` skips its root-node-name check
(our root node is the unnamed "root" joint; the body's skin is the one that
carries the skeleton-name claim).

### Verification (verified by this agent)

- `cargo fmt --all` — clean.
- `cargo check --workspace` — green.
- `cargo test --workspace` — 27 passed (was 20; +6 in `avatar` for slot/equip,
  +1 in `asset_builder` for round-trip).
- `cargo run --bin asset_builder -- gen-fixture-top` — writes 2 files. Re-runs
  overwrite cleanly.
- `cargo run --bin avatar_desktop`:
  - Catalog scans 3 assets (`duck`, `phase4_rig`, `phase7_top`).
  - Loaded **Phase 4 Rig** → `wearable equipped slot=Body` (the body itself
    is also recorded as a slot row).
  - Switched to Outfit → list shows `Phase 7 Top`.
  - Clicked it → `wearable equipped asset_id=top_phase7_basic_001 slot=Top`.
  - Re-click is idempotent (replace).
- No new ERROR/WARN in the runtime log beyond the existing Vulkan-loader /
  egui-sRGB noise.

### Known caveats (Phase 7)

- The phase7_top tube has 16 verts; deformation is minimal during idle
  because the chest/spine bones barely rotate. Increase `TUBE_SEGMENTS` or
  add ring sections if you want richer deformation.
- The wearable z-fights the body in places (radius 0.20m vs the rig's
  blocky chest). Cosmetic — Phase 8 (color) and Phase 13 (polish) can
  inflate or hide.
- "Load custom GLB…" routing: a rigged + skinned GLB is treated as a body;
  anything else goes static. There's no path to load a non-cataloged
  wearable yet — that's a Phase 12 (asset_builder import) job.
- Body-replace clearing wearables is intentional (incompat risk) but the
  warning message is terse. Phase 13 polish can spell it out.

---


## Read-through verification (Claude Opus 4.7, 2026-05-15)

This entry is appended by the current agent (Claude Opus 4.7) after the user
asked it to read every source file and confirm the handover.

- `cargo check --workspace` → green (0.39 s incremental).
- `cargo test --workspace` → 20 passed, 0 failed (14 in `animation`, 6 in
  `renderer`).
- Workspace member graph and crate boundaries verified to match this doc.
- The implicit Phase 3c texture work (UVs in `Vertex`, base color sampling in
  `mesh.wgsl`, `Texture::from_dynamic_image`) is in place — the duck renders
  with its native baseColor texture, not just a flat tint.
- Phase 4 / 5 / 6 entry points line up with the file list below:
    - Skeleton: `Skeleton::from_gltf` extracts `avatar_skeleton_v1`, validates
      MVP bone names, surfaces warnings (`crates/animation/src/skeleton.rs`).
    - Skinning: `SkinningPalette::from_world_transforms` writes 64-bone palette;
      GPU shader path keyed off `InstanceUniforms.skinning.x`
      (`crates/renderer/src/scene.rs:118-345`, `shaders/mesh.wgsl`).
    - Animation: `AnimationPlayer::tick` advances time, samples T/R/S tracks
      with step/linear interpolation, feeds the palette
      (`crates/animation/src/{clip,player}.rs`, `apps/avatar_desktop/src/app.rs:538-606`).
- Debug skeleton overlay uses `DebugLineRenderer` with `LineList` topology,
  alpha-blended, `CompareFunction::Always` so bones stay visible through opaque
  mesh interiors (`crates/renderer/src/debug_lines.rs:107`).
- New cross-cutting requirement in `docs/roadmap.md`: agent-visual-verification
  via screenshots under `user_data/debug_screenshots/`. Not implemented yet —
  belongs in Phase 16 plus a deterministic dev-mode launch path. Not blocking
  Phase 7 work.

## Current Status

The project has completed the engine proof for Phases 4, 5, and 6.

- Phase 4: `avatar_skeleton_v1` skeletons are parsed, validated, displayed in UI, and visualized as cyan debug bones.
- Phase 5: skinned mesh rendering works. GLB `JOINTS_0` and `WEIGHTS_0` are loaded, validated, normalized, and used by the GPU shader.
- Phase 6: glTF animation clips are parsed, sampled over time, and fed into the existing skinning palette. The side panel now has play, loop, and scrub controls for the `idle` clip.
- Static GLBs still work. `Rubber Duck` remains the regression asset for the non-skeletal path.
- The in-repo mannequin fixture is intentionally blocky. It is an engine/test asset, not final avatar art.

## Important Files

- `crates/animation/src/skeleton.rs`
  - Owns `Skeleton`, `Bone`, `BoneIndex`, `avatar_skeleton_v1` validation, bind-pose transforms, and debug-pose transforms.
- `crates/animation/src/skinning.rs`
  - Owns `SkinningPalette`, `MAX_BONES = 64`, bind-pose palette, debug-pose palette, and palette validation.
- `crates/animation/src/clip.rs`
  - Owns `AnimationClip`, `AnimationChannel`, `TransformTrack`, `Keyframe`, `Interpolation`, and `Pose`.
  - Samples translation, rotation, and scale tracks with step/linear interpolation.
- `crates/animation/src/player.rs`
  - Owns `AnimationPlayer`: play/pause, loop, seek, playback speed, and time ticking.
- `crates/renderer/src/mesh.rs`
  - `Vertex` now includes `joints: [u32; 4]` and `weights: [f32; 4]`.
  - Static meshes use default joints `[0, 0, 0, 0]` and weights `[1, 0, 0, 0]`.
- `crates/renderer/src/gltf_loader.rs`
  - Loads mesh data, optional skeleton, skinning attributes, animation clips, base color texture, and warnings.
  - Rejects invalid skinned data, including missing paired attributes, mixed static/skinned primitives, zero weights, and out-of-range joint indices.
  - Parses glTF animation channels for skeleton bones and rejects unsupported cubic spline interpolation.
- `crates/renderer/src/scene.rs`
  - Uses one render path for static and skinned meshes.
  - Adds a 64-bone skin palette bind group.
- `crates/renderer/src/shaders/mesh.wgsl`
  - Applies GPU skinning in the vertex shader when enabled.
- `crates/renderer/src/debug_lines.rs`
  - Draws x-ray skeleton debug lines.
  - Uses posed world transforms when Debug pose is enabled.
- `crates/ui/src/layout.rs`
  - Side panel now shows `Animation`, `Skinning`, `Debug pose`, `Skeleton`, and `Show skeleton`.
- `apps/avatar_desktop/src/app.rs`
  - Wires loaded skeleton/skinning/animation state, animated palettes, and UI toggles into the render loop.
- `assets/processed/avatars/bodies/phase4_rig.glb`
  - Generated fixture: blocky mannequin with `avatar_skeleton_v1`, `JOINTS_0`, `WEIGHTS_0`, inverse bind matrices, and a 2-second looping `idle` animation clip.
- `assets/processed/metadata/phase4_rig.json`
  - Catalog metadata for `Phase 4 Rig`.

## Verification Commands

Run from `avatar-studio/`:

```powershell
cargo fmt
cargo check
cargo test
cargo build --bin avatar_desktop
```

If the app is already running, Windows may lock `target/debug/avatar_desktop.exe`.
Stop it first:

```powershell
Stop-Process -Name avatar_desktop -Force -ErrorAction SilentlyContinue
cargo build --bin avatar_desktop
```

Launch:

```powershell
cargo run --bin avatar_desktop
```

## Manual UI Smoke Test

1. Open `Avatar Studio`.
2. Select `Phase 4 Rig`.
3. Confirm:
   - `Skinning` shows `Skinned mesh`.
   - `Animation` shows `Clip: idle`.
   - `Play` and `Loop` controls are enabled.
   - The scrub slider ranges from `0.0` to about `2.0`.
   - `Debug pose` checkbox is disabled while animation is active.
   - `Skeleton` shows `Skeleton: avatar_skeleton_v1`.
   - `Show skeleton` checkbox is enabled.
4. Pause playback and move the scrub slider.
   - The mannequin pose should change.
   - Cyan skeleton lines should follow the animated bones.
5. Toggle `Show skeleton`.
   - Cyan skeleton lines should hide/show.
6. Select `Rubber Duck`.
   - It should render normally.
   - `Skinning` should show `Static mesh`.
   - Animation, skeleton, and debug controls should disable or report no skeleton/animation.

## Known Caveats

- The mannequin is intentionally blocky and fixture-like.
- The Debug pose remains a fallback/testing path and is hidden from the primary workflow while an animation clip is active.
- There is no real avatar art pipeline yet.
- Multiple materials/submeshes are still collapsed into the current merged mesh path.
- The debug skeleton overlay uses thin cyan lines; it is readable but not polished.
- The app must be rebuilt after code changes before launching `target/debug/avatar_desktop.exe`, or a stale binary may appear.

## Next Phase

Phase 7: Avatar asset/customization pipeline.

Goal: start turning the engine fixture into an avatar-building workflow by loading compatible body/hair/outfit/accessory assets against `avatar_skeleton_v1`.

Expected work:

- Expand catalog metadata around body parts and compatibility.
- Load multiple avatar slots instead of one merged body mesh.
- Keep every loaded wearable aligned to the same skeleton.
- Start replacing the blocky fixture with better-authored avatar assets.
- Preserve static asset regression behavior for non-avatar GLBs.

Acceptance:

- A base body can load with at least one compatible wearable/slot asset.
- The asset metadata prevents incompatible selections.
- Animated/skinned rendering still works on the base rig.
- Static assets still render normally.
- `cargo fmt`, `cargo check`, and `cargo test` pass.

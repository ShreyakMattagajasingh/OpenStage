//! winit `ApplicationHandler` for Avatar Studio.
//!
//! Two render modes:
//!   * **Avatar mode** (a body asset is equipped) â€” multiple slots stack on
//!     the same skeleton; one shared `SkinningPalette` drives every
//!     wearable's GPU skinning.
//!   * **Static mode** (no body) â€” one non-rigged mesh + material, used for
//!     the placeholder cube and ad-hoc GLB loads via the file picker.
//!
//! Each redraw:
//!   1. Drain winit events into egui (already done in `on_window_event`).
//!   2. Run an egui frame and tessellate.
//!   3. Build the per-frame `Vec<SceneInstance>` (slot iteration in avatar
//!      mode, single-instance in static mode).
//!   4. `renderer.render(|fc| scene.draw + debug_lines + egui paint)`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use animation::{AnimationClip, AnimationPlayer, Pose, Skeleton, SkinningPalette};
use assets::{AssetMeta, Catalog};
use avatar::{new_character_id, Avatar, AvatarSave, CharacterStore, Expression, Slot};
use commands::{
    AvatarEquipAssetPayload, CommandEnvelope, CommandError, CommandPayload, CommandRouter,
    CommandRuntime, CommandSource, MaterialSetColorPayload, MaterialTarget, SelectionSetPayload,
    ValidationResult,
};
use egui_notify::{Anchor, Toast, ToastLevel, Toasts};
use engine_core::{resolve_paths, Config, FrameClock, FrameSample, FrameStats, ResolvedPaths};
use export::{
    png::{self, ExportView, PngExportOptions},
    video::{self, GifExportOptions},
};
use glam::{Mat4, Quat, Vec3};
use image::DynamicImage;
use renderer::camera::ORBIT_SENS;
use renderer::mesh::Vertex;
use renderer::{
    load_glb, Aabb, DebugLineRenderer, Light, LoadedGlb, Material, Mesh, OrbitCamera, Renderer,
    SceneInstance, SceneRenderer, Texture,
};
use scene::{SceneGraph, SceneId, SceneObject, SceneObjectKind, SceneSelection};
use serde_json::json;
use tracing::{debug, error, info, warn};
use ui::{
    asset_categories_for, draw_mode_bar, draw_side_panel, EditorCategory, EditorMode,
    EquippedSlotRow, SavedCharacterRow, SidePanelAction, SidePanelStatus,
};

use crate::face_textures;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowId},
};

#[derive(Default, Debug)]
struct MouseState {
    lmb: bool,
    rmb: bool,
    last_pos: Option<PhysicalPosition<f64>>,
}

impl MouseState {
    fn on_button(&mut self, button: MouseButton, state: ElementState) {
        let pressed = matches!(state, ElementState::Pressed);
        match button {
            MouseButton::Left => self.lmb = pressed,
            MouseButton::Right => self.rmb = pressed,
            _ => {}
        }
        if !pressed {
            // Stop dragging on release; next press will resample last_pos.
            self.last_pos = None;
        }
    }
}

const TARGET_HEIGHT_M: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    Interactive,
    AgentCapture,
    AgentPerf,
    AgentGif,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupOptions {
    pub mode: StartupMode,
    pub perf_frames: usize,
    pub show_perf: bool,
    pub vsync: bool,
    /// Freeze animation + use stable output filenames so QA captures are
    /// reproducible. Wired in Phase 16.
    pub deterministic: bool,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            mode: StartupMode::Interactive,
            perf_frames: 300,
            show_perf: false,
            vsync: true,
            deterministic: false,
        }
    }
}

/// Compute a model matrix that:
///  - centers the mesh on the XZ axes,
///  - places the bottom of the bbox at y = 0 (standing on ground),
///  - uniformly scales so the longest axis is â‰¤ `TARGET_HEIGHT_M`.
fn fit_to_avatar_height(bbox: Aabb) -> Mat4 {
    let longest = bbox.longest_axis().max(1e-6);
    let scale = TARGET_HEIGHT_M / longest;
    let center = bbox.center();
    let translate = glam::Vec3::new(-center.x, -bbox.min.y, -center.z);
    Mat4::from_scale(glam::Vec3::splat(scale)) * Mat4::from_translation(translate)
}

/// Write an sRGB tint into a Material's linear `base_color`.
fn apply_tint_to_material(material: &mut Material, srgb: [f32; 3]) {
    material.base_color = renderer::color::srgb_to_linear([srgb[0], srgb[1], srgb[2], 1.0]);
}

/// Default visible tint for each wearable slot when the asset metadata
/// doesn't override via `default_color`. Interpreted as sRGB (the user-facing
/// convention); converted to linear before reaching the GPU.
fn default_slot_tint(slot: Slot) -> [f32; 4] {
    match slot {
        Slot::Top => [0.15, 0.55, 0.85, 1.0],       // ocean blue
        Slot::Bottom => [0.25, 0.25, 0.30, 1.0],    // dark grey
        Slot::Shoes => [0.10, 0.10, 0.10, 1.0],     // near-black
        Slot::Hair => [0.18, 0.10, 0.05, 1.0],      // dark brown
        Slot::Hat => [0.85, 0.20, 0.20, 1.0],       // red
        Slot::Glasses => [0.10, 0.10, 0.10, 1.0],   // near-black
        Slot::Accessory => [0.85, 0.70, 0.10, 1.0], // gold
        Slot::Head | Slot::Body => [1.0, 1.0, 1.0, 1.0],
    }
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn slot_stable_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Body => "body",
        Slot::Head => "head",
        Slot::Hair => "hair",
        Slot::Top => "top",
        Slot::Bottom => "bottom",
        Slot::Shoes => "shoes",
        Slot::Hat => "hat",
        Slot::Glasses => "glasses",
        Slot::Accessory => "accessory",
    }
}

fn stable_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn slot_mesh_id(slot: Slot) -> String {
    format!("mesh_{}_001", slot_stable_name(slot))
}

fn slot_material_id(slot: Slot) -> String {
    format!("mat_{}_primary", slot_stable_name(slot))
}

fn bone_scene_id(name: &str) -> String {
    format!("bone_{}", stable_token(name))
}

fn clip_scene_id(name: &str, index: usize) -> String {
    let token = stable_token(name);
    if token.is_empty() {
        format!("clip_{index:03}")
    } else {
        format!("clip_{token}")
    }
}

/// One equipped slot's GPU resources. Lives in `App.equipped_slots`.
struct EquippedRuntime {
    /// Kept for future save/load (Phase 10) and debugging traces.
    #[allow(dead_code)]
    asset_id: String,
    display_name: String,
    mesh: Mesh,
    material: Material,
    is_skinned: bool,
    /// User-facing sRGB tint. Source of truth for the UI; mirrored to
    /// `material.base_color` (in linear space) on every change.
    tint_srgb: [f32; 3],
    /// True when the user is allowed to recolor this slot. Body is always
    /// recolorable regardless of metadata.
    supports_color: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationSnapshot {
    clip_name: Option<String>,
    time: f32,
    playing: bool,
    looping: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppSnapshot {
    Avatar {
        avatar: Avatar,
        character_id: Option<String>,
        character_name: String,
        animation: AnimationSnapshot,
        selection: SceneSelection,
        object_overrides: HashMap<String, ObjectRuntimeOverrides>,
    },
    Static {
        asset_id: Option<String>,
        label: String,
        selection: SceneSelection,
        object_overrides: HashMap<String, ObjectRuntimeOverrides>,
    },
}

fn elapsed_ms(start: Instant) -> f32 {
    start.elapsed().as_secs_f32() * 1000.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoseCacheKey {
    generation: u64,
    bone_count: usize,
    debug_pose: bool,
    selected_clip: Option<usize>,
    animation_time_bits: u32,
}

#[derive(Debug, Default, Clone)]
struct PoseCache {
    key: Option<PoseCacheKey>,
    world_transforms: Vec<Mat4>,
    palette: Option<SkinningPalette>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ObjectRuntimeOverrides {
    visible: Option<bool>,
    locked: Option<bool>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    scale: Option<[f32; 3]>,
}

impl ObjectRuntimeOverrides {
    fn is_empty(&self) -> bool {
        self.visible.is_none()
            && self.locked.is_none()
            && self.translation.is_none()
            && self.rotation.is_none()
            && self.scale.is_none()
    }

    fn apply_to_transform(&self, mut base: scene::SceneTransform) -> scene::SceneTransform {
        if let Some(translation) = self.translation {
            base.translation = translation;
        }
        if let Some(rotation) = self.rotation {
            base.rotation = rotation;
        }
        if let Some(scale) = self.scale {
            base.scale = scale;
        }
        base
    }
}

pub struct App {
    startup_mode: StartupMode,
    perf_target_frames: usize,
    vsync: bool,
    deterministic: bool,
    config: Config,
    paths: ResolvedPaths,
    clock: FrameClock,
    current_mode: EditorMode,
    selected_category: EditorCategory,

    // None until the first `resumed` event.
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    scene: Option<SceneRenderer>,
    debug_lines: Option<DebugLineRenderer>,

    // --- Avatar mode --------------------------------------------------------
    equipped_slots: HashMap<Slot, EquippedRuntime>,
    skeleton: Option<Skeleton>,
    animation_clips: Vec<AnimationClip>,
    animation_player: Option<AnimationPlayer>,
    /// Per-expression face textures (CPU side, re-uploaded on pick).
    face_images: HashMap<Expression, DynamicImage>,
    /// Procedural face quad. Present iff a body with a `head` bone is equipped.
    face_runtime: Option<EquippedRuntime>,

    // --- Static mode --------------------------------------------------------
    static_mesh: Option<Mesh>,
    static_material: Option<Material>,
    static_label: String,
    static_asset_id: Option<String>,

    /// Cached model matrix for whichever mode is active. Recomputed when the
    /// body (or static mesh) is loaded; reused every frame for every slot.
    fit_matrix: Mat4,

    /// Logical avatar state â€” `body_type` + slotâ†’asset id map.
    avatar: Avatar,
    character_store: CharacterStore,
    current_character_id: Option<String>,
    current_character_name: String,
    export_options: PngExportOptions,
    gif_export_options: GifExportOptions,

    /// SQLite-backed asset catalog. None if it failed to open (logged at WARN);
    /// the app still runs with custom-GLB loading.
    catalog: Option<Catalog>,
    current_assets: Vec<AssetMeta>,
    last_queried_category: Option<EditorCategory>,

    camera: OrbitCamera,
    light: Light,
    mouse: MouseState,
    modifiers: ModifiersState,
    panel_status: SidePanelStatus,
    toasts: Toasts,
    perf_stats: FrameStats,
    equipped_rows_dirty: bool,
    pose_cache: PoseCache,
    pose_generation: u64,
    command_router: Option<CommandRouter<AppSnapshot>>,
    scene_graph: SceneGraph,
    scene_selection: SceneSelection,
    object_overrides: HashMap<String, ObjectRuntimeOverrides>,
    inspector_filter: String,
    tool_state: ui::ToolState,
    command_counter: u64,

    // egui-wgpu pieces. Lazily constructed alongside the renderer because
    // egui_wgpu::Renderer needs the wgpu Device + surface format.
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
}

impl App {
    pub fn new(options: StartupOptions) -> anyhow::Result<Self> {
        let paths = resolve_paths();
        info!(
            mode = ?paths.mode,
            assets = %paths.assets.display(),
            user_data = %paths.user_data.display(),
            "resolved runtime paths"
        );
        let cfg_path = paths.settings();
        let config = Config::load_or_default(&cfg_path)
            .map_err(|e| anyhow::anyhow!("loading {}: {e}", cfg_path.display()))?;
        info!(
            title = %config.window.title,
            width = config.window.width,
            height = config.window.height,
            "config loaded"
        );
        let initial_aspect =
            (config.window.width.max(1) as f32) / (config.window.height.max(1) as f32);
        let egui_ctx = egui::Context::default();
        ui::fonts::install(&egui_ctx);
        ui::theme::apply(&egui_ctx);
        egui_extras::install_image_loaders(&egui_ctx);

        let character_store = CharacterStore::new(paths.characters_dir());
        let mut clock = FrameClock::new();
        if options.deterministic {
            clock.pin_dt(std::time::Duration::from_secs_f32(1.0 / 60.0));
        }
        let current_mode = config.editor.last_mode;
        Ok(Self {
            startup_mode: options.mode,
            perf_target_frames: options.perf_frames,
            vsync: options.vsync,
            deterministic: options.deterministic,
            config,
            paths,
            clock,
            current_mode,
            selected_category: EditorCategory::Character,
            window: None,
            renderer: None,
            scene: None,
            debug_lines: None,
            equipped_slots: HashMap::new(),
            skeleton: None,
            animation_clips: Vec::new(),
            animation_player: None,
            face_images: HashMap::new(),
            face_runtime: None,
            static_mesh: None,
            static_material: None,
            static_label: String::new(),
            static_asset_id: None,
            fit_matrix: Mat4::IDENTITY,
            avatar: Avatar::default(),
            character_store,
            current_character_id: None,
            current_character_name: "Avatar".to_string(),
            export_options: PngExportOptions::default(),
            gif_export_options: GifExportOptions::default(),
            catalog: None,
            current_assets: Vec::new(),
            last_queried_category: None,
            camera: OrbitCamera::new(initial_aspect),
            light: Light::default(),
            mouse: MouseState::default(),
            modifiers: ModifiersState::empty(),
            panel_status: SidePanelStatus {
                export_size: 1024,
                export_transparent: true,
                show_diagnostics: options.show_perf,
                ..SidePanelStatus::default()
            },
            toasts: Toasts::new().with_anchor(Anchor::BottomRight),
            perf_stats: FrameStats::default(),
            equipped_rows_dirty: true,
            pose_cache: PoseCache::default(),
            pose_generation: 0,
            command_router: Some(CommandRouter::default()),
            scene_graph: SceneGraph::new(),
            scene_selection: SceneSelection::default(),
            object_overrides: HashMap::new(),
            inspector_filter: String::new(),
            tool_state: ui::ToolState::default(),
            command_counter: 0,
            egui_ctx,
            egui_state: None,
            egui_renderer: None,
        })
    }

    /// Pick a window size + position that fits the user's monitor.
    fn choose_window_geometry(
        &self,
        event_loop: &ActiveEventLoop,
    ) -> (
        winit::dpi::LogicalSize<f64>,
        Option<winit::dpi::PhysicalPosition<i32>>,
    ) {
        let want_w = self.config.window.width as f64;
        let want_h = self.config.window.height as f64;

        let Some(monitor) = event_loop.primary_monitor() else {
            return (winit::dpi::LogicalSize::new(want_w, want_h), None);
        };

        let mon_phys = monitor.size();
        let scale = monitor.scale_factor();
        let mon_logical_w = mon_phys.width as f64 / scale;
        let mon_logical_h = mon_phys.height as f64 / scale;

        let max_w = mon_logical_w * 0.90;
        let max_h = mon_logical_h * 0.85;

        let final_w = want_w.min(max_w);
        let final_h = want_h.min(max_h);
        let size = winit::dpi::LogicalSize::new(final_w, final_h);

        let monitor_pos = monitor.position();
        let win_phys_w = (final_w * scale).round() as i32;
        let win_phys_h = (final_h * scale).round() as i32;
        let pos = winit::dpi::PhysicalPosition::new(
            monitor_pos.x + (mon_phys.width as i32 - win_phys_w) / 2,
            monitor_pos.y
                + ((mon_phys.height as i32 - win_phys_h) / 2 - (30.0 * scale) as i32).max(0),
        );

        (size, Some(pos))
    }

    fn init_window_and_gpu(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let (inner_size, position) = self.choose_window_geometry(event_loop);

        let mut attrs = Window::default_attributes()
            .with_title(&self.config.window.title)
            .with_inner_size(inner_size);
        if let Some(pos) = position {
            attrs = attrs.with_position(pos);
        }
        let window = Arc::new(event_loop.create_window(attrs)?);

        let renderer = Renderer::new(window.clone(), self.config.render.clear_color, self.vsync)?;
        let surface_format = renderer.surface_format();

        let scene = SceneRenderer::new(&renderer.gpu.device, surface_format);
        let debug_lines = DebugLineRenderer::new(&renderer.gpu.device, surface_format);

        // Boot in static mode with the placeholder cube.
        let cube = Mesh::cube(&renderer.gpu.device, 1.0);
        let cube_material = scene.make_material(
            &renderer.gpu.device,
            &renderer.gpu.queue,
            None,
            [0.42, 0.55, 0.85, 1.0], // cornflower tint Ã— white texture
            "cube",
        );
        self.fit_matrix = fit_to_avatar_height(cube.bbox);
        self.static_mesh = Some(cube);
        self.static_material = Some(cube_material);
        self.static_label = "cube (placeholder)".into();
        self.panel_status.current_mesh_label = self.static_label.clone();
        self.panel_status.skeleton_label = "No skeleton".into();
        self.panel_status.avatar_mode = false;

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&renderer.gpu.device, surface_format, None, 1, false);

        self.camera.set_aspect(renderer.size());

        info!("window + GPU + scene + egui initialized");
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.scene = Some(scene);
        self.debug_lines = Some(debug_lines);
        self.egui_state = Some(egui_state);
        self.egui_renderer = Some(egui_renderer);

        // --- Asset catalog --------------------------------------------------
        let db_path = self.paths.catalog_db();
        match Catalog::open(&db_path) {
            Ok(mut catalog) => {
                let assets_root = self.paths.assets_processed();
                let scanned = assets::scan_metadata_dir(&assets_root);
                match catalog.upsert_many(&scanned) {
                    Ok(ingested) => info!(
                        scanned = scanned.len(),
                        ingested,
                        path = %assets_root.display(),
                        "asset catalog refreshed"
                    ),
                    Err(e) => warn!(error = %e, "catalog upsert failed"),
                }
                self.catalog = Some(catalog);
            }
            Err(e) => {
                warn!(path = %db_path.display(), error = %e, "asset catalog unavailable");
            }
        }

        self.refresh_current_assets();
        self.refresh_gallery_rows();
        self.sync_scene_graph();
        Ok(())
    }

    fn ensure_assets_fresh(&mut self) {
        if self.last_queried_category != Some(self.selected_category) {
            self.refresh_current_assets();
        }
    }

    fn refresh_current_assets(&mut self) {
        let cats = asset_categories_for(self.selected_category);
        let mut out = Vec::new();
        if let Some(catalog) = self.catalog.as_ref() {
            for &c in cats {
                match catalog.by_category(c) {
                    Ok(mut rows) => out.append(&mut rows),
                    Err(e) => warn!(category = ?c, error = %e, "catalog query failed"),
                }
            }
        }
        self.current_assets = out;
        self.last_queried_category = Some(self.selected_category);
    }

    fn refresh_gallery_rows(&mut self) {
        let characters_dir = self.paths.characters_dir();
        match self.character_store.list() {
            Ok(rows) => {
                self.panel_status.gallery_rows = rows
                    .into_iter()
                    .map(|row| {
                        let thumb_path = characters_dir.join(format!("{}.png", row.id));
                        let thumb_uri = if thumb_path.exists() {
                            thumb_path
                                .canonicalize()
                                .ok()
                                .and_then(|p| p.to_str().map(|s| format!("file://{}", s)))
                        } else {
                            None
                        };
                        SavedCharacterRow {
                            id: row.id,
                            name: row.name,
                            updated_at: row.updated_at,
                            thumb_uri,
                        }
                    })
                    .collect();
            }
            Err(e) => {
                warn!(error = %e, "character gallery refresh failed");
                self.panel_status.last_error = Some(format!("Gallery refresh failed: {e}"));
            }
        }
    }

    fn toast_success(&mut self, text: impl Into<String>, secs: u64) {
        let toast = Toast::custom(
            text.into(),
            ToastLevel::Custom("✓".into(), ui::theme::TOKENS.success),
        );
        self.toasts
            .add(toast)
            .duration(Some(Duration::from_secs(secs)));
    }

    fn toast_info(&mut self, text: impl Into<String>, secs: u64) {
        let toast = Toast::custom(
            text.into(),
            ToastLevel::Custom("ℹ".into(), ui::theme::TOKENS.accent),
        );
        self.toasts
            .add(toast)
            .duration(Some(Duration::from_secs(secs)));
    }

    fn toast_error(&mut self, text: impl Into<String>, secs: u64) {
        let toast = Toast::custom(
            text.into(),
            ToastLevel::Custom("✕".into(), ui::theme::TOKENS.error),
        );
        self.toasts
            .add(toast)
            .duration(Some(Duration::from_secs(secs)));
    }

    fn invalidate_pose_cache(&mut self) {
        self.pose_generation = self.pose_generation.wrapping_add(1);
        self.pose_cache = PoseCache::default();
    }

    fn current_snapshot(&self) -> AppSnapshot {
        if self.panel_status.avatar_mode {
            AppSnapshot::Avatar {
                avatar: self.avatar.clone(),
                character_id: self.current_character_id.clone(),
                character_name: self.current_character_name.clone(),
                animation: AnimationSnapshot {
                    clip_name: self.current_animation_name(),
                    time: self
                        .animation_player
                        .as_ref()
                        .map(|p| p.time)
                        .unwrap_or(0.0),
                    playing: self
                        .animation_player
                        .as_ref()
                        .map(|p| p.playing)
                        .unwrap_or(false),
                    looping: self
                        .animation_player
                        .as_ref()
                        .map(|p| p.looping)
                        .unwrap_or(false),
                },
                selection: self.scene_selection.clone(),
                object_overrides: self.object_overrides.clone(),
            }
        } else {
            AppSnapshot::Static {
                asset_id: self.static_asset_id.clone(),
                label: self.static_label.clone(),
                selection: self.scene_selection.clone(),
                object_overrides: self.object_overrides.clone(),
            }
        }
    }

    fn refresh_history_status(&mut self) {
        let Some(router) = self.command_router.as_ref() else {
            self.panel_status.can_undo = false;
            self.panel_status.can_redo = false;
            return;
        };
        self.panel_status.can_undo = router.can_undo();
        self.panel_status.can_redo = router.can_redo();
    }

    fn restore_snapshot(&mut self, snapshot: AppSnapshot) {
        match snapshot {
            AppSnapshot::Avatar {
                avatar,
                character_id,
                character_name,
                animation,
                selection,
                object_overrides,
            } => {
                if let Some(body) = avatar.equipped(Slot::Body).map(str::to_owned) {
                    self.load_asset_by_id(&body);
                    for (slot, asset_id) in avatar.iter() {
                        if slot != Slot::Body {
                            self.load_asset_by_id(asset_id);
                        }
                    }
                    for (slot, srgb) in &avatar.slot_colors {
                        self.avatar.set_slot_color(*slot, *srgb);
                        if let Some(rt) = self.equipped_slots.get_mut(slot) {
                            rt.tint_srgb = *srgb;
                            apply_tint_to_material(&mut rt.material, *srgb);
                        }
                    }
                    self.avatar.expression = avatar.expression;
                    self.panel_status.current_expression = avatar.expression;
                    self.apply_face_expression(avatar.expression);
                    if let Some(player) = self.animation_player.as_mut() {
                        if let Some(name) = animation.clip_name.as_deref() {
                            if let Some(idx) = self
                                .animation_clips
                                .iter()
                                .position(|clip| clip.name == name)
                            {
                                player.selected_clip = idx;
                            }
                        }
                        player.time = animation.time;
                        player.playing = animation.playing;
                        player.looping = animation.looping;
                    }
                    self.current_character_id = character_id;
                    self.current_character_name = character_name;
                    self.equipped_rows_dirty = true;
                    self.invalidate_pose_cache();
                    self.panel_status.last_error = None;
                } else {
                    self.reset_to_cube();
                }
                self.object_overrides = object_overrides;
                self.apply_scene_selection(selection);
            }
            AppSnapshot::Static {
                asset_id,
                label,
                selection,
                object_overrides,
            } => {
                if let Some(id) = asset_id {
                    self.load_asset_by_id(&id);
                } else {
                    self.reset_to_cube();
                    self.static_label = label;
                    self.panel_status.current_mesh_label = self.static_label.clone();
                }
                self.object_overrides = object_overrides;
                self.apply_scene_selection(selection);
            }
        }
        self.refresh_history_status();
        self.sync_scene_graph();
    }

    fn next_command_id(&mut self) -> String {
        self.command_counter = self.command_counter.wrapping_add(1);
        format!("cmd_{}_{}", timestamp_millis(), self.command_counter)
    }

    fn command(&mut self, source: CommandSource, payload: CommandPayload) -> CommandEnvelope {
        CommandEnvelope::new(
            self.next_command_id(),
            timestamp_millis().to_string(),
            source,
            payload,
        )
    }

    fn execute_command(&mut self, command: CommandEnvelope) -> Result<(), CommandError> {
        let mut router = self.command_router.take().unwrap_or_default();
        let result = router.execute(self, command);
        self.command_router = Some(router);
        self.refresh_history_status();
        match result {
            Ok(result) => {
                self.sync_scene_graph();
                match result.command_name {
                    commands::CommandName::HistoryUndo => self.toast_info("Undid last edit", 2),
                    commands::CommandName::HistoryRedo => self.toast_info("Redid edit", 2),
                    _ => {}
                }
                self.panel_status.last_error = None;
                Ok(())
            }
            Err(err) => {
                self.panel_status.last_error = Some(format!("Command failed: {err}"));
                Err(err)
            }
        }
    }

    fn run_legacy_undoable(&mut self, label: impl Into<String>, mutation: impl FnOnce(&mut Self)) {
        let before = self.current_snapshot();
        mutation(self);
        self.sync_scene_graph();
        let after = self.current_snapshot();
        if before == after {
            self.refresh_history_status();
            return;
        }
        let command = self.command(
            CommandSource::Ui,
            CommandPayload::LegacyUndoable {
                label: label.into(),
            },
        );
        let Some(router) = self.command_router.as_mut() else {
            return;
        };
        if let Err(err) = router.record_external(command, before, after) {
            self.panel_status.last_error = Some(format!("History failed: {err}"));
        }
        self.refresh_history_status();
    }

    fn apply_scene_selection(&mut self, selection: SceneSelection) {
        self.scene_selection = selection;
        self.scene_selection
            .selected_objects
            .retain(|id| self.scene_graph.contains(id));
        self.scene_selection
            .selected_bones
            .retain(|id| self.scene_graph.contains(id));
        if self
            .scene_selection
            .active_object
            .as_ref()
            .is_some_and(|id| !self.scene_graph.contains(id))
        {
            self.scene_selection.active_object =
                self.scene_selection.selected_objects.first().cloned();
        }
        if self
            .scene_selection
            .active_bone
            .as_ref()
            .is_some_and(|id| !self.scene_graph.contains(id))
        {
            self.scene_selection.active_bone = self.scene_selection.selected_bones.first().cloned();
        }
        if self
            .scene_selection
            .active_clip
            .as_ref()
            .is_some_and(|id| !self.scene_graph.contains(id))
        {
            self.scene_selection.active_clip = None;
        }
        self.scene_graph.selection = self.scene_selection.clone();
    }

    fn scene_transform_to_mat4(transform: scene::SceneTransform) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from(transform.scale),
            Quat::from_array(transform.rotation).normalize(),
            Vec3::from(transform.translation),
        )
    }

    fn effective_scene_transform(
        &self,
        object_id: &str,
        base: scene::SceneTransform,
    ) -> scene::SceneTransform {
        self.object_overrides
            .get(object_id)
            .map(|overrides| overrides.apply_to_transform(base))
            .unwrap_or(base)
    }

    fn effective_visibility(&self, object_id: &str, default_visible: bool) -> bool {
        self.object_overrides
            .get(object_id)
            .and_then(|overrides| overrides.visible)
            .unwrap_or(default_visible)
    }

    fn effective_locked(&self, object_id: &str, default_locked: bool) -> bool {
        self.object_overrides
            .get(object_id)
            .and_then(|overrides| overrides.locked)
            .unwrap_or(default_locked)
    }

    fn set_object_translation_override(&mut self, object_id: &str, translation: [f32; 3]) {
        self.object_overrides
            .entry(object_id.to_string())
            .or_default()
            .translation = Some(translation);
    }

    fn set_object_rotation_override(&mut self, object_id: &str, rotation: [f32; 4]) {
        self.object_overrides
            .entry(object_id.to_string())
            .or_default()
            .rotation = Some(rotation);
    }

    fn set_object_scale_override(&mut self, object_id: &str, scale: [f32; 3]) {
        self.object_overrides
            .entry(object_id.to_string())
            .or_default()
            .scale = Some(scale);
    }

    fn set_object_visible_override(&mut self, object_id: &str, visible: bool) {
        self.object_overrides
            .entry(object_id.to_string())
            .or_default()
            .visible = Some(visible);
        self.prune_object_override(object_id);
    }

    fn set_object_locked_override(&mut self, object_id: &str, locked: bool) {
        self.object_overrides
            .entry(object_id.to_string())
            .or_default()
            .locked = Some(locked);
        self.prune_object_override(object_id);
    }

    fn clear_object_transform_override(&mut self, object_id: &str) {
        if let Some(overrides) = self.object_overrides.get_mut(object_id) {
            overrides.translation = None;
            overrides.rotation = None;
            overrides.scale = None;
        }
        self.prune_object_override(object_id);
    }

    fn prune_object_override(&mut self, object_id: &str) {
        if self
            .object_overrides
            .get(object_id)
            .is_some_and(ObjectRuntimeOverrides::is_empty)
        {
            self.object_overrides.remove(object_id);
        }
    }

    fn slot_from_mesh_id(object_id: &str) -> Option<Slot> {
        match object_id {
            "mesh_body_001" => Some(Slot::Body),
            "mesh_head_001" => Some(Slot::Head),
            "mesh_hair_001" => Some(Slot::Hair),
            "mesh_top_001" => Some(Slot::Top),
            "mesh_bottom_001" => Some(Slot::Bottom),
            "mesh_shoes_001" => Some(Slot::Shoes),
            "mesh_hat_001" => Some(Slot::Hat),
            "mesh_glasses_001" => Some(Slot::Glasses),
            "mesh_accessory_001" => Some(Slot::Accessory),
            _ => None,
        }
    }

    fn bone_index_from_object_id(&self, object_id: &str) -> Option<usize> {
        let skeleton = self.skeleton.as_ref()?;
        object_id
            .strip_prefix("bone_")
            .and_then(|name| skeleton.bone_index(name))
            .map(|index| index.0)
    }

    fn base_bone_local_transform(
        &self,
        skeleton: &Skeleton,
        bone_index: usize,
    ) -> scene::SceneTransform {
        let bone = &skeleton.bones[bone_index];
        let (bind_scale, bind_rotation, bind_translation) =
            bone.local_bind_transform.to_scale_rotation_translation();
        if let Some(pose) = self
            .animation_player
            .as_ref()
            .and_then(|player| player.sample_pose(&self.animation_clips, skeleton.bones.len()))
        {
            scene::SceneTransform {
                translation: pose.translations[bone_index]
                    .unwrap_or(bind_translation)
                    .to_array(),
                rotation: pose.rotations[bone_index]
                    .unwrap_or(bind_rotation)
                    .normalize()
                    .to_array(),
                scale: pose.scales[bone_index].unwrap_or(bind_scale).to_array(),
            }
        } else {
            scene::SceneTransform {
                translation: bind_translation.to_array(),
                rotation: bind_rotation.normalize().to_array(),
                scale: bind_scale.to_array(),
            }
        }
    }

    fn build_bone_world_transforms(
        &self,
        skeleton: &Skeleton,
        pose: Option<&Pose>,
        debug_pose: bool,
    ) -> Vec<Mat4> {
        if pose.is_none()
            && debug_pose
            && !skeleton.bones.iter().any(|bone| {
                self.object_overrides
                    .contains_key(&bone_scene_id(&bone.name))
            })
        {
            return skeleton.posed_world_transforms(true);
        }

        let mut out = vec![Mat4::IDENTITY; skeleton.bones.len()];
        for (idx, bone) in skeleton.bones.iter().enumerate() {
            let (bind_scale, bind_rotation, bind_translation) =
                bone.local_bind_transform.to_scale_rotation_translation();
            let mut local = scene::SceneTransform {
                translation: bind_translation.to_array(),
                rotation: bind_rotation.normalize().to_array(),
                scale: bind_scale.to_array(),
            };
            if let Some(pose) = pose {
                local.translation = pose.translations[idx]
                    .unwrap_or(bind_translation)
                    .to_array();
                local.rotation = pose.rotations[idx]
                    .unwrap_or(bind_rotation)
                    .normalize()
                    .to_array();
                local.scale = pose.scales[idx].unwrap_or(bind_scale).to_array();
            }
            let local = self.effective_scene_transform(&bone_scene_id(&bone.name), local);
            let local = Self::scene_transform_to_mat4(local);
            out[idx] = match bone.parent {
                Some(parent) => out[parent.0] * local,
                None => local,
            };
        }
        out
    }

    fn selected_object_world_transform(&self, posed_world_transforms: &[Mat4]) -> Option<Mat4> {
        let active = self.scene_selection.active_object.as_ref()?.as_str();
        if active == "avatar_001" {
            return Some(Self::scene_transform_to_mat4(
                self.effective_scene_transform("avatar_001", scene::SceneTransform::default()),
            ));
        }
        if active == "mesh_static_001" {
            let local =
                self.effective_scene_transform("mesh_static_001", scene::SceneTransform::default());
            return Some(Self::scene_transform_to_mat4(local) * self.fit_matrix);
        }
        if let Some(slot) = Self::slot_from_mesh_id(active) {
            let avatar_world = Self::scene_transform_to_mat4(
                self.effective_scene_transform("avatar_001", scene::SceneTransform::default()),
            );
            let local = Self::scene_transform_to_mat4(
                self.effective_scene_transform(active, scene::SceneTransform::default()),
            );
            if self.equipped_slots.contains_key(&slot) {
                return Some(avatar_world * local * self.fit_matrix);
            }
        }
        if let Some(index) = self.bone_index_from_object_id(active) {
            let avatar_world = Self::scene_transform_to_mat4(
                self.effective_scene_transform("avatar_001", scene::SceneTransform::default()),
            );
            let bone_world = *posed_world_transforms.get(index)?;
            return Some(avatar_world * bone_world);
        }
        self.scene_graph
            .get_object(self.scene_selection.active_object.as_ref()?)
            .map(|obj| Self::scene_transform_to_mat4(obj.transform))
    }

    fn sync_scene_graph(&mut self) {
        let prior_selection = self.scene_selection.clone();
        let mut graph = SceneGraph::new();
        if let Err(err) = self.populate_scene_graph(&mut graph) {
            warn!(error = %err, "scene graph sync failed");
            return;
        }
        self.scene_graph = graph;
        self.apply_scene_selection(prior_selection);
    }

    fn populate_scene_graph(&self, graph: &mut SceneGraph) -> Result<(), scene::SceneError> {
        let eye = self.camera.eye();
        let mut camera = SceneObject::new("camera_main", "Main Camera", SceneObjectKind::Camera)?
            .with_metadata("eye", json!(eye.to_array()))
            .with_metadata("target", json!(self.camera.target.to_array()))
            .with_metadata("yaw", json!(self.camera.yaw))
            .with_metadata("pitch", json!(self.camera.pitch))
            .with_metadata("distance", json!(self.camera.distance));
        camera.visible = self.effective_visibility("camera_main", camera.visible);
        camera.locked = self.effective_locked("camera_main", camera.locked);
        graph.insert(camera)?;
        let mut light = SceneObject::new("light_key", "Key Light", SceneObjectKind::Light)?
            .with_metadata("direction", json!(self.light.direction.to_array()))
            .with_metadata("color", json!(self.light.color.to_array()))
            .with_metadata("intensity", json!(self.light.intensity))
            .with_metadata("ambient", json!(self.light.ambient));
        light.visible = self.effective_visibility("light_key", light.visible);
        light.locked = self.effective_locked("light_key", light.locked);
        graph.insert(light)?;

        if self.panel_status.avatar_mode {
            let mut avatar = SceneObject::new(
                "avatar_001",
                &self.current_character_name,
                SceneObjectKind::Avatar,
            )?
            .with_metadata("body_type", json!(self.avatar.body_type))
            .with_metadata("skin_tone", json!(self.avatar.skin_tone))
            .with_metadata("character_id", json!(self.current_character_id));
            avatar.transform = self.effective_scene_transform("avatar_001", avatar.transform);
            avatar.visible = self.effective_visibility("avatar_001", avatar.visible);
            avatar.locked = self.effective_locked("avatar_001", avatar.locked);
            graph.insert(avatar)?;
            for &slot in Slot::ALL.iter() {
                if let Some(rt) = self.equipped_slots.get(&slot) {
                    self.insert_slot_scene_objects(graph, slot, rt)?;
                }
            }
            if let Some(face) = self.face_runtime.as_ref() {
                let mut face = SceneObject::new(
                    "blendshape_face_001",
                    "Face Expression",
                    SceneObjectKind::BlendshapeSet,
                )?
                .with_parent("avatar_001")?
                .with_asset_id(face.asset_id.clone())
                .with_metadata("expression", json!(self.avatar.expression.as_save_str()));
                face.transform =
                    self.effective_scene_transform("blendshape_face_001", face.transform);
                face.visible = self.effective_visibility("blendshape_face_001", face.visible);
                face.locked = self.effective_locked("blendshape_face_001", face.locked);
                graph.insert(face)?;
            }
            if let Some(skeleton) = self.skeleton.as_ref() {
                self.insert_skeleton_scene_objects(graph, skeleton)?;
            }
            for (idx, clip) in self.animation_clips.iter().enumerate() {
                let clip_id = clip_scene_id(&clip.name, idx);
                let mut clip_object =
                    SceneObject::new(clip_id.as_str(), &clip.name, SceneObjectKind::AnimationClip)?
                        .with_parent("avatar_001")?
                        .with_metadata("duration", json!(clip.duration))
                        .with_metadata("channel_count", json!(clip.channels.len()));
                clip_object.visible = self.effective_visibility(&clip_id, clip_object.visible);
                clip_object.locked = self.effective_locked(&clip_id, clip_object.locked);
                graph.insert(clip_object)?;
            }
        } else if self.static_mesh.is_some() {
            let mut object = SceneObject::new(
                "mesh_static_001",
                self.static_label.as_str(),
                SceneObjectKind::MeshInstance,
            )?;
            object.asset_id = self.static_asset_id.clone();
            object.transform = self.effective_scene_transform("mesh_static_001", object.transform);
            object.visible = self.effective_visibility("mesh_static_001", object.visible);
            object.locked = self.effective_locked("mesh_static_001", object.locked);
            graph.insert(object)?;
        }

        Ok(())
    }

    fn insert_slot_scene_objects(
        &self,
        graph: &mut SceneGraph,
        slot: Slot,
        rt: &EquippedRuntime,
    ) -> Result<(), scene::SceneError> {
        let mesh_id = slot_mesh_id(slot);
        let material_id = slot_material_id(slot);
        let kind = if rt.is_skinned {
            SceneObjectKind::SkinnedMeshInstance
        } else {
            SceneObjectKind::MeshInstance
        };
        let mut mesh = SceneObject::new(mesh_id.as_str(), &rt.display_name, kind)?
            .with_parent("avatar_001")?
            .with_asset_id(rt.asset_id.clone())
            .with_metadata("slot", json!(slot_stable_name(slot)))
            .with_metadata("is_skinned", json!(rt.is_skinned))
            .with_metadata("index_count", json!(rt.mesh.index_count));
        mesh.transform = self.effective_scene_transform(&mesh_id, mesh.transform);
        mesh.visible = self.effective_visibility(&mesh_id, mesh.visible);
        mesh.locked = self.effective_locked(&mesh_id, mesh.locked);
        graph.insert(mesh)?;
        let mut material = SceneObject::new(
            material_id.as_str(),
            format!("{} Material", slot.label()),
            SceneObjectKind::Material,
        )?
        .with_parent(mesh_id.as_str())?
        .with_metadata("slot", json!(slot_stable_name(slot)))
        .with_metadata("supports_color", json!(rt.supports_color))
        .with_metadata("color_srgb", json!(rt.tint_srgb));
        material.visible = self.effective_visibility(&material_id, material.visible);
        material.locked = self.effective_locked(&material_id, material.locked);
        graph.insert(material)?;
        Ok(())
    }

    fn insert_skeleton_scene_objects(
        &self,
        graph: &mut SceneGraph,
        skeleton: &Skeleton,
    ) -> Result<(), scene::SceneError> {
        let sampled_pose = self
            .animation_player
            .as_ref()
            .and_then(|player| player.sample_pose(&self.animation_clips, skeleton.bones.len()));
        let mut skeleton_object = SceneObject::new(
            "skeleton_avatar_001",
            &skeleton.name,
            SceneObjectKind::Skeleton,
        )?
        .with_parent("avatar_001")?
        .with_metadata("bone_count", json!(skeleton.bones.len()))
        .with_metadata("warnings", json!(skeleton.warnings));
        skeleton_object.transform =
            self.effective_scene_transform("skeleton_avatar_001", skeleton_object.transform);
        skeleton_object.visible =
            self.effective_visibility("skeleton_avatar_001", skeleton_object.visible);
        skeleton_object.locked =
            self.effective_locked("skeleton_avatar_001", skeleton_object.locked);
        graph.insert(skeleton_object)?;
        for (index, bone) in skeleton.bones.iter().enumerate() {
            let id = bone_scene_id(&bone.name);
            let parent = bone
                .parent
                .and_then(|parent| skeleton.bones.get(parent.0))
                .map(|parent| bone_scene_id(&parent.name))
                .unwrap_or_else(|| "skeleton_avatar_001".to_string());
            let base_transform = sampled_pose
                .as_ref()
                .map(|_| self.base_bone_local_transform(skeleton, index))
                .unwrap_or_else(|| self.base_bone_local_transform(skeleton, index));
            let mut bone_object = SceneObject::new(id.as_str(), &bone.name, SceneObjectKind::Bone)?
                .with_parent(parent.as_str())?
                .with_metadata("bone_name", json!(bone.name))
                .with_metadata("bone_index", json!(index))
                .with_metadata(
                    "local_bind_transform",
                    json!(bone.local_bind_transform.to_cols_array()),
                )
                .with_metadata(
                    "world_bind_transform",
                    json!(bone.world_bind_transform.to_cols_array()),
                )
                .with_metadata(
                    "inverse_bind_matrix",
                    json!(bone.inverse_bind_matrix.to_cols_array()),
                );
            bone_object.transform = self.effective_scene_transform(&id, base_transform);
            bone_object.visible = self.effective_visibility(&id, bone_object.visible);
            bone_object.locked = self.effective_locked(&id, bone_object.locked);
            graph.insert(bone_object)?;
        }
        Ok(())
    }

    pub fn scene_summary_json(&mut self) -> serde_json::Result<String> {
        self.sync_scene_graph();
        serde_json::to_string_pretty(&self.scene_graph.get_scene_summary())
    }

    pub fn scene_graph_json(&mut self) -> serde_json::Result<String> {
        self.sync_scene_graph();
        self.scene_graph.to_json_pretty()
    }

    /// Stage 20 — JSON dump of the current `SceneSelection`, written
    /// alongside `latest_scene_graph.json` during agent capture so AI
    /// agents see which object(s) the deterministic run had selected.
    pub fn scene_selection_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.scene_selection)
    }

    pub fn editor_mode_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&json!({
            "current_mode": self.current_mode,
            "available_modes": ui::EditorMode::ALL,
        }))
    }

    fn command_for_asset(&mut self, id: String) -> Result<CommandEnvelope, String> {
        let meta = self
            .catalog
            .as_ref()
            .and_then(|c| c.find(&id).ok().flatten())
            .ok_or_else(|| format!("Asset '{id}' not in catalog"))?;
        let slot = Slot::from_asset_category(meta.category)
            .ok_or_else(|| format!("Asset category isn't equippable: {id}"))?;
        Ok(self.command(
            CommandSource::Ui,
            CommandPayload::AvatarEquipAsset(AvatarEquipAssetPayload {
                avatar_id: "current".to_string(),
                slot,
                asset_id: id,
            }),
        ))
    }

    fn command_for_slot_color(&mut self, slot: Slot, srgb: [f32; 3]) -> CommandEnvelope {
        self.command(
            CommandSource::Ui,
            CommandPayload::MaterialSetColor(MaterialSetColorPayload {
                target: MaterialTarget::AvatarSlot {
                    avatar_id: "current".to_string(),
                    slot,
                },
                color_srgb: srgb,
            }),
        )
    }

    fn apply_slot_color(&mut self, slot: Slot, srgb: [f32; 3]) {
        self.avatar.set_slot_color(slot, srgb);
        if let Some(rt) = self.equipped_slots.get_mut(&slot) {
            rt.tint_srgb = srgb;
            apply_tint_to_material(&mut rt.material, srgb);
        }
        self.equipped_rows_dirty = true;
        self.sync_scene_graph();
    }

    // ------------------------------------------------------------------------
    // Mode-switching helpers
    // ------------------------------------------------------------------------

    fn enter_static_mode(&mut self, mesh: Mesh, material: Material, label: String) {
        self.fit_matrix = fit_to_avatar_height(mesh.bbox);
        self.static_mesh = Some(mesh);
        self.static_material = Some(material);
        self.static_label = label;
        self.static_asset_id = None;

        // Drop avatar-mode state.
        self.equipped_slots.clear();
        self.skeleton = None;
        self.animation_clips.clear();
        self.animation_player = None;
        self.face_runtime = None;
        self.face_images.clear();
        self.avatar.clear_all();
        self.object_overrides.clear();
        self.current_character_id = None;
        self.current_character_name = "Avatar".to_string();
        self.equipped_rows_dirty = true;
        self.invalidate_pose_cache();

        // Reset status flags.
        let s = &mut self.panel_status;
        s.avatar_mode = false;
        s.current_mesh_label = self.static_label.clone();
        s.skeleton_label = "No skeleton".into();
        s.has_skeleton = false;
        s.show_skeleton = false;
        s.is_skinned = false;
        s.debug_pose = false;
        s.has_animation = false;
        s.animation_label = "No animation".into();
        s.animation_playing = false;
        s.animation_looping = false;
        s.animation_time = 0.0;
        s.animation_duration = 0.0;
        s.skeleton_warnings.clear();
        s.equipped_rows.clear();
        s.can_save_character = false;
        s.has_face = false;
        s.current_expression = Expression::default();
        s.last_error = None;
        self.sync_scene_graph();
    }

    /// Try to load a GLB from disk. `tint` is multiplied into the base color.
    fn load_glb_resources(
        &self,
        path: &std::path::Path,
        tint: [f32; 4],
    ) -> Result<(LoadedGlb, Material, String), String> {
        let renderer = self
            .renderer
            .as_ref()
            .ok_or_else(|| "renderer not ready".to_string())?;
        let scene = self
            .scene
            .as_ref()
            .ok_or_else(|| "scene not ready".to_string())?;
        let loaded = load_glb(&renderer.gpu.device, path).map_err(|e| format!("{e}"))?;
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("loaded.glb")
            .to_string();
        let material = scene.make_material(
            &renderer.gpu.device,
            &renderer.gpu.queue,
            loaded.base_color_image.as_ref(),
            tint,
            &label,
        );
        Ok((loaded, material, label))
    }

    /// Promote a freshly-loaded GLB to the active body. Switches to avatar
    /// mode, replaces any previously-equipped body and wearables.
    fn install_body(&mut self, loaded: LoadedGlb, material: Material, meta: Option<&AssetMeta>) {
        let label = meta
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| "custom body".to_string());
        let asset_id = meta
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "custom_body".to_string());
        let body_type = meta
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "custom".to_string());

        let dropped_wearables = self
            .equipped_slots
            .keys()
            .filter(|&&s| s != Slot::Body)
            .count();
        self.static_mesh = None;
        self.static_material = None;
        self.static_label.clear();
        self.static_asset_id = None;
        self.equipped_slots.clear();

        self.fit_matrix = fit_to_avatar_height(loaded.mesh.bbox);
        let has_skeleton = loaded.skeleton.is_some();
        let has_animation = !loaded.animation_clips.is_empty();
        let is_skinned = loaded.is_skinned;
        let skeleton_warnings = loaded.warnings.clone();
        let animation_label = loaded
            .animation_clips
            .first()
            .map(|clip| format!("Clip: {}", clip.name))
            .unwrap_or_else(|| "No animation".into());
        let animation_duration = loaded
            .animation_clips
            .first()
            .map(|clip| clip.duration)
            .unwrap_or(0.0);

        self.skeleton = loaded.skeleton;
        self.animation_clips = loaded.animation_clips;
        self.animation_player = has_animation.then(AnimationPlayer::new_autoplay);

        self.avatar.clear_all();
        self.object_overrides.clear();
        self.current_character_id = None;
        self.current_character_name = label.clone();
        self.avatar.body_type = body_type;
        self.avatar.equip(Slot::Body, asset_id.clone());

        let tint_srgb = self.seed_slot_color(Slot::Body, meta);
        let mut material = material;
        apply_tint_to_material(&mut material, tint_srgb);
        self.avatar.set_slot_color(Slot::Body, tint_srgb);

        self.equipped_slots.insert(
            Slot::Body,
            EquippedRuntime {
                asset_id,
                display_name: label.clone(),
                mesh: loaded.mesh,
                material,
                is_skinned,
                tint_srgb,
                supports_color: true, // body always recolorable
            },
        );
        self.equipped_rows_dirty = true;
        self.invalidate_pose_cache();

        // Build face quad if the body has a head bone (must happen before
        // we take an exclusive borrow on panel_status below).
        self.face_runtime = None;
        let head_info = self.skeleton.as_ref().and_then(|skel| {
            skel.bone_index("head")
                .map(|i| (i.0, skel.bones[i.0].world_bind_transform, skel.bones.len()))
        });
        if let Some((head_idx, head_world, bone_count)) = head_info {
            self.rebuild_face_image_cache();
            let expr = self.avatar.expression;
            self.face_runtime = self.build_face_runtime(head_idx, head_world, bone_count, expr);
        }
        let has_face = self.face_runtime.is_some();
        let current_expression = self.avatar.expression;

        // Refresh status.
        let s = &mut self.panel_status;
        s.avatar_mode = true;
        s.current_mesh_label = label;
        s.has_skeleton = has_skeleton;
        s.show_skeleton = has_skeleton;
        s.is_skinned = is_skinned;
        s.debug_pose = false;
        s.has_animation = has_animation;
        s.animation_playing = has_animation;
        s.animation_looping = has_animation;
        s.animation_time = 0.0;
        s.animation_duration = animation_duration;
        s.animation_label = animation_label;
        s.skeleton_label = if has_skeleton {
            format!("Skeleton: {}", Skeleton::AVATAR_SKELETON_V1)
        } else {
            "No skeleton".into()
        };
        s.skeleton_warnings = skeleton_warnings;
        s.current_expression = current_expression;
        s.has_face = has_face;
        s.can_save_character = true;

        s.last_error = if dropped_wearables > 0 {
            Some(format!(
                "Unequipped {dropped_wearables} wearable(s) for new body"
            ))
        } else {
            None
        };
        self.sync_scene_graph();
    }

    /// Drop a wearable's GPU resources into the slot. Body slot is rejected
    /// here â€” bodies go through `install_body`.
    fn install_wearable(
        &mut self,
        slot: Slot,
        loaded: LoadedGlb,
        material: Material,
        meta: &AssetMeta,
    ) {
        if slot == Slot::Body {
            warn!(asset_id = %meta.id, "install_wearable refused Body slot");
            return;
        }
        // If the user previously coloured this slot AND the new asset is the
        // same id, keep the colour. Otherwise reseed from metadata defaults.
        let prior_id = self.avatar.equipped(slot).map(str::to_owned);
        let keep_color = prior_id.as_deref() == Some(meta.id.as_str());
        let tint_srgb = if keep_color {
            self.avatar
                .slot_color(slot)
                .unwrap_or_else(|| self.seed_slot_color(slot, Some(meta)))
        } else {
            // Different asset â†’ drop any stale colour and reseed.
            self.avatar.slot_colors.remove(&slot);
            self.seed_slot_color(slot, Some(meta))
        };
        let mut material = material;
        apply_tint_to_material(&mut material, tint_srgb);
        self.avatar.set_slot_color(slot, tint_srgb);
        self.equipped_slots.insert(
            slot,
            EquippedRuntime {
                asset_id: meta.id.clone(),
                display_name: meta.display_name.clone(),
                mesh: loaded.mesh,
                material,
                is_skinned: loaded.is_skinned,
                tint_srgb,
                supports_color: meta.supports_color,
            },
        );
        self.equipped_rows_dirty = true;
        self.invalidate_pose_cache();
        self.avatar.equip(slot, meta.id.clone());
        self.panel_status.last_error = None;
        info!(asset_id = %meta.id, slot = ?slot, "wearable equipped");
        self.sync_scene_graph();
    }

    /// Decide an initial sRGB tint for a freshly-equipped slot.
    /// Priority: remembered user pick â†’ metadata `default_color` â†’
    /// `default_slot_tint` â†’ opaque white.
    fn seed_slot_color(&self, slot: Slot, meta: Option<&AssetMeta>) -> [f32; 3] {
        if let Some(over) = self.avatar.slot_color(slot) {
            return over;
        }
        if let Some(m) = meta {
            if let Some(c) = m.default_color {
                return c;
            }
        }
        let t = default_slot_tint(slot);
        [t[0], t[1], t[2]]
    }

    /// (Re)compute the procedural face textures and stash them on the app.
    fn rebuild_face_image_cache(&mut self) {
        self.face_images.clear();
        for (expr, img) in face_textures::generate_all() {
            self.face_images.insert(expr, img);
        }
    }

    /// Build the procedural face-quad EquippedRuntime for the given head bone.
    fn build_face_runtime(
        &self,
        head_bone_idx: usize,
        head_world: Mat4,
        bone_count: usize,
        expression: Expression,
    ) -> Option<EquippedRuntime> {
        let renderer = self.renderer.as_ref()?;
        let scene = self.scene.as_ref()?;
        let device = &renderer.gpu.device;
        let queue = &renderer.gpu.queue;

        // Head bone local axes in world space (bone-transform columns).
        let cols = head_world.to_cols_array_2d();
        let right = Vec3::new(cols[0][0], cols[0][1], cols[0][2]).normalize_or_zero();
        let up = Vec3::new(cols[1][0], cols[1][1], cols[1][2]).normalize_or_zero();
        let forward = Vec3::new(cols[2][0], cols[2][1], cols[2][2]).normalize_or_zero();
        let origin = Vec3::new(cols[3][0], cols[3][1], cols[3][2]);

        // Quad lives a hair forward of the head cube's front face.
        let half = 0.16_f32; // â†’ 0.32m square
        let push = 0.18_f32; // depth offset; head cube is ~0.16m half-width
        let center = origin + forward * push;
        let bl = center - right * half - up * half;
        let br = center + right * half - up * half;
        let tr = center + right * half + up * half;
        let tl = center - right * half + up * half;
        let n = forward.to_array();
        let head_joint = head_bone_idx as u32;

        // CCW from +forward.
        let vertices = vec![
            Vertex::skinned(
                bl.to_array(),
                n,
                [0.0, 1.0],
                [head_joint, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
            ),
            Vertex::skinned(
                br.to_array(),
                n,
                [1.0, 1.0],
                [head_joint, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
            ),
            Vertex::skinned(
                tr.to_array(),
                n,
                [1.0, 0.0],
                [head_joint, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
            ),
            Vertex::skinned(
                tl.to_array(),
                n,
                [0.0, 0.0],
                [head_joint, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
            ),
        ];
        let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        let mesh = Mesh::from_data(device, "face_quad", &vertices, &indices, true);

        let face_image = self.face_images.get(&expression);
        let material = scene.make_material(
            device,
            queue,
            face_image,
            [1.0, 1.0, 1.0, 1.0], // no tint â€” show the procedural pixels as-is
            "face",
        );

        // bone_count kept for future validators; not used yet.
        let _ = bone_count;

        Some(EquippedRuntime {
            asset_id: format!("face_{}", expression.as_save_str()),
            display_name: "Face".to_string(),
            mesh,
            material,
            is_skinned: true,
            tint_srgb: [1.0, 1.0, 1.0],
            supports_color: false,
        })
    }

    /// Swap the face quad's texture to the given expression's PNG. Cheap.
    fn apply_face_expression(&mut self, expression: Expression) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        let Some(rt) = self.face_runtime.as_mut() else {
            return;
        };
        let Some(img) = self.face_images.get(&expression) else {
            return;
        };
        let texture =
            Texture::from_dynamic_image(&renderer.gpu.device, &renderer.gpu.queue, img, "face");
        scene.rebuild_material_with_texture(
            &renderer.gpu.device,
            &mut rt.material,
            texture,
            "face",
        );
    }

    /// Returns Ok(slot) if the wearable can be equipped given current state.
    fn validate_wearable(&self, meta: &AssetMeta) -> Result<Slot, String> {
        if self.skeleton.is_none() {
            return Err("Load a body first.".into());
        }
        let slot = Slot::from_asset_category(meta.category)
            .ok_or_else(|| "Asset category isn't equippable.".to_string())?;
        if slot == Slot::Body {
            return Err("Use the Character tab to swap the body.".into());
        }
        if meta.compatible_skeleton.as_deref() != Some(Skeleton::AVATAR_SKELETON_V1) {
            return Err(format!(
                "Incompatible: requires skeleton {}",
                Skeleton::AVATAR_SKELETON_V1
            ));
        }
        if !meta.compatible_body_types.is_empty()
            && !meta.compatible_body_types.contains(&self.avatar.body_type)
        {
            return Err(format!("Incompatible with body {}", self.avatar.body_type));
        }
        Ok(slot)
    }

    fn reset_to_cube(&mut self) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        let cube = Mesh::cube(&renderer.gpu.device, 1.0);
        let mat = scene.make_material(
            &renderer.gpu.device,
            &renderer.gpu.queue,
            None,
            [0.42, 0.55, 0.85, 1.0],
            "cube",
        );
        self.enter_static_mode(cube, mat, "cube (placeholder)".into());
    }

    fn handle_side_panel_action(&mut self, action: SidePanelAction) {
        match action {
            SidePanelAction::LoadCustomGlb => {
                let picked = rfd::FileDialog::new()
                    .add_filter("glTF binary", &["glb"])
                    .add_filter("All files", &["*"])
                    .set_title("Load a model")
                    .pick_file();
                if let Some(path) = picked {
                    self.load_custom_glb(&path);
                }
            }
            SidePanelAction::LoadAsset(id) => {
                match self
                    .command_for_asset(id)
                    .and_then(|command| self.execute_command(command).map_err(|e| e.to_string()))
                {
                    Ok(()) => {}
                    Err(err) => {
                        self.panel_status.last_error = Some(err.clone());
                        self.toast_error(err, 6);
                    }
                }
            }
            SidePanelAction::Undo => {
                let command = self.command(CommandSource::Ui, CommandPayload::HistoryUndo);
                let _ = self.execute_command(command);
            }
            SidePanelAction::Redo => {
                let command = self.command(CommandSource::Ui, CommandPayload::HistoryRedo);
                let _ = self.execute_command(command);
            }
            SidePanelAction::UnequipSlot(slot) => {
                if slot == Slot::Body {
                    warn!("ignored UnequipSlot(Body) â€” use Reset to cube to drop the body");
                    return;
                }
                if self.equipped_slots.contains_key(&slot) {
                    self.run_legacy_undoable("unequip slot", |app| {
                        app.equipped_slots.remove(&slot);
                        app.avatar.unequip(slot);
                        app.panel_status.last_error = None;
                        app.equipped_rows_dirty = true;
                    });
                }
            }
            SidePanelAction::ResetToCube => {
                self.run_legacy_undoable("reset to cube", |app| app.reset_to_cube());
            }
            SidePanelAction::SaveCharacter => self.save_current_character(),
            SidePanelAction::LoadCharacter(id) => {
                self.run_legacy_undoable("load character", |app| app.load_saved_character(&id));
            }
            SidePanelAction::RefreshGallery => self.refresh_gallery_rows(),
            SidePanelAction::SetExportSize(size) => {
                if png::validate_size(size).is_ok() {
                    self.export_options.size = size;
                    self.panel_status.export_size = size;
                }
            }
            SidePanelAction::SetExportPortrait(value) => {
                self.export_options.view = if value {
                    ExportView::Portrait
                } else {
                    ExportView::FullBody
                };
                self.panel_status.export_portrait = value;
            }
            SidePanelAction::SetExportTransparent(value) => {
                self.export_options.transparent_background = value;
                self.panel_status.export_transparent = value;
            }
            SidePanelAction::ExportPng => match self.export_current_png(self.paths.exports_dir()) {
                Ok(path) => {
                    self.panel_status.last_export_label =
                        Some(format!("Exported {}", path.display()));
                    self.panel_status.last_error = None;
                    self.toast_success(format!("Exported {}", path.display()), 4);
                }
                Err(e) => {
                    self.panel_status.last_error = Some(format!("Export failed: {e}"));
                    self.toast_error(format!("Export failed: {e}"), 6);
                }
            },
            SidePanelAction::ExportGif => match self.export_current_gif(self.paths.exports_dir()) {
                Ok(path) => {
                    self.panel_status.last_export_label =
                        Some(format!("Exported {}", path.display()));
                    self.panel_status.last_error = None;
                    self.toast_success(format!("Exported {}", path.display()), 4);
                }
                Err(e) => {
                    self.panel_status.last_error = Some(format!("GIF export failed: {e}"));
                    self.toast_error(format!("GIF export failed: {e}"), 6);
                }
            },
            SidePanelAction::SetSlotColor(slot, srgb) => {
                let command = self.command_for_slot_color(slot, srgb);
                if let Err(err) = self.execute_command(command) {
                    self.toast_error(format!("Command failed: {err}"), 6);
                }
            }
            SidePanelAction::SetExpression(expr) => {
                self.run_legacy_undoable("set expression", |app| {
                    app.avatar.expression = expr;
                    app.panel_status.current_expression = expr;
                    app.apply_face_expression(expr);
                });
            }
            SidePanelAction::SetShowSkeleton(value) => {
                self.panel_status.show_skeleton = value && self.skeleton.is_some();
            }
            SidePanelAction::SetDebugPose(value) => {
                self.run_legacy_undoable("set debug pose", |app| {
                    app.panel_status.debug_pose =
                        value && app.skeleton.is_some() && app.animation_player.is_none();
                    app.invalidate_pose_cache();
                });
            }
            SidePanelAction::SetAnimationPlaying(value) => {
                self.run_legacy_undoable("set animation playing", |app| {
                    if let Some(player) = app.animation_player.as_mut() {
                        if value {
                            player.play();
                        } else {
                            player.pause();
                        }
                        app.panel_status.animation_playing = player.playing;
                    }
                });
            }
            SidePanelAction::SetAnimationLooping(value) => {
                self.run_legacy_undoable("set animation looping", |app| {
                    if let Some(player) = app.animation_player.as_mut() {
                        player.set_looping(value);
                        app.panel_status.animation_looping = player.looping;
                    }
                });
            }
            SidePanelAction::SeekAnimation(time) => {
                self.run_legacy_undoable("seek animation", |app| {
                    if let (Some(player), Some(clip)) =
                        (app.animation_player.as_mut(), app.animation_clips.first())
                    {
                        player.seek(time, clip);
                        app.panel_status.animation_time = player.time;
                        app.invalidate_pose_cache();
                    }
                });
            }
            // Stage 20 inspector actions.
            SidePanelAction::SelectObject(id) => {
                self.dispatch_select_object(id);
            }
            SidePanelAction::DeselectAll => {
                self.dispatch_deselect_all();
            }
            SidePanelAction::SetInspectorFilter(filter) => {
                self.inspector_filter = filter;
            }
            SidePanelAction::SetSceneObjectVisible { id, visible } => {
                self.dispatch_scene_set_visible(id, visible);
            }
            SidePanelAction::SetSceneObjectLocked { id, locked } => {
                self.dispatch_scene_set_locked(id, locked);
            }
            SidePanelAction::SetActiveTool(tool) => {
                self.tool_state.active = tool;
                if matches!(tool, ui::EditorTool::Select) {
                    self.tool_state.axis = ui::AxisConstraint::None;
                }
            }
            SidePanelAction::SetAxisConstraint(axis) => {
                self.tool_state.axis = axis;
            }
            SidePanelAction::SetObjectTranslation { id, translation } => {
                self.dispatch_set_object_translation(id, translation);
            }
            SidePanelAction::SetObjectRotation { id, rotation_quat } => {
                self.dispatch_set_object_rotation(id, rotation_quat);
            }
            SidePanelAction::SetObjectScale { id, scale } => {
                self.dispatch_set_object_scale(id, scale);
            }
            SidePanelAction::ResetObjectTransform(id) => {
                self.dispatch_reset_object_transform(id);
            }
            SidePanelAction::SetEditorMode(mode) => {
                self.dispatch_set_editor_mode(mode);
            }
        }
    }

    fn dispatch_select_object(&mut self, id: String) {
        let payload = commands::SelectionSetPayload {
            object_ids: vec![id],
        };
        let env = self.command(
            commands::CommandSource::Ui,
            commands::CommandPayload::SelectionSet(payload),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "selection.set dispatch failed");
        }
    }

    fn dispatch_deselect_all(&mut self) {
        let env = self.command(
            commands::CommandSource::Ui,
            commands::CommandPayload::SelectionClear(commands::SelectionClearPayload::default()),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "selection.clear dispatch failed");
        }
    }

    fn dispatch_set_editor_mode(&mut self, mode: EditorMode) {
        let env = self.command(
            commands::CommandSource::Ui,
            commands::CommandPayload::EditorSetMode(commands::EditorSetModePayload { mode }),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "editor.set_mode dispatch failed");
        }
    }

    fn dispatch_scene_set_visible(&mut self, id: String, visible: bool) {
        let payload = commands::SceneSetVisiblePayload {
            object_id: id,
            visible,
        };
        let env = self.command(
            commands::CommandSource::Ui,
            commands::CommandPayload::SceneSetVisible(payload),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "scene.set_visible dispatch failed");
        }
    }

    fn dispatch_scene_set_locked(&mut self, id: String, locked: bool) {
        let payload = commands::SceneSetLockedPayload {
            object_id: id,
            locked,
        };
        let env = self.command(
            commands::CommandSource::Ui,
            commands::CommandPayload::SceneSetLocked(payload),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "scene.set_locked dispatch failed");
        }
    }

    fn dispatch_set_object_translation(&mut self, id: String, translation: [f32; 3]) {
        let env = self.command(
            CommandSource::Ui,
            CommandPayload::TransformSetTranslation(commands::TransformSetTranslationPayload {
                object_id: id,
                translation,
            }),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "transform.set_translation dispatch failed");
        }
    }

    fn dispatch_set_object_rotation(&mut self, id: String, rotation: [f32; 4]) {
        let env = self.command(
            CommandSource::Ui,
            CommandPayload::TransformSetRotation(commands::TransformSetRotationPayload {
                object_id: id,
                rotation,
            }),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "transform.set_rotation dispatch failed");
        }
    }

    fn dispatch_set_object_scale(&mut self, id: String, scale: [f32; 3]) {
        let env = self.command(
            CommandSource::Ui,
            CommandPayload::TransformSetScale(commands::TransformSetScalePayload {
                object_id: id,
                scale,
            }),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "transform.set_scale dispatch failed");
        }
    }

    fn dispatch_reset_object_transform(&mut self, id: String) {
        let env = self.command(
            CommandSource::Ui,
            CommandPayload::TransformReset(commands::TransformResetPayload { object_id: id }),
        );
        if let Err(e) = self.execute_command(env) {
            warn!(error = %e, "transform.reset dispatch failed");
        }
    }

    /// Side-panel asset row click â€” dispatches by the asset's category.
    fn current_animation_name(&self) -> Option<String> {
        self.animation_player.as_ref().and_then(|player| {
            self.animation_clips
                .get(player.selected_clip)
                .map(|clip| clip.name.clone())
        })
    }

    fn save_current_character(&mut self) {
        if !self.panel_status.avatar_mode || self.avatar.equipped(Slot::Body).is_none() {
            self.panel_status.last_error = Some("Load a body before saving.".into());
            self.toast_error("Load a body before saving.", 6);
            return;
        }
        let id = self
            .current_character_id
            .clone()
            .unwrap_or_else(new_character_id);
        let save = AvatarSave::new(
            id.clone(),
            self.current_character_name.clone(),
            &self.avatar,
            self.current_animation_name(),
        );
        match self.character_store.save(&save) {
            Ok(path) => {
                self.current_character_id = Some(id.clone());
                self.current_character_name = save.name;
                self.panel_status.last_save_label = Some(format!("Saved {}", path.display()));
                self.panel_status.last_error = None;
                self.toast_success(format!("Saved '{}'", self.current_character_name), 3);
                // Best-effort thumbnail; don't fail the save if it errors.
                if let Err(e) = self.write_character_thumbnail(&id) {
                    warn!(error = %e, id = %id, "character thumbnail failed");
                }
                self.refresh_gallery_rows();
            }
            Err(e) => {
                warn!(error = %e, "character save failed");
                self.panel_status.last_error = Some(format!("Save failed: {e}"));
                self.toast_error(format!("Save failed: {e}"), 6);
            }
        }
    }

    fn write_character_thumbnail(&mut self, id: &str) -> anyhow::Result<()> {
        const THUMB_SIZE: u32 = 256;
        let dir = self.paths.characters_dir();
        let shot = self.capture_scene_internal(THUMB_SIZE, ExportView::FullBody, false)?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{id}.png"));
        image::save_buffer(
            &path,
            &shot.pixels,
            shot.width,
            shot.height,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }

    fn load_saved_character(&mut self, id: &str) {
        let save = match self.character_store.load(id) {
            Ok(save) => save,
            Err(e) => {
                self.panel_status.last_error = Some(format!("Load failed: {e}"));
                self.toast_error(format!("Load failed: {e}"), 6);
                return;
            }
        };
        let body_id = save.base_body.clone();
        if body_id == avatar::DEFAULT_BODY_TYPE {
            self.panel_status.last_error = Some("Saved character has no body.".into());
            self.toast_error("Saved character has no body.", 6);
            return;
        }

        self.load_asset_by_id(&body_id);
        if self.avatar.equipped(Slot::Body) != Some(body_id.as_str()) {
            self.panel_status.last_error =
                Some(format!("Could not load saved body asset {body_id}"));
            self.toast_error(format!("Could not load saved body asset {body_id}"), 6);
            return;
        }

        for asset_id in save.slots.values() {
            self.load_asset_by_id(asset_id);
        }
        for (slot, srgb) in &save.colors {
            self.avatar.set_slot_color(*slot, *srgb);
            if let Some(rt) = self.equipped_slots.get_mut(slot) {
                rt.tint_srgb = *srgb;
                apply_tint_to_material(&mut rt.material, *srgb);
            }
        }
        self.avatar.expression = save.expression;
        self.panel_status.current_expression = save.expression;
        self.apply_face_expression(save.expression);
        if let (Some(anim_name), Some(player)) =
            (save.animation.as_deref(), self.animation_player.as_mut())
        {
            if let Some(idx) = self
                .animation_clips
                .iter()
                .position(|clip| clip.name == anim_name)
            {
                player.selected_clip = idx;
                player.time = 0.0;
            }
        }
        self.current_character_id = Some(save.id.clone());
        self.current_character_name = save.name.clone();
        self.panel_status.last_save_label = Some(format!("Loaded {}", save.name));
        self.panel_status.last_error = None;
        self.toast_info(format!("Loaded '{}'", self.current_character_name), 2);
        self.refresh_gallery_rows();
    }

    fn load_asset_by_id(&mut self, id: &str) {
        let Some(meta) = self
            .catalog
            .as_ref()
            .and_then(|c| c.find(id).ok().flatten())
        else {
            warn!(asset_id = %id, "asset id not found in catalog");
            self.panel_status.last_error = Some(format!("Asset '{id}' not in catalog"));
            return;
        };

        let path = self.paths.assets_processed().join(&meta.model);
        let slot_kind = Slot::from_asset_category(meta.category);

        match slot_kind {
            Some(Slot::Body) => {
                // Body-category assets can be either real avatar bodies
                // (rigged/skinned) or static sample meshes like Rubber Duck.
                match self.load_glb_resources(&path, [1.0, 1.0, 1.0, 1.0]) {
                    Ok((loaded, material, _label)) => {
                        if loaded.skeleton.is_some() && loaded.is_skinned {
                            self.install_body(loaded, material, Some(&meta));
                            return;
                        }
                        if meta.compatible_skeleton.is_some() {
                            self.panel_status.last_error = Some(format!(
                                "{} declares a skeleton but did not load as a skinned body.",
                                meta.display_name
                            ));
                            return;
                        }

                        self.enter_static_mode(loaded.mesh, material, meta.display_name.clone());
                        self.static_asset_id = Some(meta.id.clone());
                        self.sync_scene_graph();
                    }
                    Err(e) => {
                        warn!(file = %path.display(), error = %e, "body load failed");
                        self.panel_status.last_error = Some(format!("Load failed: {e}"));
                    }
                }
            }
            Some(slot) => {
                // Wearable. Validate, then load + install.
                let slot_check = match self.validate_wearable(&meta) {
                    Ok(s) => s,
                    Err(e) => {
                        self.panel_status.last_error = Some(e);
                        return;
                    }
                };
                debug_assert_eq!(slot_check, slot);
                // Pass a neutral tint; `install_wearable` overwrites
                // `material.base_color` with the seeded slot color.
                match self.load_glb_resources(&path, [1.0, 1.0, 1.0, 1.0]) {
                    Ok((loaded, material, _label)) => {
                        self.install_wearable(slot, loaded, material, &meta);
                    }
                    Err(e) => {
                        warn!(file = %path.display(), error = %e, "wearable load failed");
                        self.panel_status.last_error = Some(format!("Load failed: {e}"));
                    }
                }
            }
            None => {
                self.panel_status.last_error =
                    Some(format!("Asset category isn't equippable: {}", id));
            }
        }
    }

    /// "Load custom GLBâ€¦" file-picker path. Anything we don't recognize as a
    /// rigged body goes into static mode.
    fn load_custom_glb(&mut self, path: &std::path::Path) {
        match self.load_glb_resources(path, [1.0, 1.0, 1.0, 1.0]) {
            Ok((loaded, material, label)) => {
                if loaded.skeleton.is_some() && loaded.is_skinned {
                    // Treat as a custom body. No metadata â†’ use defaults.
                    self.install_body(loaded, material, None);
                } else {
                    self.enter_static_mode(loaded.mesh, material, label);
                }
            }
            Err(e) => {
                warn!(file = %path.display(), error = %e, "custom GLB load failed");
                self.panel_status.last_error = Some(format!("Load failed: {e}"));
            }
        }
    }

    fn capture_current_scene(
        &mut self,
        options: PngExportOptions,
    ) -> anyhow::Result<renderer::RgbaScreenshot> {
        png::validate_size(options.size)?;
        self.capture_scene_internal(options.size, options.view, options.transparent_background)
    }

    /// Same draw path as `capture_current_scene` but without the export-size
    /// gate. Used for per-character gallery thumbnails (e.g. 256×256).
    fn capture_scene_internal(
        &mut self,
        size: u32,
        view: ExportView,
        transparent_background: bool,
    ) -> anyhow::Result<renderer::RgbaScreenshot> {
        self.capture_scene_with_yaw(size, view, transparent_background, 0.0)
    }

    fn capture_scene_with_yaw(
        &mut self,
        size: u32,
        view: ExportView,
        transparent_background: bool,
        yaw_offset: f32,
    ) -> anyhow::Result<renderer::RgbaScreenshot> {
        let mut camera = self.camera;
        camera.set_aspect([size, size]);
        match view {
            ExportView::FullBody => camera.preset_full_body(),
            ExportView::Portrait => camera.preset_face(),
        }
        camera.yaw += yaw_offset;

        let posed_world_transforms = self
            .skeleton
            .as_ref()
            .map(|s| {
                if let Some(pose) = self
                    .animation_player
                    .as_ref()
                    .and_then(|p| p.sample_pose(&self.animation_clips, s.bones.len()))
                {
                    s.world_transforms_from_pose(&pose)
                } else {
                    s.posed_world_transforms(self.panel_status.debug_pose)
                }
            })
            .unwrap_or_default();
        let skinning_palette = self.skeleton.as_ref().and_then(|s| {
            if posed_world_transforms.is_empty() {
                None
            } else {
                SkinningPalette::from_world_transforms(s, &posed_world_transforms).ok()
            }
        });
        let palette_ref = skinning_palette.as_ref();
        let model = self.fit_matrix;
        let mut instances: Vec<SceneInstance<'_>> = Vec::new();
        if self.panel_status.avatar_mode {
            for &slot in Slot::ALL.iter() {
                if let Some(rt) = self.equipped_slots.get(&slot) {
                    instances.push(SceneInstance {
                        mesh: &rt.mesh,
                        model,
                        material: &rt.material,
                        skinning_palette: if rt.is_skinned { palette_ref } else { None },
                    });
                }
            }
            if let Some(rt) = self.face_runtime.as_ref() {
                instances.push(SceneInstance {
                    mesh: &rt.mesh,
                    model,
                    material: &rt.material,
                    skinning_palette: if rt.is_skinned { palette_ref } else { None },
                });
            }
        } else if let (Some(mesh), Some(material)) =
            (self.static_mesh.as_ref(), self.static_material.as_ref())
        {
            instances.push(SceneInstance {
                mesh,
                model,
                material,
                skinning_palette: None,
            });
        }
        if instances.is_empty() {
            anyhow::bail!("nothing to export");
        }

        let linear_clear = renderer::color::srgb_to_linear(self.config.render.clear_color);
        let clear = if transparent_background {
            wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
        } else {
            wgpu::Color {
                r: linear_clear[0] as f64,
                g: linear_clear[1] as f64,
                b: linear_clear[2] as f64,
                a: 1.0,
            }
        };
        let scene = self
            .scene
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("scene not ready"))?;
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("renderer not ready"))?;
        let viewport = [0, 0, size, size];
        let screenshot = renderer.capture_rgba(size, size, clear, |fc| {
            scene.draw(fc, viewport, &camera, &self.light, &instances);
        })?;
        if !screenshot.is_non_empty() {
            anyhow::bail!("captured PNG was empty");
        }
        Ok(screenshot)
    }

    fn export_current_png(&mut self, dir: impl AsRef<Path>) -> Result<PathBuf, String> {
        let options = self.export_options;
        let screenshot = self
            .capture_current_scene(options)
            .map_err(|e| e.to_string())?;
        let file = format!(
            "avatar_{}_{}_{}.png",
            options.view.as_str(),
            options.size,
            timestamp_millis()
        );
        let path = dir.as_ref().join(file);
        png::write_rgba_png(
            &path,
            screenshot.width,
            screenshot.height,
            &screenshot.pixels,
        )
        .map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn export_current_gif(&mut self, dir: impl AsRef<Path>) -> Result<PathBuf, String> {
        let options = self.gif_export_options;
        video::validate_options(options).map_err(|e| e.to_string())?;
        let mut frames = Vec::with_capacity(options.frames as usize);
        for i in 0..options.frames {
            let yaw = (i as f32 / options.frames as f32) * std::f32::consts::TAU;
            let shot = self
                .capture_scene_with_yaw(options.size, ExportView::FullBody, false, yaw)
                .map_err(|e| e.to_string())?;
            frames.push(shot.pixels);
        }
        let delay_ms = (options.duration_ms / options.frames).max(1);
        let file = if self.deterministic {
            "agent_turntable.gif".to_string()
        } else {
            format!(
                "avatar_turntable_{}_{}f_{}.gif",
                options.size,
                options.frames,
                timestamp_millis()
            )
        };
        let path = dir.as_ref().join(file);
        video::write_rgba_gif(&path, options.size, options.size, &frames, delay_ms)
            .map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn run_agent_gif(&mut self) -> anyhow::Result<PathBuf> {
        self.load_asset_by_id("body_phase4_rig_001");
        self.load_asset_by_id("top_phase7_basic_001");
        if self.avatar.equipped(Slot::Body).is_none() {
            anyhow::bail!("agent gif could not load Phase 4 Rig body");
        }
        if let Some(player) = self.animation_player.as_mut() {
            player.playing = false;
            player.looping = false;
            player.time = 0.0;
        }
        self.export_current_gif(self.paths.exports_dir())
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn run_agent_capture(&mut self) -> anyhow::Result<Vec<PathBuf>> {
        self.load_asset_by_id("body_phase4_rig_001");
        self.load_asset_by_id("top_phase7_basic_001");
        if self.avatar.equipped(Slot::Body).is_none() {
            anyhow::bail!("agent capture could not load Phase 4 Rig body");
        }
        self.tool_state.show_gizmo = false;
        if self.deterministic {
            self.current_mode = EditorMode::Character;
            self.config.editor.last_mode = self.current_mode;
            // Freeze the animation player at t=0 so the captured pose is
            // bit-stable across runs.
            if let Some(player) = self.animation_player.as_mut() {
                player.playing = false;
                player.looping = false;
                player.time = 0.0;
            }
            // Auto-select the active avatar so the exported selection JSON
            // is reproducible (otherwise it'd be empty).
            self.sync_scene_graph();
            let avatar_id = SceneId::new("avatar_001").expect("static id is valid");
            if self.scene_graph.contains(&avatar_id) {
                self.scene_selection.set_objects([avatar_id]);
                self.scene_graph.selection = self.scene_selection.clone();
            }
        }

        let out_dir = self.paths.debug_screenshots_dir();
        std::fs::create_dir_all(&out_dir)?;
        let mut paths = Vec::new();
        let graph_path = out_dir.join("latest_scene_graph.json");
        std::fs::write(&graph_path, self.scene_graph_json()?)?;
        paths.push(graph_path);
        let summary_path = out_dir.join("latest_scene_summary.json");
        std::fs::write(&summary_path, self.scene_summary_json()?)?;
        paths.push(summary_path);
        let selection_path = out_dir.join("latest_selection.json");
        std::fs::write(&selection_path, self.scene_selection_json()?)?;
        paths.push(selection_path);
        let mode_path = out_dir.join("latest_editor_mode.json");
        std::fs::write(&mode_path, self.editor_mode_json()?)?;
        paths.push(mode_path);
        for view in [ExportView::FullBody, ExportView::Portrait] {
            let options = PngExportOptions {
                size: 1024,
                transparent_background: false,
                view,
            };
            let shot = self.capture_current_scene(options)?;
            let filename = if self.deterministic {
                format!("agent_{}.png", view.as_str())
            } else {
                format!("agent_{}_{}.png", view.as_str(), timestamp_millis())
            };
            let path = out_dir.join(filename);
            png::write_rgba_png(&path, shot.width, shot.height, &shot.pixels)?;
            paths.push(path);
        }
        let manifest = out_dir.join("latest_agent_capture.json");
        let body = if self.deterministic {
            format!(
                "{{\n  \"deterministic\": true,\n  \"files\": [{}]\n}}\n",
                paths
                    .iter()
                    .map(|p| format!("\"{}\"", p.file_name().unwrap().to_string_lossy()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            format!(
                "{{\n  \"capturedAt\": \"{}\",\n  \"files\": [{}]\n}}\n",
                timestamp_millis(),
                paths
                    .iter()
                    .map(|p| format!("\"{}\"", p.display().to_string().replace('\\', "/")))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        std::fs::write(&manifest, body)?;
        paths.push(manifest);
        Ok(paths)
    }

    fn setup_agent_perf(&mut self) -> anyhow::Result<()> {
        self.load_asset_by_id("body_phase4_rig_001");
        self.load_asset_by_id("top_phase7_basic_001");
        if self.avatar.equipped(Slot::Body).is_none() {
            anyhow::bail!("agent perf could not load Phase 4 Rig body");
        }
        self.panel_status.show_diagnostics = true;
        self.perf_stats.clear();
        self.clock.reset();
        Ok(())
    }

    fn write_agent_perf_report(&self) -> anyhow::Result<PathBuf> {
        let out_dir = self.paths.perf_dir();
        std::fs::create_dir_all(&out_dir)?;
        let path = out_dir.join("latest_perf_report.json");
        let size = self.renderer.as_ref().map(|r| r.size()).unwrap_or([0, 0]);
        let report = self.perf_stats.to_report(
            timestamp_millis().to_string(),
            size,
            if self.panel_status.avatar_mode {
                "avatar"
            } else {
                "static"
            },
            self.panel_status.diagnostics.instance_count,
        );
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Refresh `panel_status.equipped_rows` from `equipped_slots` only when
    /// equipment or colors changed. The export/save flags still mirror every
    /// frame because they are cheap scalar copies.
    fn refresh_equipped_rows(&mut self) {
        if self.equipped_rows_dirty {
            let mut rows = Vec::with_capacity(self.equipped_slots.len());
            for &slot in Slot::ALL.iter() {
                if let Some(rt) = self.equipped_slots.get(&slot) {
                    rows.push(EquippedSlotRow {
                        slot,
                        display_name: rt.display_name.clone(),
                        color_srgb: rt.tint_srgb,
                        supports_color: rt.supports_color,
                    });
                }
            }
            self.panel_status.equipped_rows = rows;
            self.equipped_rows_dirty = false;
        }
        self.panel_status.can_save_character =
            self.panel_status.avatar_mode && self.avatar.equipped(Slot::Body).is_some();
        self.panel_status.export_size = self.export_options.size;
        self.panel_status.export_portrait = self.export_options.view == ExportView::Portrait;
        self.panel_status.export_transparent = self.export_options.transparent_background;
        self.panel_status.inspector_visible = ui::mode_layout(self.current_mode).show_inspector;
        self.panel_status.active_tool = self.tool_state.active;
        self.panel_status.axis_constraint = self.tool_state.axis;
        self.panel_status.show_gizmo = self.tool_state.show_gizmo;
        self.panel_status.inspector = ui::build_inspector_status(
            &self.scene_graph,
            &self.scene_selection,
            &self.inspector_filter,
        );
    }

    /// Runs one frame. Returns any side-panel action the user triggered so
    /// the caller can act on it *after* the borrow-heavy render block ends.
    fn redraw(&mut self) -> Option<SidePanelAction> {
        let frame_start = Instant::now();

        // Tick animation here so the slot-iteration block below can use a
        // shared `&` borrow of self.skeleton + self.animation_*.
        let dt = self.clock.tick();
        if let (Some(player), Some(clip)) =
            (self.animation_player.as_mut(), self.animation_clips.first())
        {
            player.tick(dt, clip);
            self.panel_status.animation_time = player.time;
            self.panel_status.animation_playing = player.playing;
            self.panel_status.animation_looping = player.looping;
            self.panel_status.animation_duration = clip.duration;
        }
        self.sync_scene_graph();
        self.refresh_equipped_rows();

        // --- 1. Run an egui frame --------------------------------------------
        let window = self.window.as_ref()?;
        let egui_state = self.egui_state.as_mut()?;
        let egui_start = Instant::now();
        let raw_input = egui_state.take_egui_input(window);
        let mut panel_action: Option<SidePanelAction> = None;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let top = draw_mode_bar(ctx, self.current_mode);
            let left = draw_side_panel(
                ctx,
                &mut self.selected_category,
                self.current_mode,
                &self.panel_status,
                &self.current_assets,
            );
            let right = if self.panel_status.inspector_visible {
                ui::draw_inspector_panel(ctx, &self.panel_status)
            } else {
                None
            };
            // Inspector wins when both produce an action in the same frame
            // (typical case: user clicks inspector while left panel state
            // also changed). Left actions still flow through next tick.
            panel_action = right.or(left).or(top);
            self.toasts.show(ctx);
        });
        egui_state.handle_platform_output(window, full_output.platform_output);
        let egui_ms = elapsed_ms(egui_start);

        let tessellate_start = Instant::now();
        let pixels_per_point = full_output.pixels_per_point;
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);
        let textures_delta = full_output.textures_delta;
        let tessellate_ms = elapsed_ms(tessellate_start);

        // Carve viewport so the scene sits between the left and right
        // panels (Stage 20 added the right-side Inspector panel).
        let renderer_ref = self.renderer.as_ref()?;
        let [fb_w, fb_h] = renderer_ref.size();
        let left_px = (ui::SIDE_PANEL_WIDTH * pixels_per_point).round() as u32;
        let inspector_px = if self.panel_status.inspector_visible {
            (ui::INSPECTOR_PANEL_WIDTH * pixels_per_point).round() as u32
        } else {
            0
        };
        let reserved_px = left_px + inspector_px;
        let reserved_px = reserved_px.min(fb_w.saturating_sub(1));
        let left_px = left_px.min(reserved_px);
        let top_px = (ui::MODE_BAR_HEIGHT * pixels_per_point).round() as u32;
        let top_px = top_px.min(fb_h.saturating_sub(1));
        let viewport_w = fb_w - reserved_px;
        let viewport_h = fb_h.saturating_sub(top_px).max(1);
        let viewport_rect = [left_px, top_px, viewport_w, viewport_h];
        self.camera.set_aspect([viewport_w, viewport_h]);

        // --- 2. Pose math ----------------------------------------------------
        let pose_start = Instant::now();
        let sampled_pose = self.animation_player.as_ref().and_then(|player| {
            player.sample_pose(
                &self.animation_clips,
                self.skeleton.as_ref().map(|s| s.bones.len()).unwrap_or(0),
            )
        });
        let (posed_world_transforms, skinning_palette) = if let Some(s) = self.skeleton.as_ref() {
            let selected_clip = self.animation_player.as_ref().map(|p| p.selected_clip);
            let animation_time = self
                .animation_player
                .as_ref()
                .map(|p| p.time)
                .unwrap_or(0.0);
            let key = PoseCacheKey {
                generation: self.pose_generation,
                bone_count: s.bones.len(),
                debug_pose: self.panel_status.debug_pose,
                selected_clip,
                animation_time_bits: animation_time.to_bits(),
            };
            if self.pose_cache.key == Some(key) {
                (
                    self.pose_cache.world_transforms.clone(),
                    self.pose_cache.palette.clone(),
                )
            } else {
                let world = self.build_bone_world_transforms(
                    s,
                    sampled_pose.as_ref(),
                    self.panel_status.debug_pose,
                );
                let palette = if world.is_empty() {
                    None
                } else {
                    match SkinningPalette::from_world_transforms(s, &world) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            warn!(error = %e, "skinning palette unavailable");
                            None
                        }
                    }
                };
                self.pose_cache.key = Some(key);
                self.pose_cache.world_transforms = world.clone();
                self.pose_cache.palette = palette.clone();
                (world, palette)
            }
        } else {
            (Vec::new(), None)
        };
        let pose_ms = elapsed_ms(pose_start);

        // --- 3. Build scene instance list -----------------------------------
        let scene_build_start = Instant::now();
        let avatar_mode = self.panel_status.avatar_mode;
        let fit = self.fit_matrix;
        let show_skeleton = self.panel_status.show_skeleton;
        let show_gizmo = self.tool_state.show_gizmo;
        let avatar_visible = self.effective_visibility("avatar_001", true);
        let avatar_world = Self::scene_transform_to_mat4(
            self.effective_scene_transform("avatar_001", scene::SceneTransform::default()),
        );
        let slot_models: Vec<(Slot, Mat4)> = if avatar_mode && avatar_visible {
            Slot::ALL
                .iter()
                .filter_map(|&slot| {
                    self.equipped_slots.get(&slot)?;
                    let mesh_id = slot_mesh_id(slot);
                    if !self.effective_visibility(&mesh_id, true) {
                        return None;
                    }
                    let local = Self::scene_transform_to_mat4(
                        self.effective_scene_transform(&mesh_id, scene::SceneTransform::default()),
                    );
                    Some((slot, avatar_world * local * fit))
                })
                .collect()
        } else {
            Vec::new()
        };
        let face_model = if avatar_mode
            && avatar_visible
            && self.effective_visibility("blendshape_face_001", true)
            && self.face_runtime.is_some()
        {
            let local = Self::scene_transform_to_mat4(self.effective_scene_transform(
                "blendshape_face_001",
                scene::SceneTransform::default(),
            ));
            Some(avatar_world * local * fit)
        } else {
            None
        };
        let static_model = if !avatar_mode
            && self.static_mesh.is_some()
            && self.static_material.is_some()
            && self.effective_visibility("mesh_static_001", true)
        {
            let local = Self::scene_transform_to_mat4(
                self.effective_scene_transform("mesh_static_001", scene::SceneTransform::default()),
            );
            Some(local * fit)
        } else {
            None
        };
        let selected_gizmo_world = if show_gizmo {
            self.selected_object_world_transform(&posed_world_transforms)
        } else {
            None
        };

        let palette_ref = skinning_palette.as_ref();
        let mut instances: Vec<SceneInstance<'_>> = Vec::new();
        if avatar_mode {
            for (slot, model) in &slot_models {
                if let Some(rt) = self.equipped_slots.get(slot) {
                    instances.push(SceneInstance {
                        mesh: &rt.mesh,
                        model: *model,
                        material: &rt.material,
                        skinning_palette: if rt.is_skinned { palette_ref } else { None },
                    });
                }
            }
            if let (Some(rt), Some(model)) = (self.face_runtime.as_ref(), face_model) {
                instances.push(SceneInstance {
                    mesh: &rt.mesh,
                    model,
                    material: &rt.material,
                    skinning_palette: if rt.is_skinned { palette_ref } else { None },
                });
            }
        } else if let (Some(mesh), Some(material), Some(model)) = (
            self.static_mesh.as_ref(),
            self.static_material.as_ref(),
            static_model,
        ) {
            instances.push(SceneInstance {
                mesh,
                model,
                material,
                skinning_palette: None,
            });
        }
        let instance_count = instances.len() as u32;
        let scene_build_ms = elapsed_ms(scene_build_start);

        let (Some(renderer), Some(scene), Some(debug_lines), Some(egui_renderer)) = (
            self.renderer.as_mut(),
            self.scene.as_ref(),
            self.debug_lines.as_ref(),
            self.egui_renderer.as_mut(),
        ) else {
            return None;
        };
        let camera = &self.camera;
        let light = &self.light;
        let skeleton = &self.skeleton;

        // --- 4. Render -------------------------------------------------------
        let render_start = Instant::now();
        let result = renderer.render(|fc| {
            if !instances.is_empty() {
                scene.draw(fc, viewport_rect, camera, light, &instances);
            }
            if show_skeleton {
                if let Some(skeleton) = skeleton.as_ref() {
                    debug_lines.draw_skeleton(
                        fc,
                        viewport_rect,
                        camera,
                        skeleton,
                        &posed_world_transforms,
                        avatar_world,
                    );
                }
            }
            if let Some(world) = selected_gizmo_world {
                debug_lines.draw_axes(fc, viewport_rect, camera, world, 0.25);
            }

            // egui paint
            let screen_desc = egui_wgpu::ScreenDescriptor {
                size_in_pixels: fc.size,
                pixels_per_point,
            };
            for (id, delta) in &textures_delta.set {
                egui_renderer.update_texture(fc.device, fc.queue, *id, delta);
            }
            egui_renderer.update_buffers(
                fc.device,
                fc.queue,
                fc.encoder,
                &paint_jobs,
                &screen_desc,
            );
            let mut pass = fc
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: fc.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            egui_renderer.render(&mut pass, &paint_jobs, &screen_desc);
            drop(pass);
            for id in &textures_delta.free {
                egui_renderer.free_texture(id);
            }
        });

        if let Err(e) = result {
            error!("render error: {e:?}");
        }
        let render_submit_ms = elapsed_ms(render_start);
        let gpu_ms = renderer.last_gpu_ms();

        let total_ms = elapsed_ms(frame_start);
        let sample = FrameSample {
            total_ms,
            egui_ms,
            tessellate_ms,
            pose_ms,
            scene_build_ms,
            render_submit_ms,
            gpu_ms,
            instance_count,
        };
        self.perf_stats.push(sample);
        self.panel_status.diagnostics.current_fps = if total_ms > 0.0 {
            1000.0 / total_ms
        } else {
            0.0
        };
        self.panel_status.diagnostics.average_fps = self.perf_stats.average_fps();
        self.panel_status.diagnostics.last_frame_ms = total_ms;
        self.panel_status.diagnostics.p95_frame_ms = self.perf_stats.p95_frame_ms();
        self.panel_status.diagnostics.total_frame_ms = total_ms;
        self.panel_status.diagnostics.egui_ms = egui_ms;
        self.panel_status.diagnostics.tessellate_ms = tessellate_ms;
        self.panel_status.diagnostics.pose_ms = pose_ms;
        self.panel_status.diagnostics.scene_build_ms = scene_build_ms;
        self.panel_status.diagnostics.render_submit_ms = render_submit_ms;
        self.panel_status.diagnostics.gpu_ms = self.perf_stats.average_gpu_ms();
        self.panel_status.diagnostics.instance_count = instance_count;
        self.panel_status.diagnostics.sample_count = self.perf_stats.len() as u32;

        panel_action
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.init_window_and_gpu(event_loop) {
            error!("init failed: {e:?}");
            event_loop.exit();
            return;
        }
        match self.startup_mode {
            StartupMode::AgentCapture => {
                match self.run_agent_capture() {
                    Ok(paths) => {
                        for path in paths {
                            info!(path = %path.display(), "agent capture artifact written");
                        }
                    }
                    Err(e) => error!("agent capture failed: {e:?}"),
                }
                event_loop.exit();
            }
            StartupMode::AgentPerf => {
                if let Err(e) = self.setup_agent_perf() {
                    error!("agent perf setup failed: {e:?}");
                    event_loop.exit();
                    return;
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            StartupMode::AgentGif => {
                match self.run_agent_gif() {
                    Ok(path) => info!(path = %path.display(), "agent GIF artifact written"),
                    Err(e) => error!("agent GIF export failed: {e:?}"),
                }
                event_loop.exit();
            }
            StartupMode::Interactive => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(w) = self.window.as_ref() {
            if w.id() != window_id {
                return;
            }
        }

        let egui_consumed =
            if let (Some(state), Some(window)) = (self.egui_state.as_mut(), self.window.as_ref()) {
                let response = state.on_window_event(window, &event);
                if response.repaint {
                    window.request_redraw();
                }
                response.consumed
            } else {
                false
            };

        match event {
            WindowEvent::CloseRequested => {
                info!("close requested; shutting down");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                debug!(?size, "resize");
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size);
                }
                self.camera.set_aspect([size.width, size.height]);
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::RedrawRequested => {
                let action = self.redraw();
                self.ensure_assets_fresh();
                if let Some(action) = action {
                    self.handle_side_panel_action(action);
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                if self.startup_mode == StartupMode::AgentPerf
                    && self.perf_stats.len() >= self.perf_target_frames
                {
                    match self.write_agent_perf_report() {
                        Ok(path) => info!(path = %path.display(), "agent perf report written"),
                        Err(e) => error!("agent perf report failed: {e:?}"),
                    }
                    event_loop.exit();
                }
            }

            WindowEvent::MouseInput { state, button, .. } if !egui_consumed => {
                self.mouse.on_button(button, state);
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse.last_pos = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if !egui_consumed {
                    if let Some(last) = self.mouse.last_pos {
                        let dx = (position.x - last.x) as f32;
                        let dy = (position.y - last.y) as f32;
                        if self.mouse.lmb {
                            self.camera.orbit(dx * ORBIT_SENS, -dy * ORBIT_SENS);
                        }
                        if self.mouse.rmb {
                            self.camera.pan(dx, dy);
                        }
                    }
                }
                self.mouse.last_pos = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } if !egui_consumed => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y as f32) / 50.0,
                };
                self.camera.zoom(steps);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } if !egui_consumed && !self.egui_ctx.wants_keyboard_input() => match code {
                KeyCode::KeyZ if self.modifiers.control_key() && self.modifiers.shift_key() => {
                    let command = self.command(CommandSource::Ui, CommandPayload::HistoryRedo);
                    let _ = self.execute_command(command);
                }
                KeyCode::KeyZ if self.modifiers.control_key() => {
                    let command = self.command(CommandSource::Ui, CommandPayload::HistoryUndo);
                    let _ = self.execute_command(command);
                }
                KeyCode::KeyY if self.modifiers.control_key() => {
                    let command = self.command(CommandSource::Ui, CommandPayload::HistoryRedo);
                    let _ = self.execute_command(command);
                }
                KeyCode::KeyQ if !self.modifiers.control_key() => {
                    self.tool_state.active = ui::EditorTool::Select;
                    self.tool_state.axis = ui::AxisConstraint::None;
                }
                KeyCode::KeyW if !self.modifiers.control_key() => {
                    self.tool_state.active = ui::EditorTool::Move;
                }
                KeyCode::KeyE if !self.modifiers.control_key() => {
                    self.tool_state.active = ui::EditorTool::Rotate;
                }
                KeyCode::KeyR if !self.modifiers.control_key() => {
                    self.tool_state.active = ui::EditorTool::Scale;
                }
                KeyCode::KeyX if !self.modifiers.control_key() => {
                    self.tool_state.axis = ui::AxisConstraint::X;
                }
                KeyCode::KeyY if !self.modifiers.control_key() => {
                    self.tool_state.axis = ui::AxisConstraint::Y;
                }
                KeyCode::KeyZ if !self.modifiers.control_key() => {
                    self.tool_state.axis = ui::AxisConstraint::Z;
                }
                KeyCode::Escape => {
                    self.tool_state.axis = ui::AxisConstraint::None;
                }
                KeyCode::KeyF => self.camera.focus(),
                KeyCode::F3 => {
                    self.panel_status.show_diagnostics = !self.panel_status.show_diagnostics;
                }
                KeyCode::Digit1 => self.camera.preset_full_body(),
                KeyCode::Digit2 => self.camera.preset_face(),
                _ => {}
            },

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(e) = self.config.save(&self.paths.settings()) {
            warn!("could not save settings: {e}");
        }
    }
}

impl CommandRuntime for App {
    type Snapshot = AppSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        self.current_snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: &Self::Snapshot) -> Result<(), CommandError> {
        App::restore_snapshot(self, snapshot.clone());
        Ok(())
    }

    fn validate_selection_set(&self, payload: &SelectionSetPayload) -> ValidationResult {
        let mut selection = SceneSelection::default();
        let mut ids = Vec::with_capacity(payload.object_ids.len());
        for raw in &payload.object_ids {
            let Ok(id) = SceneId::new(raw.clone()) else {
                return ValidationResult::error(
                    "INVALID_OBJECT_ID",
                    format!("invalid scene object id: {raw}"),
                );
            };
            if !self.scene_graph.contains(&id) {
                return ValidationResult::error(
                    "OBJECT_NOT_FOUND",
                    format!("scene object not found: {raw}"),
                );
            }
            ids.push(id);
        }
        selection.set_objects(ids);
        match selection.validate_against(&self.scene_graph) {
            Ok(()) => ValidationResult::valid(),
            Err(err) => ValidationResult::error("INVALID_SELECTION", err.to_string()),
        }
    }

    fn set_selection(&mut self, payload: &SelectionSetPayload) -> Result<(), CommandError> {
        let ids = payload
            .object_ids
            .iter()
            .map(|id| SceneId::new(id.clone()).map_err(|e| CommandError::Validation(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        let mut selection = SceneSelection::default();
        selection.set_objects(ids);
        selection
            .validate_against(&self.scene_graph)
            .map_err(|e| CommandError::Validation(e.to_string()))?;
        self.scene_selection = selection;
        self.scene_graph.selection = self.scene_selection.clone();
        Ok(())
    }

    fn clear_selection(&mut self) -> Result<(), CommandError> {
        self.scene_selection.clear();
        self.scene_graph.selection.clear();
        Ok(())
    }

    fn validate_avatar_equip_asset(&self, payload: &AvatarEquipAssetPayload) -> ValidationResult {
        if !matches!(payload.avatar_id.as_str(), "current" | "avatar_001") {
            return ValidationResult::error("UNKNOWN_AVATAR", "unknown avatar id");
        }
        let Some(meta) = self
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.find(&payload.asset_id).ok().flatten())
        else {
            return ValidationResult::error("ASSET_NOT_FOUND", "asset not found");
        };
        let Some(slot) = Slot::from_asset_category(meta.category) else {
            return ValidationResult::error("NOT_EQUIPPABLE", "asset category is not equippable");
        };
        if slot != payload.slot {
            return ValidationResult::error(
                "SLOT_MISMATCH",
                format!(
                    "asset belongs to slot {}, not {}",
                    slot.label(),
                    payload.slot.label()
                ),
            );
        }
        if slot == Slot::Body {
            return ValidationResult::valid();
        }
        match self.validate_wearable(&meta) {
            Ok(_) => ValidationResult::valid(),
            Err(err) => ValidationResult::error("INCOMPATIBLE_ASSET", err),
        }
    }

    fn equip_asset(&mut self, payload: &AvatarEquipAssetPayload) -> Result<(), CommandError> {
        self.load_asset_by_id(&payload.asset_id);
        if let Some(err) = self.panel_status.last_error.clone() {
            let failed = err.starts_with("Load failed")
                || err.contains("not in catalog")
                || err.starts_with("Incompatible")
                || err.contains("declares a skeleton")
                || err.contains("isn't equippable");
            if failed {
                return Err(CommandError::Runtime(err));
            }
        }
        Ok(())
    }

    fn validate_material_set_color(&self, payload: &MaterialSetColorPayload) -> ValidationResult {
        let in_range = payload
            .color_srgb
            .iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c));
        if !in_range {
            return ValidationResult::error("COLOR_OUT_OF_RANGE", "color must be finite 0..=1");
        }
        match &payload.target {
            MaterialTarget::AvatarSlot { avatar_id, slot } => {
                if !matches!(avatar_id.as_str(), "current" | "avatar_001") {
                    return ValidationResult::error("UNKNOWN_AVATAR", "unknown avatar id");
                }
                let Some(rt) = self.equipped_slots.get(slot) else {
                    return ValidationResult::error("SLOT_EMPTY", "slot is not equipped");
                };
                if rt.supports_color {
                    ValidationResult::valid()
                } else {
                    ValidationResult::error("COLOR_LOCKED", "slot does not support color edits")
                }
            }
        }
    }

    fn set_material_color(
        &mut self,
        payload: &MaterialSetColorPayload,
    ) -> Result<(), CommandError> {
        match payload.target {
            MaterialTarget::AvatarSlot { slot, .. } => {
                self.apply_slot_color(slot, payload.color_srgb);
                Ok(())
            }
        }
    }

    fn validate_scene_set_visible(
        &self,
        payload: &commands::SceneSetVisiblePayload,
    ) -> ValidationResult {
        if payload.object_id.trim().is_empty() {
            return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
        }
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error(
                "INVALID_OBJECT_ID",
                format!("invalid scene object id: {}", payload.object_id),
            );
        };
        if !self.scene_graph.contains(&id) {
            return ValidationResult::error(
                "OBJECT_NOT_FOUND",
                format!("scene object not found: {}", payload.object_id),
            );
        }
        ValidationResult::valid()
    }

    fn scene_set_visible(
        &mut self,
        payload: &commands::SceneSetVisiblePayload,
    ) -> Result<(), CommandError> {
        SceneId::new(payload.object_id.clone())
            .map_err(|e| CommandError::Validation(e.to_string()))?;
        self.set_object_visible_override(&payload.object_id, payload.visible);
        Ok(())
    }

    fn validate_scene_set_locked(
        &self,
        payload: &commands::SceneSetLockedPayload,
    ) -> ValidationResult {
        if payload.object_id.trim().is_empty() {
            return ValidationResult::error("EMPTY_OBJECT_ID", "object id is empty");
        }
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error(
                "INVALID_OBJECT_ID",
                format!("invalid scene object id: {}", payload.object_id),
            );
        };
        if !self.scene_graph.contains(&id) {
            return ValidationResult::error(
                "OBJECT_NOT_FOUND",
                format!("scene object not found: {}", payload.object_id),
            );
        }
        ValidationResult::valid()
    }

    fn scene_set_locked(
        &mut self,
        payload: &commands::SceneSetLockedPayload,
    ) -> Result<(), CommandError> {
        SceneId::new(payload.object_id.clone())
            .map_err(|e| CommandError::Validation(e.to_string()))?;
        self.set_object_locked_override(&payload.object_id, payload.locked);
        Ok(())
    }

    fn validate_transform_set_translation(
        &self,
        payload: &commands::TransformSetTranslationPayload,
    ) -> ValidationResult {
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error("INVALID_OBJECT_ID", "invalid object id");
        };
        let Some(object) = self.scene_graph.get_object(&id) else {
            return ValidationResult::error("OBJECT_NOT_FOUND", "scene object not found");
        };
        if object.locked {
            return ValidationResult::error("OBJECT_LOCKED", "object is locked");
        }
        if payload.translation.iter().all(|value| value.is_finite()) {
            ValidationResult::valid()
        } else {
            ValidationResult::error("INVALID_TRANSLATION", "translation must be finite")
        }
    }

    fn transform_set_translation(
        &mut self,
        payload: &commands::TransformSetTranslationPayload,
    ) -> Result<(), CommandError> {
        self.set_object_translation_override(&payload.object_id, payload.translation);
        if payload.object_id.starts_with("bone_") {
            self.invalidate_pose_cache();
        }
        Ok(())
    }

    fn validate_transform_set_rotation(
        &self,
        payload: &commands::TransformSetRotationPayload,
    ) -> ValidationResult {
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error("INVALID_OBJECT_ID", "invalid object id");
        };
        let Some(object) = self.scene_graph.get_object(&id) else {
            return ValidationResult::error("OBJECT_NOT_FOUND", "scene object not found");
        };
        if object.locked {
            return ValidationResult::error("OBJECT_LOCKED", "object is locked");
        }
        if !payload.rotation.iter().all(|value| value.is_finite()) {
            return ValidationResult::error("INVALID_ROTATION", "rotation must be finite");
        }
        let length = payload
            .rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if (length - 1.0).abs() > 1e-3 {
            ValidationResult::error("NON_UNIT_ROTATION", "rotation must be unit length")
        } else {
            ValidationResult::valid()
        }
    }

    fn transform_set_rotation(
        &mut self,
        payload: &commands::TransformSetRotationPayload,
    ) -> Result<(), CommandError> {
        self.set_object_rotation_override(&payload.object_id, payload.rotation);
        if payload.object_id.starts_with("bone_") {
            self.invalidate_pose_cache();
        }
        Ok(())
    }

    fn validate_transform_set_scale(
        &self,
        payload: &commands::TransformSetScalePayload,
    ) -> ValidationResult {
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error("INVALID_OBJECT_ID", "invalid object id");
        };
        let Some(object) = self.scene_graph.get_object(&id) else {
            return ValidationResult::error("OBJECT_NOT_FOUND", "scene object not found");
        };
        if object.locked {
            return ValidationResult::error("OBJECT_LOCKED", "object is locked");
        }
        if payload
            .scale
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            ValidationResult::valid()
        } else {
            ValidationResult::error("INVALID_SCALE", "scale must be finite and positive")
        }
    }

    fn transform_set_scale(
        &mut self,
        payload: &commands::TransformSetScalePayload,
    ) -> Result<(), CommandError> {
        self.set_object_scale_override(&payload.object_id, payload.scale);
        if payload.object_id.starts_with("bone_") {
            self.invalidate_pose_cache();
        }
        Ok(())
    }

    fn validate_transform_apply_delta(
        &self,
        payload: &commands::TransformApplyDeltaPayload,
    ) -> ValidationResult {
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error("INVALID_OBJECT_ID", "invalid object id");
        };
        let Some(object) = self.scene_graph.get_object(&id) else {
            return ValidationResult::error("OBJECT_NOT_FOUND", "scene object not found");
        };
        if object.locked {
            return ValidationResult::error("OBJECT_LOCKED", "object is locked");
        }
        if let Some(translation) = payload.delta_translation {
            if !translation.iter().all(|value| value.is_finite()) {
                return ValidationResult::error(
                    "INVALID_TRANSLATION",
                    "delta translation must be finite",
                );
            }
        }
        if let Some(rotation) = payload.delta_rotation_quat {
            if !rotation.iter().all(|value| value.is_finite()) {
                return ValidationResult::error(
                    "INVALID_ROTATION",
                    "delta rotation must be finite",
                );
            }
            let length = rotation
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            if (length - 1.0).abs() > 1e-3 {
                return ValidationResult::error(
                    "NON_UNIT_ROTATION",
                    "delta rotation must be unit length",
                );
            }
        }
        if let Some(scale) = payload.delta_scale {
            if !scale.iter().all(|value| value.is_finite()) {
                return ValidationResult::error("INVALID_SCALE", "delta scale must be finite");
            }
        }
        ValidationResult::valid()
    }

    fn transform_apply_delta(
        &mut self,
        payload: &commands::TransformApplyDeltaPayload,
    ) -> Result<(), CommandError> {
        let current = self
            .scene_graph
            .get_object(
                &SceneId::new(payload.object_id.clone())
                    .map_err(|e| CommandError::Validation(e.to_string()))?,
            )
            .map(|object| object.transform)
            .ok_or_else(|| CommandError::Validation("scene object not found".to_string()))?;
        if let Some(delta) = payload.delta_translation {
            let next = [
                current.translation[0] + delta[0],
                current.translation[1] + delta[1],
                current.translation[2] + delta[2],
            ];
            self.set_object_translation_override(&payload.object_id, next);
        }
        if let Some(delta) = payload.delta_rotation_quat {
            let next = (Quat::from_array(delta) * Quat::from_array(current.rotation))
                .normalize()
                .to_array();
            self.set_object_rotation_override(&payload.object_id, next);
        }
        if let Some(delta) = payload.delta_scale {
            let next = [
                (current.scale[0] + delta[0]).max(0.001),
                (current.scale[1] + delta[1]).max(0.001),
                (current.scale[2] + delta[2]).max(0.001),
            ];
            self.set_object_scale_override(&payload.object_id, next);
        }
        if payload.object_id.starts_with("bone_") {
            self.invalidate_pose_cache();
        }
        Ok(())
    }

    fn validate_transform_reset(
        &self,
        payload: &commands::TransformResetPayload,
    ) -> ValidationResult {
        let Ok(id) = SceneId::new(payload.object_id.clone()) else {
            return ValidationResult::error("INVALID_OBJECT_ID", "invalid object id");
        };
        if self.scene_graph.contains(&id) {
            ValidationResult::valid()
        } else {
            ValidationResult::error("OBJECT_NOT_FOUND", "scene object not found")
        }
    }

    fn transform_reset(
        &mut self,
        payload: &commands::TransformResetPayload,
    ) -> Result<(), CommandError> {
        self.clear_object_transform_override(&payload.object_id);
        if payload.object_id.starts_with("bone_") {
            self.invalidate_pose_cache();
        }
        Ok(())
    }

    fn validate_editor_set_mode(
        &self,
        payload: &commands::EditorSetModePayload,
    ) -> ValidationResult {
        if ui::EditorMode::ALL.contains(&payload.mode) {
            ValidationResult::valid()
        } else {
            ValidationResult::error("INVALID_MODE", "editor mode is not supported")
        }
    }

    fn set_editor_mode(
        &mut self,
        payload: &commands::EditorSetModePayload,
    ) -> Result<(), CommandError> {
        self.current_mode = payload.mode;
        self.config.editor.last_mode = payload.mode;
        self.panel_status.inspector_visible = ui::mode_layout(self.current_mode).show_inspector;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_sync_exports_camera_and_light_for_empty_app() {
        let mut app = App::new(StartupOptions::default()).unwrap();
        app.sync_scene_graph();
        let summary = app.scene_graph.get_scene_summary();
        assert_eq!(summary.object_count, 2);
        assert!(app
            .scene_graph
            .contains(&SceneId::new("camera_main").unwrap()));
        assert!(app
            .scene_graph
            .contains(&SceneId::new("light_key").unwrap()));
        let json = app.scene_graph_json().unwrap();
        assert!(json.contains("camera_main"));
    }

    #[test]
    fn selection_commands_validate_against_scene_graph_ids() {
        let mut app = App::new(StartupOptions::default()).unwrap();
        app.sync_scene_graph();
        let valid = app.validate_selection_set(&SelectionSetPayload {
            object_ids: vec!["camera_main".to_string()],
        });
        assert!(valid.valid);
        app.set_selection(&SelectionSetPayload {
            object_ids: vec!["camera_main".to_string()],
        })
        .unwrap();
        assert_eq!(
            app.scene_selection.active_object,
            Some(SceneId::new("camera_main").unwrap())
        );

        let invalid = app.validate_selection_set(&SelectionSetPayload {
            object_ids: vec!["missing".to_string()],
        });
        assert!(!invalid.valid);
    }

    #[test]
    fn set_mode_updates_current_mode_and_config() {
        let mut app = App::new(StartupOptions::default()).unwrap();
        app.set_editor_mode(&commands::EditorSetModePayload {
            mode: EditorMode::Object,
        })
        .unwrap();
        assert_eq!(app.current_mode, EditorMode::Object);
        assert_eq!(app.config.editor.last_mode, EditorMode::Object);
    }

    #[test]
    fn editor_mode_json_reports_current_and_available_modes() {
        let mut app = App::new(StartupOptions::default()).unwrap();
        app.current_mode = EditorMode::Material;
        let json = app.editor_mode_json().unwrap();
        assert!(json.contains("\"current_mode\": \"material\""));
        assert!(json.contains("\"available_modes\""));
        assert!(json.contains("\"ai\""));
    }
}

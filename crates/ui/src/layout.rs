//! Top-level layout for the editor screen.

use std::path::Path;

use assets::{AssetCategory, AssetMeta};
use avatar::{Expression, Slot};
use egui::{Color32, Context, RichText};
use glam::{EulerRot, Quat};

use crate::{
    components, icons, mode_layout, theme, AxisConstraint, EditorMode, EditorTool, LeftSection,
};

/// Logical-pixel width of the left side panel. Shared so the app can offset
/// the 3D scene viewport by the same amount.
pub const SIDE_PANEL_WIDTH: f32 = 232.0;

/// Logical-pixel width of the right-side Inspector panel (Stage 20).
pub const INSPECTOR_PANEL_WIDTH: f32 = 280.0;
pub const MODE_BAR_HEIGHT: f32 = 32.0;

/// Total horizontal space reserved by both side panels. Used by the
/// viewport-carve math so the 3D scene sits between them.
pub const RESERVED_PANEL_WIDTH: f32 = SIDE_PANEL_WIDTH + INSPECTOR_PANEL_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorCategory {
    Character,
    Hair,
    Outfit,
    Accessories,
    Animations,
    Export,
}

impl EditorCategory {
    pub const ALL: [Self; 6] = [
        Self::Character,
        Self::Hair,
        Self::Outfit,
        Self::Accessories,
        Self::Animations,
        Self::Export,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Hair => "Hair",
            Self::Outfit => "Outfit",
            Self::Accessories => "Accessories",
            Self::Animations => "Animations",
            Self::Export => "Export",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Character => icons::USER_CIRCLE,
            Self::Hair => icons::PALETTE,
            Self::Outfit => icons::SHIRT,
            Self::Accessories => icons::SNEAKER,
            Self::Animations => icons::SLIDERS,
            Self::Export => icons::FILE_ARROW_DOWN,
        }
    }
}

pub fn asset_categories_for(ec: EditorCategory) -> &'static [AssetCategory] {
    match ec {
        EditorCategory::Character => &[AssetCategory::Body, AssetCategory::Head],
        EditorCategory::Hair => &[AssetCategory::Hair],
        EditorCategory::Outfit => &[
            AssetCategory::Top,
            AssetCategory::Bottom,
            AssetCategory::Shoes,
        ],
        EditorCategory::Accessories => &[
            AssetCategory::Hat,
            AssetCategory::Glasses,
            AssetCategory::Accessory,
        ],
        EditorCategory::Animations => &[AssetCategory::Animation, AssetCategory::Pose],
        EditorCategory::Export => &[],
    }
}

/// One row in the "Equipped" panel.
#[derive(Debug, Clone)]
pub struct EquippedSlotRow {
    pub slot: Slot,
    pub display_name: String,
    /// User-facing sRGB tint applied to this slot's material. Drives the
    /// color swatch + picker.
    pub color_srgb: [f32; 3],
    /// If false, the swatch renders as a static read-only preview.
    pub supports_color: bool,
}

#[derive(Debug, Clone)]
pub struct SavedCharacterRow {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    /// `file://` URI of the per-character thumbnail PNG, if one exists.
    pub thumb_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidePanelAction {
    LoadAsset(String),
    LoadCustomGlb,
    ResetToCube,
    Undo,
    Redo,
    SaveCharacter,
    LoadCharacter(String),
    RefreshGallery,
    SetExportSize(u32),
    SetExportPortrait(bool),
    SetExportTransparent(bool),
    ExportPng,
    ExportGif,
    UnequipSlot(Slot),
    /// User dragged the color picker for `slot`. Value is sRGB.
    SetSlotColor(Slot, [f32; 3]),
    /// User picked an expression preset for the face.
    SetExpression(Expression),
    SetShowSkeleton(bool),
    SetDebugPose(bool),
    SetAnimationPlaying(bool),
    SetAnimationLooping(bool),
    SeekAnimation(f32),
    /// Stage 20: outliner row clicked. Dispatches `selection.set`.
    SelectObject(String),
    /// Stage 20: outliner "Clear" button. Dispatches `selection.clear`.
    DeselectAll,
    /// Stage 20: inspector filter text changed.
    SetInspectorFilter(String),
    /// Stage 20: inspector toggled the Visible flag on a mesh.
    SetSceneObjectVisible {
        id: String,
        visible: bool,
    },
    /// Stage 20: inspector toggled the Locked flag.
    SetSceneObjectLocked {
        id: String,
        locked: bool,
    },
    SetActiveTool(EditorTool),
    SetAxisConstraint(AxisConstraint),
    SetObjectTranslation {
        id: String,
        translation: [f32; 3],
    },
    SetObjectRotation {
        id: String,
        rotation_quat: [f32; 4],
    },
    SetObjectScale {
        id: String,
        scale: [f32; 3],
    },
    ResetObjectTransform(String),
    SetEditorMode(EditorMode),
}

#[derive(Debug, Default, Clone)]
pub struct SidePanelStatus {
    pub current_mesh_label: String,
    pub skeleton_label: String,
    pub has_skeleton: bool,
    pub show_skeleton: bool,
    pub is_skinned: bool,
    pub debug_pose: bool,
    pub animation_label: String,
    pub has_animation: bool,
    pub animation_playing: bool,
    pub animation_looping: bool,
    pub animation_time: f32,
    pub animation_duration: f32,
    pub skeleton_warnings: Vec<String>,
    pub last_error: Option<String>,
    pub gallery_rows: Vec<SavedCharacterRow>,
    pub can_save_character: bool,
    pub last_save_label: Option<String>,
    pub export_size: u32,
    pub export_portrait: bool,
    pub export_transparent: bool,
    pub last_export_label: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// True when a body asset is equipped; false when static fallback is active.
    pub avatar_mode: bool,
    /// Sorted in [`Slot::ALL`] order; the app fills this before each frame.
    pub equipped_rows: Vec<EquippedSlotRow>,
    /// True when an avatar with a face quad is in scene.
    pub has_face: bool,
    pub current_expression: Expression,
    pub show_diagnostics: bool,
    pub diagnostics: DiagnosticsStatus,
    /// Stage 20 inspector data; populated each frame from the SceneGraph.
    pub inspector: crate::inspector::InspectorStatus,
    /// Reserved for a future collapse toggle; today the panel is always shown.
    pub inspector_visible: bool,
    pub active_tool: EditorTool,
    pub axis_constraint: AxisConstraint,
    pub show_gizmo: bool,
}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticsStatus {
    pub current_fps: f32,
    pub average_fps: f32,
    pub last_frame_ms: f32,
    pub p95_frame_ms: f32,
    pub total_frame_ms: f32,
    pub egui_ms: f32,
    pub tessellate_ms: f32,
    pub pose_ms: f32,
    pub scene_build_ms: f32,
    pub render_submit_ms: f32,
    /// Average GPU frame time over the rolling window (when supported).
    pub gpu_ms: Option<f32>,
    pub instance_count: u32,
    pub sample_count: u32,
}

pub fn draw_side_panel(
    ctx: &Context,
    selected: &mut EditorCategory,
    current_mode: EditorMode,
    status: &SidePanelStatus,
    assets: &[AssetMeta],
) -> Option<SidePanelAction> {
    let mut action: Option<SidePanelAction> = None;
    let layout = mode_layout(current_mode);

    egui::SidePanel::left("avatar_studio.side_panel")
        .resizable(false)
        .exact_width(SIDE_PANEL_WIDTH)
        .frame(egui::Frame::none().fill(theme::TOKENS.bg))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("side-panel-root")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(theme::space::S2);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Avatar Studio")
                                .heading()
                                .color(theme::TOKENS.text),
                        );
                        ui.label(RichText::new(icons::SPARKLE).color(theme::TOKENS.accent));
                    });
                    ui.add_space(theme::space::S2);

                    if !current_mode.implemented() {
                        components::section(ui, current_mode.label(), current_mode.icon(), |ui| {
                            let stage = current_mode.coming_soon_stage().unwrap_or(0);
                            components::empty_state(
                                ui,
                                current_mode.icon(),
                                &format!(
                                    "{} editor - coming in Stage {}",
                                    current_mode.label(),
                                    stage
                                ),
                            );
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Categories) {
                        components::section(ui, "Categories", icons::SLIDERS, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for cat in EditorCategory::ALL {
                                    if components::tab(
                                        ui,
                                        cat.icon(),
                                        cat.label(),
                                        *selected == cat,
                                    )
                                    .clicked()
                                    {
                                        *selected = cat;
                                    }
                                }
                            });
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::AssetList) {
                        components::section(ui, selected.label(), selected.icon(), |ui| {
                            components::subheader(ui, &format!("{} assets", assets.len()));
                            ui.add_space(theme::space::S1);
                            egui::ScrollArea::vertical()
                                .id_salt("assets-list")
                                .max_height(144.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    if assets.is_empty() {
                                        components::empty_state(
                                            ui,
                                            icons::IMAGE,
                                            "No assets in this category yet.",
                                        );
                                    } else {
                                        for asset in assets {
                                            let thumb_uri =
                                                thumbnail_uri(asset.thumbnail.as_deref());
                                            if components::asset_row(
                                                ui,
                                                thumb_uri.as_deref(),
                                                category_icon(asset.category),
                                                &asset.display_name,
                                                false,
                                            )
                                            .clicked()
                                            {
                                                action = Some(SidePanelAction::LoadAsset(
                                                    asset.id.clone(),
                                                ));
                                            }
                                        }
                                    }
                                });
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Equipped) {
                        components::section(ui, "Equipped", icons::USER, |ui| {
                            if !status.avatar_mode {
                                components::empty_state(
                                    ui,
                                    icons::USER_CIRCLE,
                                    "Load a body to start equipping.",
                                );
                            } else if status.equipped_rows.is_empty() {
                                components::empty_state(ui, icons::USER, "Nothing equipped.");
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("equipped-list")
                                    .max_height(132.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for row in &status.equipped_rows {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(row.slot.label())
                                                        .strong()
                                                        .color(theme::TOKENS.text),
                                                );
                                                ui.weak(&row.display_name);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let unequip_enabled =
                                                            row.slot != Slot::Body;
                                                        if ui
                                                            .add_enabled_ui(unequip_enabled, |ui| {
                                                                components::icon_button(
                                                                    ui,
                                                                    icons::X,
                                                                    "Unequip slot",
                                                                )
                                                            })
                                                            .inner
                                                            .clicked()
                                                        {
                                                            action =
                                                                Some(SidePanelAction::UnequipSlot(
                                                                    row.slot,
                                                                ));
                                                        }
                                                        let color = Color32::from_rgb(
                                                            (row.color_srgb[0] * 255.0) as u8,
                                                            (row.color_srgb[1] * 255.0) as u8,
                                                            (row.color_srgb[2] * 255.0) as u8,
                                                        );
                                                        if row.supports_color {
                                                            let mut srgb = row.color_srgb;
                                                            let resp =
                                                                ui.color_edit_button_rgb(&mut srgb);
                                                            if resp.changed() {
                                                                action = Some(
                                                                    SidePanelAction::SetSlotColor(
                                                                        row.slot, srgb,
                                                                    ),
                                                                );
                                                            }
                                                        } else {
                                                            components::swatch(ui, color, false)
                                                                .on_hover_text("Color locked");
                                                        }
                                                    },
                                                );
                                            });
                                        }
                                    });
                            }
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Gallery) {
                        components::section(ui, "Gallery", icons::IMAGE, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled_ui(status.can_save_character, |ui| {
                                        components::primary_button(ui, icons::FLOPPY_DISK, "Save")
                                    })
                                    .inner
                                    .clicked()
                                {
                                    action = Some(SidePanelAction::SaveCharacter);
                                }
                                if components::secondary_button(
                                    ui,
                                    icons::ARROW_CLOCKWISE,
                                    "Refresh",
                                )
                                .clicked()
                                {
                                    action = Some(SidePanelAction::RefreshGallery);
                                }
                            });
                            if let Some(label) = &status.last_save_label {
                                ui.label(
                                    RichText::new(label)
                                        .small()
                                        .color(theme::TOKENS.text_subtle),
                                );
                            }
                            if status.gallery_rows.is_empty() {
                                components::empty_state(
                                    ui,
                                    icons::IMAGE,
                                    "No saved characters yet.",
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("gallery-list")
                                    .max_height(104.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for row in &status.gallery_rows {
                                            if components::gallery_row(
                                                ui,
                                                row.thumb_uri.as_deref(),
                                                &row.name,
                                                &row.updated_at,
                                            )
                                            .clicked()
                                            {
                                                action = Some(SidePanelAction::LoadCharacter(
                                                    row.id.clone(),
                                                ));
                                            }
                                        }
                                    });
                            }
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Model) {
                        components::section(ui, "Model", icons::CUBE, |ui| {
                            ui.label(
                                RichText::new(if status.current_mesh_label.is_empty() {
                                    "cube (placeholder)"
                                } else {
                                    status.current_mesh_label.as_str()
                                })
                                .color(theme::TOKENS.text_muted),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled_ui(status.can_undo, |ui| {
                                        components::secondary_button(
                                            ui,
                                            icons::ARROW_COUNTER_CLOCKWISE,
                                            "Undo",
                                        )
                                    })
                                    .inner
                                    .clicked()
                                {
                                    action = Some(SidePanelAction::Undo);
                                }
                                if ui
                                    .add_enabled_ui(status.can_redo, |ui| {
                                        components::secondary_button(
                                            ui,
                                            icons::ARROW_CLOCKWISE,
                                            "Redo",
                                        )
                                    })
                                    .inner
                                    .clicked()
                                {
                                    action = Some(SidePanelAction::Redo);
                                }
                                if components::secondary_button(ui, icons::FOLDER_OPEN, "Load GLB")
                                    .clicked()
                                {
                                    action = Some(SidePanelAction::LoadCustomGlb);
                                }
                                if components::secondary_button(
                                    ui,
                                    icons::ARROW_COUNTER_CLOCKWISE,
                                    "Reset",
                                )
                                .clicked()
                                {
                                    action = Some(SidePanelAction::ResetToCube);
                                }
                            });
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Export) {
                        components::section(ui, "Export", icons::FILE_ARROW_DOWN, |ui| {
                            ui.horizontal(|ui| {
                                let mut size = status.export_size.max(512);
                                egui::ComboBox::from_id_salt("png-size")
                                    .selected_text(size.to_string())
                                    .show_ui(ui, |ui| {
                                        for candidate in [512, 1024, 2048] {
                                            if ui
                                                .selectable_value(
                                                    &mut size,
                                                    candidate,
                                                    candidate.to_string(),
                                                )
                                                .changed()
                                            {
                                                action =
                                                    Some(SidePanelAction::SetExportSize(candidate));
                                            }
                                        }
                                    });
                                let mut portrait = status.export_portrait;
                                egui::ComboBox::from_id_salt("png-view")
                                    .selected_text(if portrait { "Portrait" } else { "Full body" })
                                    .show_ui(ui, |ui| {
                                        if ui
                                            .selectable_value(&mut portrait, false, "Full body")
                                            .changed()
                                        {
                                            action =
                                                Some(SidePanelAction::SetExportPortrait(false));
                                        }
                                        if ui
                                            .selectable_value(&mut portrait, true, "Portrait")
                                            .changed()
                                        {
                                            action = Some(SidePanelAction::SetExportPortrait(true));
                                        }
                                    });
                            });
                            let mut transparent = status.export_transparent;
                            if ui
                                .checkbox(&mut transparent, "Transparent background")
                                .changed()
                            {
                                action = Some(SidePanelAction::SetExportTransparent(transparent));
                            }
                            if components::primary_button(ui, icons::FILE_ARROW_DOWN, "Export PNG")
                                .clicked()
                            {
                                action = Some(SidePanelAction::ExportPng);
                            }
                            if components::secondary_button(
                                ui,
                                icons::FILE_ARROW_DOWN,
                                "Export GIF",
                            )
                            .clicked()
                            {
                                action = Some(SidePanelAction::ExportGif);
                            }
                            if let Some(label) = &status.last_export_label {
                                ui.label(
                                    RichText::new(label)
                                        .small()
                                        .color(theme::TOKENS.text_subtle),
                                );
                            }
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Animation) {
                        components::section(ui, "Animation", icons::SLIDERS, |ui| {
                            ui.label(
                                RichText::new(if status.animation_label.is_empty() {
                                    "No animation"
                                } else {
                                    status.animation_label.as_str()
                                })
                                .color(theme::TOKENS.text_muted),
                            );
                            if !status.has_animation {
                                components::empty_state(ui, icons::PLAY, "No animation loaded.");
                            } else {
                                let mut playing = status.animation_playing;
                                if ui.checkbox(&mut playing, "Play").changed() {
                                    action = Some(SidePanelAction::SetAnimationPlaying(playing));
                                }
                                let mut looping = status.animation_looping;
                                if ui.checkbox(&mut looping, "Loop").changed() {
                                    action = Some(SidePanelAction::SetAnimationLooping(looping));
                                }
                                let mut time = status.animation_time;
                                let duration = status.animation_duration.max(0.0);
                                let slider = egui::Slider::new(&mut time, 0.0..=duration)
                                    .show_value(true)
                                    .text("Time");
                                if ui.add(slider).changed() {
                                    action = Some(SidePanelAction::SeekAnimation(time));
                                }
                            }
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Expression) {
                        components::section(ui, "Expression", icons::SMILEY, |ui| {
                            if !status.has_face {
                                components::empty_state(ui, icons::SMILEY, "No face loaded.");
                            }
                            ui.horizontal_wrapped(|ui| {
                                for expr in Expression::ALL {
                                    let selected = status.current_expression == expr;
                                    let resp = ui.add_enabled(
                                        status.has_face,
                                        egui::SelectableLabel::new(selected, expr.label()),
                                    );
                                    if resp.clicked() {
                                        action = Some(SidePanelAction::SetExpression(expr));
                                    }
                                }
                            });
                        });
                        ui.add_space(theme::space::S2);
                    }

                    if layout.has_section(LeftSection::Skinning) {
                        components::section(ui, "Skinning + Bones", icons::SPARKLE, |ui| {
                            ui.label(
                                RichText::new(if status.is_skinned {
                                    "Skinned mesh"
                                } else {
                                    "Static mesh"
                                })
                                .color(theme::TOKENS.text_muted),
                            );
                            let mut debug_pose = status.debug_pose;
                            let pose_response = ui.add_enabled(
                                status.is_skinned && status.has_skeleton && !status.has_animation,
                                egui::Checkbox::new(&mut debug_pose, "Debug pose"),
                            );
                            if pose_response.changed() {
                                action = Some(SidePanelAction::SetDebugPose(debug_pose));
                            }
                            ui.add_space(theme::space::S2);
                            ui.label(
                                RichText::new(if status.skeleton_label.is_empty() {
                                    "No skeleton"
                                } else {
                                    status.skeleton_label.as_str()
                                })
                                .color(theme::TOKENS.text_muted),
                            );
                            let mut show_skeleton = status.show_skeleton;
                            let response = ui.add_enabled(
                                status.has_skeleton,
                                egui::Checkbox::new(&mut show_skeleton, "Show skeleton"),
                            );
                            if response.changed() {
                                action = Some(SidePanelAction::SetShowSkeleton(show_skeleton));
                            }
                            for warning in status.skeleton_warnings.iter().take(3) {
                                ui.colored_label(theme::TOKENS.warning, warning);
                            }
                        });
                    }

                    if status.show_diagnostics && layout.has_section(LeftSection::Diagnostics) {
                        ui.add_space(theme::space::S2);
                        components::section(ui, "Diagnostics", icons::SLIDERS, |ui| {
                            diagnostics_row(
                                ui,
                                "FPS",
                                &format!(
                                    "{:.1} avg / {:.1} now",
                                    status.diagnostics.average_fps, status.diagnostics.current_fps
                                ),
                            );
                            diagnostics_row(
                                ui,
                                "Frame",
                                &format!(
                                    "{:.2} ms last / {:.2} ms p95",
                                    status.diagnostics.last_frame_ms,
                                    status.diagnostics.p95_frame_ms
                                ),
                            );
                            diagnostics_row(
                                ui,
                                "Passes",
                                &format!(
                                    "egui {:.2}, tess {:.2}, pose {:.2}",
                                    status.diagnostics.egui_ms,
                                    status.diagnostics.tessellate_ms,
                                    status.diagnostics.pose_ms
                                ),
                            );
                            diagnostics_row(
                                ui,
                                "Render",
                                &format!(
                                    "build {:.2}, submit {:.2}",
                                    status.diagnostics.scene_build_ms,
                                    status.diagnostics.render_submit_ms
                                ),
                            );
                            diagnostics_row(
                                ui,
                                "GPU",
                                &match status.diagnostics.gpu_ms {
                                    Some(ms) => format!("{ms:.2} ms"),
                                    None => "unavailable".to_string(),
                                },
                            );
                            diagnostics_row(
                                ui,
                                "Scene",
                                &format!(
                                    "{} inst, {} samples",
                                    status.diagnostics.instance_count,
                                    status.diagnostics.sample_count
                                ),
                            );
                            diagnostics_row(
                                ui,
                                "Mode",
                                if status.avatar_mode {
                                    if status.is_skinned {
                                        "avatar / skinned"
                                    } else {
                                        "avatar / static"
                                    }
                                } else {
                                    "static"
                                },
                            );
                            diagnostics_row(
                                ui,
                                "State",
                                &format!(
                                    "{} skeleton, {} animation",
                                    if status.show_skeleton { "show" } else { "hide" },
                                    if status.animation_playing {
                                        "play"
                                    } else {
                                        "pause"
                                    }
                                ),
                            );
                        });
                    }

                    if let Some(err) = &status.last_error {
                        ui.add_space(theme::space::S2);
                        components::section(ui, "Attention", icons::WARNING, |ui| {
                            ui.colored_label(theme::TOKENS.error, err);
                        });
                    }

                    ui.add_space(theme::space::S2);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Phase 13 - UI/UX polish")
                                .small()
                                .color(theme::TOKENS.text_subtle),
                        );
                    });
                    ui.add_space(theme::space::S2);
                });
        });

    action
}

pub fn draw_mode_bar(ctx: &Context, current_mode: EditorMode) -> Option<SidePanelAction> {
    let mut action = None;
    egui::TopBottomPanel::top("avatar_studio.mode_bar")
        .resizable(false)
        .exact_height(MODE_BAR_HEIGHT)
        .frame(egui::Frame::none().fill(theme::TOKENS.bg))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for mode in EditorMode::ALL {
                    let label = if mode.implemented() {
                        mode.label().to_string()
                    } else {
                        format!("{} {}", mode.label(), icons::WARNING)
                    };
                    let response = components::tab(ui, mode.icon(), &label, current_mode == mode);
                    let response = response.on_hover_text("Mode changes are not undoable");
                    if response.clicked() {
                        action = Some(SidePanelAction::SetEditorMode(mode));
                    }
                }
            });
        });
    action
}

fn category_icon(category: AssetCategory) -> &'static str {
    match category {
        AssetCategory::Body | AssetCategory::Head => icons::USER,
        AssetCategory::Hair | AssetCategory::Material => icons::PALETTE,
        AssetCategory::Top | AssetCategory::Bottom => icons::SHIRT,
        AssetCategory::Shoes
        | AssetCategory::Hat
        | AssetCategory::Glasses
        | AssetCategory::Accessory => icons::SNEAKER,
        AssetCategory::Animation | AssetCategory::Pose => icons::SLIDERS,
        AssetCategory::Background => icons::IMAGE,
    }
}

fn thumbnail_uri(thumbnail: Option<&str>) -> Option<String> {
    let thumb = thumbnail?;
    let path = Path::new("assets").join("processed").join(thumb);
    let abs = std::env::current_dir().ok()?.join(path);
    if !abs.exists() {
        return None;
    }
    Some(format!(
        "file:///{}",
        abs.display().to_string().replace('\\', "/")
    ))
}

fn diagnostics_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .small()
                .color(theme::TOKENS.text_subtle),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).small().color(theme::TOKENS.text_muted));
        });
    });
}

/// Stage 20 inspector right-side panel. Mirrors the left panel's egui
/// idiom and returns a single `SidePanelAction` per frame.
pub fn draw_inspector_panel(ctx: &Context, status: &SidePanelStatus) -> Option<SidePanelAction> {
    if !status.inspector_visible {
        return None;
    }
    let mut action: Option<SidePanelAction> = None;
    egui::SidePanel::right("avatar_studio.inspector_panel")
        .resizable(false)
        .exact_width(INSPECTOR_PANEL_WIDTH)
        .frame(egui::Frame::none().fill(theme::TOKENS.bg))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("inspector-panel-root")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(theme::space::S2);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Inspector")
                                .heading()
                                .color(theme::TOKENS.text),
                        );
                        ui.label(RichText::new(icons::EYE).color(theme::TOKENS.accent));
                    });
                    ui.add_space(theme::space::S1);
                    ui.horizontal_wrapped(|ui| {
                        for tool in EditorTool::ALL {
                            if components::tab(ui, "", tool.label(), status.active_tool == tool)
                                .clicked()
                            {
                                action = Some(SidePanelAction::SetActiveTool(tool));
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Axis: {}", status.axis_constraint.label()))
                                .small()
                                .color(theme::TOKENS.text_subtle),
                        );
                        for axis in [
                            AxisConstraint::None,
                            AxisConstraint::X,
                            AxisConstraint::Y,
                            AxisConstraint::Z,
                        ] {
                            if components::tab(ui, "", axis.label(), status.axis_constraint == axis)
                                .clicked()
                            {
                                action = Some(SidePanelAction::SetAxisConstraint(axis));
                            }
                        }
                    });
                    ui.add_space(theme::space::S2);

                    components::section(ui, "Outliner", icons::SLIDERS, |ui| {
                        let mut filter = status.inspector.filter.clone();
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut filter)
                                .hint_text("Filter by name or id"),
                        );
                        if response.changed() && filter != status.inspector.filter {
                            action = Some(SidePanelAction::SetInspectorFilter(filter.clone()));
                        }
                        ui.add_space(theme::space::S1);

                        if status.inspector.objects.is_empty() {
                            components::empty_state(
                                ui,
                                icons::IMAGE,
                                "No objects match this filter.",
                            );
                        } else {
                            components::subheader(
                                ui,
                                &format!("{} objects", status.inspector.objects.len()),
                            );
                            ui.add_space(theme::space::S1);
                            egui::ScrollArea::vertical()
                                .id_salt("inspector-outliner")
                                .max_height(220.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for row in &status.inspector.objects {
                                        let active = status.inspector.active.as_deref()
                                            == Some(row.id.as_str());
                                        let label = format!(
                                            "{}{}  [{}]",
                                            "  ".repeat(row.depth as usize),
                                            row.name,
                                            crate::inspector::kind_label(row.kind),
                                        );
                                        if components::asset_row(
                                            ui,
                                            None,
                                            icon_for(row.kind),
                                            &label,
                                            active,
                                        )
                                        .clicked()
                                        {
                                            action =
                                                Some(SidePanelAction::SelectObject(row.id.clone()));
                                        }
                                    }
                                });
                            ui.add_space(theme::space::S1);
                            ui.horizontal(|ui| {
                                if status.inspector.active.is_some()
                                    && components::secondary_button(ui, icons::X, "Clear selection")
                                        .clicked()
                                {
                                    action = Some(SidePanelAction::DeselectAll);
                                }
                            });
                        }
                    });
                    ui.add_space(theme::space::S2);

                    components::section(ui, "Details", icons::USER_CIRCLE, |ui| {
                        match status.inspector.detail.as_ref() {
                            None => {
                                components::empty_state(ui, icons::EYE, "Nothing selected.");
                            }
                            Some(detail) => {
                                if let Some(a) = draw_inspector_detail(
                                    ui,
                                    detail,
                                    status.active_tool,
                                    status.axis_constraint,
                                ) {
                                    action = Some(a);
                                }
                            }
                        }
                    });
                    ui.add_space(theme::space::S2);
                });
        });
    action
}

fn icon_for(kind: scene::SceneObjectKind) -> &'static str {
    use scene::SceneObjectKind as K;
    match kind {
        K::Avatar => icons::USER_CIRCLE,
        K::MeshInstance | K::SkinnedMeshInstance => icons::CUBE,
        K::Skeleton => icons::SLIDERS,
        K::Bone => icons::SPARKLE,
        K::Material => icons::PALETTE,
        K::AnimationClip => icons::PLAY,
        K::Camera => icons::EYE,
        K::Light => icons::SPARKLE,
        K::Accessory => icons::SNEAKER,
        K::AttachmentPoint => icons::SPARKLE,
        K::Empty => icons::CUBE,
        K::Constraint => icons::SPARKLE,
        K::Pose => icons::SMILEY,
        K::BlendshapeSet => icons::SMILEY,
    }
}

fn draw_inspector_detail(
    ui: &mut egui::Ui,
    detail: &crate::inspector::InspectorDetail,
    active_tool: EditorTool,
    axis_constraint: AxisConstraint,
) -> Option<SidePanelAction> {
    use crate::inspector::InspectorDetail;
    let mut action: Option<SidePanelAction> = None;
    match detail {
        InspectorDetail::Avatar(a) => {
            field_row(ui, "ID", &a.id);
            field_row(ui, "Name", &a.name);
            if let Some(body) = &a.body {
                field_row(ui, "Body", body);
            }
            field_row(ui, "Slots", &a.slot_count.to_string());
            if let Some(s) = &a.skeleton {
                field_row(ui, "Skeleton", s);
            }
            if let Some(clip) = &a.current_clip {
                field_row(ui, "Animation", clip);
            }
            if let Some(expr) = &a.expression {
                field_row(ui, "Expression", expr);
            }
            action = transform_section(
                ui,
                &a.id,
                a.translation,
                a.rotation,
                a.scale,
                a.locked,
                active_tool,
                axis_constraint,
            );
        }
        InspectorDetail::MeshInstance(m) => {
            field_row(ui, "ID", &m.id);
            field_row(ui, "Name", &m.name);
            if let Some(asset) = &m.asset_id {
                field_row(ui, "Asset", asset);
            }
            if let Some(parent) = &m.parent {
                field_row(ui, "Parent", parent);
            }
            field_row(ui, "Skinned", if m.skinned { "yes" } else { "no" });
            action = transform_section(
                ui,
                &m.id,
                m.translation,
                m.rotation,
                m.scale,
                m.locked,
                active_tool,
                axis_constraint,
            );
            let mut visible = m.visible;
            if ui.checkbox(&mut visible, "Visible").changed() {
                action = Some(SidePanelAction::SetSceneObjectVisible {
                    id: m.id.clone(),
                    visible,
                });
            }
            let mut locked = m.locked;
            if ui.checkbox(&mut locked, "Locked").changed() {
                action = Some(SidePanelAction::SetSceneObjectLocked {
                    id: m.id.clone(),
                    locked,
                });
            }
        }
        InspectorDetail::Skeleton(s) => {
            field_row(ui, "ID", &s.id);
            field_row(ui, "Name", &s.name);
            field_row(ui, "Bones", &s.bone_count.to_string());
            if let Some(root) = &s.root_bone {
                field_row(ui, "Root", root);
            }
            action = transform_section(
                ui,
                &s.id,
                s.translation,
                s.rotation,
                s.scale,
                s.locked,
                active_tool,
                axis_constraint,
            );
        }
        InspectorDetail::Bone(b) => {
            field_row(ui, "ID", &b.id);
            field_row(ui, "Name", &b.name);
            if let Some(parent) = &b.parent {
                field_row(ui, "Parent", parent);
            }
            action = transform_section(
                ui,
                &b.id,
                b.translation,
                b.rotation,
                b.scale,
                b.locked,
                active_tool,
                axis_constraint,
            );
        }
        InspectorDetail::Material(m) => {
            field_row(ui, "ID", &m.id);
            field_row(ui, "Name", &m.name);
            if let Some(c) = m.base_color {
                let color = Color32::from_rgb(
                    (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                );
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Base color")
                            .small()
                            .color(theme::TOKENS.text_subtle),
                    );
                    components::swatch(ui, color, false);
                });
            }
        }
        InspectorDetail::AnimationClip(c) => {
            field_row(ui, "ID", &c.id);
            field_row(ui, "Name", &c.name);
            if let Some(fps) = c.fps {
                field_row(ui, "FPS", &format!("{fps:.1}"));
            }
            if let Some(d) = c.duration_frames {
                field_row(ui, "Duration", &format!("{d} frames"));
            }
            if let Some(l) = c.looping {
                field_row(ui, "Looping", if l { "yes" } else { "no" });
            }
        }
        InspectorDetail::Camera(b) | InspectorDetail::Light(b) | InspectorDetail::Other(b) => {
            field_row(ui, "ID", &b.id);
            field_row(ui, "Name", &b.name);
            field_row(ui, "Kind", &b.kind);
            if matches!(detail, InspectorDetail::Other(_)) {
                action = transform_section(
                    ui,
                    &b.id,
                    b.translation,
                    b.rotation,
                    b.scale,
                    b.locked,
                    active_tool,
                    axis_constraint,
                );
            } else {
                field_row(ui, "Translation", &format_vec3(b.translation));
                field_row(ui, "Rotation", &format_vec4(b.rotation));
                field_row(ui, "Scale", &format_vec3(b.scale));
            }
            field_row(ui, "Visible", if b.visible { "yes" } else { "no" });
            field_row(ui, "Locked", if b.locked { "yes" } else { "no" });
        }
    }
    action
}

#[allow(clippy::too_many_arguments)]
fn transform_section(
    ui: &mut egui::Ui,
    id: &str,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
    locked: bool,
    active_tool: EditorTool,
    axis_constraint: AxisConstraint,
) -> Option<SidePanelAction> {
    let mut action = None;
    let rotation_euler = quat_to_euler_deg(rotation);
    let axis_for_translate = if matches!(active_tool, EditorTool::Move) {
        axis_constraint
    } else {
        AxisConstraint::None
    };
    let axis_for_rotate = if matches!(active_tool, EditorTool::Rotate) {
        axis_constraint
    } else {
        AxisConstraint::None
    };
    let axis_for_scale = if matches!(active_tool, EditorTool::Scale) {
        axis_constraint
    } else {
        AxisConstraint::None
    };

    let mut translation = translation;
    if vec3_drag_row(
        ui,
        "Translation",
        &mut translation,
        axis_for_translate,
        locked,
    )
    .changed()
    {
        action = Some(SidePanelAction::SetObjectTranslation {
            id: id.to_string(),
            translation,
        });
    }
    let mut rotation_deg = rotation_euler;
    if vec3_drag_row(ui, "Rotation", &mut rotation_deg, axis_for_rotate, locked).changed() {
        action = Some(SidePanelAction::SetObjectRotation {
            id: id.to_string(),
            rotation_quat: euler_deg_to_quat(rotation_deg),
        });
    }
    let mut scale = scale;
    if vec3_drag_row(ui, "Scale", &mut scale, axis_for_scale, locked)
        .on_hover_text("Scale values stay positive")
        .changed()
    {
        let clamped = [
            scale[0].max(0.001),
            scale[1].max(0.001),
            scale[2].max(0.001),
        ];
        action = Some(SidePanelAction::SetObjectScale {
            id: id.to_string(),
            scale: clamped,
        });
    }
    if locked {
        ui.label(
            RichText::new("Transform editing disabled while locked")
                .small()
                .color(theme::TOKENS.warning),
        );
    } else if components::secondary_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Reset transform")
        .clicked()
    {
        action = Some(SidePanelAction::ResetObjectTransform(id.to_string()));
    }
    action
}

fn field_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .small()
                .color(theme::TOKENS.text_subtle),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).small().color(theme::TOKENS.text));
        });
    });
}

fn format_vec3(v: [f32; 3]) -> String {
    format!("{:.3}, {:.3}, {:.3}", v[0], v[1], v[2])
}

fn format_vec4(v: [f32; 4]) -> String {
    format!("{:.3}, {:.3}, {:.3}, {:.3}", v[0], v[1], v[2], v[3])
}

fn vec3_drag_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut [f32; 3],
    axis: AxisConstraint,
    locked: bool,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .small()
                .color(theme::TOKENS.text_subtle),
        );
        let mut response = ui.label("");
        for (index, axis_label) in ["X", "Y", "Z"].into_iter().enumerate() {
            let enabled = !locked && axis_channel_enabled(axis, index);
            ui.label(
                RichText::new(axis_label)
                    .small()
                    .color(theme::TOKENS.text_subtle),
            );
            let drag = egui::DragValue::new(&mut value[index])
                .speed(0.01)
                .range(-100.0..=100.0)
                .fixed_decimals(3);
            let drag_response = ui.add_enabled(enabled, drag);
            response |= drag_response;
        }
        response
    })
    .inner
}

fn axis_channel_enabled(axis: AxisConstraint, index: usize) -> bool {
    match axis {
        AxisConstraint::None => true,
        AxisConstraint::X => index == 0,
        AxisConstraint::Y => index == 1,
        AxisConstraint::Z => index == 2,
    }
}

fn quat_to_euler_deg(rotation: [f32; 4]) -> [f32; 3] {
    let quat = Quat::from_array(rotation).normalize();
    let (x, y, z) = quat.to_euler(EulerRot::XYZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

fn euler_deg_to_quat(rotation_deg: [f32; 3]) -> [f32; 4] {
    Quat::from_euler(
        EulerRot::XYZ,
        rotation_deg[0].to_radians(),
        rotation_deg[1].to_radians(),
        rotation_deg[2].to_radians(),
    )
    .normalize()
    .to_array()
}

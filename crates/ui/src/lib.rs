//! UI crate. Pure egui — no rendering backend assumptions; the desktop app
//! wires this into `egui-wgpu`.

pub mod components;
pub mod editor;
pub mod export;
pub mod fonts;
pub mod gallery;
pub mod icons;
pub mod inspector;
pub mod layout;
pub mod modes;
pub mod theme;
pub mod tools;
pub mod widgets;

pub use avatar::{Expression, Slot};
pub use inspector::{
    build_inspector_status, kind_label, InspectorAvatar, InspectorBasic, InspectorBone,
    InspectorClip, InspectorDetail, InspectorMaterial, InspectorMesh, InspectorObjectRow,
    InspectorSkeleton, InspectorStatus,
};
pub use layout::{
    asset_categories_for, draw_inspector_panel, draw_mode_bar, draw_side_panel, EditorCategory,
    EquippedSlotRow, SavedCharacterRow, SidePanelAction, SidePanelStatus, INSPECTOR_PANEL_WIDTH,
    MODE_BAR_HEIGHT, RESERVED_PANEL_WIDTH, SIDE_PANEL_WIDTH,
};
pub use modes::{mode_layout, EditorMode, LeftSection, ModeLayout};
pub use tools::{AxisConstraint, EditorTool, ToolState};

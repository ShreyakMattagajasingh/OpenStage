//! Catppuccin Mocha theme tokens for Avatar Studio.

use egui::{style::WidgetVisuals, Color32, Margin, Rounding, Stroke, TextStyle, Vec2, Visuals};

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub bg: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_subtle: Color32,
    pub text_on_accent: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_active: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

pub mod space {
    pub const S1: f32 = 4.0;
    pub const S2: f32 = 8.0;
    pub const S3: f32 = 12.0;
    pub const S4: f32 = 16.0;
    pub const S5: f32 = 24.0;
}

pub mod radius {
    pub const SM: f32 = 4.0;
    pub const MD: f32 = 6.0;
    pub const LG: f32 = 8.0;
}

pub const TOKENS: Tokens = Tokens {
    bg: Color32::from_rgb(0x1e, 0x1e, 0x2e),
    panel: Color32::from_rgb(0x24, 0x25, 0x36),
    surface: Color32::from_rgb(0x2d, 0x2e, 0x42),
    surface_hover: Color32::from_rgb(0x38, 0x3a, 0x50),
    surface_active: Color32::from_rgb(0x45, 0x47, 0x5a),
    border: Color32::from_rgb(0x43, 0x45, 0x58),
    divider: Color32::from_rgb(0x31, 0x32, 0x44),
    text: Color32::from_rgb(0xcd, 0xd6, 0xf4),
    text_muted: Color32::from_rgb(0xa6, 0xad, 0xc8),
    text_subtle: Color32::from_rgb(0x7f, 0x84, 0x9c),
    text_on_accent: Color32::from_rgb(0x1e, 0x1e, 0x2e),
    accent: Color32::from_rgb(0x89, 0xb4, 0xfa),
    accent_hover: Color32::from_rgb(0xa5, 0xc4, 0xfb),
    accent_active: Color32::from_rgb(0xb4, 0xbe, 0xfe),
    success: Color32::from_rgb(0xa6, 0xe3, 0xa1),
    warning: Color32::from_rgb(0xf9, 0xe2, 0xaf),
    error: Color32::from_rgb(0xf3, 0x8b, 0xa8),
};

pub fn apply(ctx: &egui::Context) {
    let t = &TOKENS;
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(t.text);
    visuals.panel_fill = t.bg;
    visuals.window_fill = t.panel;
    visuals.faint_bg_color = t.panel;
    visuals.extreme_bg_color = t.bg;
    visuals.code_bg_color = t.panel;
    visuals.hyperlink_color = t.accent;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0x89, 0xb4, 0xfa, 0x33);
    visuals.selection.stroke = Stroke::new(1.0, t.accent);
    visuals.widgets.noninteractive = WidgetVisuals {
        bg_fill: t.panel,
        weak_bg_fill: t.panel,
        bg_stroke: Stroke::new(1.0, t.divider),
        rounding: Rounding::same(radius::MD),
        fg_stroke: Stroke::new(1.0, t.text_muted),
        expansion: 0.0,
    };
    visuals.widgets.inactive = WidgetVisuals {
        bg_fill: t.surface,
        weak_bg_fill: t.surface,
        bg_stroke: Stroke::NONE,
        rounding: Rounding::same(radius::MD),
        fg_stroke: Stroke::new(1.0, t.text),
        expansion: 0.0,
    };
    visuals.widgets.hovered = WidgetVisuals {
        bg_fill: t.surface_hover,
        weak_bg_fill: t.surface_hover,
        bg_stroke: Stroke::new(1.0, t.border),
        rounding: Rounding::same(radius::MD),
        fg_stroke: Stroke::new(1.5, t.text),
        expansion: 1.0,
    };
    visuals.widgets.active = WidgetVisuals {
        bg_fill: t.surface_active,
        weak_bg_fill: t.surface_active,
        bg_stroke: Stroke::new(1.0, t.accent),
        rounding: Rounding::same(radius::MD),
        fg_stroke: Stroke::new(2.0, t.text),
        expansion: 1.0,
    };
    visuals.widgets.open = visuals.widgets.active;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(space::S2, space::S1);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.interact_size.y = 22.0;
    style.spacing.window_margin = Margin::same(space::S2);
    style.interaction.tooltip_delay = 0.3;
    style.animation_time = 0.18;
    style.text_styles.insert(
        TextStyle::Heading,
        egui::FontId::new(21.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

#[cfg(test)]
fn relative_luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_in_expected_range() {
        assert!(relative_luminance(TOKENS.text) >= 0.7);
        assert!(relative_luminance(TOKENS.bg) <= 0.2);
        let spread =
            TOKENS.accent.b().max(TOKENS.accent.r()) - TOKENS.accent.r().min(TOKENS.accent.b());
        assert!(spread > 70);
    }

    #[test]
    fn apply_preserves_dark_mode() {
        let ctx = egui::Context::default();
        apply(&ctx);
        assert!(ctx.style().visuals.dark_mode);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn spacing_is_monotonic() {
        assert!(space::S1 < space::S2);
        assert!(space::S2 < space::S3);
        assert!(space::S3 < space::S4);
        assert!(space::S4 < space::S5);
    }
}

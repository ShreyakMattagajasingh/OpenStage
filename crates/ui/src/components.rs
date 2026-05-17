//! Reusable UI components for the Avatar Studio side panel.

use egui::{Align2, Color32, FontId, Frame, Id, Margin, Response, RichText, Sense, Stroke, Vec2};

use crate::theme::{radius, space, TOKENS};

pub fn section<R>(
    ui: &mut egui::Ui,
    title: &str,
    icon: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = Frame::none()
        .fill(TOKENS.panel)
        .stroke(Stroke::new(1.0, TOKENS.divider))
        .rounding(radius::MD)
        .inner_margin(Margin::same(space::S2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(TOKENS.accent).size(13.0));
                ui.label(
                    RichText::new(title.to_ascii_uppercase())
                        .small()
                        .strong()
                        .color(TOKENS.text_muted),
                );
            });
            ui.add_space(space::S1);
            content(ui)
        });
    inner.inner
}

pub fn subheader(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .small()
            .strong()
            .color(TOKENS.text_muted),
    );
}

pub fn primary_button(ui: &mut egui::Ui, icon: &str, label: &str) -> Response {
    let text = RichText::new(format!("{icon} {label}")).color(TOKENS.text_on_accent);
    ui.add(egui::Button::new(text).fill(TOKENS.accent))
}

pub fn secondary_button(ui: &mut egui::Ui, icon: &str, label: &str) -> Response {
    let text = RichText::new(format!("{icon} {label}")).color(TOKENS.text);
    ui.add(egui::Button::new(text).fill(TOKENS.surface))
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> Response {
    ui.add_sized(
        Vec2::splat(22.0),
        egui::Button::new(RichText::new(icon).color(TOKENS.text).size(14.0)).fill(TOKENS.surface),
    )
    .on_hover_text(tooltip)
}

pub fn tab(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> Response {
    let fill = if selected {
        Color32::from_rgba_unmultiplied(0x89, 0xb4, 0xfa, 0x33)
    } else {
        TOKENS.surface
    };
    let stroke = if selected {
        Stroke::new(1.0, TOKENS.accent)
    } else {
        Stroke::new(1.0, TOKENS.border)
    };
    ui.add_sized(
        Vec2::new(104.0, 26.0),
        egui::Button::new(RichText::new(format!("{icon} {label}")).color(TOKENS.text))
            .fill(fill)
            .stroke(stroke),
    )
}

pub fn swatch(ui: &mut egui::Ui, color: Color32, selected: bool) -> Response {
    let size = Vec2::splat(20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool(response.id, response.hovered());
    let stroke = if selected {
        Stroke::new(2.0, TOKENS.accent)
    } else {
        Stroke::new(1.0 + hover, TOKENS.border)
    };
    ui.painter()
        .rect_filled(rect, radius::SM, color.gamma_multiply(1.0 + hover * 0.08));
    ui.painter().rect_stroke(rect, radius::SM, stroke);
    response
}

pub fn asset_row(
    ui: &mut egui::Ui,
    thumb_uri: Option<&str>,
    icon: &str,
    label: &str,
    selected: bool,
) -> Response {
    list_row(
        ui,
        Id::new(("asset-row", label)),
        thumb_uri,
        icon,
        label,
        None,
        selected,
    )
}

pub fn gallery_row(
    ui: &mut egui::Ui,
    thumb_uri: Option<&str>,
    name: &str,
    timestamp_label: &str,
) -> Response {
    list_row(
        ui,
        Id::new(("gallery-row", name, timestamp_label)),
        thumb_uri,
        crate::icons::IMAGE,
        name,
        Some(timestamp_label),
        false,
    )
}

fn list_row(
    ui: &mut egui::Ui,
    id: Id,
    thumb_uri: Option<&str>,
    fallback_icon: &str,
    title: &str,
    subtitle: Option<&str>,
    selected: bool,
) -> Response {
    let width = ui.available_width();
    let row_height = if subtitle.is_some() { 40.0 } else { 32.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_height), Sense::click());
    let _hover = ui.ctx().animate_bool(id, response.hovered() || selected);
    let fill = if selected {
        Color32::from_rgba_unmultiplied(0x89, 0xb4, 0xfa, 0x33)
    } else if response.hovered() {
        TOKENS.surface
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, radius::MD, fill);
    if response.hovered() || selected {
        let bar = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 2.0, rect.max.y));
        ui.painter().rect_filled(bar, radius::SM, TOKENS.accent);
    }

    #[allow(deprecated)]
    let mut child = ui.child_ui(rect.shrink2(Vec2::new(6.0, 4.0)), *ui.layout(), None);
    child.horizontal(|ui| {
        let thumb_size = Vec2::splat(28.0);
        if let Some(uri) = thumb_uri {
            ui.add(
                egui::Image::from_uri(uri)
                    .fit_to_exact_size(thumb_size)
                    .maintain_aspect_ratio(true),
            );
        } else {
            let (icon_rect, _) = ui.allocate_exact_size(thumb_size, Sense::hover());
            ui.painter().rect_filled(icon_rect, radius::MD, TOKENS.bg);
            ui.painter().text(
                icon_rect.center(),
                Align2::CENTER_CENTER,
                fallback_icon,
                FontId::proportional(15.0),
                TOKENS.text_subtle,
            );
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(title).color(TOKENS.text));
            if let Some(sub) = subtitle {
                ui.label(RichText::new(sub).small().color(TOKENS.text_subtle));
            }
        });
    });
    response
}

pub fn empty_state(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(space::S1);
        ui.label(RichText::new(icon).size(22.0).color(TOKENS.text_subtle));
        ui.label(RichText::new(text).small().color(TOKENS.text_subtle));
        ui.add_space(space::S1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_compiles() {
        let ctx = egui::Context::default();
        ctx.begin_pass(Default::default());
        egui::CentralPanel::default().show(&ctx, |ui| {
            section(ui, "Test", crate::icons::CUBE, |ui| {
                ui.label("inside");
            });
        });
        let _ = ctx.end_pass();
    }
}

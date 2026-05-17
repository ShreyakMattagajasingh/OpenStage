//! Font installation for the desktop UI.

use egui::{FontData, FontDefinitions, FontFamily};

// Static Regular cut from rsms/inter v4.1 (extras/ttf/Inter-Regular.ttf).
// ~412 KB vs ~880 KB for the variable file; we never render multiple weights.
const INTER: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("inter".into(), FontData::from_static(INTER));
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

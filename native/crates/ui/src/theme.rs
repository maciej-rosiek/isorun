//! The web build's look as egui values: the colours of `styles.css` `:root`, and the visuals that
//! put them on egui's own widgets.

use egui::Color32;

/// `--panel`: the translucent dark of every panel.
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(6, 8, 11, 199);
/// `--panel-border`.
pub const PANEL_BORDER: Color32 = Color32::from_rgba_premultiplied(31, 31, 31, 31);
/// `--accent`.
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xd4, 0x5c);
/// `--danger`.
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x55, 0x55);
/// `--gold`, `--gold-bright`.
pub const GOLD: Color32 = Color32::from_rgb(0xd8, 0xb4, 0x5f);
pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(0xf3, 0xd9, 0x8a);
/// `--ink`, `--ink-dim`: text.
pub const INK: Color32 = Color32::from_rgb(0xe9, 0xe2, 0xd0);
pub const INK_DIM: Color32 = Color32::from_rgb(0x9d, 0x94, 0x83);

/// The body text's size at a UI scale of 1 (`font-size: 15px`).
pub const TEXT_PX: f32 = 15.0;

/// egui's visuals in the web build's colours.
pub fn install(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(INK);
    v.panel_fill = Color32::from_rgb(0x0b, 0x0d, 0x12);
    v.window_fill = PANEL;
    v.window_stroke = egui::Stroke::new(1.0, PANEL_BORDER);
    v.extreme_bg_color = Color32::from_rgb(4, 5, 7);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xff, 0xd4, 0x5c, 60);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.hyperlink_color = GOLD_BRIGHT;
    ctx.all_styles_mut(|s| {
        s.visuals = v.clone();
        for (style, size) in [(egui::TextStyle::Body, TEXT_PX), (egui::TextStyle::Button, TEXT_PX), (egui::TextStyle::Monospace, TEXT_PX), (egui::TextStyle::Small, 12.0), (egui::TextStyle::Heading, 22.0)] {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
    });
}

//! crawl's formatted text (`<red>...</red>`) in egui, in the web client's terminal colours.

use isorun_webtiles::formatted_string_to_spans;

/// The terminal's sixteen colours, as the web build's styles.css has them (`--color-0` .. `--color-15`).
pub const TERM_COLOURS: [egui::Color32; 16] = [
    egui::Color32::from_rgb(0x00, 0x00, 0x00),
    egui::Color32::from_rgb(0x00, 0x5a, 0xfa),
    egui::Color32::from_rgb(0x4e, 0x9a, 0x06),
    egui::Color32::from_rgb(0x06, 0x98, 0x9a),
    egui::Color32::from_rgb(0xb3, 0x00, 0x09),
    egui::Color32::from_rgb(0xcd, 0x21, 0xcb),
    egui::Color32::from_rgb(0x8f, 0x59, 0x02),
    egui::Color32::from_rgb(0xba, 0xbd, 0xb6),
    egui::Color32::from_rgb(0x55, 0x57, 0x53),
    egui::Color32::from_rgb(0x5e, 0x78, 0xff),
    egui::Color32::from_rgb(0x8a, 0xe2, 0x34),
    egui::Color32::from_rgb(0x34, 0xe2, 0xe2),
    egui::Color32::from_rgb(0xef, 0x29, 0x29),
    egui::Color32::from_rgb(0xfd, 0x59, 0xfa),
    egui::Color32::from_rgb(0xfc, 0xe9, 0x4f),
    egui::Color32::from_rgb(0xee, 0xee, 0xec),
];

/// A crawl formatted string as an egui layout.
pub fn formatted(text: &str, size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for span in formatted_string_to_spans(text) {
        let colour = span.fg.map(|c| TERM_COLOURS[c as usize & 15]).unwrap_or(TERM_COLOURS[7]);
        let mut format = egui::TextFormat { font_id: egui::FontId::monospace(size), color: colour, ..Default::default() };
        if let Some(bg) = span.bg {
            format.background = TERM_COLOURS[bg as usize & 15];
        }
        job.append(&span.text, 0.0, format);
    }
    job
}


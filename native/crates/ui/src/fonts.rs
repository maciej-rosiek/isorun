//! The web build's fonts: DejaVu Sans Mono for everything, Cinzel for small capitals, Grenze
//! Gotisch for titles.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub fn title() -> FontFamily {
    FontFamily::Name("title".into())
}

pub fn caps() -> FontFamily {
    FontFamily::Name("caps".into())
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add("mono", include_bytes!("../assets/fonts/dejavu-sans-mono-400.ttf"));
    add("mono-bold", include_bytes!("../assets/fonts/dejavu-sans-mono-700.ttf"));
    add("cinzel", include_bytes!("../assets/fonts/cinzel-latin.ttf"));
    add("grenze", include_bytes!("../assets/fonts/grenze-gotisch-700-latin.ttf"));
    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        fonts.families.entry(family).or_default().insert(0, "mono".into());
    }
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let with = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(title(), with("grenze"));
    fonts.families.insert(caps(), with("cinzel"));
    fonts.families.insert(bold(), with("mono-bold"));
    ctx.set_fonts(fonts);
}

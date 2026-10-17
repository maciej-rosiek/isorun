//! What a cell holds, in words (the port of `cell-info.ts`), for the card about the cell the
//! pointer or the aim is on, and the head of the cell menu.

use isorun_scene::{has_items_under, Attitude, BillboardKind, CellKind, Scene, Threat, Visibility, WoundLevel};

/// How a line of a cell's details is coloured (styles.css `.inspect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Trivial,
    Easy,
    Tough,
    Nasty,
    Invisible,
    Friendly,
    Neutral,
    Note,
}

impl Tone {
    /// The colour the web build's `.inspect .<tone>` gives the line.
    pub fn colour(self) -> egui::Color32 {
        use egui::Color32 as C;
        match self {
            Tone::Trivial => C::from_rgb(0x9a, 0x9a, 0x9a),
            Tone::Easy => C::from_rgb(0xe8, 0xe8, 0xe8),
            Tone::Tough => C::from_rgb(0xff, 0xd7, 0x5e),
            Tone::Nasty => C::from_rgb(0xff, 0x5a, 0x4a),
            Tone::Friendly => C::from_rgb(0x7f, 0xd6, 0x7f),
            Tone::Neutral => C::from_rgb(0x9f, 0xc4, 0xff),
            Tone::Note => C::from_rgb(0xb8, 0xb0, 0xa0),
            Tone::Invisible => C::from_rgb(0xd9, 0xcc, 0xb0),
        }
    }
}

/// One line of a cell's details; `tone` colours it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellDetail {
    pub text: String,
    pub tone: Option<Tone>,
    pub wound: Option<WoundLevel>,
}

fn wound_words(w: WoundLevel) -> Option<&'static str> {
    Some(match w {
        WoundLevel::LightlyDamaged => "lightly wounded",
        WoundLevel::ModeratelyDamaged => "moderately wounded",
        WoundLevel::HeavilyDamaged => "heavily wounded",
        WoundLevel::SeverelyDamaged => "severely wounded",
        WoundLevel::AlmostDead => "almost dead",
        WoundLevel::Uninjured => return None,
    })
}

fn threat_tone(t: Threat) -> Tone {
    match t {
        Threat::Trivial => Tone::Trivial,
        Threat::Easy => Tone::Easy,
        Threat::Tough => Tone::Tough,
        Threat::Nasty => Tone::Nasty,
        Threat::Invisible => Tone::Invisible,
    }
}

fn threat_word(t: Threat) -> &'static str {
    match t {
        Threat::Trivial => "trivial",
        Threat::Easy => "easy",
        Threat::Tough => "tough",
        Threat::Nasty => "nasty",
        Threat::Invisible => "invisible",
    }
}

/// What a cell holds, in words: a monster in sight with its threat, wounds and attitude, the
/// items that lie there (the top one of a pile, as the server names no more) and the terrain
/// (plain floor and walls say nothing). Nothing for a cell never seen.
pub fn cell_details(scene: &Scene, x: i32, y: i32) -> Vec<CellDetail> {
    let Some(cell) = scene.cell(x, y).filter(|c| c.visibility != Visibility::Unseen) else { return Vec::new() };
    let here: Vec<_> = scene.billboards_at(x, y).collect();
    let mut details = Vec::new();
    for b in &here {
        let Some(name) = b.name.as_ref().filter(|_| b.kind == BillboardKind::Monster && cell.visibility == Visibility::Visible) else { continue };
        let friendly = b.attitude == Some(Attitude::Friendly);
        let neutral = matches!(b.attitude, Some(Attitude::Neutral) | Some(Attitude::GoodNeutral));
        let tone = if friendly {
            Some(Tone::Friendly)
        } else if neutral {
            Some(Tone::Neutral)
        } else {
            b.threat.map(threat_tone)
        };
        details.push(CellDetail { text: capitalise(name), tone, wound: Some(b.damage.unwrap_or(WoundLevel::Uninjured)) });
        let mut about = Vec::new();
        if friendly {
            about.push("friendly".to_string());
        } else if neutral {
            about.push("neutral".to_string());
        } else if let Some(t) = b.threat.filter(|t| *t != Threat::Invisible) {
            about.push(format!("{} threat", threat_word(t)));
        }
        if let Some(w) = b.damage.and_then(wound_words) {
            about.push(w.to_string());
        }
        if !about.is_empty() {
            details.push(CellDetail { text: about.join(", "), tone: Some(Tone::Note), wound: None });
        }
        if has_items_under(b) {
            details.push(CellDetail { text: "Items under it".into(), tone: Some(Tone::Note), wound: None });
        }
    }
    let items: Vec<_> = here.iter().filter(|b| b.kind == BillboardKind::Item && b.name.as_ref().is_some_and(|n| !n.is_empty()) && !b.scenery).collect();
    if !items.is_empty() {
        let names: Vec<&str> = items.iter().filter_map(|b| b.name.as_deref()).collect();
        let more = if items.iter().any(|b| has_items_under(b)) { " and more items" } else { "" };
        details.push(CellDetail { text: capitalise(&names.join(", ")) + more, tone: None, wound: None });
    }
    if let Some(label) = cell.label.as_ref().filter(|l| !l.is_empty() && *l != "floor" && cell.kind != CellKind::Wall) {
        let remembered = if cell.visibility == Visibility::Remembered { " (remembered)" } else { "" };
        details.push(CellDetail { text: capitalise(label) + remembered, tone: Some(Tone::Note), wound: None });
    }
    details
}

fn capitalise(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

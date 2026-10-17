//! The stats pane on the grid.
//!
//! The sidebar's columns hold the WebTiles `#stats` pane as text rows, one console row each, in
//! the order and with the captions player.js `update_stats_pane` uses and game.html `#stats` lays
//! out. Every width is the stylesheet's percentage of the pane, in whole cells: the HP and MP bars
//! are `#stats_hp_bar`'s 55% (23 of 42 cells), the left column `#stats_leftcolumn`'s 45% (the 19
//! cells before the bar), the noise bar `#stats_noise` 95% × `#stats_noise_inter` 61% ×
//! `#stats_noise_bar` 82% (9 cells; 11 when superloud, the bar "goes to eleven"). A bar is
//! painted on those cells: `update_bar`'s four segments as fractions of the maximum, on the track.
//!
//! Colours and thresholds come from the player's rc through the `options` message (`hp_colour`,
//! `mp_colour`, `stat_colour`, `show_game_time`, `always_show_doom_contam`); nothing is decided
//! here. Pure: a `player` message in, rows out, so a recorded session pins it.

use super::console::STAT_WIDTH;
use super::rows::{blank, cut_row, from_formatted, join_rows, pad_row, row_length, wrap_row, BarPaint, Bg, Row, Span};
use isorun_webtiles::{has_status, index_to_letter, Obj, PlayerState};
use regex::Regex;

/// style.css `.stats_caption`: brown
const CAPTION: u8 = 6;
/// style.css `#stats_titleline, #stats_species_god, #stats_piety`: yellow
const TITLE: u8 = 14;
/// style.css `#stats_wizmode`: blue
const WIZMODE: u8 = 1;
/// style.css `#stats_piety.penance`: red; `.monk`: darkgray
const PENANCE: u8 = 4;
const MONK: u8 = 8;

/// The Health and Magic bars are WebTiles' own (game.html `#stats_hp_bar` / `#stats_mp_bar`):
/// flat blocks the row's full height, `update_bar`'s four segments laid left to right on a
/// darkgray track, in style.css's colours. The track is the palette's darkgray taken most of the
/// way to black (`--stats-bar-track`): WebTiles stands its bar on a black page, Isorun's pane is
/// bare over a lit dungeon, so the black is folded into the colour. The whole bar is one
/// background across its cells, so a boundary lands on the exact fraction `update_bar` computes
/// and not on the nearest cell.
const BAR_POISON: u8 = 14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarColours {
    pub full: u8,
    pub decrease: u8,
    pub increase: u8,
}

/// `#stats_hp_bar_full` lightgreen, `_decrease` red, `_increase` green
pub const HP_BAR: BarColours = BarColours { full: 10, decrease: 4, increase: 2 };
/// `#stats_mp_bar_full` lightblue, `_decrease` magenta, `_increase` blue
pub const MP_BAR: BarColours = BarColours { full: 9, decrease: 5, increase: 1 };

#[derive(Clone, Copy, Debug, PartialEq, Default)]
struct BarSegments {
    full: f32,
    poison: f32,
    decrease: f32,
    increase: f32,
}

/// `update_bar`'s segments as one span `cells` wide: the full segment, the poison segment, then
/// the change since the last update, and the track for whatever is left of the bar.
fn paint_bar(cells: usize, seg: BarSegments, c: BarColours) -> Row {
    if cells == 0 {
        return vec![];
    }
    let mut stops = Vec::new();
    let mut at = 0.0;
    for (width, colour) in [(seg.full, c.full), (seg.poison, BAR_POISON), (seg.decrease, c.decrease), (seg.increase, c.increase)] {
        if width > 0.0 {
            stops.push((at, at + width, BarPaint::Term(colour)));
        }
        at += width;
    }
    if stops.is_empty() {
        return vec![blank(cells, Some(Bg::Stripes(vec![(0.0, 1.0, BarPaint::Track)])))];
    }
    if at < 1.0 {
        stops.push((at, 1.0, BarPaint::Track));
    }
    vec![blank(cells, Some(Bg::Stripes(stops)))]
}

/// style.css `#stats_noise_bar`: the track; `_decrease` darkgray; `_full` by `data-level`
const NOISE_TRACK: [u8; 3] = [0x2f, 0x2f, 0x2f];
const NOISE_DECREASE: u8 = 8;
fn noise_full(cat: &str) -> [u8; 3] {
    match cat {
        "quiet" => [0xd3, 0xd3, 0xd3],
        "loud" => [0xff, 0xd7, 0x00],
        "veryloud" => [0xff, 0x00, 0x00],
        "superloud" => [0xff, 0x00, 0xff],
        _ => [0, 0, 0],
    }
}

/// player.js `stat_boosters`: a status that paints the stat lightblue
fn booster(name: &str) -> Regex {
    Regex::new(match name {
        "str" | "int" | "dex" => "vitalised",
        "hp" => "divinely vigorous|berserking",
        _ => "divinely vigorous",
    })
    .unwrap()
}

fn status(p: &PlayerState, pattern: &str) -> bool {
    has_status(p, &Regex::new(pattern).unwrap())
}

/// player.js `old_hp`, `old_mp`, `old_noise`: the values a bar last showed, sizing its change
/// segment
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BarMemory {
    pub hp: Option<i64>,
    pub mp: Option<i64>,
    pub noise: Option<i64>,
}

/// The portrait: the player's own cell, drawn at the pane's top-left corner, stands this many
/// rows tall, beside the title and the species-of-god lines, which start after it. The rows below
/// it run the pane's full width, so the bars and the two columns keep their cells.
pub const PORTRAIT_ROWS: usize = 4;

/// The pane's narrow layout. WebTiles has no such thing: `#stats` is always `enums.stat_width`
/// wide. Isorun's column gives way on a narrow screen, and below this many cells the captions no
/// longer fit their columns, so the pane prints the same lines with the captions cut to what
/// names them and the portrait shrinks to two rows.
const COMPACT_WIDTH: usize = 36;
const COMPACT_PORTRAIT_ROWS: usize = 2;

fn compact_stats(width: usize) -> bool {
    width < COMPACT_WIDTH
}

/// the rows the portrait stands over at `width` cells: the square, or two rows when compact
pub fn portrait_rows(width: usize) -> usize {
    if compact_stats(width) {
        COMPACT_PORTRAIT_ROWS
    } else {
        PORTRAIT_ROWS
    }
}

/// the pane's split for a sidebar `width` wide: #stats_hp_bar floats right at 55%;
/// #stats_leftcolumn is the 45% before it, so the two share a column
fn stats_split(width: usize) -> usize {
    let bar = (width as f32 * 0.55).round() as usize;
    width - bar
}

/// `update_bar` in player.js as fractions of the bar: the full segment, then (for hp) the yellow
/// poison segment for `hp - poison_survival`, then the change since the last update as a decrease
/// or increase, on the track. The segments are its percentages of the maximum, cumulative so they
/// never sum past the bar, and are left unrounded.
fn bar_cells(value: i64, max: i64, old: Option<i64>, poison_survival: Option<i64>) -> BarSegments {
    let max = if max == 0 { 1 } else { max } as f64;
    let value = value.max(0) as f64;
    let mut old_value = old.map_or(value, |o| o as f64);
    if old_value > max {
        old_value = max;
    }
    let increase = old_value < value;
    let mut full = ((10000.0 * if increase { old_value } else { value }) / max).round();
    let mut change = ((10000.0 * (old_value - value).abs()) / max).floor();
    let mut poison = 0.0;
    if let Some(ps) = poison_survival.map(|p| p as f64) {
        if ps < value {
            poison = ((10000.0 * (value - ps)) / max).round();
            full = ((10000.0 * ps) / max).round();
        }
    }
    if full + poison + change > 10000.0 {
        change = 10000.0 - poison - full;
    }
    let at = |pct: f64| (pct / 10000.0).min(1.0) as f32;
    let full_c = at(full);
    let poison_c = at(full + poison) - full_c;
    let change_c = at(full + poison + change) - full_c - poison_c;
    BarSegments { full: full_c, poison: poison_c, decrease: if increase { 0.0 } else { change_c }, increase: if increase { change_c } else { 0.0 } }
}

fn threshold_colour(limits: Option<&serde_json::Value>, pct: f64) -> Option<String> {
    let mut colour = None;
    for l in limits?.as_array()? {
        let v = l.get("value").and_then(|v| v.as_f64()).unwrap_or(f64::NEG_INFINITY);
        if pct <= v {
            colour = l.get("colour").and_then(|c| c.as_str()).map(String::from);
        }
    }
    colour
}

fn with_cls(mut s: Span, cls: Option<String>) -> Span {
    s.cls = cls;
    s
}

/// `percentage_color` in player.js: a booster status paints the value lightblue; otherwise the
/// rc's `hp_colour` / `mp_colour` thresholds apply, against the real maximum when drained.
fn percentage_colour(p: &PlayerState, opts: &Obj, name: &str) -> Option<String> {
    if has_status(p, &booster(name)) {
        return Some("boosted_stat".into());
    }
    let (value, max) = if name == "hp" { (p.hp, if p.real_hp_max != 0 && p.real_hp_max != p.hp_max { p.real_hp_max } else { p.hp_max }) } else { (p.mp, p.mp_max) };
    threshold_colour(opts.get(&format!("{name}_colour")), value as f64 / (if max == 0 { 1 } else { max }) as f64 * 100.0).map(|c| format!("colour_{c}"))
}

fn stat_values(p: &PlayerState, name: &str) -> (i64, i64) {
    match name {
        "str" => (p.str, p.str_max),
        "int" => (p.int, p.int_max),
        _ => (p.dex, p.dex_max),
    }
}

/// `stat_class` in player.js: lost, then rc `stat_colour`, then boosted, then below maximum.
fn stat_class(p: &PlayerState, opts: &Obj, name: &str) -> Option<String> {
    let (v, max) = stat_values(p, name);
    if status(p, &format!("lost {name}")) {
        return Some("zero_stat".into());
    }
    if let Some(c) = threshold_colour(opts.get("stat_colour"), v as f64) {
        return Some(format!("colour_{c}"));
    }
    if has_status(p, &booster(name)) {
        return Some("boosted_stat".into());
    }
    (v < max).then(|| "degenerated_stat".into())
}

/// `update_defense` in player.js: lightblue when boosted, red when degenerated
fn defense(value: i64, m: i64) -> Span {
    let cls = if m > 0 {
        Some("boosted_defense".to_string())
    } else if m < 0 {
        Some("degenerated_defense".to_string())
    } else {
        None
    };
    with_cls(Span::new(value.to_string()), cls)
}

/// `update_stat` in player.js: the value, and the maximum in parentheses when below it
fn stat(p: &PlayerState, opts: &Obj, name: &str) -> Span {
    let (v, max) = stat_values(p, name);
    let text = if v < max { format!("{v} ({max})") } else { v.to_string() };
    with_cls(Span::new(text), stat_class(p, opts, name))
}

/// `wielded_weapon` in player.js: the unarmed attack in `weapon_colour`, or the item, coloured by
/// `weapon_colour` unless it is -1 (servers before 0.34 leave it there, so the item's own colour,
/// or lightgray for unarmed, applies as in the older client).
fn wielded_weapon(p: &PlayerState, offhand: bool) -> Span {
    let index = if offhand { p.offhand_index } else { p.weapon_index };
    let colour = if offhand { p.offhand_weapon_colour } else { p.weapon_colour };
    if index == -1 {
        return Span::fg(p.unarmed_attack.clone(), if colour != -1 { colour as u8 } else { 7 });
    }
    let item = p.inv.get(&index);
    let col = if colour != -1 { Some(colour) } else { item.and_then(|i| i.col).filter(|c| *c != -1) };
    Span { text: item.and_then(|i| i.name.clone()).unwrap_or_default(), fg: col.map(|c| c as u8), ..Default::default() }
}

fn cap(text: &str) -> Span {
    Span::fg(text, CAPTION)
}

/// a caption and, on a compact pane, the shorter one that stands for it; the long one is its
/// tooltip there
fn cap_short(compact: bool, text: &str, short: &str) -> Span {
    if compact {
        Span { text: short.into(), fg: Some(CAPTION), title: Some(text.trim_end_matches(':').trim().into()), ..Default::default() }
    } else {
        cap(text)
    }
}

fn rep(ch: &str, n: i64) -> String {
    ch.repeat(n.max(0) as usize)
}

fn titled(text: String, title: &str, cls: Option<String>) -> Span {
    Span { text, title: Some(title.into()), cls, ..Default::default() }
}

/// The pane as rows of `width` cells, in `update_stats_pane` order: title (with `*WIZARD*` /
/// `*EXPLORE*` at the right end), species of god with piety, Health and Magic with their bars,
/// the two columns (AC, EV, SH, XL and Next, Noise on the left; Str with Doom, Int with Contam,
/// Dex, Place, Time or Turn on the right), the weapon, the off-hand weapon for Coglins, the
/// quiver, and the status lights, wrapped. `prev` is the bar memory from the last call; the
/// returned one feeds the next. `portrait` is the cells the first `portrait_rows(width)` rows
/// leave blank at the left for the portrait.
pub fn stats_rows(p: &PlayerState, opts: &Obj, prev: BarMemory, width: usize, portrait: usize) -> (Vec<Row>, BarMemory) {
    let split = stats_split(width);
    let compact = compact_stats(width);
    let mut rows: Vec<Row> = Vec::new();
    let mut next = BarMemory::default();
    let lead = portrait.min(width);
    let lead_at = |i: usize| if i < portrait_rows(width) { lead } else { 0 };
    let beside = |rows: &Vec<Row>, row: Row| -> Row {
        let l = lead_at(rows.len());
        if l > 0 {
            std::iter::once(blank(l, None)).chain(row).collect()
        } else {
            row
        }
    };
    let beside_w = width - lead_at(0);

    // title line, the wizmode marker floated right
    let wiz = if p.wizard {
        "*WIZARD*"
    } else if p.explore {
        "*EXPLORE*"
    } else {
        ""
    };
    let sep = if p.title.starts_with(',') { "" } else { " " };
    let title = vec![Span::fg(format!("{}{sep}{}", p.name, p.title), TITLE)];
    let r = if wiz.is_empty() { cut_row(title, beside_w, false) } else { join_rows(title, beside_w.saturating_sub(wiz.len()), vec![Span::fg(wiz, WIZMODE)]) };
    let r = beside(&rows, r);
    rows.push(r);

    // species of god, piety pips, Gozag's gold
    let mut species_god = if !p.species_display_name.is_empty() { p.species_display_name.clone() } else { p.species.clone() };
    if !p.god.is_empty() {
        species_god += &format!(" of {}", p.god);
    }
    let piety_colour = if p.penance {
        PENANCE
    } else if !p.god.is_empty() {
        TITLE
    } else {
        MONK
    };
    let mut piety: Row = vec![];
    let rank = p.piety_rank;
    if p.god == "Xom" {
        piety.push(Span::fg(if rank >= 0 { format!("{}*{}", rep(".", rank), rep(".", 5 - rank)) } else { "......".into() }, piety_colour));
    } else if (rank > 0 || !p.god.is_empty()) && p.god != "Gozag" {
        piety.push(Span::fg(rep("*", rank) + &rep(".", 6 - rank - p.ostracism_pips), piety_colour));
        if p.ostracism_pips > 0 {
            piety.push(Span::fg(rep("X", p.ostracism_pips), 5));
        }
    }
    let mut line2 = vec![Span::fg(species_god, TITLE)];
    if !piety.is_empty() {
        line2.push(Span::new(" "));
        line2.extend(piety);
    }
    if p.god == "Gozag" {
        line2.push(cap_short(compact, " Gold: ", " $:"));
        line2.push(with_cls(Span::new(p.gold.to_string()), status(p, "gold aura").then(|| "boosted_stat".into())));
    }
    let r = beside(&rows, cut_row(line2, beside_w, false));
    rows.push(r);

    // Health and Magic: the bar over the row, one cell, then the value/max left-aligned in one
    // column at the pane's right edge (as wide as the longer of the two, so the bars end
    // together). The bar's colour names it and the word is the value's tooltip. player.js
    // remembers nothing before the first `player` message with a maximum, or the first message
    // would paint the whole bar as an increase.
    let real_hp = p.real_hp_max != p.hp_max;
    if p.hp_max != 0 {
        next.hp = Some(p.hp.max(0));
    }
    let hp_title = if real_hp { "HP" } else { "Health" };
    let mut hp_value = vec![titled(p.hp.to_string(), hp_title, percentage_colour(p, opts, "hp")), titled(format!("/{}", p.hp_max), hp_title, None)];
    if real_hp {
        hp_value.push(titled(format!(" ({})", p.real_hp_max), "HP", None));
    }
    let mut mp_value = None;
    if p.species != "Djinni" {
        if p.mp_max != 0 {
            next.mp = Some(p.mp.max(0));
        }
        let mut v = vec![titled(p.mp.to_string(), "Magic", percentage_colour(p, opts, "mp")), titled(format!("/{}", p.mp_max), "Magic", None)];
        if p.species == "Deep Dwarf" && p.dd_real_mp_max != p.mp_max {
            v.push(titled(format!(" ({})", p.dd_real_mp_max), "Magic", None));
        }
        mp_value = Some(v);
    } else {
        next.mp = prev.mp;
    }
    // the column is as wide as the value at its maximum, so health ticking past ten or a
    // hundred moves nothing: the bar only changes length when a maximum does
    let widest = |value: &Row, v: i64, max: i64| row_length(value) + max.to_string().len().saturating_sub(v.to_string().len());
    let value_w = widest(&hp_value, p.hp, p.hp_max).max(mp_value.as_ref().map_or(0, |m| widest(m, p.mp, p.mp_max)));
    let value_bar = |value: &Row, room: usize, seg: BarSegments, c: BarColours| -> Row {
        let mut r = paint_bar(room.saturating_sub(value_w + 1).max(1), seg, c);
        r.push(blank(1, None));
        r.extend(pad_row(cut_row(value.clone(), value_w, false), value_w));
        r
    };
    let room = |rows: &Vec<Row>| width - lead_at(rows.len());
    let r = value_bar(&hp_value, room(&rows), bar_cells(p.hp, p.hp_max, prev.hp, Some(p.poison_survival)), HP_BAR);
    let r = beside(&rows, cut_row(r, room(&rows), false));
    rows.push(r);
    if let Some(mp) = &mp_value {
        let r = value_bar(mp, room(&rows), bar_cells(p.mp, p.mp_max, prev.mp, None), MP_BAR);
        let r = beside(&rows, cut_row(r, room(&rows), false));
        rows.push(r);
    }

    // the two columns
    let show_dc = |v: i64| !(v == 0 && opts.get("always_show_doom_contam") == Some(&serde_json::Value::Bool(false)));
    let doom_colour = if p.doom >= 75 {
        5
    } else if p.doom >= 50 {
        12
    } else if p.doom >= 25 {
        14
    } else if p.doom == 0 {
        8
    } else {
        7
    };
    let contam_colour = if p.contam >= 200 {
        4
    } else if p.contam >= 100 {
        14
    } else {
        8
    };
    let show_time = opts.get("show_game_time") == Some(&serde_json::Value::Bool(true));
    let mut time = if show_time { format!("{:.1}", p.time as f64 / 10.0) } else { p.turn.to_string() };
    if p.time_delta != 0 {
        time += &format!(" ({:.1})", p.time_delta as f64 / 10.0);
    }
    let xl_row = if compact {
        vec![cap("XL:"), Span::new(format!(" {} ", p.xl)), Span { text: format!("{}%", p.progress), title: Some("Next".into()), ..Default::default() }]
    } else {
        vec![cap("XL:"), Span::new(format!(" {} ", p.xl)), cap("Next:"), Span::new(format!(" {}%", p.progress))]
    };
    let left: Vec<Row> = vec![
        vec![cap("AC:"), Span::new(" "), defense(p.ac, p.ac_mod)],
        vec![cap("EV:"), Span::new(" "), defense(p.ev, p.ev_mod)],
        vec![cap("SH:"), Span::new(" "), defense(p.sh, p.sh_mod)],
        xl_row,
        noise_row(p, prev, &mut next, split, compact),
    ];
    let place = if p.depth != 0 { format!("{}:{}", p.place, p.depth) } else { p.place.clone() };
    let mut right: Vec<Row> = vec![
        vec![cap_short(compact, "Str:", "St:"), Span::new(" "), stat(p, opts, "str")],
        vec![cap_short(compact, "Int:", "In:"), Span::new(" "), stat(p, opts, "int")],
        vec![cap_short(compact, "Dex:", "Dx:"), Span::new(" "), stat(p, opts, "dex")],
        vec![cap_short(compact, "Place:", "@:"), Span::new(format!(" {place}"))],
        vec![cap_short(compact, if show_time { "Time:" } else { "Turn:" }, "T:"), Span::new(format!(" {time}"))],
    ];
    // game.html: Doom sits 2.2em after Str, Contam 1.1em after Int
    if show_dc(p.doom) {
        right[0].push(Span::new(if compact { " " } else { "  " }));
        right[0].push(cap_short(compact, "Doom:", "Dm:"));
        right[0].push(Span { text: format!(" {}%", p.doom), fg: Some(doom_colour), title: (!p.doom_desc.is_empty()).then(|| p.doom_desc.clone()), ..Default::default() });
    }
    if show_dc(p.contam) {
        right[1].push(Span::new(" "));
        right[1].push(cap_short(compact, "Contam:", "Cn:"));
        right[1].push(Span::fg(format!(" {}%", p.contam), contam_colour));
    }
    // a Djinni's pane has no Magic row, so its AC row still sits beside the portrait
    for (l, r) in left.into_iter().zip(right) {
        let rm = room(&rows);
        let row = cut_row(join_rows(l, stats_split(rm), r), rm, false);
        let row = beside(&rows, row);
        rows.push(row);
    }

    // weapon, off-hand weapon (Coglins), quiver
    rows.push(cut_row(vec![cap(&format!("{})", index_to_letter(p.weapon_index))), Span::new(" "), wielded_weapon(p, false)], width, true));
    if p.offhand_weapon {
        rows.push(cut_row(vec![cap(&format!("{})", index_to_letter(p.offhand_index))), Span::new(" "), wielded_weapon(p, true)], width, true));
    }
    rows.push(cut_row(from_formatted(&p.quiver_desc, None), width, true));

    // status lights: only entries with a light, in the server's colour and order, wrapped
    let mut lights: Row = vec![];
    for st in &p.status {
        let Some(light) = st.light.as_ref().filter(|l| !l.is_empty()) else { continue };
        if !lights.is_empty() {
            lights.push(Span::new(" "));
        }
        lights.push(Span { text: light.clone(), fg: Some(st.col.unwrap_or(7) as u8), title: st.desc.clone().filter(|d| !d.is_empty()), ..Default::default() });
    }
    rows.extend(wrap_row(&lights, width));
    let _ = STAT_WIDTH;
    (rows, next)
}

/// `update_bar_noise` in player.js on the left column: the caption, then the bar right-aligned in
/// `#stats_noise_inter` (61% of the 95% row; 45% when wizard mode shows the number), 82% of that
/// wide, all of it when superloud. Silenced blanks the bar and prints `Silenced` where it was.
fn noise_row(p: &PlayerState, prev: BarMemory, next: &mut BarMemory, split: usize, compact: bool) -> Row {
    let max = 1000.0;
    let mut level = p.adjusted_noise.max(0);
    let mut old = prev.noise.unwrap_or(level).min(1000);
    let mut cat = if level <= 333 {
        "quiet"
    } else if level <= 666 {
        "loud"
    } else if level < 1000 {
        "veryloud"
    } else {
        "superloud"
    };
    let silenced = status(p, "silence");
    if silenced {
        level = 0;
        old = 0;
        cat = "blank";
    }
    next.noise = Some(level);
    let show_num = p.wizard && !silenced;
    let noise_w = (split as f32 * 0.95).round() as usize;
    let inter_w = (noise_w as f32 * if show_num { 0.45 } else { 0.61 }).round() as usize;
    let bar_w = if cat == "superloud" { inter_w } else { (inter_w as f32 * 0.82).round() as usize };
    let full = ((10000.0 * level as f64) / max).round();
    let mut change = ((10000.0 * (old - level).abs() as f64) / max).round();
    if full + change > 10000.0 {
        change = 10000.0 - full;
    }
    let full_c = bar_w.min(((bar_w as f64 * full) / 10000.0).round() as usize);
    let change_c = (bar_w - full_c).min(((bar_w as f64 * change) / 10000.0).round() as usize);
    let mut row = vec![cap_short(compact, "Noise:", "N:")];
    if show_num {
        row.push(Span::fg(format!(" {}", p.noise), 7));
    }
    let lead = noise_w - inter_w;
    if silenced {
        // style.css #stats_noise_status: magenta, at the right end of the row
        let status = "Silenced";
        let at = row_length(&row).max(noise_w.saturating_sub(status.len()));
        let mut r = pad_row(row, at);
        r.push(Span::fg(status, 5));
        return r;
    }
    let mut bar = vec![];
    if full_c > 0 {
        bar.push(Span { text: " ".repeat(full_c), bg: Some(Bg::Rgb(noise_full(cat))), cls: Some(format!("noise_{cat}")), ..Default::default() });
    }
    if change_c > 0 {
        bar.push(blank(change_c, Some(Bg::Term(NOISE_DECREASE))));
    }
    if bar_w > full_c + change_c {
        bar.push(blank(bar_w - full_c - change_c, Some(Bg::Rgb(NOISE_TRACK))));
    }
    let mut r = pad_row(cut_row(row, lead, false), lead);
    r.extend(bar);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_webtiles::StatusEntry;

    fn text(r: &Row) -> String {
        r.iter().map(|s| s.text.as_str()).collect()
    }

    fn player() -> PlayerState {
        PlayerState {
            name: "Ada".into(),
            title: "the Skirmisher".into(),
            species: "Hill Orc".into(),
            god: "Trog".into(),
            piety_rank: 2,
            hp: 18,
            hp_max: 20,
            real_hp_max: 20,
            poison_survival: 18,
            mp: 3,
            mp_max: 3,
            dd_real_mp_max: 3,
            ac: 3,
            ev: 9,
            sh: 0,
            str: 14,
            str_max: 14,
            int: 7,
            int_max: 8,
            dex: 11,
            dex_max: 11,
            xl: 2,
            progress: 44,
            place: "Dungeon".into(),
            depth: 2,
            turn: 512,
            unarmed_attack: "Nothing wielded".into(),
            status: vec![StatusEntry { light: Some("Fast".into()), col: Some(10), desc: Some("You are moving fast.".into()), ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn rows_in_order() {
        let (rows, mem) = stats_rows(&player(), &Obj::new(), BarMemory::default(), 42, 0);
        assert_eq!(text(&rows[0]), "Ada the Skirmisher");
        assert_eq!(text(&rows[1]), "Hill Orc of Trog **....");
        assert!(text(&rows[2]).ends_with(" 18/20"));
        assert_eq!(row_length(&rows[2]), 42);
        assert!(text(&rows[4]).starts_with("AC: 3"));
        assert_eq!(&text(&rows[4])[19..], "Str: 14  Doom: 0%");
        assert!(text(&rows[5]).contains("Int: 7 (8)"));
        assert_eq!(rows[5].iter().find(|s| s.text == "7 (8)").unwrap().cls.as_deref(), Some("degenerated_stat"));
        assert_eq!(text(&rows[7]), "XL: 2 Next: 44%    Place: Dungeon:2");
        assert_eq!(text(&rows[9]), "-) Nothing wielded");
        assert_eq!(text(rows.last().unwrap()), "Fast");
        assert_eq!(mem.hp, Some(18));
    }

    #[test]
    fn bars_keep_the_change() {
        let s = bar_cells(10, 20, Some(15), None);
        assert_eq!((s.full, s.decrease, s.increase), (0.5, 0.25, 0.0));
        let s = bar_cells(10, 20, Some(5), Some(8));
        assert!((s.full - 0.4).abs() < 1e-6 && (s.poison - 0.1).abs() < 1e-6);
        assert!(s.increase > 0.24 && s.increase < 0.26);
    }

    #[test]
    fn compact_and_portrait() {
        let (rows, _) = stats_rows(&player(), &Obj::new(), BarMemory::default(), 30, 5);
        assert!(text(&rows[0]).starts_with("     Ada"));
        assert!(text(&rows[1]).starts_with("     Hill Orc"));
        assert!(matches!(rows[2][0].bg, Some(Bg::Stripes(_))));
        assert!(rows.iter().any(|r| text(r).starts_with("XL: 2 44%")));
    }
}

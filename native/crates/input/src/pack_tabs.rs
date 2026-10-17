//! The pack's pages as tabs: Gear (3/52) | Potions | Scrolls (| Evocables) (the port of
//! `pack-tabs.ts`, which the context and the equipment button need).
//!
//! Crawl's pack (`i`, and drop) is paged by category already: Left and Right
//! turn the page (invent.cc `InvMenu::cycle_page`), skipping the empty ones,
//! and the title names the page it is on (`InvMenu::set_title`: "Gear: ...",
//! "Potions: ", "Scrolls: ", "Evocable Items: "). What crawl lacks is a
//! picture of the pages: the strip draws them across the top in place of the
//! title, lights the one the title names, and the bumpers turn them as Left
//! and Right do.

use crate::bindings::SeqItem;
use isorun_webtiles::{formatted_string_to_text, GameState, Keys, MenuFlag, MenuState};

#[derive(Clone, Debug, PartialEq)]
pub struct PackTab {
    /// the page, whatever its label says (the Gear tab's count changes)
    pub id: &'static str,
    pub label: String,
    /// the start of crawl's title on this page
    pub title: &'static str,
    /// its icon, a tile name as the command lists name theirs
    pub tile: &'static str,
    /// nothing of its carried: the tab stands, dimmed, and crawl's turns pass it by (invent.cc `cycle_page`)
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackStrip {
    pub tabs: Vec<PackTab>,
    /// what the title says past the page's name: crawl's word on the keys that turn it
    pub hint: String,
    /// the tab of the page up, or -1 when the title names none of them
    pub current: i32,
}

/// the pack's gear slots, a-z and A-Z (invent.h MAX_GEAR); consumables take the slots after them
const MAX_GEAR: i64 = 52;
/// crawl's `object_class_type` (TAG_MAJOR_VERSION 34, so OBJ_FOOD and OBJ_RODS still hold their places)
const OBJ_WANDS: i64 = 3;
const OBJ_SCROLLS: i64 = 5;
const OBJ_POTIONS: i64 = 7;
const OBJ_MISCELLANY: i64 = 11;
const OBJ_BAUBLES: i64 = 19;

pub struct Page {
    pub id: &'static str,
    pub label: &'static str,
    pub title: &'static str,
    pub tile: &'static str,
    /// the item classes it holds; the first page holds the rest
    pub holds: &'static [i64],
    /// a page shown only while something of it is carried; the others stand empty so the player knows they are there
    pub rare: bool,
}

/// crawl's pages, in its order (invent.cc `modes`)
pub const PAGES: [Page; 4] = [
    Page { id: "gear", label: "Gear", title: "Gear:", tile: "CMD_DISPLAY_INVENTORY", holds: &[], rare: false },
    Page { id: "potions", label: "Potions", title: "Potions:", tile: "POTION_OFFSET", holds: &[OBJ_POTIONS], rare: false },
    Page { id: "scrolls", label: "Scrolls", title: "Scrolls:", tile: "SCROLL", holds: &[OBJ_SCROLLS], rare: false },
    Page { id: "evocables", label: "Evocables", title: "Evocable Items:", tile: "WAND_OFFSET", holds: &[OBJ_WANDS, OBJ_MISCELLANY, OBJ_BAUBLES], rare: true },
];

/// Crawl's paged pack, the one that is browsed: `i`, not drop (which marks rows) nor known items (which has no pages).
pub fn is_pack(menu: &MenuState) -> bool {
    menu.tag == "inventory" && menu.flags & MenuFlag::PAGED_INVENTORY != 0 && menu.flags & MenuFlag::MULTISELECT == 0
}

static HINT_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| regex::Regex::new(r"\([^()]*\)\s*$").unwrap());

/// The strip over the pack, lit on the page the title names; None for any other menu.
pub fn pack_strip(menu: &MenuState, st: &GameState) -> Option<PackStrip> {
    if !is_pack(menu) {
        return None;
    }
    let tabs = pack_pages(st);
    let title = formatted_string_to_text(&menu.title.text);
    let hint = HINT_RE.find(&title).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
    let current = tabs.iter().position(|t| title.starts_with(t.title)).map_or(-1, |i| i as i32);
    Some(PackStrip { tabs, hint, current })
}

/// The pack's pages: Gear, Potions and Scrolls always, the ones with nothing
/// carried marked empty, and Evocables only while some are carried. Gear
/// wears the title's slot count, "Gear (3/52)".
fn pack_pages(st: &GameState) -> Vec<PackTab> {
    let mut carried = std::collections::HashSet::new();
    let mut gear = 0;
    for it in st.player.inv.values() {
        let (Some(q), Some(base)) = (it.quantity, it.base_type) else { continue };
        if q <= 0 {
            continue;
        }
        carried.insert(base);
        if it.slot < MAX_GEAR {
            gear += 1;
        }
    }
    let mut out = Vec::new();
    for p in &PAGES {
        let tab = PackTab { id: p.id, label: p.label.to_string(), title: p.title, tile: p.tile, empty: false };
        if p.holds.is_empty() {
            out.push(PackTab { label: format!("{} ({gear}/{MAX_GEAR})", p.label), ..tab });
        } else if p.holds.iter().any(|b| carried.contains(b)) {
            out.push(tab);
        } else if !p.rare {
            out.push(PackTab { empty: true, ..tab });
        }
    }
    out
}

/// The keys that open the pack on the page left open last (its tab's `id`),
/// or on its first page when that page is gone or none was. crawl
/// always opens on the first page; the turns wait for the menu to be up.
pub fn open_pack_keys(st: &GameState, page: Option<&str>) -> Vec<SeqItem> {
    let tabs = pack_pages(st);
    let at = page.and_then(|p| tabs.iter().position(|t| t.id == p && !t.empty)).map_or(-1, |i| i as i32);
    let turns = if at > 0 { turn_keys(&PackStrip { tabs, hint: String::new(), current: 0 }, at).unwrap_or_default() } else { Vec::new() };
    let mut out = vec![SeqItem::text("i")];
    for (i, k) in turns.into_iter().enumerate() {
        out.push(if i == 0 { k.awaiting() } else { k });
    }
    out
}

/// The tabs crawl turns to, as places in the strip: the ones with something on them.
fn turnable(strip: &PackStrip) -> Vec<i32> {
    strip.tabs.iter().enumerate().filter(|(_, t)| !t.empty).map(|(i, _)| i as i32).collect()
}

/// The tab `step` turns on from the current one, wrapping round and passing the empty ones as crawl does; -1 with nowhere else to turn.
pub fn neighbour(strip: &PackStrip, step: i32) -> i32 {
    let live = turnable(strip);
    let n = live.len() as i32;
    if n < 2 {
        return -1;
    }
    let from = live.iter().position(|&i| i == strip.current).map_or(0, |i| i as i32);
    let to = live[(from + step).rem_euclid(n) as usize];
    if to == strip.current {
        -1
    } else {
        to
    }
}

/// The keys that turn the pack to tab `to`: crawl's Left or Right, once per page between. None where it is already, or for an empty tab.
pub fn turn_keys(strip: &PackStrip, to: i32) -> Option<Vec<SeqItem>> {
    let live = turnable(strip);
    let target = live.iter().position(|&i| i == to)? as i32;
    if to == strip.current {
        return None;
    }
    let n = live.len() as i32;
    let from = live.iter().position(|&i| i == strip.current).map_or(0, |i| i as i32);
    // the short way round
    let right = (target - from).rem_euclid(n);
    let key = if right <= n - right { Keys::CK_RIGHT } else { Keys::CK_LEFT };
    Some((0..right.min(n - right)).map(|_| SeqItem::key(key)).collect())
}

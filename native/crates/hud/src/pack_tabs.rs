//! The pack's pages as tabs: Gear (3/52) | Potions | Scrolls (| Evocables).
//!
//! Crawl's pack (`i`, and drop) is paged by category already: Left and Right turn the page
//! (invent.cc `InvMenu::cycle_page`), skipping the empty ones, and the title names the page it is
//! on (`InvMenu::set_title`: "Gear: ...", "Potions: ", "Scrolls: ", "Evocable Items: "). What
//! crawl lacks is a picture of the pages: the strip draws them across the top in place of the
//! title, lights the one the title names, and the bumpers turn them as Left and Right do. The
//! title is folded into the strip: its slot count ("Gear: 3/52 gear slots") onto the Gear tab,
//! where it stands on every page, and its word on the keys at the strip's other end.

use crate::gear::KeyStep;
use isorun_webtiles::{formatted_string_to_text, InvItem, Keys, MenuFlag, MenuState};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct PackTab {
    /// the page, whatever its label says (the Gear tab's count changes)
    pub id: &'static str,
    pub label: String,
    /// the start of crawl's title on this page
    pub title: &'static str,
    /// its icon, a tile name as the command lists name theirs
    pub tile: &'static str,
    /// nothing of its carried: the tab stands, dimmed, and crawl's turns pass it by
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackStrip {
    pub tabs: Vec<PackTab>,
    /// what the title says past the page's name: crawl's word on the keys that turn it
    pub hint: String,
    /// the tab of the page up, or None when the title names none of them
    pub current: Option<usize>,
}

/// the pack's gear slots, a-z and A-Z (invent.h MAX_GEAR); consumables take the slots after them
const MAX_GEAR: i64 = 52;

struct Page {
    id: &'static str,
    label: &'static str,
    title: &'static str,
    tile: &'static str,
    /// the item classes it holds; the first holds the rest
    holds: &'static [i64],
    /// a page shown only while something of it is carried; the others stand empty so the player
    /// knows they are there
    rare: bool,
}

/// crawl's pages, in its order (invent.cc `modes`)
const PAGES: [Page; 4] = [
    Page { id: "gear", label: "Gear", title: "Gear:", tile: "CMD_DISPLAY_INVENTORY", holds: &[], rare: false },
    Page { id: "potions", label: "Potions", title: "Potions:", tile: "POTION_OFFSET", holds: &[7], rare: false },
    Page { id: "scrolls", label: "Scrolls", title: "Scrolls:", tile: "SCROLL", holds: &[5], rare: false },
    Page { id: "evocables", label: "Evocables", title: "Evocable Items:", tile: "WAND_OFFSET", holds: &[3, 11, 19], rare: true },
];

/// Crawl's paged pack, the one that is browsed: `i`, not drop (which marks rows) nor known items
/// (which has no pages).
pub fn is_pack(menu: &MenuState) -> bool {
    menu.tag == "inventory" && menu.flags & MenuFlag::PAGED_INVENTORY != 0 && menu.flags & MenuFlag::MULTISELECT == 0
}

static HINT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\([^()]*\)\s*$").unwrap());

/// The strip over the pack, lit on the page the title names; None for any other menu.
pub fn pack_strip(menu: &MenuState, inv: &BTreeMap<i64, InvItem>) -> Option<PackStrip> {
    if !is_pack(menu) {
        return None;
    }
    let tabs = pack_pages(inv);
    let title = formatted_string_to_text(&menu.title.text);
    let hint = HINT.find(&title).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
    let current = tabs.iter().position(|t| title.starts_with(t.title));
    Some(PackStrip { tabs, hint, current })
}

fn carried(inv: &BTreeMap<i64, InvItem>) -> impl Iterator<Item = &InvItem> {
    inv.values().filter(|it| it.quantity.unwrap_or(0) > 0 && it.base_type.is_some())
}

/// The pack's pages: Gear, Potions and Scrolls always, the ones with nothing carried marked empty,
/// and Evocables only while some are carried. Gear wears the title's slot count, "Gear (3/52)":
/// the gear slots in use (invent.cc `slot_description`).
fn pack_pages(inv: &BTreeMap<i64, InvItem>) -> Vec<PackTab> {
    let mut classes = BTreeSet::new();
    let mut gear = 0;
    for it in carried(inv) {
        classes.insert(it.base_type.unwrap_or(-1));
        if it.slot < MAX_GEAR {
            gear += 1;
        }
    }
    let mut out = Vec::new();
    for p in &PAGES {
        let tab = |label: String, empty| PackTab { id: p.id, label, title: p.title, tile: p.tile, empty };
        if p.holds.is_empty() {
            out.push(tab(format!("{} ({gear}/{MAX_GEAR})", p.label), false));
        } else if p.holds.iter().any(|b| classes.contains(b)) {
            out.push(tab(p.label.into(), false));
        } else if !p.rare {
            out.push(tab(p.label.into(), true));
        }
    }
    out
}

/// The keys that open the pack on the page left open last (its tab's `id`), or on its first page
/// when that page is gone or none was. crawl always opens on the first page; the turns wait for
/// the menu to be up, so a pack crawl will not open ("You aren't carrying anything") leaves them
/// unsent.
pub fn open_pack_keys(inv: &BTreeMap<i64, InvItem>, page: Option<&str>) -> Vec<KeyStep> {
    let tabs = pack_pages(inv);
    let at = page.and_then(|p| tabs.iter().position(|t| t.id == p && !t.empty));
    let turns = match at {
        Some(at) if at > 0 => turn_keys(&PackStrip { tabs, hint: String::new(), current: Some(0) }, at).unwrap_or_default(),
        _ => vec![],
    };
    std::iter::once(KeyStep::text("i")).chain(turns.into_iter().enumerate().map(|(i, k)| if i == 0 { k.awaiting() } else { k })).collect()
}

/// The rows of the pack's fullest page: its items, and a header over each class of them (crawl
/// heads the pack's sections by item class). The pack is sized for it, so turning the page never
/// resizes it.
pub fn pack_rows(inv: &BTreeMap<i64, InvItem>) -> usize {
    let mut pages: Vec<(usize, BTreeSet<i64>)> = PAGES.iter().map(|_| (0, BTreeSet::new())).collect();
    for it in carried(inv) {
        let b = it.base_type.unwrap_or(-1);
        let at = PAGES.iter().position(|p| p.holds.contains(&b)).unwrap_or(0);
        pages[at].0 += 1;
        pages[at].1.insert(b);
    }
    pages.iter().map(|(n, c)| n + c.len()).max().unwrap_or(0)
}

/// The tabs crawl turns to, as places in the strip: the ones with something on them.
fn turnable(strip: &PackStrip) -> Vec<usize> {
    strip.tabs.iter().enumerate().filter(|(_, t)| !t.empty).map(|(i, _)| i).collect()
}

/// The tab `step` turns on from the current one, wrapping round and passing the empty ones as
/// crawl does; None with nowhere else to turn.
pub fn neighbour(strip: &PackStrip, step: i64) -> Option<usize> {
    let live = turnable(strip);
    let n = live.len() as i64;
    if n < 2 {
        return None;
    }
    let from = strip.current.and_then(|c| live.iter().position(|&i| i == c)).unwrap_or(0) as i64;
    let to = live[((from + step).rem_euclid(n)) as usize];
    (Some(to) != strip.current).then_some(to)
}

/// The keys that turn the pack to tab `to`: crawl's Left or Right, once per page between (the
/// empty ones are not). None where it is already, or for an empty tab.
pub fn turn_keys(strip: &PackStrip, to: usize) -> Option<Vec<KeyStep>> {
    let live = turnable(strip);
    let target = live.iter().position(|&i| i == to)? as i64;
    if Some(to) == strip.current {
        return None;
    }
    let n = live.len() as i64;
    let from = strip.current.and_then(|c| live.iter().position(|&i| i == c)).unwrap_or(0) as i64;
    // the short way round
    let right = (target - from).rem_euclid(n);
    let key = if right <= n - right { Keys::CK_RIGHT } else { Keys::CK_LEFT };
    Some((0..right.min(n - right)).map(|_| KeyStep::key(key)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::obj;
    use isorun_webtiles::MenuTitle;

    fn inv(items: &[(i64, i64)]) -> BTreeMap<i64, InvItem> {
        items.iter().map(|&(slot, base)| (slot, InvItem { slot, base_type: Some(base), quantity: Some(1), ..Default::default() })).collect()
    }

    fn pack_menu(title: &str) -> MenuState {
        MenuState { tag: "inventory".into(), flags: MenuFlag::PAGED_INVENTORY, title: MenuTitle { text: title.into(), ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn strip_lights_the_page() {
        let inv = inv(&[(0, obj::WEAPONS), (1, obj::ARMOUR), (53, obj::POTIONS)]);
        let s = pack_strip(&pack_menu("Potions: (Left/Right/Tab to switch category)"), &inv).unwrap();
        assert_eq!(s.tabs.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["Gear (2/52)", "Potions", "Scrolls"]);
        assert!(s.tabs[2].empty);
        assert_eq!(s.current, Some(1));
        assert_eq!(s.hint, "(Left/Right/Tab to switch category)");
        assert_eq!(neighbour(&s, 1), Some(0));
        assert_eq!(turn_keys(&s, 0), Some(vec![KeyStep::key(Keys::CK_RIGHT)]));
        assert_eq!(turn_keys(&s, 2), None);
        let mut drop = pack_menu("Drop");
        drop.flags |= MenuFlag::MULTISELECT;
        assert!(pack_strip(&drop, &inv).is_none());
    }

    #[test]
    fn evocables_only_when_carried() {
        let inv = inv(&[(0, obj::WEAPONS), (52, obj::WANDS), (53, obj::POTIONS), (54, obj::SCROLLS)]);
        let s = pack_strip(&pack_menu("Gear: 1/52"), &inv).unwrap();
        assert_eq!(s.tabs.len(), 4);
        assert_eq!(turn_keys(&s, 3), Some(vec![KeyStep::key(Keys::CK_LEFT)]));
        assert_eq!(open_pack_keys(&inv, Some("scrolls")), vec![KeyStep::text("i"), KeyStep::key(Keys::CK_RIGHT).awaiting(), KeyStep::key(Keys::CK_RIGHT)]);
        assert_eq!(pack_rows(&inv), 2);
    }
}

//! The order the Find screen lists results in, and what it marks on them.
//!
//! crawl sorts its results by distance (stash.cc `_compare_by_distance`): the levels by the
//! travel distance between them (`level_distance`, which knows the branch structure: the
//! client does not), the current level's results by their distance from the player, then by
//! name. That order is kept as the backbone, as the screen is for finding what is where and
//! going there: a level comes before the levels further away, and the first level crawl lists
//! after the player's own cell is the nearest.
//!
//! Within one level, what is new to the player comes first, then the rest, then what they hold
//! one of already, so a quick look at a level shows what is worth the walk:
//! - new: an unidentified item (its kind unknown: a potion, scroll, wand or piece of jewellery
//!   with no "of" in its name), an artefact, a rune or the Orb, a shop not visited yet (crawl
//!   colours it cyan);
//! - plain: everything else;
//! - known: an item of a kind the player carries (the pack's own rows of the same search).
//!
//! Inside each tier crawl's order stands (nearest first on the current level). Places (stairs,
//! altars, portals) are never "known": the player cannot carry one.

use crate::entry::{Category, Entry, Where, UNVISITED_SHOP_COLOUR};
use std::collections::{HashMap, HashSet};

/// How an entry stands with the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    New,
    Plain,
    Known,
}

/// How the list is sorted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sort {
    /// by level (nearest first), new before plain before known, then crawl's order
    #[default]
    Smart,
    /// by name, then crawl's order
    Name,
}

/// A result with what the screen shows beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub entry: Entry,
    pub tier: Tier,
    /// the level's place in crawl's distance order (0: the player's own)
    pub level_rank: usize,
    /// the level, as crawl names it (the player's own for `[right here]`)
    pub level: String,
    /// the order crawl gave it
    pub order: usize,
}

/// An item's name with the parts that differ between copies of one kind taken off: the count
/// or article, a plural, what crawl adds in brackets (`(weapon)`, `(worn)`), inscriptions.
pub fn kind_of(name: &str) -> String {
    let mut s = name.to_lowercase();
    for (open, close) in [('{', '}'), ('(', ')')] {
        while let (Some(a), Some(b)) = (s.find(open), s.find(close)) {
            if b <= a {
                break;
            }
            s.replace_range(a..=b, "");
        }
    }
    let mut words: Vec<&str> = s.split_whitespace().collect();
    if words.first().is_some_and(|w| matches!(*w, "a" | "an" | "the") || w.parse::<u32>().is_ok()) {
        words.remove(0);
    }
    words.iter().map(|w| if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") { &w[..w.len() - 1] } else { w }).collect::<Vec<_>>().join(" ")
}

/// The item's kind is not known yet: a potion, scroll, wand or ring whose name says what it
/// looks like, not what it is (`a murky emerald potion`, `a scroll labelled FOO`).
pub fn unidentified(e: &Entry) -> bool {
    matches!(e.category, Category::Potions | Category::Scrolls | Category::Wands | Category::Jewellery) && !e.name.contains(" of ")
}

/// An artefact by its name: an unrandart's own tile, a randart's quoted name or its properties
/// in braces.
pub fn artefact(e: &Entry) -> bool {
    e.category == Category::Artefacts || (e.is_item() && (e.name.contains('"') || e.name.contains('{')))
}

/// The tier of one result, given the kinds the player carries.
pub fn tier(e: &Entry, carried: &HashSet<String>) -> Tier {
    if e.category == Category::Shops {
        return if e.colour == Some(UNVISITED_SHOP_COLOUR) { Tier::New } else { Tier::Plain };
    }
    if e.category.is_place() {
        return Tier::Plain;
    }
    if unidentified(e) || artefact(e) || (e.category == Category::Treasure && !e.name.contains("gold")) {
        return Tier::New;
    }
    if carried.contains(&kind_of(&e.name)) {
        return Tier::Known;
    }
    Tier::Plain
}

/// Rank crawl's results: the pack's own rows are left out (they are the player's already) but
/// tell which kinds the player holds; `here` is the player's level as crawl names it, for the
/// results under their feet.
pub fn rank(entries: &[Entry], here: &str, sort: Sort) -> Vec<Ranked> {
    let carried: HashSet<String> = entries.iter().filter(|e| matches!(e.place, Where::Carried(_))).map(|e| kind_of(&e.name)).collect();
    let mut levels: HashMap<String, usize> = HashMap::new();
    // the player's own level first, whatever crawl lists first
    levels.insert(here.to_string(), 0);
    let mut out = Vec::new();
    for (order, e) in entries.iter().enumerate() {
        let level = match &e.place {
            Where::Carried(_) => continue,
            Where::Here => here.to_string(),
            Where::Level(l) => l.clone(),
        };
        let n = levels.len();
        let level_rank = *levels.entry(level.clone()).or_insert(n);
        out.push(Ranked { tier: tier(e, &carried), entry: e.clone(), level_rank, level, order });
    }
    match sort {
        Sort::Smart => out.sort_by_key(|r| (r.level_rank, r.tier, r.order)),
        Sort::Name => out.sort_by(|a, b| kind_of(&a.entry.name).cmp(&kind_of(&b.entry.name)).then(a.order.cmp(&b.order))),
    }
    out
}

/// How far a level is from the player's, in words where the branch is the same: "this level",
/// "2 levels down"; else None (another branch: its name says where).
pub fn distance_words(here: &str, level: &str) -> Option<String> {
    if here == level {
        return Some("this level".into());
    }
    let (hb, hd) = crate::entry::branch_depth(here);
    let (lb, ld) = crate::entry::branch_depth(level);
    if hb != lb {
        return None;
    }
    let d = ld? - hd?;
    let n = d.abs();
    let way = if d > 0 { "down" } else { "up" };
    Some(if n == 1 { format!("1 level {way}") } else { format!("{n} levels {way}") })
}

/// How many results each category has.
pub fn counts(list: &[Ranked]) -> HashMap<Category, usize> {
    let mut m = HashMap::new();
    for r in list {
        *m.entry(r.entry.category).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::tests::{fixture, names};
    use crate::entry::{parse_menu, parse_row};
    use isorun_webtiles::MenuItem;

    #[test]
    fn kinds() {
        assert_eq!(kind_of("2 potions of might"), "potion of might");
        assert_eq!(kind_of("a potion of might"), "potion of might");
        assert_eq!(kind_of("a +0 rapier (weapon)"), "+0 rapier");
        assert_eq!(kind_of("a +0 rapier {fast}"), "+0 rapier");
        assert_eq!(kind_of("3 scrolls labelled FOO BAR"), "scroll labelled foo bar");
    }

    #[test]
    fn the_recorded_search_ranked() {
        let e = parse_menu(&fixture(), &names);
        let r = rank(&e, "D:2", Sort::Smart);
        // the pack is left out
        assert!(r.iter().all(|x| !matches!(x.entry.place, Where::Carried(_))));
        assert_eq!(r.len(), e.len() - 7);
        // the player's level first, D:1 after it
        assert_eq!(r.first().unwrap().level, "D:2");
        assert_eq!(r.last().unwrap().level, "D:1");
        assert!(r.windows(2).all(|w| (w[0].level_rank, w[0].tier) <= (w[1].level_rank, w[1].tier)));
        // crawl's nearest-first order holds within a tier
        let d2: Vec<&str> = r.iter().filter(|x| x.level == "D:2" && x.tier == Tier::Plain).map(|x| x.entry.name.as_str()).collect();
        assert_eq!(&d2[..3], &["4 poisoned darts", "a falchion", "a +0 dagger"]);
        assert_eq!(counts(&r).get(&Category::Stairs), Some(&9));
        assert_eq!(distance_words("D:2", "D:1").as_deref(), Some("1 level up"));
        assert_eq!(distance_words("D:2", "Lair:1"), None);
    }

    #[test]
    fn new_before_plain_before_known() {
        let none = |_: &isorun_webtiles::TileRef| None;
        let row = |i: usize, text: &str| parse_row(i, &MenuItem { text: text.into(), level: Some(2), ..Default::default() }, &none).unwrap();
        let e = vec![
            row(0, " a - [carried] m - 2 potions of might"),
            row(1, " b - [D:3] a potion of might"),
            row(2, " c - [D:3] a +0 dagger"),
            row(3, " d - [D:3] a murky emerald potion"),
            row(4, " e - [D:4] a scroll labelled XYZ"),
        ];
        let r = rank(&e, "D:3", Sort::Smart);
        let names: Vec<&str> = r.iter().map(|x| x.entry.name.as_str()).collect();
        assert_eq!(names, ["a murky emerald potion", "a +0 dagger", "a potion of might", "a scroll labelled XYZ"]);
        assert_eq!(r[2].tier, Tier::Known);
        let by_name = rank(&e, "D:3", Sort::Name);
        assert_eq!(by_name[0].entry.name, "a +0 dagger");
    }
}

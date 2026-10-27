//! crawl's stash search results (stash.cc `display_search_results`) read back from its menu: each
//! row is `<hotkey> - [<where>] <what>`, with `[right here]` for the player's own cell, `[carried]
//! <letter> - ` for the pack (`_inventory_search`), an optional `(<n>) ` waypoint before the
//! place, and ` (<n> further duplicate(s)[ in <m> pile(s)])` after a match crawl folded others
//! into (`_stash_filter_duplicates`). Two subtitle rows head the player's position and the rest.
//! The row's tile (crawl's own: the item's, the shop's, the feature's) says what it is: a
//! category by its name in the tile tables, the text where there is no name.

use isorun_webtiles::{formatted_string_to_text, MenuItem, MenuState, TileRef};

/// What a result is, for the screen's categories, in the order they are offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Weapons,
    Armour,
    Ammo,
    Wands,
    Scrolls,
    Potions,
    Jewellery,
    /// staves, rods, spellbooks, manuals
    Magic,
    /// evokers, gizmos, talismans
    Misc,
    /// unrandarts (their tiles are their own, whatever the item)
    Artefacts,
    /// gold, gems, runes, the Orb
    Treasure,
    /// corpses, food and what no other category takes
    OtherItems,
    Stairs,
    /// branch entrances and exits, portals, transporters
    Portals,
    Altars,
    Shops,
    Traps,
    /// gates, runed doors, statues and the other notable terrain the tracker keeps
    Features,
}

impl Category {
    pub const ALL: [Category; 18] = [
        Category::Weapons,
        Category::Armour,
        Category::Ammo,
        Category::Wands,
        Category::Scrolls,
        Category::Potions,
        Category::Jewellery,
        Category::Magic,
        Category::Misc,
        Category::Artefacts,
        Category::Treasure,
        Category::OtherItems,
        Category::Stairs,
        Category::Portals,
        Category::Altars,
        Category::Shops,
        Category::Traps,
        Category::Features,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Weapons => "Weapons",
            Category::Armour => "Armour",
            Category::Ammo => "Ammo",
            Category::Wands => "Wands",
            Category::Scrolls => "Scrolls",
            Category::Potions => "Potions",
            Category::Jewellery => "Jewellery",
            Category::Magic => "Books & staves",
            Category::Misc => "Evokers",
            Category::Artefacts => "Artefacts",
            Category::Treasure => "Treasure",
            Category::OtherItems => "Other items",
            Category::Stairs => "Stairs",
            Category::Portals => "Branches & portals",
            Category::Altars => "Altars",
            Category::Shops => "Shops",
            Category::Traps => "Traps",
            Category::Features => "Features",
        }
    }

    /// A place to go rather than an item to pick up.
    pub fn is_place(self) -> bool {
        self >= Category::Stairs
    }

    /// The icon of the category's chip: a tile by its name in crawl's tile tables (the gui's
    /// command icons and the dungeon's and items' own), the first the gamedata has.
    pub fn icons(self) -> &'static [&'static str] {
        match self {
            Category::Weapons => &["WPN_LONG_SWORD", "WPN_SHORT_SWORD"],
            Category::Armour => &["ARM_CHAIN_MAIL", "ARM_LEATHER_ARMOUR"],
            Category::Ammo => &["MI_ARROW", "MI_DART"],
            Category::Wands => &["WAND_OFFSET", "I_WAND"],
            Category::Scrolls => &["SCR_BG", "SCROLL"],
            Category::Potions => &["POTION_OFFSET", "POT_BG"],
            Category::Jewellery => &["RING_NORMAL_OFFSET", "AMU_NORMAL_OFFSET"],
            Category::Magic => &["BOOK_PAPER_OFFSET", "STAFF_OFFSET"],
            Category::Misc => &["MISC_CRYSTAL_BALL_OF_ENERGY", "MISC_BOX_OF_BEASTS", "TALISMAN_BEAST"],
            Category::Artefacts => &["UNRAND_SINGING_SWORD", "UNRAND_BLOODBANE"],
            Category::Treasure => &["GOLD16", "RUNE_DIS"],
            Category::OtherItems => &["CORPSE_RAT", "FOOD_RATION"],
            Category::Stairs => &["DNGN_STONE_STAIRS_DOWN"],
            Category::Portals => &["DNGN_ENTER_LAIR", "DNGN_PORTAL"],
            Category::Altars => &["DNGN_ALTAR_TROG", "DNGN_UNKNOWN_ALTAR"],
            Category::Shops => &["SHOP_GENERAL", "DNGN_ENTER_SHOP"],
            Category::Traps => &["DNGN_TRAP_TELEPORT", "DNGN_TRAP_SHAFT"],
            Category::Features => &["DNGN_RUNED_DOOR", "GRANITE_STATUE"],
        }
    }
}

/// Where a result is, as the row says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Where {
    /// in the pack: `[carried] c - `
    Carried(char),
    /// under the player: `[right here]`
    Here,
    /// a level, as crawl names it (`D:2`, `Lair:1`, `Temple`)
    Level(String),
}

/// One result of crawl's search.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// the row's index in crawl's menu: what `menu_hover` names to pick it
    pub index: usize,
    /// what it is, in plain text, as crawl names it (`4 poisoned darts`, `an altar of Trog`)
    pub name: String,
    pub place: Where,
    /// the travel waypoint on it (`(3)`)
    pub waypoint: Option<char>,
    /// others crawl folded into it: count, and in how many piles
    pub duplicates: u32,
    pub piles: u32,
    /// crawl's colour for the row (menu_colour's: useless items dark grey, an unvisited shop cyan)
    pub colour: Option<i64>,
    pub tiles: Vec<TileRef>,
    pub category: Category,
}

/// crawl's CYAN: a shop not yet visited (`res.shop && !res.shop->is_visited()`).
pub const UNVISITED_SHOP_COLOUR: i64 = 3;

impl Entry {
    /// Something to pick up rather than somewhere to go.
    pub fn is_item(&self) -> bool {
        !self.category.is_place()
    }

    /// The row as crawl lists it, for a search over it.
    pub fn haystack(&self) -> String {
        let at = match &self.place {
            Where::Carried(_) => "carried".to_string(),
            Where::Here => "right here".to_string(),
            Where::Level(l) => l.clone(),
        };
        format!("{} {} {}", self.name, at, self.category.label()).to_lowercase()
    }
}

/// The name of a tile the server sent, by its texture's module: the dungeon's for its three
/// atlases, the items' (main), the player's, the gui's, the icons'. Variations resolve to their
/// base tile's name.
pub fn tile_name<'a>(gd: &'a isorun_gamedata::Gamedata, r: &TileRef) -> Option<&'a str> {
    let m = gd.module_for_texture(r.tex as u32)?;
    let id = gd.server_tile_for_texture(r.t as u32, r.tex as u32);
    let index = match m.atlas {
        "floor" | "wall" | "feat" => &gd.dngn,
        "main" => &gd.main,
        "player" => &gd.player,
        "gui" => &gd.gui,
        "icons" => &gd.icons,
        _ => return None,
    };
    index.base_name(id).or_else(|| index.name(id))
}

/// A category by the tile's name (tileinfo-main.js and tileinfo-feat.js), None for a name it
/// does not know.
pub fn category_of_tile(name: &str) -> Option<Category> {
    let n = name;
    let has = |p: &str| n.starts_with(p);
    Some(if has("UNRAND_") {
        Category::Artefacts
    } else if has("WPN_") {
        Category::Weapons
    } else if has("ARM_") || has("THELM") {
        Category::Armour
    } else if has("MI_") {
        Category::Ammo
    } else if has("WAND_") || n == "I_WAND" {
        Category::Wands
    } else if has("SCR_") || has("SCROLL") {
        Category::Scrolls
    } else if has("POT_") || has("POTION") {
        Category::Potions
    } else if has("RING_") || has("AMU_") {
        Category::Jewellery
    } else if has("STAFF_") || has("ROD_") || has("BOOK_") || has("PARCHMENT") {
        Category::Magic
    } else if has("MISC_") || has("GIZMO") || has("TALISMAN") || has("BAUBLE") {
        Category::Misc
    } else if has("GOLD") || has("GEM_") || has("RUNE_") || has("UNCOLLECTED_RUNE") || has("ORB") {
        Category::Treasure
    } else if has("CORPSE") || has("FOOD") {
        Category::OtherItems
    } else if has("SHOP_") || n == "DNGN_ABANDONED_SHOP" || n == "DNGN_ENTER_SHOP" {
        Category::Shops
    } else if has("DNGN_ALTAR") || n == "DNGN_UNKNOWN_ALTAR" {
        Category::Altars
    } else if has("DNGN_TRAP_") {
        Category::Traps
    } else if n.contains("STAIRS") || n.contains("ESCAPE_HATCH") || n == "DNGN_EXIT_DUNGEON" || n == "DNGN_ABYSSAL_STAIR" {
        Category::Stairs
    } else if has("DNGN_ENTER") || has("DNGN_EXIT") || has("DNGN_RETURN") || n.contains("PORTAL") || n.contains("TRANSPORTER") || n.contains("TRANSIT") {
        Category::Portals
    } else if has("DNGN_") || n.contains("STATUE") {
        Category::Features
    } else {
        return None;
    })
}

/// A category by crawl's words, where the tile has no name the client knows (no gamedata yet,
/// a server's new tile): the feature descriptions (feature-data.h) and the item names' kinds.
pub fn category_of_text(name: &str) -> Category {
    let t = name.to_lowercase();
    let has = |w: &str| t.contains(w);
    let word = |w: &str| t.split(|c: char| !c.is_alphanumeric()).any(|x| x == w);
    if has("staircase") || has("escape hatch") || has("stairs") {
        Category::Stairs
    } else if has("altar") {
        Category::Altars
    } else if has("trap") || word("shaft") || has("pressure plate") {
        Category::Traps
    } else if has("gate") || has("portal") || has("entrance") || has("exit") || has("gateway") || has("passage") || has("transporter") || has("hole to") {
        Category::Portals
    } else if has("shop") || has("store") || has("emporium") || has("boutique") || has("antique") || has("bazaar") || has("assorted") {
        Category::Shops
    } else if word("wand") {
        Category::Wands
    } else if word("scroll") || word("scrolls") {
        Category::Scrolls
    } else if word("potion") || word("potions") {
        Category::Potions
    } else if word("ring") || word("amulet") {
        Category::Jewellery
    } else if word("staff") || word("book") || word("manual") || word("parchment") || word("grimoire") {
        Category::Magic
    } else if word("talisman") {
        Category::Misc
    } else if word("gold") || word("rune") || word("gem") || has("orb of zot") {
        Category::Treasure
    } else if ["darts", "dart", "stones", "stone", "arrows", "arrow", "bolts", "bolt", "javelins", "javelin", "boomerangs", "boomerang", "bullets", "nets"]
        .iter()
        .any(|w| word(w))
    {
        Category::Ammo
    } else if word("corpse") || word("skeleton") || word("ration") || word("rations") {
        Category::OtherItems
    } else {
        Category::OtherItems
    }
}

/// Read one row: None for a subtitle or a row not sent yet.
pub fn parse_row(index: usize, item: &MenuItem, name_of: &dyn Fn(&TileRef) -> Option<String>) -> Option<Entry> {
    // MEL_SUBTITLE (1) and MEL_TITLE (0) are headings, MEL_ITEM (2) results
    if item.level.unwrap_or(2) < 2 {
        return None;
    }
    let text = formatted_string_to_text(&item.text);
    let mut rest = text.trim_start();
    // the hotkey: `a - `
    if let Some((k, r)) = rest.split_once(" - ") {
        if k.chars().count() == 1 {
            rest = r;
        }
    }
    let mut waypoint = None;
    if let Some(r) = rest.strip_prefix('(') {
        if let Some((n, after)) = r.split_once(") ") {
            if n.chars().count() == 1 {
                waypoint = n.chars().next();
                rest = after;
            }
        }
    }
    let mut place = None;
    if let Some(r) = rest.strip_prefix('[') {
        if let Some((p, after)) = r.split_once("] ") {
            rest = after;
            place = Some(if p == "right here" {
                Where::Here
            } else if p == "carried" {
                // `[carried] c - `: the pack's letter
                let mut letter = '?';
                if let Some((k, r)) = rest.split_once(" - ") {
                    if k.chars().count() == 1 {
                        letter = k.chars().next().unwrap_or('?');
                        rest = r;
                    }
                }
                Where::Carried(letter)
            } else {
                Where::Level(p.to_string())
            });
        }
    }
    // a result with no place is the pack's (older crawls write no `[carried]`)
    let place = place.unwrap_or(Where::Carried('?'));
    let (name, duplicates, piles) = split_duplicates(rest.trim());
    let tiles = item.tiles.clone().unwrap_or_default();
    let category = tiles.first().and_then(|t| name_of(t)).and_then(|n| category_of_tile(&n)).unwrap_or_else(|| category_of_text(&name));
    Some(Entry { index, name, place, waypoint, duplicates, piles, colour: item.colour, tiles, category })
}

/// ` (3 further duplicates in 2 piles)` off the end of a match.
fn split_duplicates(s: &str) -> (String, u32, u32) {
    if let Some(open) = s.rfind(" (") {
        let inner = s[open + 2..].trim_end_matches(')');
        if let Some((n, more)) = inner.split_once(" further duplicate") {
            if let Ok(n) = n.parse::<u32>() {
                let piles = more.split(" in ").nth(1).and_then(|p| p.split(' ').next()).and_then(|p| p.parse().ok()).unwrap_or(0);
                return (s[..open].to_string(), n, piles);
            }
        }
    }
    (s.to_string(), 0, 0)
}

/// Every result of crawl's stash menu (tag `stash`), in its order.
pub fn parse_menu(menu: &MenuState, name_of: &dyn Fn(&TileRef) -> Option<String>) -> Vec<Entry> {
    menu.items.iter().enumerate().filter_map(|(i, it)| it.as_ref().and_then(|it| parse_row(i, it, name_of))).collect()
}

/// The menu crawl's stash search shows its results in.
pub fn is_stash_menu(menu: &MenuState) -> bool {
    menu.tag == "stash"
}

/// The level the player is on, as crawl names levels in the results (`level_id::describe`: the
/// branch's abbreviation and the depth), from the `player` message's `place` (the branch's short
/// name, with an article for a portal's one-level branch) and `depth` (0 in a one-level branch).
pub fn level_label(place: &str, depth: i64) -> String {
    let p = place.trim();
    let p = ["The ", "the ", "an ", "a ", "An ", "A "].iter().find_map(|a| p.strip_prefix(a)).unwrap_or(p);
    // branch-data.h: the short names whose abbreviation differs
    const ABBREV: [(&str, &str); 14] = [
        ("Dungeon", "D"),
        ("Orcish Mines", "Orc"),
        ("Elven Halls", "Elf"),
        ("Dwarven Hall", "Dwarf"),
        ("Snake Pit", "Snake"),
        ("Spider Nest", "Spider"),
        ("Slime Pits", "Slime"),
        ("Hall of Blades", "Blade"),
        ("Gehenna", "Geh"),
        ("Cocytus", "Coc"),
        ("Tartarus", "Tar"),
        ("Pandemonium", "Pan"),
        ("Ziggurat", "Zig"),
        ("Labyrinth", "Lab"),
    ];
    let abbrev = ABBREV.iter().find(|(s, _)| s.eq_ignore_ascii_case(p)).map(|(_, a)| *a).unwrap_or(match p {
        "Ice Cave" | "ice cave" => "IceCv",
        "Wizlab" => "WizLab",
        _ => p,
    });
    if depth > 0 {
        format!("{abbrev}:{depth}")
    } else {
        abbrev.to_string()
    }
}

/// Split a label into its branch and depth (`D:2` is ("D", Some(2))).
pub fn branch_depth(label: &str) -> (&str, Option<i64>) {
    match label.split_once(':') {
        Some((b, d)) => (b, d.parse().ok()),
        None => (label, None),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture() -> MenuState {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/menus/menu-stash-1.json")).unwrap();
        let mut st = isorun_webtiles::GameState::default();
        for m in v["msgs"].as_array().unwrap() {
            isorun_webtiles::reduce(&mut st, m);
        }
        st.menus.last().unwrap().clone()
    }

    /// The fixture's tiles by their 0.34 names (tex 2 is the dungeon's features, 4 the items).
    pub fn names(r: &TileRef) -> Option<String> {
        Some(
            match (r.tex, r.t) {
                (2, 3023) => "DNGN_EXIT_DUNGEON",
                (2, 3024) => "DNGN_STONE_STAIRS_DOWN",
                (2, 3025) => "DNGN_STONE_STAIRS_UP",
                (2, 3027) => "DNGN_ESCAPE_HATCH_UP",
                (4, 3840) | (4, 3786) => "MI_DART",
                (4, 3621..=3700) => "WPN_DAGGER",
                (4, 3883..=3960) => "ARM_ROBE",
                (4, 4124) => "SCR_BG",
                (4, 4174) => "RING_NORMAL_OFFSET",
                (4, 4281..=4300) => "POTION_OFFSET",
                (4, 4606) => "TALISMAN_FLUX",
                _ => return None,
            }
            .to_string(),
        )
    }

    #[test]
    fn rows_of_the_recorded_search() {
        let m = fixture();
        assert!(is_stash_menu(&m));
        let e = parse_menu(&m, &names);
        // two subtitles are not results
        assert_eq!(e.len(), m.items.len() - 2);
        let carried: Vec<&Entry> = e.iter().filter(|x| matches!(x.place, Where::Carried(_))).collect();
        assert_eq!(carried[0].place, Where::Carried('a'));
        assert_eq!(carried[0].name, "a +0 rapier (weapon)");
        assert_eq!(carried[0].category, Category::Weapons);
        let darts = e.iter().find(|x| x.name == "4 poisoned darts").unwrap();
        assert_eq!((darts.place.clone(), darts.category, darts.index), (Where::Level("D:2".into()), Category::Ammo, 9));
        let dagger = e.iter().find(|x| x.name == "a +0 dagger").unwrap();
        assert_eq!(dagger.duplicates, 3);
        let hatch = e.iter().find(|x| x.name.contains("escape hatch")).unwrap();
        assert_eq!(hatch.category, Category::Stairs);
        let out = e.iter().find(|x| x.name.contains("out of the dungeon")).unwrap();
        assert_eq!(out.category, Category::Stairs);
        assert_eq!(e.iter().find(|x| x.name == "a spore talisman").unwrap().category, Category::Misc);
    }

    #[test]
    fn rows_crawl_writes() {
        let row = |text: &str| MenuItem { text: text.into(), level: Some(2), ..Default::default() };
        let none = |_: &TileRef| None;
        let e = parse_row(3, &row(" c - (2) [Lair:3] an altar of Trog"), &none).unwrap();
        assert_eq!((e.waypoint, e.place, e.name.as_str(), e.category), (Some('2'), Where::Level("Lair:3".into()), "an altar of Trog", Category::Altars));
        let e = parse_row(0, &row(" a - [right here] 12 gold pieces"), &none).unwrap();
        assert_eq!((e.place, e.category), (Where::Here, Category::Treasure));
        let e = parse_row(0, &row(" b - [D:4] a +0 arrow (14 further duplicates in 3 piles)"), &none).unwrap();
        assert_eq!((e.name.as_str(), e.duplicates, e.piles, e.category), ("a +0 arrow", 14, 3, Category::Ammo));
        let e = parse_row(0, &row(" d - [D:5] a gateway to a bazaar"), &none).unwrap();
        assert_eq!(e.category, Category::Portals);
        assert_eq!(parse_row(0, &MenuItem { text: "Results elsewhere".into(), level: Some(1), ..Default::default() }, &none), None);
    }

    #[test]
    fn tile_names_to_categories() {
        assert_eq!(category_of_tile("DNGN_ALTAR_TROG"), Some(Category::Altars));
        assert_eq!(category_of_tile("DNGN_ENTER_LAIR"), Some(Category::Portals));
        assert_eq!(category_of_tile("DNGN_PORTAL_BAZAAR"), Some(Category::Portals));
        assert_eq!(category_of_tile("DNGN_EXIT_DUNGEON"), Some(Category::Stairs));
        assert_eq!(category_of_tile("SHOP_WEAPONS"), Some(Category::Shops));
        assert_eq!(category_of_tile("DNGN_TRAP_SHAFT"), Some(Category::Traps));
        assert_eq!(category_of_tile("UNRAND_SINGING_SWORD"), Some(Category::Artefacts));
        assert_eq!(category_of_tile("THELM_HELM"), Some(Category::Armour));
        assert_eq!(category_of_tile("RUNE_DIS"), Some(Category::Treasure));
        assert_eq!(category_of_tile("WAND_OFFSET"), Some(Category::Wands));
    }

    #[test]
    fn the_players_level_as_crawl_names_it() {
        assert_eq!(level_label("Dungeon", 2), "D:2");
        assert_eq!(level_label("Orcish Mines", 1), "Orc:1");
        assert_eq!(level_label("Temple", 0), "Temple");
        assert_eq!(level_label("a Bazaar", 0), "Bazaar");
        assert_eq!(level_label("The Abyss", 0), "Abyss");
        assert_eq!(branch_depth("Lair:3"), ("Lair", Some(3)));
    }
}

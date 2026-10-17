//! The dock's quick slots: spells, abilities, the quivered shot and the consumables, each one
//! click (or one press of A on the pad) away.
//!
//! WebTiles sends no list of spells or abilities; crawl's own menus are the only place they are
//! named. Every time one of them is up, its rows are kept (`spells_from_menu`,
//! `abilities_from_menu`), so the slots know them from then on. A slot the player pinned keeps
//! what was put on it; the others fill in by themselves in a fixed order (`quick_slots`).

use crate::gear::PackItem;
use isorun_webtiles::{formatted_string_to_text, ItemTile, MenuState, TileLayer as WireLayer};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};

pub const SLOT_COUNT: usize = 10;

/// One layer of an icon: a tile in a texture, in the server's numbering.
#[derive(Clone, Debug, PartialEq)]
pub struct TileLayer {
    pub t: i64,
    pub tex: Option<i64>,
    pub ymax: Option<f64>,
}

/// A spell or ability as crawl's menu listed it: its letter, name and icon.
#[derive(Clone, Debug, PartialEq)]
pub struct KnownAction {
    pub letter: String,
    pub name: String,
    pub tiles: Vec<TileLayer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlotRef {
    Spell(String),
    Ability(String),
    Item(i64),
    Fire,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSlot {
    pub r: SlotRef,
    pub label: String,
    pub tiles: Vec<TileLayer>,
    /// the stack's count, for a consumable
    pub count: Option<i64>,
    /// the item is of no use to this character (the server's `useless`)
    pub useless: bool,
    /// the player put it here; the rest fill in by themselves
    pub pinned: bool,
    pub verb: Option<String>,
    pub letter: Option<String>,
}

pub struct SlotSources<'a> {
    pub spells: &'a [KnownAction],
    pub abilities: &'a [KnownAction],
    /// the pack's consumables, in the action panel's order (webtiles `action_panel_items`)
    pub consumables: Vec<PackItem>,
    pub items: Vec<PackItem>,
    /// crawl's description of the quivered action, when there is one ("Fire: 3 stones")
    pub quiver: String,
    /// the item the quiver fires, when it fires one (`quiver_item`)
    pub quiver_item: Option<PackItem>,
}

/// spl-cast.cc and ability.cc rows: the letter, a marker, then the name up to the next run of
/// spaces.
static KNOWN_ROW_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*([a-zA-Z])\s+[-+#]\s+(.+?)(?:\s{2,}|$)").unwrap());

/// The spells crawl's lists of known spells hold (`z *`, `I`: "Your spells (cast)", "Your spells
/// (describe)"); the memorise menu shares their tag and lists others.
pub fn spells_from_menu(menu: &MenuState) -> Option<Vec<KnownAction>> {
    (menu.tag == "spell" && formatted_string_to_text(&menu.title.text).trim_start().starts_with("Your spells")).then(|| known_rows(menu))
}

/// The abilities crawl's `a` menu lists: its tag is `ability`.
pub fn abilities_from_menu(menu: &MenuState) -> Option<Vec<KnownAction>> {
    (menu.tag == "ability").then(|| known_rows(menu))
}

fn known_rows(menu: &MenuState) -> Vec<KnownAction> {
    let mut out = Vec::new();
    for item in menu.items.iter().flatten() {
        if item.hotkeys.as_ref().is_none_or(|h| h.is_empty()) {
            continue;
        }
        let text = formatted_string_to_text(&item.text);
        let Some(m) = KNOWN_ROW_RE.captures(&text) else { continue };
        let tiles = item.tiles.iter().flatten().map(|t| TileLayer { t: t.t, tex: Some(t.tex), ymax: t.ymax }).collect();
        out.push(KnownAction { letter: m[1].to_string(), name: m[2].trim().to_string(), tiles });
    }
    out
}

/// tiles.h `TEX_DEFAULT`, the main atlas, in the server's texture numbering
pub const MAIN_TEXTURE: i64 = 4;

/// An item's icon layers: tileweb.cc `_send_item` sends plain ids in the main texture, the known
/// base item first.
pub fn item_tiles(item: &PackItem) -> Vec<TileLayer> {
    let layer = |l: &WireLayer| match l {
        WireLayer::Id(t) => TileLayer { t: *t, tex: Some(MAIN_TEXTURE), ymax: None },
        WireLayer::Tex(r) => TileLayer { t: r.t, tex: Some(r.tex), ymax: r.ymax },
    };
    match &item.item.tile {
        None => vec![],
        Some(ItemTile::Id(t)) => vec![TileLayer { t: *t, tex: Some(MAIN_TEXTURE), ymax: None }],
        Some(ItemTile::Layers(ls)) => ls.iter().map(layer).collect(),
    }
}

/// A slot for what `r` points at now, or None when it is gone (the spell forgotten, the last
/// potion drunk).
fn slot_for(r: &SlotRef, src: &SlotSources, pinned: bool) -> Option<QuickSlot> {
    let base = |label: String, tiles| QuickSlot { r: r.clone(), label, tiles, count: None, useless: false, pinned, verb: None, letter: None };
    match r {
        SlotRef::Fire => {
            if src.quiver.is_empty() {
                return None;
            }
            let mut s = base(src.quiver.clone(), src.quiver_item.as_ref().map(item_tiles).unwrap_or_default());
            s.count = src.quiver_item.as_ref().and_then(|i| i.item.quantity);
            Some(s)
        }
        SlotRef::Item(slot) => {
            let item = src.items.iter().find(|i| i.item.slot == *slot)?;
            let label = item.item.name.clone().unwrap_or_else(|| item.letter.clone());
            let mut s = base(label, item_tiles(item));
            s.count = item.count();
            s.useless = item.useless();
            s.verb = item.action_verb().map(String::from);
            s.letter = Some(item.letter.clone());
            Some(s)
        }
        SlotRef::Spell(letter) | SlotRef::Ability(letter) => {
            let list = if matches!(r, SlotRef::Spell(_)) { src.spells } else { src.abilities };
            let known = list.iter().find(|k| &k.letter == letter)?;
            Some(base(known.name.clone(), known.tiles.clone()))
        }
    }
}

/// The slots, SLOT_COUNT of them: each pinned one where the player put it (empty if what it held
/// is gone), the rest filled in order with the quivered shot, the spells, the abilities and then
/// the consumables.
pub fn quick_slots(pins: &[Option<SlotRef>], src: &SlotSources) -> Vec<Option<QuickSlot>> {
    let pin = |i: usize| pins.get(i).cloned().flatten();
    let mut out: Vec<Option<QuickSlot>> = (0..SLOT_COUNT).map(|i| pin(i).and_then(|r| slot_for(&r, src, true))).collect();
    let taken: Vec<SlotRef> = pins.iter().flatten().cloned().collect();
    let auto: Vec<SlotRef> = std::iter::once(SlotRef::Fire)
        .chain(src.spells.iter().map(|s| SlotRef::Spell(s.letter.clone())))
        .chain(src.abilities.iter().map(|a| SlotRef::Ability(a.letter.clone())))
        .chain(src.consumables.iter().map(|c| SlotRef::Item(c.item.slot)))
        .filter(|r| !taken.contains(r))
        .collect();
    let mut next = 0;
    for (i, slot) in out.iter_mut().enumerate() {
        if pin(i).is_some() {
            continue;
        }
        while next < auto.len() && slot.is_none() {
            *slot = slot_for(&auto[next], src, false);
            next += 1;
        }
    }
    out
}

/// What a slot can hold right now, for the chooser that pins one.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotChoice {
    pub r: SlotRef,
    pub label: String,
    pub group: &'static str,
    pub tiles: Vec<TileLayer>,
}

pub fn slot_choices(src: &SlotSources) -> Vec<SlotChoice> {
    let mut out = Vec::new();
    if !src.quiver.is_empty() {
        out.push(SlotChoice { r: SlotRef::Fire, label: src.quiver.clone(), group: "Quiver", tiles: src.quiver_item.as_ref().map(item_tiles).unwrap_or_default() });
    }
    for s in src.spells {
        out.push(SlotChoice { r: SlotRef::Spell(s.letter.clone()), label: s.name.clone(), group: "Spell", tiles: s.tiles.clone() });
    }
    for a in src.abilities {
        out.push(SlotChoice { r: SlotRef::Ability(a.letter.clone()), label: a.name.clone(), group: "Ability", tiles: a.tiles.clone() });
    }
    for c in src.items.iter().filter(|i| i.action_verb().is_some()) {
        out.push(SlotChoice { r: SlotRef::Item(c.item.slot), label: c.item.name.clone().unwrap_or_else(|| c.letter.clone()), group: "Item", tiles: item_tiles(c) });
    }
    out
}

impl SlotRef {
    /// As storage keeps it, the same shape as the web build's (`{kind, letter}`, `{kind, slot}`),
    /// so a profile carried over reads the same.
    pub fn to_json(&self) -> Value {
        match self {
            SlotRef::Spell(l) => json!({ "kind": "spell", "letter": l }),
            SlotRef::Ability(l) => json!({ "kind": "ability", "letter": l }),
            SlotRef::Item(s) => json!({ "kind": "item", "slot": s }),
            SlotRef::Fire => json!({ "kind": "fire" }),
        }
    }

    /// A slot's ref, checked: what storage gives back is whatever was written there.
    pub fn from_json(v: &Value) -> Option<SlotRef> {
        let o = v.as_object()?;
        match o.get("kind")?.as_str()? {
            "fire" => Some(SlotRef::Fire),
            "spell" => Some(SlotRef::Spell(o.get("letter")?.as_str()?.to_string())),
            "ability" => Some(SlotRef::Ability(o.get("letter")?.as_str()?.to_string())),
            "item" => Some(SlotRef::Item(o.get("slot")?.as_f64()? as i64)),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            SlotRef::Spell(_) => "spell",
            SlotRef::Ability(_) => "ability",
            SlotRef::Item(_) => "item",
            SlotRef::Fire => "fire",
        }
    }
}

impl KnownAction {
    pub fn to_json(&self) -> Value {
        let tiles: Vec<Value> = self
            .tiles
            .iter()
            .map(|t| {
                let mut o = serde_json::Map::new();
                o.insert("t".into(), json!(t.t));
                if let Some(tex) = t.tex {
                    o.insert("tex".into(), json!(tex));
                }
                if let Some(y) = t.ymax {
                    o.insert("ymax".into(), json!(y));
                }
                Value::Object(o)
            })
            .collect();
        json!({ "letter": self.letter, "name": self.name, "tiles": tiles })
    }

    /// A remembered spell or ability, checked as `SlotRef::from_json` checks a ref.
    pub fn from_json(v: &Value) -> Option<KnownAction> {
        let o = v.as_object()?;
        let letter = o.get("letter")?.as_str()?.to_string();
        let name = o.get("name")?.as_str()?.to_string();
        let tiles = o
            .get("tiles")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().filter_map(|t| Some(TileLayer { t: t.get("t")?.as_f64()? as i64, tex: t.get("tex").and_then(|x| x.as_f64()).map(|x| x as i64), ymax: None })).collect())
            .unwrap_or_default();
        Some(KnownAction { letter, name, tiles })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::obj;
    use crate::gear::tests::{item, pack};
    use isorun_webtiles::{MenuItem, MenuTitle, TileRef};

    fn known(letter: &str, name: &str) -> KnownAction {
        KnownAction { letter: letter.into(), name: name.into(), tiles: vec![] }
    }

    fn menu(tag: &str, title: &str, rows: &[&str]) -> MenuState {
        MenuState {
            tag: tag.into(),
            title: MenuTitle { text: title.into(), ..Default::default() },
            items: rows.iter().map(|r| Some(MenuItem { text: r.to_string(), hotkeys: Some(vec![r.trim().chars().next().unwrap() as i64]), tiles: Some(vec![TileRef { t: 7, tex: 3, ymax: None }]), ..Default::default() })).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn spells_and_abilities_from_menus() {
        let m = menu("spell", "<w>Your spells (cast)</w>", &[" a - Magic Dart              Conj      1    0%", " b - Foxfire     Conj/Fire   1   12%"]);
        let s = spells_from_menu(&m).unwrap();
        assert_eq!(s, vec![KnownAction { letter: "a".into(), name: "Magic Dart".into(), tiles: vec![TileLayer { t: 7, tex: Some(3), ymax: None }] }, KnownAction { letter: "b".into(), name: "Foxfire".into(), tiles: vec![TileLayer { t: 7, tex: Some(3), ymax: None }] }]);
        assert!(spells_from_menu(&menu("spell", "Memorise which spell?", &[])).is_none());
        assert_eq!(abilities_from_menu(&menu("ability", "Abilities", &["a + Berserk     Piety"])).unwrap()[0].name, "Berserk");
    }

    #[test]
    fn fills_in_order_around_pins() {
        let mut potion = pack(&[item(53, "3 potions of curing", obj::POTIONS, 1)]).remove(0);
        potion.item.quantity = Some(3);
        potion.item.extra.insert("action_verb".into(), serde_json::json!("Quaff"));
        let spells = [known("a", "Magic Dart"), known("b", "Foxfire")];
        let abilities = [known("a", "Berserk")];
        let src = SlotSources { spells: &spells, abilities: &abilities, consumables: vec![potion.clone()], items: vec![potion], quiver: "Fire: 3 stones".into(), quiver_item: None };
        let mut pins = vec![None; SLOT_COUNT];
        pins[2] = Some(SlotRef::Spell("b".into()));
        pins[9] = Some(SlotRef::Spell("z".into()));
        let s = quick_slots(&pins, &src);
        let refs: Vec<Option<SlotRef>> = s.iter().map(|q| q.as_ref().map(|q| q.r.clone())).collect();
        assert_eq!(refs[0], Some(SlotRef::Fire));
        assert_eq!(refs[1], Some(SlotRef::Spell("a".into())));
        assert_eq!(refs[2], Some(SlotRef::Spell("b".into())));
        assert!(s[2].as_ref().unwrap().pinned);
        assert_eq!(refs[3], Some(SlotRef::Ability("a".into())));
        assert_eq!(refs[4], Some(SlotRef::Item(53)));
        assert_eq!(s[4].as_ref().unwrap().count, Some(3));
        assert_eq!(s[4].as_ref().unwrap().verb.as_deref(), Some("Quaff"));
        assert_eq!(refs[5], None);
        // a pin whose spell is gone stays empty rather than filling in
        assert_eq!(refs[9], None);
        assert_eq!(slot_choices(&src).iter().map(|c| c.group).collect::<Vec<_>>(), ["Quiver", "Spell", "Spell", "Ability", "Item"]);
    }

    #[test]
    fn refs_round_trip_and_reject_junk() {
        for r in [SlotRef::Fire, SlotRef::Spell("a".into()), SlotRef::Ability("b".into()), SlotRef::Item(54)] {
            assert_eq!(SlotRef::from_json(&r.to_json()), Some(r));
        }
        assert_eq!(SlotRef::from_json(&serde_json::json!({ "kind": "spell" })), None);
        assert_eq!(SlotRef::from_json(&serde_json::json!(3)), None);
        let k = KnownAction { letter: "a".into(), name: "Magic Dart".into(), tiles: vec![TileLayer { t: 1, tex: Some(2), ymax: None }] };
        assert_eq!(KnownAction::from_json(&k.to_json()), Some(k));
    }
}

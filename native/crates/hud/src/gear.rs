//! The pack as the inventory screen draws it: what each item is worn as, and where on the paper
//! doll it hangs.
//!
//! The `player` message names the wielded weapon (`weapon_index`) and an offhand weapon
//! (`offhand_index`), and nothing else about what is worn (tileweb.cc `_send_player`). Crawl's
//! own pack says it after each name (item-name.cc `DESC_INVENTORY_EQUIP`: "(weapon)", "(worn)",
//! ...), so the screen reads the rest off that menu (`worn_from_menu`).

use isorun_webtiles::{formatted_string_to_text, index_to_letter, InvItem, MenuItem, PlayerState};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;

/// crawl's `object_class_type` (TAG_MAJOR_VERSION 34)
pub mod obj {
    pub const WEAPONS: i64 = 0;
    pub const MISSILES: i64 = 1;
    pub const ARMOUR: i64 = 2;
    pub const WANDS: i64 = 3;
    pub const SCROLLS: i64 = 5;
    pub const JEWELLERY: i64 = 6;
    pub const POTIONS: i64 = 7;
    pub const BOOKS: i64 = 8;
    pub const STAVES: i64 = 9;
    pub const MISCELLANY: i64 = 11;
    pub const TALISMANS: i64 = 16;
    pub const GIZMOS: i64 = 18;
    pub const BAUBLES: i64 = 19;
}

/// jewellery.h: the amulets start here (`AMU_FIRST_AMULET`), the rings are below
const FIRST_AMULET: i64 = 35;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Weapon,
    Offhand,
    Body,
    Cloak,
    Helmet,
    Gloves,
    Boots,
    Amulet,
    Ring,
    Talisman,
    Gizmo,
}

/// item-prop-enum.h `armour_type` under TAG_MAJOR_VERSION 34, by the slot each goes on; the rest
/// are body armour
fn armour_slot(sub: i64) -> EquipSlot {
    match sub {
        6 | 41 => EquipSlot::Cloak,
        7..=9 => EquipSlot::Helmet,
        10 => EquipSlot::Gloves,
        11 | 35 | 36 => EquipSlot::Boots,
        12..=14 | 42 => EquipSlot::Offhand,
        _ => EquipSlot::Body,
    }
}

/// What crawl prints after an item's name in the pack (item-name.cc `DESC_INVENTORY_EQUIP`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Worn {
    Weapon,
    Offhand,
    Worn,
    Installed,
    Melded,
    Quivered,
    Haunted,
}

impl Worn {
    pub fn parse(s: &str) -> Option<Worn> {
        Some(match s {
            "weapon" => Worn::Weapon,
            "offhand" => Worn::Offhand,
            "worn" => Worn::Worn,
            "installed" => Worn::Installed,
            "melded" => Worn::Melded,
            "quivered" => Worn::Quivered,
            "haunted" => Worn::Haunted,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Worn::Weapon => "weapon",
            Worn::Offhand => "offhand",
            Worn::Worn => "worn",
            Worn::Installed => "installed",
            Worn::Melded => "melded",
            Worn::Quivered => "quivered",
            Worn::Haunted => "haunted",
        }
    }

    /// How a card says it (screens.ts `wornWord`).
    pub fn word(self) -> &'static str {
        match self {
            Worn::Weapon => "wielded",
            Worn::Offhand => "in the off hand",
            Worn::Worn => "worn",
            Worn::Installed => "installed",
            Worn::Melded => "melded",
            Worn::Quivered => "quivered",
            Worn::Haunted => "haunted",
        }
    }
}

/// An item carried, with its inventory letter as crawl prints it: the gear's a-z and A-Z, the
/// consumables' own after them.
#[derive(Clone, Debug, PartialEq)]
pub struct PackItem {
    pub item: InvItem,
    pub letter: String,
}

impl PackItem {
    pub fn slot(&self) -> i64 {
        self.item.slot
    }
    pub fn name(&self) -> &str {
        self.item.name.as_deref().unwrap_or("")
    }
    /// The server's verb for using it (`action_verb`: Quaff, Read, Evoke, Throw), when it has one.
    pub fn action_verb(&self) -> Option<&str> {
        self.item.extra.get("action_verb").and_then(|v| v.as_str())
    }
    /// The server marks what is of no use to this character.
    pub fn useless(&self) -> bool {
        self.item.extra.get("useless").and_then(|v| v.as_bool()).unwrap_or(false)
    }
    /// The count shown on the item: the field `qty_field` names (charges), else the quantity.
    pub fn count(&self) -> Option<i64> {
        match self.item.extra.get("qty_field").and_then(|v| v.as_str()) {
            Some("quantity") => self.item.quantity,
            Some(f) => self.item.extra.get(f).and_then(|v| v.as_i64()),
            None => self.item.quantity,
        }
    }
    pub fn col(&self) -> i64 {
        self.item.col.unwrap_or(7)
    }
}

static WORN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\((weapon|offhand|worn|installed|melded|quivered|haunted)\)").unwrap());
static ROW_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*([a-zA-Z])\s*[-+#]\s").unwrap());

/// The items carried, in pack order, each with its letter (tileweb.cc `_send_item` sends it as
/// a character code).
pub fn pack_items(inv: &BTreeMap<i64, InvItem>) -> Vec<PackItem> {
    let mut out: Vec<PackItem> = inv
        .values()
        .filter(|it| it.quantity.unwrap_or(0) > 0 && it.base_type.is_some())
        .map(|it| {
            let letter = match it.extra.get("letter") {
                Some(serde_json::Value::Number(n)) => n.as_u64().and_then(|c| char::from_u32(c as u32)).map(String::from).unwrap_or_else(|| index_to_letter(it.slot)),
                Some(serde_json::Value::String(s)) if !s.is_empty() => s.clone(),
                _ => index_to_letter(it.slot),
            };
            PackItem { item: it.clone(), letter }
        })
        .collect();
    out.sort_by_key(|p| p.item.slot);
    out
}

/// What each letter of crawl's pack is worn as, read off the pack's rows ("a - a +0 war axe
/// (weapon)").
pub fn worn_from_menu<'a>(items: impl IntoIterator<Item = &'a Option<MenuItem>>) -> BTreeMap<String, Worn> {
    let mut out = BTreeMap::new();
    for item in items.into_iter().flatten() {
        let text = formatted_string_to_text(&item.text);
        if let (Some(row), Some(worn)) = (ROW_RE.captures(&text), WORN_RE.captures(&text)) {
            if let Some(w) = Worn::parse(&worn[1]) {
                out.insert(row[1].to_string(), w);
            }
        }
    }
    out
}

/// The doll slot an item goes on, or None for what is not worn (potions, scrolls, missiles).
pub fn equip_slot_of(item: &InvItem) -> Option<EquipSlot> {
    match item.base_type? {
        obj::WEAPONS | obj::STAVES => Some(EquipSlot::Weapon),
        obj::ARMOUR => Some(armour_slot(item.sub_type.unwrap_or(-1))),
        obj::JEWELLERY => Some(if item.sub_type.unwrap_or(0) >= FIRST_AMULET { EquipSlot::Amulet } else { EquipSlot::Ring }),
        obj::TALISMANS => Some(EquipSlot::Talisman),
        obj::GIZMOS => Some(EquipSlot::Gizmo),
        _ => None,
    }
}

/// What is worn, by letter: the pack's own word where it was read (`known`), and the `player`
/// message's weapon and offhand weapon over it, which are always current.
pub fn worn_now(player: &PlayerState, items: &[PackItem], known: &BTreeMap<String, Worn>) -> BTreeMap<String, Worn> {
    let mut out: BTreeMap<String, Worn> = known.iter().filter(|(_, w)| !matches!(w, Worn::Weapon | Worn::Offhand)).map(|(k, v)| (k.clone(), *v)).collect();
    if let Some(w) = items.iter().find(|i| i.item.slot == player.weapon_index) {
        out.insert(w.letter.clone(), Worn::Weapon);
    }
    if let Some(o) = items.iter().find(|i| i.item.slot == player.offhand_index) {
        out.insert(o.letter.clone(), Worn::Offhand);
    }
    out
}

/// The paper doll's places, in the order the pad walks them: the left column, the right column,
/// the row under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DollPlace {
    Helmet,
    Cloak,
    Body,
    Gloves,
    Boots,
    Amulet,
    Ring1,
    Ring2,
    Weapon,
    Offhand,
    Talisman,
    Gizmo,
    Quiver,
}

pub const DOLL_PLACES: [DollPlace; 13] = [
    DollPlace::Helmet,
    DollPlace::Cloak,
    DollPlace::Body,
    DollPlace::Gloves,
    DollPlace::Boots,
    DollPlace::Amulet,
    DollPlace::Ring1,
    DollPlace::Ring2,
    DollPlace::Weapon,
    DollPlace::Offhand,
    DollPlace::Talisman,
    DollPlace::Gizmo,
    DollPlace::Quiver,
];

impl DollPlace {
    pub fn id(self) -> &'static str {
        match self {
            DollPlace::Helmet => "helmet",
            DollPlace::Cloak => "cloak",
            DollPlace::Body => "body",
            DollPlace::Gloves => "gloves",
            DollPlace::Boots => "boots",
            DollPlace::Amulet => "amulet",
            DollPlace::Ring1 => "ring1",
            DollPlace::Ring2 => "ring2",
            DollPlace::Weapon => "weapon",
            DollPlace::Offhand => "offhand",
            DollPlace::Talisman => "talisman",
            DollPlace::Gizmo => "gizmo",
            DollPlace::Quiver => "quiver",
        }
    }

    /// The equip slot this place takes, or None for the quiver.
    pub fn slot(self) -> Option<EquipSlot> {
        Some(match self {
            DollPlace::Helmet => EquipSlot::Helmet,
            DollPlace::Cloak => EquipSlot::Cloak,
            DollPlace::Body => EquipSlot::Body,
            DollPlace::Gloves => EquipSlot::Gloves,
            DollPlace::Boots => EquipSlot::Boots,
            DollPlace::Amulet => EquipSlot::Amulet,
            DollPlace::Ring1 | DollPlace::Ring2 => EquipSlot::Ring,
            DollPlace::Weapon => EquipSlot::Weapon,
            DollPlace::Offhand => EquipSlot::Offhand,
            DollPlace::Talisman => EquipSlot::Talisman,
            DollPlace::Gizmo => EquipSlot::Gizmo,
            DollPlace::Quiver => return None,
        })
    }
}

/// The item on each place of the doll; the quiver is the quivered missile or launcher ammunition.
pub fn paper_doll<'a>(items: &'a [PackItem], worn: &BTreeMap<String, Worn>) -> BTreeMap<DollPlace, &'a PackItem> {
    let mut doll: BTreeMap<DollPlace, &PackItem> = BTreeMap::new();
    for item in items {
        let Some(w) = worn.get(&item.letter) else { continue };
        if *w == Worn::Quivered {
            doll.entry(DollPlace::Quiver).or_insert(item);
            continue;
        }
        if *w == Worn::Offhand {
            doll.insert(DollPlace::Offhand, item);
            continue;
        }
        let Some(slot) = equip_slot_of(&item.item) else { continue };
        let place = match slot {
            EquipSlot::Ring => {
                if !doll.contains_key(&DollPlace::Ring1) {
                    DollPlace::Ring1
                } else {
                    DollPlace::Ring2
                }
            }
            EquipSlot::Weapon => DollPlace::Weapon,
            EquipSlot::Offhand => DollPlace::Offhand,
            EquipSlot::Body => DollPlace::Body,
            EquipSlot::Cloak => DollPlace::Cloak,
            EquipSlot::Helmet => DollPlace::Helmet,
            EquipSlot::Gloves => DollPlace::Gloves,
            EquipSlot::Boots => DollPlace::Boots,
            EquipSlot::Amulet => DollPlace::Amulet,
            EquipSlot::Talisman => DollPlace::Talisman,
            EquipSlot::Gizmo => DollPlace::Gizmo,
        };
        doll.entry(place).or_insert(item);
    }
    doll
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemActionId {
    Equip,
    Unequip,
    Use,
    Drop,
    Describe,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemAction {
    pub id: ItemActionId,
    pub label: String,
}

/// The verb crawl's own commands use for putting the item on, by its slot.
fn equip_verb(slot: EquipSlot) -> &'static str {
    match slot {
        EquipSlot::Weapon => "Wield",
        EquipSlot::Amulet | EquipSlot::Ring => "Put on",
        _ => "Wear",
    }
}

fn unequip_verb(slot: EquipSlot) -> &'static str {
    match slot {
        EquipSlot::Weapon => "Unwield",
        EquipSlot::Amulet | EquipSlot::Ring => "Remove",
        _ => "Take off",
    }
}

/// What can be done with an item from the inventory screen, the most likely first: put it on or
/// take it off, use it (the server's own verb for it, `action_verb`: Quaff, Read, Evoke, Throw),
/// drop it, read its description.
pub fn item_actions(item: &PackItem, worn: Option<Worn>) -> Vec<ItemAction> {
    let mut out = Vec::new();
    let slot = equip_slot_of(&item.item);
    if let Some(slot) = slot {
        if worn.is_some_and(|w| w != Worn::Quivered) {
            out.push(ItemAction { id: ItemActionId::Unequip, label: unequip_verb(slot).into() });
        } else {
            out.push(ItemAction { id: ItemActionId::Equip, label: equip_verb(slot).into() });
        }
    }
    let verb = item.action_verb().unwrap_or("");
    if !verb.is_empty() && verb.to_lowercase() != equip_verb(slot.unwrap_or(EquipSlot::Body)).to_lowercase() {
        out.push(ItemAction { id: ItemActionId::Use, label: sentence_case(verb) });
    }
    out.push(ItemAction { id: ItemActionId::Drop, label: "Drop".into() });
    out.push(ItemAction { id: ItemActionId::Describe, label: "Describe".into() });
    out
}

/// One key or text of a sequence sent to crawl, and whether it waits for crawl's prompt first
/// (bindings.ts `KeyOrText`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyStep {
    Text { text: String, await_prompt: bool },
    Key { key: i32, await_prompt: bool },
}

impl KeyStep {
    pub fn text(t: &str) -> KeyStep {
        KeyStep::Text { text: t.into(), await_prompt: false }
    }
    pub fn key(k: i32) -> KeyStep {
        KeyStep::Key { key: k, await_prompt: false }
    }
    pub fn awaiting(self) -> KeyStep {
        match self {
            KeyStep::Text { text, .. } => KeyStep::Text { text, await_prompt: true },
            KeyStep::Key { key, .. } => KeyStep::Key { key, await_prompt: true },
        }
    }
}

/// The keys an action sends after the item's description is up (`inv_item_describe`): its own
/// action line's key (describe.cc `_get_action`), so crawl puts the item on or takes it off, or
/// says why it cannot (a Minotaur's helmet). Crawl's equip (`e`) and unequip (`c`) menus leave
/// out what cannot be worn, and the letter would do nothing there. A gizmo has no such key and
/// goes through those menus. Drop is the description's `d` too, since crawl's drop menu shows one
/// page of the pack at a time and a letter there means a row of that page only.
pub fn item_action_keys(id: ItemActionId, item: &PackItem) -> Option<(bool, Vec<KeyStep>)> {
    let slot = equip_slot_of(&item.item);
    if id == ItemActionId::Drop {
        return Some((true, vec![KeyStep::text("d")]));
    }
    if id != ItemActionId::Equip && id != ItemActionId::Unequip {
        return None;
    }
    let equip = id == ItemActionId::Equip;
    match slot {
        None | Some(EquipSlot::Gizmo) => Some((false, vec![KeyStep::text(if equip { "e" } else { "c" }), KeyStep::text(&item.letter).awaiting()])),
        Some(slot) => {
            let (on, off) = match slot {
                EquipSlot::Weapon => ("w", "u"),
                EquipSlot::Amulet | EquipSlot::Ring | EquipSlot::Talisman => ("p", "r"),
                _ => ("w", "t"),
            };
            Some((true, vec![KeyStep::text(if equip { on } else { off })]))
        }
    }
}

/// The key crawl uses an item with, by the server's verb for it.
pub fn use_key(verb: Option<&str>) -> Option<&'static str> {
    match verb?.to_lowercase().as_str() {
        "quaff" | "drink" => Some("q"),
        "read" => Some("r"),
        "evoke" => Some("V"),
        "throw" | "fire" => Some("F"),
        _ => None,
    }
}

/// The keys an action is on in crawl, for the keycaps beside it.
pub fn item_action_keycaps(action: &ItemAction, letter: &str) -> Vec<String> {
    let k = |a: &str| vec![a.to_string(), letter.to_string()];
    match action.id {
        ItemActionId::Equip => k("e"),
        ItemActionId::Unequip => k("c"),
        ItemActionId::Drop => k("d"),
        ItemActionId::Describe => k("i"),
        ItemActionId::Use => use_key(Some(&action.label)).map(k).unwrap_or_default(),
    }
}

fn sentence_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// The item's name without the article crawl puts first ("a +0 war axe" reads "+0 war axe" on a
/// card).
pub fn item_title(name: &str) -> String {
    static ART: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(a|an|the|some)\s+").unwrap());
    ART.replace(name, "").into_owned()
}

/// The weapon line of the stats pane, as player.js writes it: the wielded item's name, or the
/// unarmed attack.
pub fn weapon_line(p: &PlayerState) -> String {
    let name = if p.weapon_index >= 0 { p.inv.get(&p.weapon_index).and_then(|i| i.name.clone()).unwrap_or_default() } else { String::new() };
    if !name.is_empty() {
        formatted_string_to_text(&name)
    } else if !p.unarmed_attack.is_empty() {
        p.unarmed_attack.clone()
    } else {
        "Nothing wielded".into()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub fn item(slot: i64, name: &str, base: i64, sub: i64) -> InvItem {
        InvItem { slot, name: Some(name.into()), base_type: Some(base), sub_type: Some(sub), quantity: Some(1), col: Some(7), ..Default::default() }
    }

    pub fn pack(items: &[InvItem]) -> Vec<PackItem> {
        let inv: BTreeMap<i64, InvItem> = items.iter().map(|i| (i.slot, i.clone())).collect();
        pack_items(&inv)
    }

    #[test]
    fn letters_and_order() {
        let mut potion = item(53, "a potion of curing", obj::POTIONS, 1);
        potion.extra.insert("letter".into(), json!(98));
        let p = pack(&[potion, item(0, "a +0 war axe", obj::WEAPONS, 9), InvItem { slot: 3, quantity: Some(0), base_type: Some(0), ..Default::default() }]);
        assert_eq!(p.iter().map(|i| i.letter.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn reads_worn_off_the_pack() {
        let row = |t: &str| Some(MenuItem { text: t.into(), ..Default::default() });
        let w = worn_from_menu(&[row("a - a +0 war axe (weapon)"), row("b - a +2 leather armour (worn)"), row("<white>c + 3 darts (quivered)</white>"), row("Hand Weapons"), None]);
        assert_eq!(w.get("a"), Some(&Worn::Weapon));
        assert_eq!(w.get("b"), Some(&Worn::Worn));
        assert_eq!(w.get("c"), Some(&Worn::Quivered));
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn slots() {
        assert_eq!(equip_slot_of(&item(0, "", obj::ARMOUR, 7)), Some(EquipSlot::Helmet));
        assert_eq!(equip_slot_of(&item(0, "", obj::ARMOUR, 13)), Some(EquipSlot::Offhand));
        assert_eq!(equip_slot_of(&item(0, "", obj::ARMOUR, 1)), Some(EquipSlot::Body));
        assert_eq!(equip_slot_of(&item(0, "", obj::JEWELLERY, 40)), Some(EquipSlot::Amulet));
        assert_eq!(equip_slot_of(&item(0, "", obj::JEWELLERY, 3)), Some(EquipSlot::Ring));
        assert_eq!(equip_slot_of(&item(0, "", obj::POTIONS, 3)), None);
    }

    #[test]
    fn the_player_message_wins_for_weapons() {
        let items = pack(&[item(0, "a dagger", obj::WEAPONS, 1), item(1, "a club", obj::WEAPONS, 0), item(2, "a ring", obj::JEWELLERY, 2), item(3, "a ring", obj::JEWELLERY, 4), item(4, "a ring", obj::JEWELLERY, 5)]);
        let mut known = BTreeMap::new();
        known.insert("a".to_string(), Worn::Weapon);
        known.insert("c".to_string(), Worn::Worn);
        known.insert("d".to_string(), Worn::Worn);
        known.insert("e".to_string(), Worn::Worn);
        let p = PlayerState { weapon_index: 1, offhand_index: -1, ..Default::default() };
        let worn = worn_now(&p, &items, &known);
        assert_eq!(worn.get("a"), None);
        assert_eq!(worn.get("b"), Some(&Worn::Weapon));
        let doll = paper_doll(&items, &worn);
        assert_eq!(doll[&DollPlace::Weapon].letter, "b");
        assert_eq!(doll[&DollPlace::Ring1].letter, "c");
        assert_eq!(doll[&DollPlace::Ring2].letter, "d");
        assert_eq!(doll.len(), 3);
    }

    #[test]
    fn actions() {
        let mut potion = pack(&[item(0, "a potion of curing", obj::POTIONS, 1)]).remove(0);
        potion.item.extra.insert("action_verb".into(), json!("Quaff"));
        let a = item_actions(&potion, None);
        assert_eq!(a.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Quaff", "Drop", "Describe"]);
        assert_eq!(item_action_keycaps(&a[0], "a"), ["q", "a"]);
        let axe = pack(&[item(0, "a +0 war axe", obj::WEAPONS, 9)]).remove(0);
        let a = item_actions(&axe, Some(Worn::Weapon));
        assert_eq!(a[0].label, "Unwield");
        assert_eq!(item_action_keys(ItemActionId::Unequip, &axe), Some((true, vec![KeyStep::text("u")])));
        let gizmo = pack(&[item(0, "a gizmo", obj::GIZMOS, 0)]).remove(0);
        assert_eq!(item_action_keys(ItemActionId::Equip, &gizmo), Some((false, vec![KeyStep::text("e"), KeyStep::text("a").awaiting()])));
        assert_eq!(item_title("a +0 war axe"), "+0 war axe");
    }
}

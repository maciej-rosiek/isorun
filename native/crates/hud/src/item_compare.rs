//! An item's base stats from crawl's item-prop.cc, and the card's lines that set a candidate
//! against what it would replace (screens.ts `itemTip`).

use crate::gear::{obj, PackItem};
use crate::tooltip::{TipLine, Tone};
use isorun_webtiles::formatted_string_to_text;
use once_cell::sync::Lazy;
use regex::Regex;

fn is_shield(sub: i64) -> bool {
    matches!(sub, 12..=14)
}

/// item-prop.cc `Armour_prop`: base armour class and encumbrance, by `armour_type`.
fn armour_base(sub: i64) -> Option<(i64, i64)> {
    Some(match sub {
        0 => (2, 0),
        1 => (3, 4),
        2 => (5, 7),
        3 => (6, 10),
        4 => (8, 14),
        5 => (10, 18),
        6 => (1, 0),
        7 | 8 => (0, 0),
        9..=11 => (1, 0),
        12 => (3, 5),
        13 => (8, 10),
        14 => (13, 15),
        15 => (14, 23),
        16 => (2, 0),
        18 => (3, 4),
        20 => (8, 9),
        22 => (9, 11),
        24 => (5, 0),
        26 => (6, 5),
        28 => (10, 15),
        30 => (12, 23),
        32 => (7, 7),
        34 => (10, 11),
        35 | 36 => (4, 6),
        38 => (11, 15),
        40 => (9, 7),
        41 | 42 => (0, 0),
        _ => return None,
    })
}

/// item-prop.cc `Weapon_prop`: damage, accuracy and delay (in tenths), by `weapon_type`.
fn weapon_base(sub: i64) -> Option<(i64, i64, i64)> {
    Some(match sub {
        0 => (5, 3, 13),
        1 => (6, 2, 11),
        3 => (8, 3, 14),
        4 => (10, 0, 14),
        5 => (13, -2, 15),
        7 => (13, -3, 13),
        8 => (15, -1, 15),
        9 => (17, -4, 17),
        10 => (4, 6, 10),
        11 => (4, 6, 15),
        12 => (5, 4, 10),
        13 => (7, 4, 12),
        14 => (8, 2, 13),
        15 => (10, 1, 14),
        16 => (12, 0, 14),
        17 => (17, -3, 17),
        18 => (7, 3, 13),
        19 => (11, 0, 15),
        20 => (13, -2, 16),
        21 => (15, -4, 17),
        22 => (18, -6, 19),
        23 => (6, 4, 11),
        24 => (9, 1, 13),
        25 => (13, -3, 15),
        26 => (15, -3, 17),
        27 => (18, -6, 19),
        29 => (16, -2, 19),
        30 => (8, 2, 14),
        31 => (14, 0, 17),
        32 => (11, 1, 11),
        33 => (20, -6, 16),
        34 => (22, -7, 18),
        35 => (13, -1, 13),
        36 => (15, -1, 15),
        37 => (19, -4, 18),
        38 => (12, 1, 13),
        40 => (5, 5, 12),
        41 => (10, 3, 13),
        42 => (16, -3, 14),
        43 => (7, 0, 14),
        48 => (14, -2, 12),
        51 => (12, 0, 11),
        52 => (13, 0, 13),
        54 => (16, 3, 19),
        55 => (23, -2, 23),
        57 => (11, -3, 15),
        58 => (14, 1, 17),
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemStat {
    pub label: &'static str,
    pub value: f64,
    pub higher_is_better: bool,
    pub decimals: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ItemNameParts {
    pub enchant: Option<i64>,
    pub ego: String,
    pub properties: Vec<String>,
}

static INSCRIPTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{([^}]*)\}").unwrap());
static ENCHANT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)([+-]\d+)\s").unwrap());
static EGO: Lazy<Regex> = Lazy::new(|| Regex::new(r" of (.+?)\s*$").unwrap());
static SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\s,]+").unwrap());

/// The parts of an item's name a card shows: its enchantment, its ego (" of fire") and the
/// properties crawl inscribes on it ("{rF+ Str+2}").
pub fn item_name_parts(name: &str) -> ItemNameParts {
    let name = formatted_string_to_text(name);
    let inscription = INSCRIPTION.captures(&name).map(|c| c[1].to_string()).unwrap_or_default();
    let bare = INSCRIPTION.replace_all(&name, "").trim().to_string();
    let enchant = ENCHANT.captures(&bare).and_then(|c| c[1].parse::<i64>().ok());
    let ego = EGO.captures(&bare).map(|c| c[1].to_string()).unwrap_or_default();
    let properties = SPLIT.split(&inscription).filter(|s| !s.is_empty()).map(String::from).collect();
    ItemNameParts { enchant, ego, properties }
}

pub fn item_stats(item: &PackItem) -> Vec<ItemStat> {
    let parts = item_name_parts(item.name());
    let plus = parts.enchant.unwrap_or(0) as f64;
    let sub = item.item.sub_type.unwrap_or(-1);
    let stat = |label, value: f64, higher_is_better, decimals| ItemStat { label, value, higher_is_better, decimals };
    match item.item.base_type {
        Some(obj::ARMOUR) => {
            let Some((ac, enc)) = armour_base(sub) else { return vec![] };
            let defence = stat(if is_shield(sub) { "Shield" } else { "Armour" }, ac as f64 + plus, true, 0);
            if enc != 0 || is_shield(sub) {
                vec![defence, stat("Encumbrance", enc as f64, false, 0)]
            } else {
                vec![defence]
            }
        }
        Some(obj::WEAPONS) => {
            let Some((dam, acc, delay)) = weapon_base(sub) else { return vec![] };
            vec![stat("Damage", dam as f64 + plus, true, 0), stat("Accuracy", acc as f64 + plus, true, 0), stat("Attack delay", delay as f64 / 10.0, false, 1)]
        }
        Some(obj::JEWELLERY) => match parts.enchant {
            Some(e) => vec![stat("Enchantment", e as f64, true, 0)],
            None => vec![],
        },
        _ => vec![],
    }
}

fn fixed(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}")
}

/// The candidate's stats as card lines, each against what is worn there now when there is
/// something: the difference after the value, better or worse by which way the stat points.
pub fn compare_stats(candidate: &PackItem, worn: Option<&PackItem>) -> Vec<TipLine> {
    let theirs = worn.map(item_stats).unwrap_or_default();
    item_stats(candidate)
        .into_iter()
        .map(|s| {
            let value = fixed(s.value, s.decimals);
            if worn.is_none() {
                return TipLine { label: s.label.into(), value, tone: None };
            }
            let other = theirs.iter().find(|t| t.label == s.label).map(|t| t.value).unwrap_or(0.0);
            let diff = s.value - other;
            if diff.abs() < 1e-9 {
                return TipLine { label: s.label.into(), value, tone: Some(Tone::Same) };
            }
            let better = if s.higher_is_better { diff > 0.0 } else { diff < 0.0 };
            let sign = if diff > 0.0 { "+" } else { "" };
            TipLine {
                label: s.label.into(),
                value: format!("{value} ({sign}{})", fixed(diff, s.decimals)),
                tone: Some(if better { Tone::Better } else { Tone::Worse }),
            }
        })
        .collect()
}

/// A property a card lists as better or worse (screens.ts `propertyTone`): a leading `-` or `*`,
/// or a trailing `-`, is a drawback.
pub fn property_tone(text: &str) -> Tone {
    if text.starts_with('-') || text.starts_with('*') || text.ends_with('-') {
        Tone::Worse
    } else {
        Tone::Better
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::tests::{item, pack};

    #[test]
    fn name_parts() {
        let p = item_name_parts("a +2 broad axe of flaming {rF+ Str+2, -Cast}");
        assert_eq!(p.enchant, Some(2));
        assert_eq!(p.ego, "flaming");
        assert_eq!(p.properties, ["rF+", "Str+2", "-Cast"]);
        assert_eq!(item_name_parts("a ring of protection").enchant, None);
    }

    #[test]
    fn weapons_against_the_wielded() {
        let items = pack(&[item(0, "a +3 war axe", obj::WEAPONS, 9), item(1, "a +0 hand axe", obj::WEAPONS, 7)]);
        let lines = compare_stats(&items[0], Some(&items[1]));
        assert_eq!(lines[0], TipLine { label: "Damage".into(), value: "20 (+7)".into(), tone: Some(Tone::Better) });
        assert_eq!(lines[1], TipLine { label: "Accuracy".into(), value: "-1 (+2)".into(), tone: Some(Tone::Better) });
        assert_eq!(lines[2], TipLine { label: "Attack delay".into(), value: "1.7 (+0.4)".into(), tone: Some(Tone::Worse) });
        let alone = compare_stats(&items[1], None);
        assert_eq!(alone[0].value, "13");
        assert_eq!(alone[0].tone, None);
    }

    #[test]
    fn armour_and_shields() {
        let items = pack(&[item(0, "a +1 buckler", obj::ARMOUR, 12), item(1, "a cloak", obj::ARMOUR, 6)]);
        let s = item_stats(&items[0]);
        assert_eq!((s[0].label, s[0].value), ("Shield", 4.0));
        assert_eq!((s[1].label, s[1].value), ("Encumbrance", 5.0));
        assert_eq!(item_stats(&items[1]).len(), 1);
        let same = compare_stats(&items[1], Some(&items[1]));
        assert_eq!(same[0].tone, Some(Tone::Same));
    }

    #[test]
    fn tones() {
        assert_eq!(property_tone("rF+"), Tone::Better);
        assert_eq!(property_tone("-Cast"), Tone::Worse);
        assert_eq!(property_tone("*Rage"), Tone::Worse);
        assert_eq!(property_tone("rC-"), Tone::Worse);
    }
}

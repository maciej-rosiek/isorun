//! What the dock knows about a character that the server does not send on its own: the spells
//! and abilities its menus last listed, what its pack said was worn, and what the player pinned
//! on the quick slots. Kept in storage by character, so a reload or a later session finds it. A
//! new game under the same name starts over (`turn` went back).

use crate::gear::Worn;
use crate::quick_slots::{KnownAction, SlotRef, SLOT_COUNT};
use isorun_client::platform::storage;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct DockMemory {
    pub pins: Vec<Option<SlotRef>>,
    pub spells: Vec<KnownAction>,
    pub abilities: Vec<KnownAction>,
    pub worn: BTreeMap<String, Worn>,
    pub turn: i64,
}

impl Default for DockMemory {
    fn default() -> Self {
        DockMemory { pins: vec![None; SLOT_COUNT], spells: vec![], abilities: vec![], worn: BTreeMap::new(), turn: 0 }
    }
}

const KEY: &str = "isorun.dock";

fn read_all() -> Map<String, Value> {
    storage::get_raw(KEY).and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

impl DockMemory {
    /// The memory as stored, read back; `turn` is the game's turn now, and a turn earlier than
    /// the one kept means a new game.
    pub fn from_json(v: Option<&Value>, turn: i64) -> DockMemory {
        let mut m = DockMemory::default();
        let Some(v) = v.and_then(|v| v.as_object()) else { return m };
        let kept = v.get("turn").and_then(|t| t.as_i64()).unwrap_or(0);
        if turn < kept {
            return m;
        }
        m.turn = kept;
        if let Some(pins) = v.get("pins").and_then(|p| p.as_array()) {
            for (i, p) in pins.iter().take(SLOT_COUNT).enumerate() {
                m.pins[i] = SlotRef::from_json(p);
            }
        }
        let known = |k: &str| v.get(k).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(KnownAction::from_json).collect()).unwrap_or_default();
        m.spells = known("spells");
        m.abilities = known("abilities");
        if let Some(w) = v.get("worn").and_then(|w| w.as_object()) {
            for (letter, w) in w {
                if let Some(w) = w.as_str().and_then(Worn::parse) {
                    m.worn.insert(letter.clone(), w);
                }
            }
        }
        m
    }

    pub fn to_json(&self) -> Value {
        let pins: Vec<Value> = self.pins.iter().map(|p| p.as_ref().map(|r| r.to_json()).unwrap_or(Value::Null)).collect();
        let worn: Map<String, Value> = self.worn.iter().map(|(k, w)| (k.clone(), Value::from(w.as_str()))).collect();
        json!({
            "pins": pins,
            "spells": self.spells.iter().map(|s| s.to_json()).collect::<Vec<_>>(),
            "abilities": self.abilities.iter().map(|s| s.to_json()).collect::<Vec<_>>(),
            "worn": worn,
            "turn": self.turn,
        })
    }
}

/// The character's memory, or an empty one.
pub fn load_dock_memory(character: &str, turn: i64) -> DockMemory {
    DockMemory::from_json(read_all().get(character), turn)
}

pub fn save_dock_memory(character: &str, m: &DockMemory) {
    let mut all = read_all();
    all.insert(character.to_string(), m.to_json());
    if let Ok(text) = serde_json::to_string(&Value::Object(all)) {
        storage::set_raw(KEY, Some(&text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut m = DockMemory { turn: 120, ..Default::default() };
        m.pins[3] = Some(SlotRef::Item(53));
        m.spells.push(KnownAction { letter: "a".into(), name: "Magic Dart".into(), tiles: vec![] });
        m.worn.insert("b".into(), Worn::Worn);
        let back = DockMemory::from_json(Some(&m.to_json()), 130);
        assert_eq!(back, m);
    }

    #[test]
    fn a_new_game_starts_over() {
        let mut m = DockMemory { turn: 500, ..Default::default() };
        m.pins[0] = Some(SlotRef::Fire);
        assert_eq!(DockMemory::from_json(Some(&m.to_json()), 10), DockMemory::default());
    }

    #[test]
    fn junk_reads_as_nothing() {
        let v = json!({ "pins": [3, { "kind": "nope" }, { "kind": "fire" }], "spells": [{ "letter": 1 }], "worn": { "a": "worn", "b": "eaten" } });
        let m = DockMemory::from_json(Some(&v), 0);
        assert_eq!(m.pins[..3], [None, None, Some(SlotRef::Fire)]);
        assert!(m.spells.is_empty());
        assert_eq!(m.worn.len(), 1);
    }
}

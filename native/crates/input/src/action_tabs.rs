//! X's actions as tabs: Spells | Abilities | Evocables | Quiver | Memorise (the port of
//! `action-tabs.ts`).
//!
//! Unlike the pack's pages (pack_tabs.rs), these are four menus of crawl's,
//! not one: `z *` (spl-cast.cc `list_spells`), `a` (ability.cc), `V`
//! (`evoke_item`, a use-item menu) and `Q` (quiver.cc, tagged "actions").
//! Crawl has no key that turns from one to the next, so a turn is ours:
//! Escape the menu that is up, then the next one's key (game.ts
//! `openActionTab`). A tab with nothing on it stands like any other: crawl
//! refuses its key with a line ("You don't know any spells.") and the frame
//! shows that line where the menu would be (Overlays.showActionTabs).
//!
//! Swapping weapons and shouting are no lists, so they are buttons on every
//! tab instead of rows: Y and LT.

use crate::bindings::{Action, SeqItem};
use isorun_webtiles::{formatted_string_to_text, MenuState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionTabId {
    Spells,
    Abilities,
    Evocables,
    Quiver,
    Memorise,
}

pub struct ActionTab {
    pub id: ActionTabId,
    pub label: &'static str,
    /// its icon, a tile name as the command lists name theirs (overlays.ts `commandTileId`)
    pub tile: &'static str,
    /// the key that opens it; Spells' `z` asks first, and `*` lists (game.ts `trackActions`)
    pub key: &'static str,
}

fn titled(menu: &MenuState, start: &str) -> bool {
    formatted_string_to_text(&menu.title.text).trim_start().starts_with(start)
}

pub const ACTION_TABS: [ActionTab; 5] = [
    ActionTab { id: ActionTabId::Spells, label: "Spells", tile: "CMD_CAST_SPELL", key: "z" },
    ActionTab { id: ActionTabId::Abilities, label: "Abilities", tile: "CMD_USE_ABILITY", key: "a" },
    ActionTab { id: ActionTabId::Evocables, label: "Evocables", tile: "WAND_OFFSET", key: "V" },
    ActionTab { id: ActionTabId::Quiver, label: "Quiver", tile: "MI_BOOMERANG", key: "Q" },
    ActionTab { id: ActionTabId::Memorise, label: "Memorise", tile: "CMD_MEMORISE_SPELL", key: "M" },
];

impl ActionTabId {
    /// whether crawl's menu up is this tab's
    pub fn is(self, m: &MenuState) -> bool {
        match self {
            // the cast list only; `I` lists the same spells to describe them ("Your spells (describe)")
            ActionTabId::Spells => m.tag == "spell" && titled(m, "Your spells (cast)"),
            ActionTabId::Abilities => m.tag == "ability",
            ActionTabId::Evocables => m.tag == "use_item" && titled(m, "Evoke which item?"),
            ActionTabId::Quiver => m.tag == "actions",
            ActionTabId::Memorise => m.tag == "spell" && !titled(m, "Your spells"),
        }
    }
}

/// The tab crawl's menu is, or None for any other menu.
pub fn action_tab_of(menu: &MenuState) -> Option<ActionTabId> {
    ACTION_TABS.iter().find(|t| t.id.is(menu)).map(|t| t.id)
}

/// The tab `step` turns to from `id`, wrapping round; none is ever skipped, an empty one says so when it is up.
pub fn action_neighbour(id: ActionTabId, step: i32) -> ActionTabId {
    let n = ACTION_TABS.len() as i32;
    let at = ACTION_TABS.iter().position(|t| t.id == id).unwrap_or(0) as i32;
    ACTION_TABS[(at + step).rem_euclid(n) as usize].id
}

pub fn action_tab(id: ActionTabId) -> &'static ActionTab {
    ACTION_TABS.iter().find(|t| t.id == id).expect("every tab is listed")
}

/// What crawl says as it puts a menu away (MSG_OK): the Escape of a turn, never a tab's own answer.
pub const OKAY_THEN: &str = "Okay, then.";

/// The buttons beside the tabs: Y swaps weapons, LT shouts. From one of
/// crawl's menus the Escape goes first; from an empty tab (no menu up) the
/// frame goes away and the key alone is sent (game.ts).
pub const SWAP_WEAPONS_KEY: &str = "'";
pub const SHOUT_KEY: &str = "t";

// contextual: they stand under every tab, as the empty frame's do (Overlays.padPrompts)
pub fn swap_weapons() -> Action {
    Action::Keys {
        seq: vec![SeqItem::key(27), SeqItem::text(SWAP_WEAPONS_KEY)],
        label: "Swap weapons".into(),
        contextual: true,
        keeps_map: false,
        repeats: false,
    }
}

pub fn shout() -> Action {
    Action::Keys { seq: vec![SeqItem::key(27), SeqItem::text(SHOUT_KEY)], label: "Shout".into(), contextual: true, keeps_map: false, repeats: false }
}

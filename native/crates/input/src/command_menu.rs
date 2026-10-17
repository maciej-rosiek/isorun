//! The command lists the pad reaches through menus (the port of `command-menu.ts`): the Start
//! menu's character screens, and the keys the pad has direct buttons for, which the command
//! palette leaves out.

use crate::bindings::{Action, SeqItem};

#[derive(Clone, Debug, PartialEq)]
pub struct CommandEntry {
    pub label: &'static str,
    pub key: &'static str,
    /// The line under the label: what the command does, in the words the game
    /// would use, for a player who knows the dungeon but not the key. Short --
    /// the menus are opened mid-fight, so a subline is read at a glance or not
    /// at all -- and never a restatement of the label.
    pub sub: Option<&'static str>,
    /// The row's icon, as a tile name. Crawl draws its own icon for the
    /// commands its touch command bar offers (the GUI atlas's `CMD_*`), and
    /// those come first; the rest take the tile of what they act on.
    pub tile: Option<&'static str>,
    /// a control key's code, when the key is one (`Ctrl-P`); else the key is sent as text
    pub code: Option<i32>,
}

impl CommandEntry {
    pub fn action(&self) -> Action {
        let seq = vec![match self.code {
            Some(c) => SeqItem::key(c),
            None => SeqItem::text(self.key),
        }];
        Action::Keys { seq, label: self.label.into(), contextual: false, keeps_map: false, repeats: false }
    }
}

const fn command(label: &'static str, key: &'static str, sub: &'static str, tile: &'static str) -> CommandEntry {
    CommandEntry { label, key, sub: Some(sub), tile: Some(tile), code: None }
}

// Every list runs from the commands a game reaches for most to the ones it
// rarely needs, so the usual pick is near the top.

/// Character includes management screens (skills, memorisation, letter assignments), not just read-only views.
pub const CHARACTER_COMMANDS: [CommandEntry; 12] = [
    command("Skills", "m", "what you are training, and how fast", "CMD_DISPLAY_SKILLS"),
    command("Character status", "@", "what is on you right now", "CMD_DISPLAY_CHARACTER_STATUS"),
    command("Resistances / equipment", "%", "what you resist, and what you are wearing", "CMD_RESISTS_SCREEN"),
    command("Memorise spell", "M", "learn a spell out of a book you carry", "CMD_MEMORISE_SPELL"),
    command("Spell list", "I", "the spells you know, and what they cost", "BOOK"),
    command("Religion", "^", "your god, and what they ask of you", "CMD_DISPLAY_RELIGION"),
    command("Mutations", "A", "what the dungeon has made of you", "CMD_DISPLAY_MUTATIONS"),
    CommandEntry { label: "Message history", key: "Ctrl-P", sub: Some("everything the game has told you"), tile: Some("CMD_REPLAY_MESSAGES"), code: Some(16) },
    command("Adjust inventory letters", "=", "move an item to a letter you will remember", "CMD_DISPLAY_INVENTORY"),
    command("Known items / autopickup", "\\", "what you have identified, and what to pick up", "CMD_KNOWN_ITEMS"),
    command("Runes collected", "}", "the runes you are carrying out", "MISC_RUNE_OF_ZOT"),
    command("Show gold", "$", "what you have found, and what you have spent", "GOLD01"),
];

/// Repeat and Help live on the Start menu's System tab (Overlays.showSystem), with the game options.
pub const REPEAT_COMMAND: CommandEntry = CommandEntry { label: "Repeat previous command", key: "`", sub: None, tile: None, code: None };
pub const HELP_COMMAND: CommandEntry = CommandEntry { label: "Help", key: "?", sub: None, tile: None, code: None };

/// Direct pad controls (including contextual A), with Crawl's alternate keys: the palette leaves these out.
pub const GAMEPAD_COMMAND_KEYS: [&str; 13] = ["o", "5", ".", "s", "x", "f", "\t", ",", "g", "<", ">", "O", "C"];

pub fn is_gamepad_command_key(key: &str) -> bool {
    GAMEPAD_COMMAND_KEYS.contains(&key)
}

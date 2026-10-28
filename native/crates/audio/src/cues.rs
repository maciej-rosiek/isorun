//! The pure mappings from what happens in a game to sounds (`cues.ts`): a message line to its
//! sound, a fight to its cues, hearing by distance and the side of the screen, and the music and
//! the beds a place calls for.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::catalog::{AmbientLevels, Mood, SoundName};

pub type Cell = (i32, i32);

/// Who acts in a fight: the player, a monster by its id (the renderer's actor id), or a monster
/// out of sight on the cell the player struck at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fighter {
    Player,
    Monster(i64),
    Unseen(Cell),
}

/// What a fight message shows, as `combat.ts` reads it from the message lines: `Attack` swings
/// `by` at the cell `at`; `Hit`, `Miss` and `Death` land on `on`, struck from `from`. The
/// sounds need no more than this (a hit's strength and amount are the damage numbers' business).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    Attack { by: Fighter, at: Cell },
    Hit { on: Fighter, from: Option<Cell> },
    Miss { on: Fighter, from: Option<Cell> },
    Death { on: Fighter, from: Option<Cell> },
}

/// A sound a fight asks for, when the blow shows on the screen and `after` seconds more; `at`
/// is where it is heard from, the player's own ear when `None`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub sound: SoundName,
    pub at: Option<Cell>,
    pub after: f64,
    pub gain: f32,
}

impl Cue {
    fn new(sound: SoundName) -> Cue {
        Cue { sound, at: None, after: 0.0, gain: 1.0 }
    }
    fn at(self, at: Option<Cell>) -> Cue {
        Cue { at, ..self }
    }
    fn gain(self, gain: f32) -> Cue {
        Cue { gain, ..self }
    }
    fn after(self, after: f64) -> Cue {
        Cue { after, ..self }
    }
}

/// What the player spent a turn on, found by the key that asked for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseKind {
    Potion,
    Scroll,
    Cast,
}

/// How far a sound carries, in cells.
const HEARING: i32 = 12;
/// The least of a sound that is still heard, the far side of the hearing.
const FAINTEST: f32 = 0.15;
/// How far to one ear a sound at the edge of the screen goes, 1 being all of it.
const WIDEST_PAN: f32 = 0.65;
/// The part of the hit points under which the heart beats.
const HEARTBEAT_BELOW: f64 = 0.35;

const DARK_PLACES: &[&str] = &[
    "crypt",
    "cr",
    "tomb",
    "hell",
    "hel",
    "dis",
    "geh",
    "gehenna",
    "coc",
    "cocytus",
    "tar",
    "tartarus",
    "abyss",
    "pan",
    "pandemonium",
    "zot",
    "depths",
    "dep",
    "vaults",
    "vault",
    "slime",
    "sl",
];
const CAVE_PLACES: &[&str] = &["lair", "orc", "mines", "swamp", "spider", "snake", "shoals", "slime", "sl", "abyss", "dis", "geh", "coc", "tar", "hell", "hel"];
const DARK_FROM_DEPTH: i64 = 12;

static REFUSAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(okay, then|never mind|you (?:have|don't have|do not have) (?:no|nothing)|you (?:cannot|can't|are unable)|you can only|that is not|there is nothing)").unwrap()
});
static SCHOOLS: Lazy<[(Regex, SoundName); 3]> = Lazy::new(|| {
    [
        (Regex::new(r"(?i)fire|flame|burn|blaze|inferno|scorch|sear|ignit").unwrap(), SoundName::CastFire),
        (Regex::new(r"(?i)frost|ice|icy|freez|cold|chill|glaci").unwrap(), SoundName::CastIce),
        (Regex::new(r"(?i)drain|death|necro|pain|corpse|dead|bone|vamp|torment|curse").unwrap(), SoundName::CastDark),
    ]
});
static GOLD: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^you (?:now have \d+|pick up \d+) gold").unwrap());
static DIE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^you die\b").unwrap());
static LEVEL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^welcome to level|^you have reached (?:level|xl)|^you feel more experienced").unwrap());
static PICKUP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z] - ").unwrap());

/// The sound for a message line on its own: gold, a pickup, an experience level, the player's
/// death.
pub fn sound_for_line(text: &str) -> Option<SoundName> {
    if GOLD.is_match(text) {
        return Some(SoundName::Gold);
    }
    if DIE.is_match(text) {
        return Some(SoundName::Gameover);
    }
    if LEVEL.is_match(text) {
        return Some(SoundName::Levelup);
    }
    if PICKUP.is_match(text) {
        return Some(SoundName::Pickup);
    }
    None
}

/// The kind of use a key sent in the command mode starts, if it starts one.
pub fn use_kind_of_key(key: &str) -> Option<UseKind> {
    match key {
        "q" => Some(UseKind::Potion),
        "r" => Some(UseKind::Scroll),
        "z" | "Z" | "v" => Some(UseKind::Cast),
        _ => None,
    }
}

/// The sound for the first answer to a use: not a refusal, and for a spell the school the lines
/// name.
pub fn sound_for_use<S: AsRef<str>>(kind: UseKind, lines: &[S]) -> Option<SoundName> {
    let first = lines.first()?;
    if REFUSAL.is_match(first.as_ref()) {
        return None;
    }
    match kind {
        UseKind::Potion => Some(SoundName::Potion),
        UseKind::Scroll => Some(SoundName::Scroll),
        UseKind::Cast => {
            Some(SCHOOLS.iter().find(|(words, _)| lines.iter().any(|line| words.is_match(line.as_ref()))).map_or(SoundName::Cast, |(_, sound)| *sound))
        }
    }
}

/// What a fight message sounds like: an attack's swing, or the blow that lands.
pub fn cues_for_combat(event: &CombatEvent, cell_of: impl Fn(Fighter) -> Option<Cell>) -> Vec<Cue> {
    match *event {
        CombatEvent::Attack { by: Fighter::Player, .. } => vec![Cue::new(SoundName::Swing)],
        CombatEvent::Attack { by, .. } => vec![Cue::new(SoundName::Swing).at(cell_of(by)).gain(0.5)],
        CombatEvent::Miss { on, .. } => {
            let at = if on == Fighter::Player { None } else { cell_of(on) };
            vec![Cue::new(SoundName::Miss).at(at).gain(if on == Fighter::Player { 1.0 } else { 0.7 })]
        }
        CombatEvent::Death { on, .. } => {
            let at = if on == Fighter::Player { None } else { cell_of(on) };
            vec![Cue::new(SoundName::HitHeavy).at(at), Cue::new(SoundName::Death).at(at).after(0.12)]
        }
        CombatEvent::Hit { on: Fighter::Player, .. } => vec![Cue::new(SoundName::Hurt), Cue::new(SoundName::Hit).gain(0.6)],
        CombatEvent::Hit { on, .. } => vec![Cue::new(SoundName::Hit).at(cell_of(on))],
    }
}

/// How a sound is heard: `pan` -1 (the left ear) to 1 (the right), and its loudness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    pub pan: f32,
    pub gain: f32,
}

/// How far to one side and how loud a sound at `source` is heard by `listener`; `across` is
/// where it is on the screen, -1 at the left edge to 1 at the right. `None` when it is out of
/// hearing.
pub fn hearing(listener: Cell, source: Cell, across: f32) -> Option<Heard> {
    let distance = (listener.0 - source.0).abs().max((listener.1 - source.1).abs());
    if distance > HEARING {
        return None;
    }
    Some(Heard { pan: across.clamp(-1.0, 1.0) * WIDEST_PAN, gain: FAINTEST.max(1.0 - distance as f32 / HEARING as f32) })
}

/// The music a place calls for.
pub fn mood_for(place: &str, depth: i64) -> Mood {
    let branch = place.to_lowercase();
    if DARK_PLACES.contains(&branch.as_str()) {
        return Mood::Dark;
    }
    if depth >= DARK_FROM_DEPTH && (branch == "dungeon" || branch == "d") {
        Mood::Dark
    } else {
        Mood::Calm
    }
}

/// The loops that sound under a place: its bed, and the water and lava near the player (cells of
/// each in reach).
pub fn ambient_levels(place: &str, water: u32, lava: u32) -> AmbientLevels {
    let cave = CAVE_PLACES.contains(&place.to_lowercase().as_str());
    AmbientLevels {
        dungeon: if cave { 0.0 } else { 1.0 },
        cavern: if cave { 1.0 } else { 0.0 },
        water: (water as f32 / 10.0).min(1.0),
        lava: (lava as f32 / 10.0).min(1.0),
    }
}

/// Seconds from one heartbeat to the next at this part of the hit points, or `None` when the
/// heart is calm.
pub fn heartbeat_delay(hp_fraction: f64) -> Option<f64> {
    if hp_fraction.is_nan() || hp_fraction <= 0.0 || hp_fraction >= HEARTBEAT_BELOW {
        return None;
    }
    Some(0.5 + (hp_fraction / HEARTBEAT_BELOW) * 0.9)
}

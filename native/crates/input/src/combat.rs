//! The fight in the message lines (the port of `combat.ts`): who struck whom, how hard, and who
//! died, for the renderer's `act` to play. Read on each state before the scene it comes with.

use isorun_scene::{Billboard, BillboardKind, Scene};
use isorun_webtiles::{formatted_string_to_text, GameMessage, Monster};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Who acts in a fight: the player, a monster by its client id (the renderer's actor id), or a
/// monster the player cannot see, crawl's "something", on the cell the player struck at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fighter {
    Player,
    Monster(i64),
    Unseen((i32, i32)),
}

/// How hard a blow struck: `tier` is crawl's punctuation (attack.cc
/// `attack_strength_punctuation`), 0 for ".", 1 for "!", 2 for "!!", 3 for "!!!"
/// and more; `amount` is the damage done, when the line says it (" for 7",
/// engine/patches/11-damage-numbers.patch, so only on this device's games).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BlowStrength {
    pub tier: u32,
    pub amount: Option<u32>,
}

/// What a fight message shows: `Attack` swings `by` at the cell `at`; `Hit`, `Miss` and
/// `Death` land on `on`, struck from the cell `from`; a hit says how hard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatEvent {
    Attack { by: Fighter, at: (i32, i32) },
    Hit { on: Fighter, from: Option<(i32, i32)>, strength: BlowStrength },
    Miss { on: Fighter, from: Option<(i32, i32)> },
    Death { on: Fighter, from: Option<(i32, i32)> },
}

/// "You see the goblin" and the like: verbs before a monster's name that are not blows.
const NOT_BLOWS: &[&str] = &[
    "see",
    "feel",
    "sense",
    "hear",
    "notice",
    "encounter",
    "smell",
    "open",
    "close",
    "are",
    "have",
    "can",
    "cannot",
    "swap",
    "displace",
    "stop",
    "start",
    "begin",
    "finish",
    "pick",
    "drop",
    "enter",
    "leave",
    "reach",
    "reaches",
    "cast",
    "zap",
    "throw",
    "fire",
    "shoot",
    "evoke",
    "pray",
    "pacify",
    "tame",
    "charm",
];
const KILL_VERBS: [&str; 7] = ["kill", "destroy", "slay", "annihilate", "obliterate", "dismember", "demolish"];
const DEATH_ENDINGS: [&str; 17] = [
    "is killed",
    "dies",
    "is destroyed",
    "is blown up",
    "is incinerated",
    "is annihilated",
    "is obliterated",
    "is dismembered",
    "is slain",
    "is consumed",
    "is crushed",
    "is turned to ash",
    "disintegrates",
    "falls apart",
    "collapses",
    "is drowned",
    "is frozen solid",
];
/// How far a melee blow reaches, in king moves: two for a polearm.
const REACH: i32 = 2;

fn not_blow(verb: &str) -> bool {
    NOT_BLOWS.contains(&verb)
}

static STRUCK_YOU_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^the [\w\s-]+ (?:hits|strikes|burns|freezes|shocks|pierces|engulfs) you\b").unwrap());
static FOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r" for (\d+)\b").unwrap());
static NO_DAMAGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bno damage\b").unwrap());
/// The player's blow on a monster out of sight: crawl names it "something" (monster.cc `_str_monam`).
static ON_SOMETHING_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^you (?:\w+ly )?(\w+) something\b").unwrap());
/// A blow on the player from a monster out of sight.
static FROM_SOMETHING_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^something (?:\w+ly )?(\w+) you\b").unwrap());

/// The fight the message lines describe, in the order they were written:
/// the player's blows and kills ("You hit the goblin.", "You kill the
/// goblin!"), a monster's blows on the player ("The goblin hits you."), and
/// deaths and hits from anywhere ("The goblin is killed!", "The dart hits the
/// goblin."). A monster is found on the scene by the name the server gave it,
/// the nearest of that name to the player; a line that names no monster in
/// view shows nothing. `monsters` gives a monster's plural by its id (the map's monsters).
/// A monster out of sight is crawl's "something": the player's blow on it lands on `unseen`, the
/// cell the player last struck or stepped at (none: it shows nothing), and its blow on the
/// player comes from nowhere in particular.
pub fn combat_events(lines: &[GameMessage], scene: &Scene, monsters: &HashMap<i64, Monster>, unseen: Option<(i32, i32)>) -> Vec<CombatEvent> {
    let found: Vec<&Billboard> = scene.billboards.iter().filter(|b| b.kind == BillboardKind::Monster && b.ref_id.is_some()).collect();
    let mut names: Vec<(String, Vec<&Billboard>)> = Vec::new();
    for b in &found {
        for name in monster_names(b, monsters) {
            match names.iter_mut().find(|(n, _)| *n == name) {
                Some((_, v)) => v.push(b),
                None => names.push((name, vec![b])),
            }
        }
    }
    let mut by_length: Vec<&(String, Vec<&Billboard>)> = names.iter().collect();
    by_length.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));
    let player = scene.player;
    let nearest = |list: &[&Billboard]| -> Option<(i32, i32, i64)> {
        let mut best: Option<&Billboard> = None;
        for b in list {
            if best.is_none_or(|x| king_moves((x.x, x.y), player) > king_moves((b.x, b.y), player)) {
                best = Some(b);
            }
        }
        best.map(|b| (b.x, b.y, b.ref_id.unwrap_or(0)))
    };
    let mut events = Vec::new();
    for line in lines {
        for sentence in sentences(&formatted_string_to_text(&line.text)) {
            let s = sentence.to_lowercase();
            if let Some(verb) = ON_SOMETHING_RE.captures(&s).map(|c| c[1].to_string()).filter(|v| !not_blow(v)) {
                if let Some(at) = unseen {
                    if king_moves(at, player) <= REACH {
                        events.push(CombatEvent::Attack { by: Fighter::Player, at });
                    }
                    let on = Fighter::Unseen(at);
                    events.push(if KILL_VERBS.contains(&verb.as_str()) {
                        CombatEvent::Death { on, from: Some(player) }
                    } else if verb == "miss" {
                        CombatEvent::Miss { on, from: Some(player) }
                    } else {
                        CombatEvent::Hit { on, from: Some(player), strength: blow_strength(&s) }
                    });
                }
                continue;
            }
            if let Some(verb) = FROM_SOMETHING_RE.captures(&s).map(|c| c[1].to_string()).filter(|v| !not_blow(v)) {
                events.push(if verb.starts_with("miss") {
                    CombatEvent::Miss { on: Fighter::Player, from: None }
                } else {
                    CombatEvent::Hit { on: Fighter::Player, from: None, strength: blow_strength(&s) }
                });
                continue;
            }
            if STRUCK_YOU_RE.is_match(&s) && !by_length.iter().any(|(name, _)| s.starts_with(&format!("the {name} "))) {
                events.push(CombatEvent::Hit { on: Fighter::Player, from: None, strength: blow_strength(&s) });
                continue;
            }
            for (name, list) in &by_length {
                let Some((mx, my, id)) = nearest(list) else { continue };
                if !s.contains(name.as_str()) {
                    continue;
                }
                let at = (mx, my);
                let named = format!("(?:the |an? |your )?{}", regex::escape(name));
                let blow = Regex::new(&format!(r"^you (?:\w+ly )?(\w+) {named}\b")).ok().and_then(|r| r.captures(&s).map(|c| c[1].to_string()));
                if let Some(verb) = blow.filter(|v| !not_blow(v)) {
                    if king_moves(at, player) <= REACH {
                        events.push(CombatEvent::Attack { by: Fighter::Player, at });
                    }
                    let on = Fighter::Monster(id);
                    if KILL_VERBS.contains(&verb.as_str()) {
                        events.push(CombatEvent::Death { on, from: Some(player) });
                    } else if verb == "miss" {
                        events.push(CombatEvent::Miss { on, from: Some(player) });
                    } else {
                        events.push(CombatEvent::Hit { on, from: Some(player), strength: blow_strength(&s) });
                    }
                    break;
                }
                let struck = Regex::new(&format!(r"^{named} (?:\w+ly )?(\w+) you\b")).ok().and_then(|r| r.captures(&s).map(|c| c[1].to_string()));
                if let Some(verb) = struck.filter(|v| !not_blow(v)) {
                    events.push(CombatEvent::Attack { by: Fighter::Monster(id), at: player });
                    if verb.starts_with("miss") {
                        events.push(CombatEvent::Miss { on: Fighter::Player, from: Some(at) });
                    } else {
                        events.push(CombatEvent::Hit { on: Fighter::Player, from: Some(at), strength: blow_strength(&s) });
                    }
                    break;
                }
                if Regex::new(&format!(r"^{named} \w+s\b.* at you\b")).is_ok_and(|r| r.is_match(&s)) {
                    events.push(CombatEvent::Attack { by: Fighter::Monster(id), at: player });
                    break;
                }
                if DEATH_ENDINGS.iter().any(|ending| Regex::new(&format!(r"^{named} {ending}\b")).is_ok_and(|r| r.is_match(&s))) {
                    events.push(CombatEvent::Death { on: Fighter::Monster(id), from: None });
                    break;
                }
                if !s.starts_with("you ")
                    && Regex::new(&format!(r"\b(?:hits|strikes|burns|freezes|shocks|pierces|bites|stings) {named}\b")).is_ok_and(|r| r.is_match(&s))
                {
                    events.push(CombatEvent::Hit { on: Fighter::Monster(id), from: None, strength: blow_strength(&s) });
                    break;
                }
            }
        }
    }
    events
}

/// How hard the blow a sentence tells of struck: its "!"s, and the damage when it says " for 7".
fn blow_strength(sentence: &str) -> BlowStrength {
    let tier = sentence.chars().rev().take_while(|&c| c == '!').count() as u32;
    if NO_DAMAGE_RE.is_match(sentence) {
        return BlowStrength { tier, amount: Some(0) };
    }
    BlowStrength { tier, amount: FOR_RE.captures(sentence).and_then(|c| c[1].parse().ok()) }
}

/// Where the monster with this client id stands on the scene, if it is in view.
pub fn monster_cell(scene: &Scene, id: i64) -> Option<(i32, i32)> {
    scene.billboards.iter().find(|b| b.kind == BillboardKind::Monster && b.ref_id == Some(id)).map(|b| (b.x, b.y))
}

/// The names a message may call the monster by, lower case: its name and its plural.
fn monster_names(b: &Billboard, monsters: &HashMap<i64, Monster>) -> Vec<String> {
    let mon = b.ref_id.and_then(|id| monsters.get(&id));
    let mut out = Vec::new();
    let name = mon.map(|m| m.name.clone()).filter(|n| !n.is_empty()).or_else(|| b.name.clone());
    if let Some(n) = name.filter(|n| !n.is_empty()) {
        out.push(n.to_lowercase());
    }
    if let Some(p) = mon.and_then(|m| m.plural.clone()).filter(|p| !p.is_empty()) {
        out.push(p.to_lowercase());
    }
    out
}

fn king_moves(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The text split after each `.`, `!` or `?` that whitespace follows.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|n| n.is_whitespace()) {
            while chars.peek().is_some_and(|n| n.is_whitespace()) {
                chars.next();
            }
            out.push(std::mem::take(&mut cur));
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// The line read last and where it stood in the log, for `lines_since`.
#[derive(Clone, Debug, PartialEq)]
pub struct LastLine {
    pub line: GameMessage,
    pub at: usize,
}

impl LastLine {
    /// The newest line of `lines`, if any.
    pub fn newest(lines: &[GameMessage]) -> Option<LastLine> {
        lines.last().map(|l| LastLine { line: l.clone(), at: lines.len() - 1 })
    }
}

/// The same line, whatever its `marker`: the reducer sets that on a line after it
/// arrives (a new command or turn starting after it), so whole lines stop matching.
fn same_line(a: &GameMessage, b: &GameMessage) -> bool {
    a.text == b.text && a.turn == b.turn && a.channel == b.channel
}

/// The lines that arrived since `last` was the newest (game.ts `linesSince`). The web
/// finds the line by identity; here it is found where it stood, and by its text, turn
/// and channel. Repeated lines ("There are monsters nearby!") are equal by value, so the
/// place it stood is tried first; after a rollback or the log's cap moved the lines, the
/// newest equal line is. When `last` is gone (rolled back by the server, or nothing has
/// been read yet) the lines of the current `turn` stand in, so a fight is never lost and
/// never older than the turn it belongs to.
pub fn lines_since<'a>(lines: &'a [GameMessage], last: Option<&LastLine>, turn: i64) -> Vec<&'a GameMessage> {
    if let Some(l) = last {
        if lines.get(l.at).is_some_and(|x| same_line(x, &l.line)) {
            return lines[l.at + 1..].iter().collect();
        }
        if let Some(i) = lines.iter().rposition(|x| same_line(x, &l.line)) {
            return lines[i + 1..].iter().collect();
        }
    }
    lines.iter().filter(|l| l.turn == Some(turn)).collect()
}

//! What lies under the player, from what the server printed about it: [`items_underfoot`],
//! [`floor_items_label`], [`floor_items_count`], [`is_stationary_item_name`].

use once_cell::sync::Lazy;
use regex::Regex;

use crate::state::{GameState, Msgch};

/// The server's latest floor-items announcement, `item_check` (items.cc) on MSGCH_FLOOR_ITEMS,
/// by the turn it was printed, with its text as sent: the line itself, and for "Things that are
/// here:" the plain-channel line of names that follows it. `picked` counts the items taken from
/// it since (inventory or gold gained in a `player` update while standing there).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FloorItems {
    pub turn: i64,
    pub lines: Vec<String>,
    pub picked: i64,
}

static TAGS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());
static STATIONARY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(corpse|skeleton)( of .+)?$").unwrap());
/// The console glyphs a corpse and a skeleton print in "Items here: ..." (viewchar.cc
/// `dchar_table`, CSET_DEFAULT: item_corpse †, item_skeleton ÷; a black mamba corpse is
/// `(<blue>†</blue>)` in cdi-0.34-watch.ndjson). An rc that rebinds `item_glyph` for them is not
/// recognised here.
static STATIONARY_GLYPHS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[†÷]").unwrap());
static SINGLE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^You see here (.+)\.$").unwrap());
static ITEMS_HERE_PLAIN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Items here: (.*)\.$").unwrap());
static SPACES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
/// "You see here X." and "Items here: X.", the tags the line opens with kept apart from the name.
static SEE_HERE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*((?:<[^>]*>)*)You see here ([\s\S]+)\.(?:<[^>]*>)*\s*$").unwrap());
static ITEMS_HERE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*((?:<[^>]*>)*)Items here: ([\s\S]*)\.(?:<[^>]*>)*\s*$").unwrap());
static TRAILING_TAGS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?:</?[a-z:]*>)+$").unwrap());
/// items.cc `drop_item`: what the player just put down lies underfoot, though no `item_check`
/// says so.
static DROPPED_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:<[^>]*>)*You drop (.+?)\.(?:<[^>]*>)*\s*$").unwrap());

/// items.cc `pickup`: the one line that says the cell under the player is bare.
const NO_ITEMS_HERE: &str = "There are no items here.";
/// items.cc `item_check`: the head of a two-line listing; the names follow on the plain channel.
const FLOOR_LIST_HEAD: &str = "Things that are here:";
const MANY_ITEMS: &str = "There are many items here.";

fn strip_tags(s: &str) -> String {
    TAGS_RE.replace_all(s, "").into_owned()
}

/// Colour tags off, whitespace trimmed: the text as [`floor_items_label`] reads it.
fn plain(s: &str) -> String {
    strip_tags(s).trim().to_owned()
}

/// Whether the server has said items lie under the player where they stand now.
///
/// The player's own cell never says so: `tileidx_player` (tilepick-p.cc) sets no `S_UNDER` flag
/// on the player's fg tile, unlike an item under a stationary monster (tileview.cc
/// `_tile_place_monster`), and the official client draws nothing for it. What the server does
/// send is `item_check`'s line on MSGCH_FLOOR_ITEMS each time the player arrives on a pile
/// (items.cc `autopickup` runs it, after autopickup or instead of it) or a pile is pushed under
/// them (player.cc). The line is matched to the standing by turn, not by position: a recorded
/// spectator stream delivered one such line ten turns after the pile it described was left, and
/// the `player` update with the new `pos` precedes the `msgs` in the same batch
/// (cdi-0.34-watch.ndjson), so the announcement's turn is never below the arrival's when it is
/// about this cell, except by the one turn autopickup takes when it picks something up on the
/// way (`arrived_pickup`; cdi-0.34-autopickup.ndjson has the line at 29815 under the arrival at
/// 29816).
///
/// Nothing is sent when a manual pick-up empties the pile (items.cc `pickup` does not call
/// `item_check`), so the pile is counted down instead: each item taken shows up in the same
/// `player` update as an inventory slot whose quantity grew (`_send_item`, tileweb.cc) or as more
/// gold (items.cc `get_gold`), and the answer turns false once as many have been taken as
/// `item_check` listed. A pile whose size the line does not give ("There are many items here.")
/// stays until the next step or a "There are no items here." (items.cc `pickup`). A pile of
/// nothing but corpses counts as no items: `pickup` has nothing to offer there.
pub fn items_underfoot(state: &GameState) -> bool {
    let p = &state.player;
    let Some(f) = &p.floor_items else { return false };
    if f.turn < p.arrived_turn - i64::from(p.arrived_pickup) {
        return false;
    }
    match floor_items_count(&f.lines) {
        None => true,
        Some(n) => f.picked < n as i64,
    }
}

/// A corpse or skeleton, by the name the server prints (item-name.cc `item_def::name`,
/// OBJ_CORPSES: "<monster> corpse" / "<monster> skeleton", a named one "... corpse of <name>").
/// Corpses are the one stationary item kind (item-prop.cc `item_is_stationary`): `g` on one says
/// "You can't pick that up." (items.cc `pickup_single_item`), `pickup` counts only the movable
/// items of a pile (`count_movable_items`) and autopickup skips them. They are useless items too,
/// so the default rc prints them darkgrey; the colour is not the test, since a useless scroll is
/// grey and still picked up.
pub fn is_stationary_item_name(name: &str) -> bool {
    STATIONARY_RE.is_match(strip_tags(name).trim())
}

/// How many items `item_check` (items.cc) said the pile holds that `pickup` would take: one for
/// "You see here X." (its `items.size() == 1` case), one per name in the "; " list under "Things
/// that are here:" (`item_message`), one per glyph in "Items here: ...", each less the corpses
/// ([`is_stationary_item_name`], the corpse glyphs). `None` when it did not say ("There are many
/// items here.").
pub fn floor_items_count(lines: &[String]) -> Option<usize> {
    let head = lines.first().map(|l| plain(l)).unwrap_or_default();
    let names = lines.get(1).map(|l| plain(l));
    if let Some(c) = SINGLE_RE.captures(&head) {
        return Some(usize::from(!is_stationary_item_name(&c[1])));
    }
    if head == FLOOR_LIST_HEAD {
        return names.filter(|n| !n.is_empty()).map(|n| n.split("; ").filter(|n| !is_stationary_item_name(n)).count());
    }
    if let Some(c) = ITEMS_HERE_PLAIN_RE.captures(&head) {
        let glyphs = SPACES_RE.replace_all(&c[1], "");
        return Some(STATIONARY_GLYPHS.replace_all(&glyphs, "").chars().count());
    }
    None
}

/// What lies underfoot, in the words the server used (items.cc `item_check`, `item_message`),
/// colour tags and all, so a prompt for it reads as its line in the message log does:
///
/// - "You see here X." gives X, as the server named it;
/// - "Things that are here:" is followed by the names joined with "; " on the plain channel (up
///   to `msgwin_lines() - 1` items), kept as is;
/// - "Items here: ...", printed from `item_stack_summary_minimum` items up (an rc option), lists
///   one glyph per item, and gives those glyphs ("[[ ((", each in its item's colour), the log's
///   own shorthand;
/// - "There are many items here." gives "many items".
///
/// Once some of a listed pile has been taken (`picked`) the server has not said which, so what
/// is left is counted, not named: "2 items". Corpses are left out of the name and the count ("a
/// +0 mace; a bat corpse" reads "a +0 mace"), as `pickup` leaves them out of what it offers.
/// `None` when nothing is underfoot ([`items_underfoot`]), which a pile of corpses alone is not.
///
/// The tags that wrap a whole line are not part of what it names, so they are left behind with
/// the words: what comes back is the item's own colouring.
pub fn floor_items_label(state: &GameState) -> Option<String> {
    if !items_underfoot(state) {
        return None;
    }
    let f = state.player.floor_items.as_ref()?;
    let first = f.lines.first().map(String::as_str).unwrap_or("");
    let head = plain(first);
    if let Some(n) = floor_items_count(&f.lines) {
        if f.picked > 0 {
            let left = n as i64 - f.picked;
            return Some(if left == 1 { "1 item".into() } else { format!("{left} items") });
        }
    }
    if let Some(c) = SEE_HERE_RE.captures(first) {
        return Some(named(&c[1], &c[2]));
    }
    if head == FLOOR_LIST_HEAD {
        return Some(match f.lines.get(1).filter(|l| !l.is_empty()) {
            Some(names) => names.trim().split("; ").filter(|n| !is_stationary_item_name(n)).collect::<Vec<_>>().join("; "),
            None => "items".into(),
        });
    }
    if let Some(c) = ITEMS_HERE_RE.captures(first) {
        return Some(named(&c[1], &c[2]));
    }
    if head == MANY_ITEMS {
        return Some("many items".into());
    }
    Some("items".into())
}

/// The name out of one of those lines, coloured as the log shows it: the colour the line opened
/// in (`<lightgrey>You see here a +0 halberd.`) is the name's too unless the name opens its own
/// (`<green>a staff of alchemy`), and the tags left dangling at the end belong to the full stop,
/// not to the item: crawl closes a colour by opening the one before it (`<green>a
/// staff<lightgrey>.`).
fn named(lead: &str, name: &str) -> String {
    let n = TRAILING_TAGS_RE.replace(name, "");
    if n.starts_with('<') {
        n.into_owned()
    } else {
        format!("{lead}{n}")
    }
}

/// Keep `player.floor_items` (see [`items_underfoot`]) in step with the message log: a
/// MSGCH_FLOOR_ITEMS line is the server saying items lie underfoot, "There are no items here."
/// that they do not, and "You drop X." that X does. A line without a turn takes the player's
/// current one. Called after the line is appended to the log.
pub(crate) fn note_floor_items(state: &mut GameState, text: &str, turn: Option<f64>, channel: Option<f64>) {
    let at = turn.map_or(state.player.turn, |t| t as i64);
    let floor = channel == Some(Msgch::FLOOR_ITEMS as f64);
    let plain_channel = channel == Some(Msgch::PLAIN as f64);
    if floor {
        state.player.floor_items = Some(FloorItems { turn: at, lines: vec![text.to_owned()], picked: 0 });
        return;
    }
    if plain_channel {
        if let Some(c) = DROPPED_RE.captures(text) {
            let line = if items_underfoot(state) { MANY_ITEMS.to_owned() } else { format!("You see here {}.", &c[1]) };
            state.player.floor_items = Some(FloorItems { turn: at, lines: vec![line], picked: 0 });
            return;
        }
    }
    if !plain_channel || state.player.floor_items.is_none() {
        return;
    }
    if plain(text) == NO_ITEMS_HERE {
        state.player.floor_items = None;
        return;
    }
    // the names under "Things that are here:", the line right after it
    let lines = &state.messages.lines;
    let prev_is_head = lines.len() >= 2 && {
        let prev = &lines[lines.len() - 2];
        prev.channel == Some(Msgch::FLOOR_ITEMS) && plain(&prev.text) == FLOOR_LIST_HEAD
    };
    if let Some(f) = &mut state.player.floor_items {
        if f.lines.len() == 1 && prev_is_head {
            f.lines.push(text.to_owned());
        }
    }
}

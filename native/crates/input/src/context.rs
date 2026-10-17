//! The situation the input reads (the port of `context.ts`): the mode the server is in, what is
//! ahead and underfoot, the prompt or menu up and what it offers. Derived once a frame from the
//! game state and the scene ([`derive_context`]); the bindings and the runner read nothing else.

use crate::action_tabs::{action_neighbour, action_tab, action_tab_of, ActionTabId};
use crate::focus::FocusInfo;
use crate::pack_tabs::{neighbour, pack_strip};
use isorun_scene::{is_threat, BillboardKind, CellKind, Dir8, Feature, Scene, SceneCell};
use isorun_webtiles::{floor_items_label, formatted_string_to_spans, formatted_string_to_text, is_stationary_item_name, GameMessage, GameState, MenuState, MouseMode, Phase, UiState};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    Command,
    /// inputMode MACRO: the server is capturing a macro, keys go through raw
    Macro,
    More,
    Menu,
    Popup,
    Newgame,
    Targeting,
    Levelmap,
    Text,
    Yesno,
    Prompt,
    Crt,
    Dialog,
    Lobby,
    Spectating,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Layer {
    #[default]
    Micro,
    Macro,
    Info,
}

/// What a cell is, for the contextual action.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    None { label: String },
    /// `hostile` means worth swinging at: a plant ahead is not an attack prompt
    Monster { id: Option<i64>, hostile: bool, label: String },
    /// `items`: things also lie on this cell, named as the server did, colour tags and all
    Feature { feature: Feature, label: String, items: Option<String> },
    Item { label: String },
    Wall { label: String },
    DoorClosed { label: String },
    Unknown { label: String },
    Cloud { label: String },
}

impl Default for Target {
    fn default() -> Self {
        Target::None { label: String::new() }
    }
}

impl Target {
    pub fn label(&self) -> &str {
        match self {
            Target::None { label }
            | Target::Monster { label, .. }
            | Target::Feature { label, .. }
            | Target::Item { label }
            | Target::Wall { label }
            | Target::DoorClosed { label }
            | Target::Unknown { label }
            | Target::Cloud { label } => label,
        }
    }

    pub fn none(label: &str) -> Self {
        Target::None { label: label.into() }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Target::None { .. })
    }

    /// A hostile monster: the thing a step that way would swing at.
    pub fn hostile_monster(&self) -> bool {
        matches!(self, Target::Monster { hostile: true, .. })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PromptOption {
    pub hotkey: String,
    pub label: String,
    /// what the letter holds now, in the server's words (a letter picker over the inventory)
    pub held: Option<String>,
    /// the terminal colour the message pane painted the option's word in, for an option read
    /// off the log (`(S)trength`), so a chip naming it wears the same; None for an answer of our own
    pub colour: Option<i64>,
}

impl PromptOption {
    pub fn new(hotkey: &str, label: &str) -> Self {
        PromptOption { hotkey: hotkey.into(), label: label.into(), ..Default::default() }
    }
}

/// A letter picker: the prompt wants one letter (adjust.cc). `Item`: the inventory's letters,
/// so each option can say what it holds; `Spell` and `Ability`: the letters stand bare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Letters {
    Item,
    Spell,
    Ability,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParsedPrompt {
    pub text: String,
    pub options: Vec<PromptOption>,
    pub yesno: bool,
    /// the answer crawl's own Enter gives (a yes/no's default), when it has one
    pub default: Option<String>,
    pub letters: Option<Letters>,
    /// the letter being moved, named on the line before the prompt: the cursor starts there
    pub from: Option<String>,
    /// Escape means something to this prompt: false only for the stat-gain prompt, whose loop
    /// ignores Escape and Enter outright (player-stats.cc `attribute_increase`).
    pub cancel: bool,
    pub stat_gain: bool,
}

/// The pack's pages as tabs (pack_tabs.rs): the page up (its id), and the labels of the ones
/// the bumpers turn to, when there is another page to turn to.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PackContext {
    pub current: Option<String>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

/// One of X's actions as tabs (action_tabs.rs): the tab up, and the labels of the ones the bumpers turn to.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionsContext {
    pub current: ActionTabId,
    pub prev: String,
    pub next: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuContext {
    pub menu: MenuState,
    pub hoverable: Vec<usize>,
    pub arrows_select: bool,
    pub multiselect: bool,
    pub wrap: bool,
    /// the menu filters on Ctrl-F (MF_ALLOW_FILTER)
    pub filter: bool,
    /// two or more sections, headed by their titles: the bumpers jump between them (menu.cc `cycle_headers`)
    pub sections: bool,
    /// some row is marked: Enter would take something (a multiselect's accept does nothing with none)
    pub any_marked: bool,
    /// the key the menu's own help is on, when it names one ("_ for help"); absent, it has none
    pub help_key: Option<String>,
    /// a row picked takes effect at once (the known-items menu's autopickup): nothing is marked, nothing accepted
    pub toggles_at_once: bool,
    pub pack: Option<PackContext>,
    pub actions: Option<ActionsContext>,
    /// the shop menu (tag `shop`): what its letters, `$` and Enter do right now
    pub shop: Option<ShopContext>,
}

impl MenuContext {
    /// A menu context over `menu` with nothing else known (tests, and menus built by hand).
    pub fn of(menu: MenuState) -> Self {
        MenuContext { menu, ..Default::default() }
    }

    pub fn is_crt(&self) -> bool {
        self.menu.kind.as_deref() == Some("crt")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShopMode {
    Buy,
    #[default]
    Examine,
}

/// The shop (`ShopMenu`, shopping.cc) read from what the server prints. Its
/// `more` line carries the action mode (`[!] <white>buy<lightgrey>|examine items`, absent
/// when the shop will not sell), and each row starts with its letter and a
/// state sign: `+` marked for purchase, `$` on the shopping list, `-` neither.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShopContext {
    /// the shop sells to us; a view-only shop (no gold, spectating) only examines
    pub can_buy: bool,
    /// what a letter does: mark the row for purchase, or describe it
    pub mode: ShopMode,
    pub hovered_marked: bool,
    pub hovered_listed: bool,
    pub any_marked: bool,
    pub any_listed: bool,
    /// the order `/` sorts by, as the footer names it in `[/] sort (type)`
    pub sort_order: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PopupAction {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Context {
    pub mode: Mode,
    pub layer: Layer,
    /// The server's description of the quivered action; absent when nothing is quivered.
    pub readied_action: Option<String>,
    pub ahead: Target,
    pub under: Target,
    pub prompt: Option<ParsedPrompt>,
    pub menu: Option<MenuContext>,
    pub popup_type: Option<String>,
    /// Actions the top popup offers (describe-item verbs, pane switches), in pad order. Filled by the app from the overlays.
    pub popup_actions: Option<Vec<PopupAction>>,
    /// The focus layer's cursor over the top overlay: what A and B would do. Filled by the app
    /// from the overlays; absent when the screen offers nothing to focus.
    pub focus: Option<FocusInfo>,
    pub hostiles_in_view: usize,
    /// HP or MP below its maximum: resting would do something.
    pub injured: bool,
    /// Targeting: what the server's cursor rests on, so the bar can say what A will describe.
    pub cursor: Option<Target>,
    /// The targeting is the look mode `x` opened (runner `examining`). Filled by the app.
    pub examining: bool,
    /// An autoexplore is walking (runner `exploring`): LT stops it. Filled by the app.
    pub exploring: bool,
    /// The LT explore setting on Hold: LT held explores until let go, a tap explores at once. Filled by the app.
    pub explore_on_hold: bool,
    /// the crt screen's tag (`skills`) when a CRT text screen is up, in menu or crt mode
    pub crt_tag: Option<String>,
    /// the text prompt's tag (`skill_target`) in text mode
    pub text_tag: Option<String>,
    /// the pane's `--more--` row as printed, in more mode
    pub more_text: Option<String>,
    /// The top overlay has more than fits: the bumpers page only then. Filled by the app.
    pub pageable: bool,
    /// monsters in view, of any attitude: what look mode's `+` and `-` cycle
    pub monsters_in_view: usize,
    /// an aim that can change what it throws: crawl's hint names `Q - select action`
    pub aim_quiver: bool,
    /// the level map's cursor stands where the player does
    pub map_cursor_home: bool,
    /// Character creation's way back, as the screen offers it (newgame.cc): Backspace (8) or a
    /// Space (32) that changes the other choice. Absent on the first screen.
    pub newgame_back: Option<i32>,
}

/// the message pane's untagged text is the page's white (styles.css `.messages`)
pub const LOG_DEFAULT_COLOUR: i64 = 15;

/// The colour the message pane paints the character at `at` (a char offset into the plain
/// text) of a log line in: the innermost tag over it, else the pane's white.
pub fn log_colour_at(formatted: &str, at: usize) -> i64 {
    let mut n = 0;
    for s in formatted_string_to_spans(formatted) {
        n += s.text.chars().count();
        if at < n {
            return s.fg.map_or(LOG_DEFAULT_COLOUR, |f| f as i64);
        }
    }
    LOG_DEFAULT_COLOUR
}

fn char_at(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].chars().count()
}

/// Modes whose top overlay is driven by the focus layer rather than by mode-specific keys.
pub fn is_focus_mode(ctx: &Context) -> bool {
    match ctx.mode {
        Mode::Popup | Mode::Newgame | Mode::Crt | Mode::Yesno | Mode::Prompt | Mode::Dialog | Mode::Ended => true,
        // a crt menu (the skills screen) has no server hover: the focus layer's cursor is the only one
        Mode::Menu => ctx.menu.as_ref().is_some_and(|m| m.is_crt()),
        _ => false,
    }
}

/// mpr.h `msg_channel_type`: the channel a prompt is printed on (MSGCH_PROMPT)
const CH_PROMPT: i64 = 2;

pub fn derive_mode(state: &GameState) -> Mode {
    match state.phase {
        Phase::Lobby => return Mode::Lobby,
        Phase::Ended => return Mode::Ended,
        Phase::Watching => return Mode::Spectating,
        _ => {}
    }
    if state.messages.more {
        return Mode::More;
    }
    if state.text_input.is_some() {
        return Mode::Text;
    }
    if state.dialog.is_some() {
        return Mode::Dialog;
    }
    let m = state.input_mode;
    if m == MouseMode::MORE {
        return Mode::More;
    }
    // prompt.cc `yesno`, however crawl asks it: in the message pane, in a menu's footer (the shop's
    // purchase), or as its own Yes/No menu. One card answers them all
    if m == MouseMode::YESNO {
        return Mode::Yesno;
    }
    // an overlay a `ui_cutoff` hid is not up
    if let Some(popup) = state.top_popup() {
        if !popup.hidden {
            return if popup.kind == "newgame-choice" { Mode::Newgame } else { Mode::Popup };
        }
    }
    if state.menus.iter().any(|m| !m.hidden) {
        return Mode::Menu;
    }
    if state.ui_state == UiState::CRT {
        return Mode::Crt;
    }
    if state.ui_state == UiState::VIEW_MAP {
        return Mode::Levelmap;
    }
    if m == MouseMode::PROMPT {
        return Mode::Prompt;
    }
    if m == MouseMode::TARGET || m == MouseMode::TARGET_DIR || m == MouseMode::TARGET_PATH {
        return Mode::Targeting;
    }
    if m == MouseMode::MACRO {
        return Mode::Macro;
    }
    if m == MouseMode::NORMAL && raw_key_prompt(state) {
        return Mode::Prompt;
    }
    // COMMAND while the server waits for a command, NORMAL while it processes one
    Mode::Command
}

/// A prompt read with a bare `getch_ck`, outside any `mouse_control`: the
/// faded altar (god-prayer.cc `_prompt_ecu_worship`). The server never leaves
/// NORMAL for it; what tells it apart from a command still running: its
/// prompt-channel line is the last thing printed. The letters must reach it raw
/// (the facing-relative rewrite of `b` would hand it another compass letter), so
/// it is a prompt, not command mode.
fn raw_key_prompt(state: &GameState) -> bool {
    state.messages.lines.last().is_some_and(|l| l.channel == Some(CH_PROMPT))
}

/// What a cell is, for the contextual action. Items on a cell ahead are its
/// item billboard, labelled by what its tile shows, or "items" when the tile has no
/// name; under the player there is none, so `items_here` is the server's word that
/// a pile lies underfoot, in its words and colours. A corpse ahead is no item to
/// take: `g` refuses it, and a corpse tile on top means the pile holds nothing else.
fn target_for(scene: &Scene, cell: Option<&SceneCell>, is_under: bool, items_here: Option<String>) -> Target {
    let Some(cell) = cell.filter(|c| c.kind != CellKind::Unknown) else {
        return Target::Unknown { label: "unexplored".into() };
    };
    let label = |fallback: &str| cell.label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| fallback.to_string());
    let bbs: Vec<_> = scene.billboards_at(cell.x, cell.y).collect();
    if !is_under {
        if let Some(mon) = bbs.iter().find(|b| b.kind == BillboardKind::Monster && b.ref_id.is_some()) {
            // hostile means worth swinging at: a plant ahead is not an attack prompt, X autofights past it
            return Target::Monster { id: mon.ref_id, hostile: is_threat(mon), label: mon.name.clone().unwrap_or_default() };
        }
    }
    if cell.kind == CellKind::Door {
        return Target::DoorClosed { label: label("closed door") };
    }
    if cell.kind == CellKind::Wall {
        return Target::Wall { label: label("wall") };
    }
    let pile = bbs.iter().find(|b| b.kind == BillboardKind::Item && !b.name.as_deref().is_some_and(is_stationary_item_name));
    let item = match (is_under, items_here) {
        (true, Some(h)) if !h.is_empty() => Some(h),
        _ => pile.map(|p| p.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "items".into())),
    };
    if let Some(f) = cell.feature.as_ref().filter(|f| !matches!(f, Feature::Other { .. })) {
        return Target::Feature { feature: f.clone(), label: label(feature_type(f)), items: item };
    }
    if let Some(item) = item {
        return Target::Item { label: item };
    }
    if bbs.iter().any(|b| b.kind == BillboardKind::Cloud) {
        return Target::Cloud { label: "cloud".into() };
    }
    if let Some(f) = &cell.feature {
        return Target::Feature { feature: f.clone(), label: label("feature"), items: None };
    }
    Target::None { label: label("floor") }
}

/// The feature's type as the TypeScript names it ('stairs', 'door', ...).
pub fn feature_type(f: &Feature) -> &'static str {
    match f {
        Feature::Stairs { .. } => "stairs",
        Feature::Hatch { .. } => "hatch",
        Feature::Door { .. } => "door",
        Feature::Altar { .. } => "altar",
        Feature::Shop => "shop",
        Feature::Fountain { .. } => "fountain",
        Feature::Portal => "portal",
        Feature::Transporter => "transporter",
        Feature::Trap { .. } => "trap",
        Feature::Other { .. } => "other",
    }
}

macro_rules! re {
    ($name:ident, $re:expr) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($re).unwrap());
    };
}

re!(HOTKEY_RE, r"\(([A-Za-z0-9?*!.,])\)");
// items.cc `pickup`: "Pick up X? ((y)es/(n)o/(a)ll/(m)enu/*?g,/q)" reads its own keys, lowercase, under MOUSE_MODE_YESNO
re!(PICKUP_YESNO_RE, r"\(y\)es/\(n\)o");
// prompt.cc `yesno`: what it prints after a wrong key; the question is the line before
re!(YESNO_NAG_RE, r"\[Y\]es or \[N\]o(, or \[A\]lways)? only, please\.$");
// adjust.cc: the second prompt of `=`, for every kind
re!(LETTER_RE, r"^Adjust to which letter\?");
// adjust.cc `_adjust_spell`, `_adjust_ability`: the first prompt names the kind
re!(LETTER_KIND_RE, r"^Adjust which (spell|ability)\?");
// the line before the letter prompt: the thing being moved
re!(LETTER_FROM_RE, r"^([A-Za-z]) - ");
// a digit range on the prompt line: "(0-9, ..."
re!(DIGITS_RE, r"\((\d)-(\d)[,)]");
// a list of choices as `(K) Name`
re!(LISTED_RE, r"\(([A-Za-z0-9])\) (\S+)");
re!(TRAVEL_PROMPT_RE, r"^\s*Where to\?");
// god-prayer.cc `_prompt_ecu_worship`: the faded altar's prompt
re!(ECU_ALTAR_RE, r"^This altar belongs to ");
re!(HINT_ONLY_RE, r"^\(.*\)$");

/// A choice spelled `k - what it does`, on the prompt line's own parenthesis
/// (travel.cc `add_waypoint` "(0-9, D - delete waypoint)", dgn-overview.cc
/// `_prompt_annotate_branch`) or on the lines printed after it (shout.cc
/// `_issue_orders_prompt`: " t - Shout!"). The key is one character; a label runs to
/// the next comma or parenthesis, or to a gap of two spaces where two share a line. One
/// parenthesis closing the label is passed over and left out of it.
static KEYED_RE: Lazy<fancy_regex::Regex> =
    Lazy::new(|| fancy_regex::Regex::new(r"(?:^|[\s(,])([A-Za-z0-9!?*.<>_^~&%/#:]|Tab/Enter|Enter|Tab) - ([^,()]+?)(?: \([^()]*\))?(?=\s{2,}|[,)]|\s*$)").unwrap());

/// the named keys a prompt spells out, as the key to send: Tab and Enter are not text
fn named_key(k: &str) -> &str {
    match k {
        "Tab/Enter" | "Tab" => "\t",
        "Enter" => "\r",
        other => other,
    }
}

fn lists_choices(text: &str) -> bool {
    LISTED_RE.is_match(text)
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A prompt whose choices are not on its line as `(k)ey` words: they are
/// spelled `k - label` in its parenthesis or on the lines after it, or
/// listed as `(K) Name` there, or on the prompt-channel lines just before it
/// (a message-pane menu). The prompt is the last prompt-channel line near
/// the end of the log; Esc is the layer's own Cancel and is not a chip.
fn listed_prompt(lines: &[GameMessage]) -> Option<ParsedPrompt> {
    let n = lines.len();
    let mut at = None;
    for i in (n.saturating_sub(8)..n).rev() {
        if lines[i].channel == Some(CH_PROMPT) {
            at = Some(i);
            break;
        }
    }
    let at = at?;
    let mut text = formatted_string_to_text(&lines[at].text);
    // a prompt line that is only its hint ("(? for menu, Esc to cancel)") asks nothing: the question is the line above
    if HINT_ONLY_RE.is_match(text.trim()) && at > 0 && lines[at - 1].channel == Some(CH_PROMPT) {
        text = formatted_string_to_text(&lines[at - 1].text);
    }
    let mut options: Vec<PromptOption> = Vec::new();
    let mut add = |options: &mut Vec<PromptOption>, hotkey: &str, label: &str, colour: Option<i64>| {
        let hotkey = named_key(hotkey).to_string();
        if options.iter().any(|o| o.hotkey == hotkey) {
            return;
        }
        let label = label.trim().trim_end_matches(['.', ',']);
        let label = if label.is_empty() { hotkey.clone() } else { upper_first(label) };
        options.push(PromptOption { hotkey, label, held: None, colour });
    };
    // the label is the match's tail (the lookahead takes nothing), so its colour is the log's at that offset
    let keyed = |options: &mut Vec<PromptOption>, add: &mut dyn FnMut(&mut Vec<PromptOption>, &str, &str, Option<i64>), line: &str| {
        let t = formatted_string_to_text(line);
        for c in KEYED_RE.captures_iter(&t).flatten() {
            let (m0, m1, m2) = (c.get(0).unwrap(), c.get(1).unwrap(), c.get(2).unwrap());
            let label_at = m0.end() - m2.as_str().len();
            add(options, m1.as_str(), m2.as_str(), Some(log_colour_at(line, char_at(&t, label_at))));
        }
    };
    let listed = |options: &mut Vec<PromptOption>, add: &mut dyn FnMut(&mut Vec<PromptOption>, &str, &str, Option<i64>), line: &str| {
        let t = formatted_string_to_text(line);
        for c in LISTED_RE.captures_iter(&t) {
            let (m0, m1, m2) = (c.get(0).unwrap(), c.get(1).unwrap(), c.get(2).unwrap());
            let label_at = m0.end() - m2.as_str().len();
            add(options, m1.as_str(), m2.as_str(), Some(log_colour_at(line, char_at(&t, label_at))));
        }
    };
    // a menu in the message pane: its rows are the prompt-channel lines just before the prompt, in their order
    let mut first = at;
    while first > 0 && lines[first - 1].channel == Some(CH_PROMPT) && lists_choices(&formatted_string_to_text(&lines[first - 1].text)) {
        first -= 1;
    }
    for line in &lines[first..at] {
        listed(&mut options, &mut add, &line.text);
    }
    // or on the prompt line itself: the faded altar's "(a) Trog, (b) Okawaru or (c) Yredelemnul"
    listed(&mut options, &mut add, &lines[at].text);
    if TRAVEL_PROMPT_RE.is_match(&text) {
        add(&mut options, "<", "Up the stairs", None);
        add(&mut options, ">", "Down the stairs", None);
    }
    // god-prayer.cc `_prompt_ecu_worship`: "press enter to convert or escape to cancel"; the letters describe
    if ECU_ALTAR_RE.is_match(&text) {
        add(&mut options, "Enter", "Convert", None);
    }
    if let Some(d) = DIGITS_RE.captures(&text) {
        let (a, b): (u32, u32) = (d[1].parse().unwrap_or(0), d[2].parse().unwrap_or(0));
        for n in a..=b {
            add(&mut options, &n.to_string(), &n.to_string(), None);
        }
    }
    keyed(&mut options, &mut add, &lines[at].text);
    for line in &lines[at + 1..] {
        keyed(&mut options, &mut add, &line.text);
        listed(&mut options, &mut add, &line.text);
    }
    if options.is_empty() {
        return None;
    }
    Some(ParsedPrompt { text, options, yesno: false, cancel: true, ..Default::default() })
}

/// Every inventory letter in slot order (invent.h `index_to_letter`): a-z, then A-Z.
const INVENTORY_LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The letter picker for a prompt that wants one letter. Each option is a
/// letter; over the inventory it also says what the letter holds now
/// (`state.player.inv`, keyed by slot: a-z are 0-25, A-Z 26-51). `i` is the prompt's
/// line: the lines before it say what kind of letter this is and which one is moving.
fn letter_prompt(state: &GameState, lines: &[GameMessage], i: usize, text: &str) -> ParsedPrompt {
    let mut kind = if text.contains("for menu") { Some(Letters::Item) } else { None };
    let mut from = None;
    for j in (i.saturating_sub(6)..i).rev() {
        let t = formatted_string_to_text(&lines[j].text);
        if from.is_none() {
            if let Some(f) = LETTER_FROM_RE.captures(&t) {
                from = Some(f[1].to_string());
            }
        }
        if let Some(k) = LETTER_KIND_RE.captures(&t) {
            if kind.is_none() {
                kind = Some(if &k[1] == "spell" { Letters::Spell } else { Letters::Ability });
            }
            break;
        }
    }
    let kind = kind.unwrap_or(Letters::Item);
    let options = INVENTORY_LETTERS
        .chars()
        .enumerate()
        .map(|(slot, ch)| {
            let held = if kind == Letters::Item { state.player.inv.get(&(slot as i64)).and_then(|it| it.name.clone()) } else { None };
            PromptOption { hotkey: ch.to_string(), label: ch.to_string(), held, colour: None }
        })
        .collect();
    ParsedPrompt { text: text.into(), options, yesno: false, cancel: true, letters: Some(kind), from, ..Default::default() }
}

// player-stats.cc `attribute_increase`: the prompt is uppercase unless `easy_confirm = all`.
re!(STAT_GAIN_RE, r"^Increase \([Ss]\)trength, \([Ii]\)ntelligence, or \([Dd]\)exterity\?");
re!(LEAD_OPTION_RE, r"\S*\([A-Za-z0-9?*!.,]\)\S*");
re!(LEAD_PUNCT_RE, r"[,.?!:;]");

/// When a choice prompt is nothing but its options ("Increase (S)trength,
/// (I)ntelligence, or (D)exterity?"), the answer chips are the question, and
/// printing it over them says everything twice. This returns what is left of
/// the text once the option words, the punctuation and the "or"/"and" between
/// them go: at most one word, the verb the chips complete ("Increase"), or ""
/// when nothing is. None when more remains, so the card must print it.
pub fn prompt_lead(text: &str) -> Option<String> {
    let a = LEAD_OPTION_RE.replace_all(text, " ");
    let b = LEAD_PUNCT_RE.replace_all(&a, " ");
    let rest: Vec<&str> = b.split_whitespace().filter(|w| !w.eq_ignore_ascii_case("or") && !w.eq_ignore_ascii_case("and")).collect();
    if rest.len() <= 1 {
        Some(rest.concat())
    } else {
        None
    }
}

// A yes/no's own default, spelled in its question: `(y/N)` is No, `(Y/n)` Yes.
re!(YESNO_DEFAULT_RE, r"\((?:(Y)/n|y/(N))\)");
re!(MENU_ROW_LETTER_RE, r"^\s*\S\s+-\s+");

/// A yes/no crawl asks with a menu of its own: tag `prompt`, the question in
/// its title, Yes / No (and Always) rows with their hotkeys, the hover on the default
/// (prompt.cc `yesno` with `prompt_menu`); or a question in the footer of the menu that is
/// up (the shop's "Purchase items for 40 gold? (y/N)"), which the message log never shows.
fn menu_yesno(menu: &MenuState) -> Option<ParsedPrompt> {
    if menu.tag == "prompt" {
        let mut options = Vec::new();
        let mut dflt = None;
        for (i, it) in menu.items.iter().enumerate() {
            let Some(it) = it else { continue };
            let Some(hk) = it.hotkeys.as_ref().and_then(|h| h.first()) else { continue };
            let k = char::from_u32(*hk as u32).map(|c| c.to_string()).unwrap_or_default();
            let text = formatted_string_to_text(&it.text);
            let label = MENU_ROW_LETTER_RE.replace(&text, "").trim().to_string();
            options.push(PromptOption::new(&k, if label.is_empty() { &k } else { &label }));
            if i as i64 == menu.last_hovered {
                dflt = Some(k);
            }
        }
        if options.is_empty() {
            return None;
        }
        return Some(ParsedPrompt { text: formatted_string_to_text(&menu.title.text).trim().into(), options, yesno: true, cancel: true, default: dflt, ..Default::default() });
    }
    let text = formatted_string_to_text(&menu.more).trim().to_string();
    let d = YESNO_DEFAULT_RE.captures(&text)?;
    let default = Some(if d.get(1).is_some() { "Y" } else { "N" }.to_string());
    Some(ParsedPrompt { text, options: vec![PromptOption::new("Y", "Yes"), PromptOption::new("N", "No")], yesno: true, cancel: true, default, ..Default::default() })
}

/// Parse hotkeys from the latest prompt-channel message. Only when the server
/// says a prompt is up (inputMode YESNO or PROMPT): message text alone never
/// makes a prompt, so in command mode nothing here can turn A into `y`.
/// Only the prompt line itself. A prompt line with no `(k)ey` words may still
/// spell its choices another way (`listed_prompt`), or want a bare letter (`letter_prompt`).
fn parse_prompt(state: &GameState) -> Option<ParsedPrompt> {
    let yesno_mode = state.input_mode == MouseMode::YESNO;
    if yesno_mode {
        if let Some(p) = state.top_menu().and_then(menu_yesno) {
            return Some(p);
        }
    }
    let prompt_mode = state.input_mode == MouseMode::PROMPT || (state.input_mode == MouseMode::NORMAL && raw_key_prompt(state));
    if !yesno_mode && !prompt_mode {
        return None;
    }
    let lines = &state.messages.lines;
    let n = lines.len();
    for i in (n.saturating_sub(3)..n).rev() {
        let l = &lines[i];
        // a `(D) Dungeon` list printed around a prompt is not the prompt
        if l.channel.is_some_and(|c| c != CH_PROMPT) {
            continue;
        }
        let text = formatted_string_to_text(&l.text);
        if lists_choices(&text) {
            continue;
        }
        // prompt.cc `yesno` prints the question bare, and a call that passes allow_lowercase
        // false takes only an uppercase Y; uppercase is right for every call. Pickup's prompt
        // is not `yesno` and takes only lowercase.
        if yesno_mode && !YESNO_NAG_RE.is_match(&text) {
            let options = if PICKUP_YESNO_RE.is_match(&text) { vec![PromptOption::new("y", "Yes"), PromptOption::new("n", "No")] } else { vec![PromptOption::new("Y", "Yes"), PromptOption::new("N", "No")] };
            let default = YESNO_DEFAULT_RE.captures(&text).map(|d| if d.get(1).is_some() { "Y" } else { "N" }.to_string());
            return Some(ParsedPrompt { text, options, yesno: true, cancel: true, default, ..Default::default() });
        }
        if !prompt_mode {
            continue;
        }
        if LETTER_RE.is_match(&text) {
            return Some(letter_prompt(state, lines, i, &text));
        }
        let mut options = Vec::new();
        for c in HOTKEY_RE.captures_iter(&text) {
            let m = c.get(0).unwrap();
            // label: the word containing the hotkey, parens and all, so a chip reads
            // exactly as the prompt does in the message log ("(S)trength")
            let start = text[..m.start()].rfind(' ').map_or(0, |p| p + 1);
            let end = text[m.start()..].find(' ').map_or(text.len(), |p| p + m.start());
            let label = text[start..end].trim_end_matches([',', '.', '?', '!', ':', ';']);
            let hotkey = c[1].to_string();
            options.push(PromptOption { label: if label.is_empty() { hotkey.clone() } else { label.to_string() }, hotkey, held: None, colour: Some(log_colour_at(&l.text, char_at(&text, start))) });
        }
        if !options.is_empty() {
            let stat_gain = STAT_GAIN_RE.is_match(&text);
            return Some(ParsedPrompt { text, options, yesno: false, cancel: !stat_gain, stat_gain, ..Default::default() });
        }
    }
    if prompt_mode {
        return listed_prompt(lines);
    }
    // a yes/no whose question the log does not show still has its two answers
    if yesno_mode {
        Some(default_yesno())
    } else {
        None
    }
}

/// A yes/no's answers when nothing names them: prompt.cc `yesno` takes Y and N, whatever the rc.
pub fn default_yesno() -> ParsedPrompt {
    ParsedPrompt { text: String::new(), options: vec![PromptOption::new("Y", "Yes"), PromptOption::new("N", "No")], yesno: true, cancel: true, ..Default::default() }
}

re!(MARKED_RE, r"^\s*\S\s[+#]\s");
// The known-items menu, `\\` (invent.cc `KnownMenu`): picking a row toggles its autopickup there
// and then, and its `+` says autopickup is on, not that the row is marked.
re!(KNOWN_ITEMS_RE, r"^Recognised items\.");
// The help a menu offers, as its title or footer names the key: "(_ for help)", "[?] help"
re!(MENU_HELP_RE, r"\(([_?]) for help\)|\[<?\w*>?\s*([_?])\s*</?\w*>?\]\s*help");

fn menu_context(state: &GameState) -> Option<MenuContext> {
    let menu = state.top_menu()?;
    let arrows_select = menu.flags & 0x40000 != 0;
    let level = |i: usize| menu.items.get(i).and_then(|it| it.as_ref()).and_then(|it| it.level).unwrap_or(2);
    let mut hoverable = Vec::new();
    for (i, it) in menu.items.iter().enumerate() {
        let Some(it) = it else { continue };
        if it.level.unwrap_or(2) == 2 && (menu.tag == "use_item" || it.hotkeys.as_ref().is_some_and(|h| !h.is_empty()) || arrows_select) {
            hoverable.push(i);
        }
    }
    let mut blocks = 0;
    for i in 0..menu.items.len() {
        if level(i) < 2 && (i == 0 || level(i - 1) >= 2) {
            blocks += 1;
        }
    }
    // a marked row says so between its letter and its text: `a + club`, `a # 3 darts` for some of a stack
    let known = KNOWN_ITEMS_RE.is_match(&formatted_string_to_text(&menu.title.text));
    let any_marked = !known && menu.items.iter().flatten().any(|it| MARKED_RE.is_match(&formatted_string_to_text(&it.text)));
    let help_src = formatted_string_to_text(&format!("{}\n{}\n{}", menu.title.text, menu.more, menu.alt_more));
    let help_key = MENU_HELP_RE.captures(&help_src).and_then(|c| c.get(1).or(c.get(2))).map(|m| m.as_str().to_string());
    let mut ctx = MenuContext {
        menu: menu.clone(),
        hoverable,
        arrows_select,
        multiselect: menu.flags & 0x0004 != 0,
        wrap: menu.flags & 0x0080 != 0,
        filter: menu.flags & 0x0100 != 0,
        sections: blocks >= 2,
        any_marked,
        help_key,
        toggles_at_once: known,
        ..Default::default()
    };
    if menu.tag == "shop" {
        ctx.shop = Some(shop_context(menu));
    }
    if let Some(strip) = pack_strip(menu, state) {
        let tab = |i: i32| if i >= 0 { strip.tabs.get(i as usize) } else { None };
        ctx.pack = Some(PackContext {
            current: tab(strip.current).map(|t| t.id.to_string()),
            prev: tab(neighbour(&strip, -1)).map(|t| t.label.clone()),
            next: tab(neighbour(&strip, 1)).map(|t| t.label.clone()),
        });
    }
    if let Some(tab) = action_tab_of(menu) {
        ctx.actions = Some(ActionsContext { current: tab, prev: action_tab(action_neighbour(tab, -1)).label.into(), next: action_tab(action_neighbour(tab, 1)).label.into() });
    }
    Some(ctx)
}

// A shop row's state sign, from the text `ShopEntry::get_text` prints: ` a + 30 gold   item`.
re!(SHOP_ROW_RE, r"^\s*[a-zA-Z]\s([+$-])\s");
// The sort switch the more line prints: `[<w>/</w>] sort (type)`, the name being the current order.
re!(SHOP_SORT_RE, r"sort \(([^)]+)\)");
// a live shop (crawl.dcss.io, 2026-09) prints `<white>buy<lightgrey>|examine items`; `<w>` is the same colour tag
re!(SHOP_BUY_RE, r"<(?:w|white)>buy(?:</w>|<lightgrey>)\|examine");
re!(SHOP_EXAMINE_RE, r"buy\|<(?:w|white)>examine");

pub fn shop_context(menu: &MenuState) -> ShopContext {
    let more = format!("{}\n{}", menu.more, menu.alt_more);
    let buy = SHOP_BUY_RE.is_match(&more);
    let examine = SHOP_EXAMINE_RE.is_match(&more);
    let mut ctx = ShopContext { can_buy: buy || examine, mode: if buy { ShopMode::Buy } else { ShopMode::Examine }, ..Default::default() };
    for (i, it) in menu.items.iter().enumerate() {
        let Some(it) = it else { continue };
        let text = formatted_string_to_text(&it.text);
        let Some(m) = SHOP_ROW_RE.captures(&text) else { continue };
        let marked = &m[1] == "+";
        let listed = &m[1] == "$";
        ctx.any_marked |= marked;
        ctx.any_listed |= listed;
        if i as i64 == menu.last_hovered {
            ctx.hovered_marked = marked;
            ctx.hovered_listed = listed;
        }
    }
    ctx.sort_order = SHOP_SORT_RE.captures(&more).map(|c| c[1].to_string());
    ctx
}

/// The quivered action as plain text, or None when there is none. The
/// server's `quiver_desc` is a formatted string and an empty quiver is spelled out
/// rather than blank: quiver.cc `quiver_description` prints `<darkgrey>Nothing quivered</darkgrey>`.
pub fn readied_action(quiver_desc: Option<&str>) -> Option<String> {
    let text = formatted_string_to_text(quiver_desc.unwrap_or("")).trim().to_string();
    if text.is_empty() || text == "Nothing quivered" {
        None
    } else {
        Some(text)
    }
}

re!(CHANGE_RE, r"Space - Change ");

/// The new-game screen's own button back: Backspace's, or a Space that changes the other choice.
fn newgame_back(data: &isorun_webtiles::Obj) -> Option<i32> {
    // a species or background button has one `label`; the weapon screen's rows have `labels`, a column each
    let buttons = data.get("sub-items").and_then(|s| s.get("buttons")).and_then(|b| b.as_array())?;
    let hotkey = |b: &serde_json::Value| b.get("hotkey").and_then(|h| h.as_i64());
    let said = |b: &serde_json::Value| {
        let mut parts = vec![b.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string()];
        if let Some(ls) = b.get("labels").and_then(|l| l.as_array()) {
            parts.extend(ls.iter().filter_map(|l| l.as_str()).map(String::from));
        }
        formatted_string_to_text(&parts.join(" "))
    };
    if buttons.iter().any(|b| hotkey(b) == Some(8)) {
        return Some(8);
    }
    if buttons.iter().any(|b| hotkey(b) == Some(32) && CHANGE_RE.is_match(&said(b))) {
        return Some(32);
    }
    None
}

/// The context of a frame: the mode, what is ahead (the way the player last stepped,
/// `heading`) and underfoot, and what the prompt, menu or aim up offers.
pub fn derive_context(state: &GameState, scene: &Scene, heading: Dir8, layer: Layer) -> Context {
    let mode = derive_mode(state);
    let ahead = target_for(scene, scene.cell_ahead(heading, None), false, None);
    let under = target_for(scene, scene.cell_under(), true, floor_items_label(state));
    // what autofight would attack: a plant is hostile-attitude firewood and counts for nothing
    let in_view = scene.monsters_in_view();
    let hostiles = in_view.iter().filter(|b| is_threat(b)).count();
    let p = &state.player;
    let mut ctx = Context { mode, layer, ahead, under, hostiles_in_view: hostiles, monsters_in_view: in_view.len(), readied_action: readied_action(Some(&p.quiver_desc)), ..Default::default() };
    if p.hp < p.hp_max || p.mp < p.mp_max {
        ctx.injured = true;
    }
    if mode == Mode::Yesno || mode == Mode::Prompt {
        ctx.prompt = parse_prompt(state);
    }
    if mode == Mode::More {
        ctx.more_text = Some(if state.messages.more_text.is_empty() { "--more--".into() } else { state.messages.more_text.clone() });
    }
    if mode == Mode::Menu {
        ctx.menu = menu_context(state);
        if let Some(m) = ctx.menu.as_ref().filter(|m| m.is_crt()) {
            ctx.crt_tag = Some(m.menu.tag.clone());
        }
    }
    if mode == Mode::Crt {
        ctx.crt_tag = Some("crt".into());
    }
    if mode == Mode::Text {
        ctx.text_tag = state.text_input.as_ref().and_then(|t| t.tag.clone()).filter(|t| !t.is_empty());
    }
    if mode == Mode::Levelmap {
        // the map's own cursor (tileweb.cc load_dungeon: CURSOR_MAP), not the mouse's
        let c = state.cursors.get(2).copied().flatten().or(state.cursors.first().copied().flatten());
        ctx.map_cursor_home = c.is_none_or(|c| (c.x, c.y) == scene.player);
    }
    if mode == Mode::Targeting {
        // directn.cc: a fire's aim prints "Q - select action" in its hint; a spell's does not, and cycles nothing
        let lines = &state.messages.lines;
        for l in lines.iter().rev().take(6) {
            let t = formatted_string_to_text(&l.text);
            if t.starts_with("Press: ") {
                ctx.aim_quiver = t.contains("select action");
                break;
            }
        }
        if let Some(c) = state.cursors.first().copied().flatten() {
            let on_player = (c.x, c.y) == scene.player;
            let t = target_for(scene, scene.cell(c.x, c.y), on_player, if on_player { floor_items_label(state) } else { None });
            ctx.cursor = Some(if on_player && t.is_none() { Target::none("here") } else { t });
        }
    }
    if mode == Mode::Popup || mode == Mode::Newgame {
        ctx.popup_type = state.top_popup().map(|p| p.kind.clone());
    }
    if mode == Mode::Newgame {
        ctx.newgame_back = state.top_popup().and_then(|p| newgame_back(&p.data));
    }
    ctx
}

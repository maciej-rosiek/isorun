//! What each pad button (and each key that stands for one) does in each situation (the port of
//! `bindings.ts`): the binding tables by mode, the labels the action bar shows, and [`resolve`],
//! which turns a [`PadEvent`] into an [`Action`] for the runner.

use crate::action_tabs::{shout, swap_weapons};
use crate::context::{default_yesno, is_focus_mode, Context, MenuContext, Mode, ParsedPrompt, ShopContext, ShopMode, Target, LOG_DEFAULT_COLOUR};
use crate::focus::FocusOp;
use crate::gamepad::{Button, DirSource, PadEvent};
use isorun_scene::{Dir8, DoorState, Feature, UpDown};
use isorun_webtiles::{coloured_text, Keys, MenuFlag};

/// A direction relative to the screen: 0 is up (the camera's facing), clockwise.
pub type RelDir = u8;

/// A key as crawl gets it: a keycode (`{msg:'key'}`) or text (`{msg:'input'}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stroke {
    Key(i32),
    Text(String),
}

/// One key of a sequence. `await_prompt` holds the key back until the
/// server has left command mode (a prompt, menu or popup opened) so a `G>`
/// style macro cannot spill its second key into the map when the first is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeqItem {
    pub stroke: Stroke,
    pub await_prompt: bool,
}

impl SeqItem {
    pub fn key(code: i32) -> Self {
        SeqItem { stroke: Stroke::Key(code), await_prompt: false }
    }
    pub fn text(t: &str) -> Self {
        SeqItem { stroke: Stroke::Text(t.into()), await_prompt: false }
    }
    pub fn awaiting(self) -> Self {
        SeqItem { await_prompt: true, ..self }
    }
    pub fn text_is(&self, t: &str) -> bool {
        matches!(&self.stroke, Stroke::Text(s) if s == t)
    }
}

/// A press shorter than this is a tap; longer runs the hold action.
pub const HOLD_MS: f64 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuOp {
    Next,
    Prev,
    PageNext,
    PagePrev,
    SectionNext,
    SectionPrev,
    First,
    Last,
    Select,
    /// sends the hovered row's hotkey shifted (the shop's "put on shopping list")
    AltSelect,
    /// describes the hovered row where it stands (menu.cc CMD_MENU_EXAMINE)
    Examine,
    Toggle,
    Cancel,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiOp {
    Commands,
    ActionTab,
    Equipment,
    Palette,
    System,
    Keyboard,
    ResetView,
    Levelmap,
    Bindings,
    ScrollLog,
    PopupAction,
    Dock,
    InstantExplore,
    ExploreWhileHeld,
    /// the runner's own: A over a square where a feature and items meet asks which (game.ts `uiOp`)
    Interact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OskOp {
    Move,
    Type,
    Backspace,
    Space,
    Submit,
    Cancel,
    Shift,
}

/// Which section of the command palette applies: the cmd-keys.h key table for the mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    Command,
    Levelmap,
    Targeting,
    Menu,
}

/// A held direction is a typewriter of single steps, never a run: a run
/// (Shift+dir) keeps going until the server decides to stop, so a stick held a
/// moment too long would carry the player across the level. Every repeat tick
/// offers one `held` step at the gamepad's repeat cadence.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `turns`: left / right rotate the camera instead of strafing; `held`: a repeat tick of a held direction
    Step { dir: RelDir, run: bool, attack: bool, turns: bool, held: bool },
    Turn { right: bool },
    Look { dx: f32, dy: f32 },
    /// A: act underfoot, then ahead. Overlapping interactions open a chooser.
    Contextual { alt: Option<bool> },
    /// RT: one normal DCSS autofight step, including the server's safety checks.
    Fight,
    /// Open look mode ahead, or describe the cursor's cell when already looking.
    Examine,
    /// Fire the quivered action (`f`), or confirm the shot when already aiming.
    Fire,
    Hold { tap: Box<Action>, hold: Box<Action> },
    /// `contextual`: the binding exists only because of the situation, so the bar shows it unasked;
    /// `keeps_map`: the keys leave the level map for another screen, and the map stays drawn until that screen is up;
    /// `repeats`: held, the button sends the keys again and again
    Keys { seq: Vec<SeqItem>, label: String, contextual: bool, keeps_map: bool, repeats: bool },
    Menu { op: MenuOp },
    Cursor { dir: Dir8 },
    Prompt { hotkey: String },
    /// the focus layer's cursor over the top overlay (popup, prompt, CRT screen, dialog)
    Focus { op: FocusOp },
    Ui { op: UiOp, arg: Option<i32>, category: Option<CommandCategory>, section: Option<String> },
    Osk { op: OskOp, dir: Option<Dir8> },
}

impl Action {
    pub fn step(dir: RelDir) -> Self {
        Action::Step { dir, run: false, attack: false, turns: false, held: false }
    }
    pub fn ui(op: UiOp) -> Self {
        Action::Ui { op, arg: None, category: None, section: None }
    }
    pub fn ui_arg(op: UiOp, arg: i32) -> Self {
        Action::Ui { op, arg: Some(arg), category: None, section: None }
    }
    pub fn menu(op: MenuOp) -> Self {
        Action::Menu { op }
    }
    pub fn focus(op: FocusOp) -> Self {
        Action::Focus { op }
    }
    pub fn osk(op: OskOp) -> Self {
        Action::Osk { op, dir: None }
    }
    pub fn prompt(hotkey: &str) -> Self {
        Action::Prompt { hotkey: hotkey.into() }
    }
    pub fn keys(seq: Vec<SeqItem>, label: &str) -> Self {
        Action::Keys { seq, label: label.into(), contextual: false, keeps_map: false, repeats: false }
    }
    /// The keys the action sends, when it is a keys action.
    pub fn seq(&self) -> Option<&[SeqItem]> {
        match self {
            Action::Keys { seq, .. } => Some(seq),
            _ => None,
        }
    }
    pub fn label(&self) -> Option<&str> {
        match self {
            Action::Keys { label, .. } => Some(label),
            _ => None,
        }
    }
}

fn k(text: &str, label: &str) -> Action {
    Action::keys(vec![SeqItem::text(text)], label)
}
fn kc(key: i32, label: &str) -> Action {
    Action::keys(vec![SeqItem::key(key)], label)
}
/// Ctrl+letter as the official client sends it: the control code.
fn ctrl(letter: char, label: &str) -> Action {
    kc(letter.to_ascii_uppercase() as i32 - 64, label)
}
fn esc() -> Action {
    kc(Keys::ESC, "Cancel")
}
pub fn stop_explore() -> Action {
    kc(Keys::ESC, "Stop exploring")
}
fn enter() -> Action {
    kc(Keys::ENTER, "Confirm")
}
/// space, as the `--more--` loop takes it (message.cc `readkey_more`); also what a click on
/// the ringed message pane sends. It wears the game's own word rather than ours.
pub fn continue_more() -> Action {
    kc(Keys::SPACE, "--more--")
}
fn palette(category: CommandCategory) -> Action {
    Action::Ui { op: UiOp::Palette, arg: None, category: Some(category), section: None }
}
fn system() -> Action {
    Action::ui(UiOp::System)
}
fn hold(tap: Action, hold: Action) -> Action {
    Action::Hold { tap: Box::new(tap), hold: Box::new(hold) }
}
/// A keys binding the situation created (a shop with rows marked): the bar shows it unasked.
fn situational(a: Action) -> Action {
    match a {
        Action::Keys { seq, label, keeps_map, repeats, .. } => Action::Keys { seq, label, contextual: true, keeps_map, repeats },
        other => other,
    }
}

/// Shared actions the game screen sends outside the binding tables, so the bar can label them.
pub fn autofight() -> Action {
    kc(Keys::TAB, "Autofight")
}
pub fn level_map() -> Action {
    k("X", "Level map")
}

/// A binding table: what each button does now.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Table(Vec<(Button, Action)>);

impl Table {
    pub fn get(&self, b: Button) -> Option<&Action> {
        self.0.iter().find(|(x, _)| *x == b).map(|(_, a)| a)
    }
    pub fn set(&mut self, b: Button, a: Action) {
        match self.0.iter_mut().find(|(x, _)| *x == b) {
            Some(slot) => slot.1 = a,
            None => self.0.push((b, a)),
        }
    }
    pub fn remove(&mut self, b: Button) {
        self.0.retain(|(x, _)| *x != b);
    }
    pub fn iter(&self) -> impl Iterator<Item = &(Button, Action)> {
        self.0.iter()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn with(mut self, b: Button, a: Action) -> Self {
        self.set(b, a);
        self
    }
}

impl std::ops::Index<Button> for Table {
    type Output = Action;
    fn index(&self, b: Button) -> &Action {
        self.get(b).unwrap_or_else(|| panic!("no binding on {b:?}"))
    }
}

/// Direct controls: no trigger layers, and only wait/rest uses a hold.
pub fn command_table() -> Table {
    use Button::*;
    Table::default()
        .with(A, Action::Contextual { alt: None })
        .with(B, esc())
        // the two face buttons over A open screens: X the actions, Y the pack
        .with(X, Action::ui(UiOp::Commands))
        .with(Y, Action::ui(UiOp::Equipment))
        // The right hand attacks -- a shot on RB, autofight on RT, which repeats while held -- and the left hand does
        // everything that is not attacking. Explore and autofight stay on opposite hands. Wait and rest are the
        // same verb at two lengths, so they share LB, beside autoexplore: the left hand passes the time.
        .with(LB, hold(k(".", "Wait one turn"), k("5", "Rest")))
        .with(RB, Action::Fire)
        .with(LT, hold(k("o", "Autoexplore"), Action::ui(UiOp::InstantExplore)))
        .with(RT, Action::Fight)
        // L3 puts the pad's cursor on the dock; R3 examines
        .with(L3, Action::ui(UiOp::Dock))
        .with(R3, Action::Examine)
        // the level map, where the ways across the dungeon are
        .with(Select, Action::ui(UiOp::Levelmap))
        // the way out of a game from the pad: the Isorun menu, with Save and exit on it
        .with(Start, system())
}

/// The menus whose rows answer CMD_MENU_EXAMINE with a description: InvMenu's
/// (invent.cc `examine_index`: inventory, pickup, drop, use_item), the spell
/// and memorise menus, abilities, the shop, its list and the stash search.
/// Menu::examine_index itself is a no-op, so on any other menu the key does nothing.
const EXAMINING_MENUS: [&str; 7] = ["inventory", "pickup", "use_item", "spell", "ability", "shop", "stash"];

fn examines(tag: &str) -> bool {
    EXAMINING_MENUS.contains(&tag)
}

/// A server menu. Where a button stands for one of the switches the menu
/// prints in its keyhelp (menu.cc `Menu::get_keyhelp`) it wears that switch's own
/// word, lower case as the footer prints it. The rest keep our names. A button is only
/// there when it does something on this menu.
fn menu_table(m: Option<&MenuContext>, ctx: &Context) -> Table {
    use Button::*;
    let mut t = Table::default().with(A, Action::menu(MenuOp::Select)).with(B, Action::menu(MenuOp::Cancel)).with(L3, palette(CommandCategory::Menu));
    let Some(m) = m else { return t };
    // the menu's own help, on the key it names (the inventory's `_`); a menu that names none has none
    if let Some(h) = &m.help_key {
        t.set(Y, k(h, "Help"));
    }
    // X's actions as tabs: the bumpers turn them, and Y and LT are the two actions that are no list
    if m.actions.is_some() {
        if examines(&m.menu.tag) {
            t.set(X, Action::menu(MenuOp::Examine));
        }
        return t.with(LB, Action::ui_arg(UiOp::ActionTab, -1)).with(RB, Action::ui_arg(UiOp::ActionTab, 1)).with(Y, swap_weapons()).with(LT, shout());
    }
    // the hovered row, described where it stands
    if examines(&m.menu.tag) && !m.toggles_at_once {
        t.set(X, Action::menu(MenuOp::Examine));
    }
    // the previous / next section (menu.cc cycle_headers, both ways); a menu without headers pages, when it has pages.
    // In the pack the bumpers turn its pages, as Left and Right do, and the sections move to the triggers beside them
    let sectioned = m.sections || ctx.pageable;
    // (a pack with one page to it keeps the sections on the bumpers)
    let turns = m.pack.as_ref().is_some_and(|p| p.next.is_some());
    if turns {
        t.set(LB, Action::menu(MenuOp::Left));
        t.set(RB, Action::menu(MenuOp::Right));
    } else if sectioned {
        t.set(LB, Action::menu(MenuOp::SectionPrev));
        t.set(RB, Action::menu(MenuOp::SectionNext));
    }
    if m.multiselect && !m.toggles_at_once {
        t.set(LT, Action::menu(MenuOp::Toggle));
        // `,` selects all (menu.cc CMD_MENU_SELECT_ALL); a paged inventory (drop) selects a category at a time instead
        if m.menu.flags & MenuFlag::PAGED_INVENTORY == 0 {
            t.set(R3, k(",", "Select all"));
        }
        // Start is Enter here (screen_key): it takes what is marked, so it is there once something is
        if m.any_marked {
            t.set(Start, situational(kc(Keys::ENTER, "accept")));
        }
    }
    // Select stays the filter; the palette's menu section holds the rest
    if m.filter {
        t.set(Select, ctrl('F', "Filter"));
    }
    if turns && sectioned {
        t.set(LT, Action::menu(MenuOp::SectionPrev));
        t.set(RT, Action::menu(MenuOp::SectionNext));
    }
    t
}

/// The shop (`ShopMenu`): letters mark a row for purchase (or describe it in
/// examine mode), Enter buys what is marked (or the shopping list when
/// nothing is), `!` flips buy/examine, `$` moves the marks to the shopping
/// list and back, a shifted letter lists one row, `/` sorts. Every button that stands for
/// one of the switches the shop prints in its more line wears that switch's own words,
/// lower case and all. `$` is the exception: the shop binds it but never prints it.
fn shop_table(shop: &ShopContext, pageable: bool) -> Table {
    use Button::*;
    let sort = match &shop.sort_order {
        Some(o) => format!("sort ({o})"),
        None => "Sort".into(),
    };
    let mut t = Table::default()
        .with(A, Action::menu(MenuOp::Select))
        .with(B, Action::menu(MenuOp::Cancel))
        .with(Y, Action::menu(MenuOp::AltSelect))
        .with(L3, palette(CommandCategory::Menu))
        .with(R3, k("/", &sort));
    if pageable {
        t.set(LB, Action::menu(MenuOp::PagePrev));
        t.set(RB, Action::menu(MenuOp::PageNext));
    }
    // the server prints the mode as a live prompt, so the bar shows the flip unasked; the label is the
    // prompt's own text and does not change with the mode
    if shop.can_buy {
        t.set(X, situational(k("!", "buy|examine items")));
    }
    if shop.any_marked {
        t.set(LT, situational(k("$", "List marked")));
    } else if shop.can_buy && shop.any_listed {
        t.set(LT, situational(k("$", "Mark listed")));
    }
    // Enter with nothing marked and nothing listed does nothing in buy mode (purchase_selected returns)
    if shop.can_buy && shop.any_marked {
        t.set(Start, situational(kc(Keys::ENTER, "buy marked items")));
    } else if shop.can_buy && shop.any_listed {
        t.set(Start, situational(kc(Keys::ENTER, "buy shopping list")));
    } else if shop.mode == ShopMode::Examine {
        t.set(Start, kc(Keys::ENTER, "describe"));
    }
    t
}

/// An aim (a throw, a spell): shaped like the look, with the mode's own action on A and on the trigger that opened it.
fn targeting_table(ctx: &Context) -> Table {
    use Button::*;
    let mut t = Table::default()
        .with(A, Action::Fire)
        .with(B, esc())
        // the same button as opened the aim confirms it, so tapping RB again and again fires shot after shot as `f f f` does
        .with(RB, Action::Fire)
        .with(X, k("v", "Describe"))
        .with(Select, palette(CommandCategory::Targeting));
    // one button walks the targets, so the other bumper can stay the shot; with one target there is nowhere to walk
    if ctx.hostiles_in_view > 1 {
        t.set(LB, k("+", "Next target"));
    }
    // the quiver, cycled inside the aim (CMD_TARGET_CYCLE_QUIVER_FORWARD / _BACKWARD). A spell's aim cycles nothing
    if ctx.aim_quiver {
        t.set(Y, hold(k(")", "Next quiver"), k("(", "Previous quiver")));
    }
    t
}

/// Look mode, what `x` opens (directn.cc `_look_around_target`). `v` describes the cell,
/// while Enter, `.` and `5` select it, which `do_look_around` turns into travel. So A
/// describes, named for what the cursor rests on, travel moves to X and help sits on Y,
/// the three the server prints itself ("Press: ? - help, v - describe, . - travel"). The
/// bumpers cycle monsters (`-`/`+`), the triggers the items in view (`;`/`'`).
fn examine_table(ctx: &Context) -> Table {
    use Button::*;
    let mut t = Table::default()
        .with(A, Action::Examine)
        .with(B, esc())
        .with(X, situational(k(".", "Travel here")))
        .with(Y, situational(k("?", "Help")))
        // `;` and `'`, crawl's other keys for these (cmd-keys.h): `/` and `*` are the Shift and
        // Ctrl prefixes in targeting too while `use_modifier_prefix_keys` is on (the default;
        // cio.cc `unmangle_direction_keys`), so they asked for a key with "SHIFT" or "CTRL"
        .with(LT, k(";", "Previous item"))
        .with(RT, k("'", "Next item"))
        .with(Select, palette(CommandCategory::Targeting));
    // the monsters in view, one way and the other; with none there is nothing to cycle
    if ctx.monsters_in_view > 0 {
        t.set(LB, k("-", "Previous monster"));
        t.set(RB, k("+", "Next monster"));
    }
    t
}

/// Leave the level map, then send a main-map command: crawl reads the keys in
/// order, and nothing flushes between. The map stays on screen meanwhile (map_hold.rs).
fn off_map(label: &str, key: SeqItem) -> Action {
    Action::Keys { seq: vec![SeqItem::key(Keys::ESC), key], label: label.into(), contextual: true, keeps_map: true, repeats: false }
}

/// The level map (X), and Select's: the pad's way across the dungeon. The server prints no
/// key help here, so the corner names all of it. The bumpers put the cursor on the next
/// stairs up or down, A travels there; X describes the cell, and the triggers zoom
/// (`{`/`}`, which go as a keycode and as text: `{` as text would open a JSON message).
/// Travel to a branch (`G`), the overview (Ctrl-O) and the stash search (Ctrl-F) are
/// main-map commands, so those buttons close the map first.
fn levelmap_table(ctx: &Context) -> Table {
    use Button::*;
    let mut t = Table::default()
        .with(B, esc())
        .with(LB, situational(k("<", "Up stairs")))
        .with(RB, situational(k(">", "Down stairs")))
        .with(X, situational(k("v", "Describe")))
        // held, the triggers keep zooming, a step (tileweb.cc ZOOM_INC) every repeat
        .with(LT, Action::Keys { seq: vec![SeqItem::key(123)], label: "Zoom out".into(), contextual: true, keeps_map: false, repeats: true })
        .with(RT, Action::Keys { seq: vec![SeqItem::text("}")], label: "Zoom in".into(), contextual: true, keeps_map: false, repeats: true })
        .with(L3, off_map("Find…", SeqItem::key(6)))
        .with(R3, off_map("Overview", SeqItem::key(15)))
        .with(Select, hold(kc(Keys::ESC, "Close"), palette(CommandCategory::Levelmap)));
    // the cursor away from the player: somewhere to travel to, and a way back to them; on them, Y travels further
    if !ctx.map_cursor_home {
        t.set(A, k(".", "Travel here"));
        t.set(Y, situational(k("@", "Find you")));
    } else {
        t.set(Y, off_map("Travel to…", SeqItem::text("G")));
    }
    t
}

/// The focus layer's set, shared by every overlay that is not a server menu:
/// the d-pad moves one cursor over what the screen offers, A takes it, B cancels.
fn focus_base() -> Table {
    Table::default().with(Button::A, Action::focus(FocusOp::Select)).with(Button::B, Action::focus(FocusOp::Cancel)).with(Button::Select, palette(CommandCategory::Command))
}

/// The focus set, with the bumpers where there is a page to turn (`Context.pageable`).
fn focus_table(ctx: &Context) -> Table {
    let t = focus_base();
    if ctx.pageable {
        t.with(Button::LB, Action::focus(FocusOp::PagePrev)).with(Button::RB, Action::focus(FocusOp::PageNext))
    } else {
        t
    }
}

/// A popup's first action, on X: a describe screen's pane switch, or the first verb of a
/// popup without panes. The label is the row's own words.
fn popup_action() -> Action {
    Action::ui_arg(UiOp::PopupAction, 0)
}

/// A yes/no's answers on the face buttons: Yes on A, No on B, and a prompt's Always on X.
fn yesno_button(hotkey: &str) -> Button {
    match hotkey.to_uppercase().as_str() {
        "Y" => Button::A,
        "N" => Button::B,
        "A" => Button::X,
        _ => Button::A,
    }
}

/// The button a prompt's answer sits on, which is also the glyph its chip
/// wears: a yes/no puts Yes on A and No on B. A choice prompt's answers are
/// not on the face buttons at all: the card is a focus set like every other.
pub fn prompt_buttons(prompt: &ParsedPrompt) -> Vec<(String, Button)> {
    if !prompt.yesno {
        return Vec::new();
    }
    let mut out: Vec<(String, Button)> = Vec::new();
    for o in &prompt.options {
        let b = yesno_button(&o.hotkey);
        match out.iter_mut().find(|(h, _)| *h == o.hotkey) {
            Some(slot) => slot.1 = b,
            None => out.push((o.hotkey.clone(), b)),
        }
    }
    out
}

/// The focus set, less B on a prompt Escape means nothing to (the stat-gain prompt), so the
/// bar never offers a Cancel the game would ignore.
fn prompt_table(prompt: &ParsedPrompt, ctx: &Context) -> Table {
    // a yes/no: its answers are its buttons (there is no cursor), whatever crawl's default
    if prompt.yesno {
        let mut t = Table::default().with(Button::Select, palette(CommandCategory::Command));
        for (hotkey, button) in prompt_buttons(prompt) {
            t.set(button, Action::prompt(&hotkey));
        }
        return t;
    }
    let mut t = focus_table(ctx);
    if prompt.cancel {
        return t;
    }
    // the stat gain: no way back, and no answer until the cursor lights one
    t.remove(Button::B);
    if ctx.focus.as_ref().and_then(|f| f.label.as_ref()).is_none() {
        t.remove(Button::A);
    }
    t
}

/// Character creation: B is a step back, where crawl's Escape would abandon the choices made
/// so far. The step is the screen's own (`Context.newgame_back`); on the first screen B is
/// crawl's Escape, out of character creation altogether.
fn newgame_table(ctx: &Context) -> Table {
    focus_table(ctx).with(Button::X, k("*", "Random")).with(Button::Y, k("+", "Recommended")).with(Button::B, kc(ctx.newgame_back.unwrap_or(27), "Back"))
}

/// Only the keys `readkey_more` (message.cc) takes: space, Enter and Escape; every other key
/// it reads and drops. A wears the prompt on the bar; B is crawl's Escape, which also skips the
/// rest of the turn's mores (`set_more_autoclear`).
fn more_table() -> Table {
    Table::default().with(Button::A, situational(continue_more())).with(Button::B, kc(Keys::ESC, "Skip"))
}

// B leaves, as it does everywhere else; X erases, RB puts a space
fn text_table() -> Table {
    use Button::*;
    Table::default()
        .with(A, Action::osk(OskOp::Type))
        .with(B, Action::osk(OskOp::Cancel))
        .with(X, Action::osk(OskOp::Backspace))
        .with(Y, Action::osk(OskOp::Submit))
        .with(LB, Action::osk(OskOp::Shift))
        .with(RB, Action::osk(OskOp::Space))
        .with(Start, Action::osk(OskOp::Submit))
        .with(Select, Action::osk(OskOp::Cancel))
}

/// The prompts whose keyboard Y does not submit: the skill target prompt opens off Y on a
/// skill row, so a second press, or one held a beat, would send an empty target straight back.
/// Start alone confirms there.
pub fn submit_on_start_only(text_tag: Option<&str>) -> bool {
    text_tag == Some("skill_target")
}

fn spectating_table() -> Table {
    Table::default().with(Button::B, system()).with(Button::Start, system()).with(Button::R3, Action::ui(UiOp::ResetView))
}

/// What a key on a screen stands for: a pad button, or a d-pad direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenKey {
    Button(Button),
    Dir(Dir8),
}

/// On a screen, the keyboard is a pad: the keys that stand for its buttons
/// become those buttons, and go down the same path (`resolve`, the runner),
/// so a key and the button it stands for can never do two different things.
///
/// - Enter is A, the confirm, and Escape is B, the way back.
/// - Space is A too, except over text to read, where it pages.
/// - The arrows are the d-pad on a screen with a cursor. On the map, in an aim
///   and on the level map they stay the direction keys they are.
/// - PageUp and PageDown are the bumpers on a cursor's list.
///
/// Two screens pair them otherwise, on purpose: a menu where rows are marked: Enter takes
/// what is marked, so Enter is Start there; a yes/no: Enter stays crawl's, its default answer.
///
/// None leaves the key to what it is on its own: typed text, a hotkey, raw.
pub fn screen_key(key: &str, ctx: &Context, scrolls_text: bool) -> Option<ScreenKey> {
    let mode = ctx.mode;
    let cursor = is_focus_mode(ctx) || mode == Mode::Menu;
    let screen = cursor || mode == Mode::More || mode == Mode::Targeting || mode == Mode::Levelmap;
    // a yes/no's Enter and Space are its default answer (prompt.cc `yesno`)
    if !screen || (mode == Mode::Yesno && (key == "Enter" || key == " ")) {
        return None;
    }
    let dir = |d| if cursor { Some(ScreenKey::Dir(d)) } else { None };
    match key {
        "Enter" => Some(ScreenKey::Button(if mode == Mode::Menu && ctx.menu.as_ref().is_some_and(|m| m.multiselect) { Button::Start } else { Button::A })),
        "Escape" => Some(ScreenKey::Button(Button::B)),
        " " => {
            if !cursor || scrolls_text {
                None
            } else {
                Some(ScreenKey::Button(Button::A))
            }
        }
        "ArrowUp" => dir(0),
        "ArrowRight" => dir(2),
        "ArrowDown" => dir(4),
        "ArrowLeft" => dir(6),
        // the bumpers page a cursor's list; a menu pages itself and text to read scrolls
        "PageUp" => (is_focus_mode(ctx) && !scrolls_text).then_some(ScreenKey::Button(Button::LB)),
        "PageDown" => (is_focus_mode(ctx) && !scrolls_text).then_some(ScreenKey::Button(Button::RB)),
        _ => None,
    }
}

/// What a button does now, as a single press: the tap of a tap-or-hold. None when it does nothing here.
pub fn button_action(button: Button, ctx: &Context) -> Option<Action> {
    match binding_table(ctx).get(button)? {
        Action::Hold { tap, .. } => Some((**tap).clone()),
        a => Some(a.clone()),
    }
}

/// Table for the current context: what each face button does now.
pub fn binding_table(ctx: &Context) -> Table {
    if is_focus_mode(ctx) {
        return match ctx.mode {
            Mode::Newgame => newgame_table(ctx),
            Mode::Yesno => prompt_table(ctx.prompt.as_ref().unwrap_or(&default_yesno()), ctx),
            // a prompt the parser made nothing of: the official client's own hints stay reachable
            Mode::Prompt => match &ctx.prompt {
                Some(p) => prompt_table(p, ctx),
                None => focus_table(ctx).with(Button::X, k("*", "List")).with(Button::Y, k("?", "Help")),
            },
            Mode::Dialog => Table::default().with(Button::A, Action::focus(FocusOp::Select)).with(Button::B, Action::focus(FocusOp::Cancel)),
            Mode::Popup => {
                if ctx.popup_actions.as_ref().is_some_and(|p| !p.is_empty()) {
                    focus_table(ctx).with(Button::X, popup_action())
                } else {
                    focus_table(ctx)
                }
            }
            _ => {
                // the focused item's second action (a skill row's "Set target") sits on Y while the cursor rests on one
                let mut t = focus_table(ctx);
                if ctx.focus.as_ref().and_then(|f| f.alt_label.as_ref()).is_some() {
                    t.set(Button::Y, Action::focus(FocusOp::AltSelect));
                }
                t
            }
        };
    }
    match ctx.mode {
        Mode::Command => {
            // crawl stops a walk at any key and then reads that key as the next command: an `o` would explore again
            if ctx.exploring {
                return command_table().with(Button::LT, stop_explore());
            }
            if ctx.explore_on_hold {
                command_table().with(Button::LT, hold(Action::ui(UiOp::InstantExplore), Action::ui(UiOp::ExploreWhileHeld)))
            } else {
                command_table()
            }
        }
        // macro capture: the server reads raw keys; only the system menu is ours
        Mode::Macro => Table::default().with(Button::B, esc()).with(Button::Start, enter()),
        Mode::More => more_table(),
        Mode::Menu => match ctx.menu.as_ref().and_then(|m| m.shop.as_ref()) {
            Some(shop) => shop_table(shop, ctx.pageable),
            None => menu_table(ctx.menu.as_ref(), ctx),
        },
        Mode::Targeting => {
            if ctx.examining {
                examine_table(ctx)
            } else {
                targeting_table(ctx)
            }
        }
        Mode::Levelmap => levelmap_table(ctx),
        Mode::Text => {
            let mut t = text_table();
            if submit_on_start_only(ctx.text_tag.as_deref()) {
                t.remove(Button::Y);
            }
            t
        }
        Mode::Spectating => spectating_table(),
        Mode::Lobby => Table::default(),
        _ => focus_base(),
    }
}

/// The prompt up now, a yes/no's two answers when nothing was parsed.
fn prompt_of(ctx: &Context) -> Option<ParsedPrompt> {
    ctx.prompt.clone().or_else(|| (ctx.mode == Mode::Yesno).then(default_yesno))
}

/// What A does on a focus screen with nothing to focus: close a popup, confirm anything else.
pub struct FocusFallback {
    pub closes: bool,
    pub label: &'static str,
    pub cancel_label: &'static str,
}

pub fn focus_fallback(ctx: &Context) -> FocusFallback {
    let closes = ctx.mode == Mode::Popup || ctx.mode == Mode::Dialog;
    FocusFallback { closes, label: if closes { "Close" } else { "Confirm" }, cancel_label: if closes { "Close" } else { "Cancel" } }
}

/// One binding as the action bar shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingLabel {
    /// A button or a directional stick hint (not a stick click).
    pub button: Button,
    /// An unlearned control, not a contextual action; inert to pointer clicks.
    pub teaching: bool,
    pub label: String,
    pub action: Action,
    /// what a long press does, when the binding has one and it does something here
    pub hold: Option<String>,
    /// The label came from the situation (a monster ahead, stairs underfoot, a
    /// prompt's own options) rather than from the table.
    pub contextual: bool,
}

/// The bar's display order: face buttons, the d-pad (clockwise from up), then shoulders, sticks, and the system pair.
pub const BAR_ORDER: [Button; 16] = [Button::A, Button::B, Button::X, Button::Y, Button::DU, Button::DR, Button::DD, Button::DL, Button::LB, Button::RB, Button::LT, Button::RT, Button::L3, Button::R3, Button::Select, Button::Start];

/// Every binding of the context as a label, in display order.
pub fn bar_labels(ctx: &Context) -> Vec<BindingLabel> {
    let t = binding_table(ctx);
    let mut out = Vec::new();
    for b in BAR_ORDER {
        let Some(a) = t.get(b) else { continue };
        if let Action::Hold { tap, hold } = a {
            let h = action_label(hold, ctx);
            out.push(BindingLabel { button: b, teaching: false, label: action_label(tap, ctx), action: (**tap).clone(), hold: (h != NO_ACTION).then_some(h), contextual: is_contextual(a, ctx) });
        } else {
            out.push(BindingLabel { button: b, teaching: false, label: action_label(a, ctx), action: a.clone(), hold: None, contextual: is_contextual(a, ctx) });
        }
    }
    out
}

/// Contextual prompts plus menu confirmation, the readied action and bump attacks. B stays implicit.
pub fn prompt_labels(ctx: &Context) -> Vec<BindingLabel> {
    let mut labels: Vec<BindingLabel> = bar_labels(ctx).into_iter().filter(|l| l.contextual && l.button != Button::B).collect();
    if ctx.mode == Mode::Command {
        if let Target::Monster { hostile: true, label, .. } = &ctx.ahead {
            labels.push(BindingLabel { button: Button::LStickUp, teaching: false, label: format!("Attack {label}"), action: Action::step(0), hold: None, contextual: true });
        }
    }
    labels
}

/// Whether the binding's meaning here comes from the situation rather than the table.
fn is_contextual(a: &Action, ctx: &Context) -> bool {
    match a {
        Action::Contextual { alt } => contextual_label(ctx, alt.unwrap_or(false)) != NO_ACTION,
        // Autofight means the same thing every turn, so it is a lesson, not a standing prompt
        Action::Fight => false,
        // In look mode the cursor rests on something the server named.
        Action::Examine => ctx.mode == Mode::Targeting && ctx.examining,
        // the server named what is quivered; while aiming, the cursor rests on the target.
        // Standing safe, the quivered shot shows when there is something to shoot at
        Action::Fire => {
            if ctx.mode == Mode::Targeting {
                !ctx.examining
            } else {
                ctx.readied_action.is_some() && threatened(ctx)
            }
        }
        Action::Hold { tap, hold } => is_contextual(tap, ctx) || is_contextual(hold, ctx),
        Action::Keys { contextual, .. } => {
            // wait/rest is the situation's own while hurt with nothing in view: resting is what the turn is for
            if is_rest(a) && rest_worthwhile(ctx) {
                return true;
            }
            // the quiver cycle names the shot it would swap to, so it stands in the corner while there is one to swap
            if is_quiver_cycle(a) {
                return ctx.mode == Mode::Targeting && !ctx.examining && ctx.readied_action.is_some();
            }
            *contextual
        }
        Action::Prompt { hotkey } => prompt_of(ctx).is_some_and(|p| p.options.iter().any(|o| o.hotkey.to_lowercase() == hotkey.to_lowercase())),
        // the cursor rests on something with a name of its own (a spell, Yes); the fallbacks are the table's
        Action::Focus { op } => {
            let f = ctx.focus.as_ref();
            match op {
                FocusOp::Select => f.and_then(|f| f.label.as_ref()).is_some(),
                FocusOp::Cancel => f.and_then(|f| f.cancel_label.as_ref()).is_some(),
                FocusOp::AltSelect => f.and_then(|f| f.alt_label.as_ref()).is_some(),
                _ => false,
            }
        }
        Action::Menu { op } => {
            // the shop's letters mark and unmark; the label follows what the server printed
            if ctx.menu.as_ref().is_some_and(|m| m.shop.is_some()) {
                return *op == MenuOp::Select || *op == MenuOp::AltSelect;
            }
            // the cursor rests on a row that has a description behind it
            *op == MenuOp::Examine && ctx.menu.as_ref().is_some_and(menu_row_examines)
        }
        Action::Ui { op: UiOp::PopupAction, arg, .. } => ctx.popup_actions.as_ref().is_some_and(|p| p.get(arg.unwrap_or(0).max(0) as usize).is_some()),
        _ => false,
    }
}

/// The hovered row is one a description stands behind: X examines it.
fn menu_row_examines(m: &MenuContext) -> bool {
    let menu = &m.menu;
    if !examines(&menu.tag) {
        return false;
    }
    let i = menu.last_hovered;
    i >= 0 && m.hoverable.contains(&(i as usize)) && menu.items.get(i as usize).and_then(|it| it.as_ref()).and_then(|it| it.hotkeys.as_ref()).is_some_and(|h| !h.is_empty())
}

fn single_text(a: &Action) -> Option<&str> {
    match a {
        Action::Keys { seq, .. } if seq.len() == 1 => match &seq[0].stroke {
            Stroke::Text(t) => Some(t),
            _ => None,
        },
        _ => None,
    }
}

/// The wait (`.`) or rest (`5`) key on its own, as the LB tap-or-hold sends them.
fn is_rest(a: &Action) -> bool {
    matches!(single_text(a), Some(".") | Some("5"))
}

/// The quiver cycle (`)` / `(`) as the aim's Y sends it, either way round.
fn is_quiver_cycle(a: &Action) -> bool {
    matches!(single_text(a), Some(")") | Some("("))
}

/// Something worth attacking: a hostile ahead, or one in view for autofight to pick.
pub fn threatened(ctx: &Context) -> bool {
    ctx.ahead.hostile_monster() || ctx.hostiles_in_view > 0
}

/// Hurt, in command mode, with no hostile in view: the wait/rest prompt shows unasked.
fn rest_worthwhile(ctx: &Context) -> bool {
    ctx.mode == Mode::Command && ctx.injured && ctx.hostiles_in_view == 0
}

/// Which cluster of the controller a button belongs to, so the bar can group its prompts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonGroup {
    Face,
    Dpad,
    Shoulder,
    Stick,
    System,
}

pub fn button_group(b: Button) -> ButtonGroup {
    use Button::*;
    match b {
        A | B | X | Y => ButtonGroup::Face,
        DPad | DU | DR | DD | DL => ButtonGroup::Dpad,
        LB | RB | LT | RT => ButtonGroup::Shoulder,
        L3 | R3 | LStickUp | LStick | RStick => ButtonGroup::Stick,
        _ => ButtonGroup::System,
    }
}

/// One row of the controls sheet: the button, its tap and, when there is one, its hold.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetRow {
    pub button: Button,
    pub tap: String,
    pub hold: Option<String>,
}

/// The command-mode bindings as a sheet, generated from the tables so the
/// overlay can never drift from what the buttons do. Context-dependent
/// actions get their generic name.
pub fn control_sheet() -> Vec<SheetRow> {
    let t = command_table();
    BAR_ORDER
        .iter()
        .filter_map(|&button| {
            let a = t.get(button)?;
            Some(match a {
                Action::Hold { tap, hold } => SheetRow { button, tap: static_label(tap), hold: Some(static_label(hold)) },
                a => SheetRow { button, tap: static_label(a), hold: None },
            })
        })
        .collect()
}

/// A label that needs no context: what the sheet and settings show for an action.
pub fn static_label(a: &Action) -> String {
    match a {
        Action::Contextual { .. } => "Interact".into(),
        Action::Fight => "Autofight".into(),
        Action::Examine => "Examine".into(),
        Action::Fire => "Fire".into(),
        Action::Hold { tap, .. } => static_label(tap),
        Action::Keys { label, .. } => label.clone(),
        Action::Ui { op, .. } => match op {
            UiOp::Commands => "Actions",
            UiOp::ActionTab => "Actions tab",
            UiOp::Equipment => "Equipment",
            UiOp::Palette => "Commands",
            UiOp::System => "Isorun menu",
            UiOp::Keyboard => "Keyboard",
            UiOp::ResetView => "Reset view",
            UiOp::Levelmap => "Level map",
            UiOp::Bindings => "Gamepad",
            UiOp::ScrollLog => "Log",
            UiOp::PopupAction => "Action",
            UiOp::Dock => "Hotbar",
            UiOp::InstantExplore => "Explore at once",
            UiOp::ExploreWhileHeld => "Explore while held",
            UiOp::Interact => "Interact",
        }
        .into(),
        _ => action_label(a, &Context::default()),
    }
}

/// What the bar says an action does here.
pub fn action_label(a: &Action, ctx: &Context) -> String {
    match a {
        Action::Keys { label, .. } => {
            // on a --more-- the chip under A reads as the pane's more row does: its text, in the pane's white
            if ctx.mode == Mode::More && label == "--more--" {
                if let Some(t) = &ctx.more_text {
                    return coloured_text(t, LOG_DEFAULT_COLOUR, true);
                }
            }
            label.clone()
        }
        Action::Contextual { alt } => {
            if alt.is_none() && has_interaction_choice(ctx) {
                "Interact".into()
            } else {
                contextual_label(ctx, alt.unwrap_or(false))
            }
        }
        Action::Fight => "Autofight".into(),
        Action::Examine => {
            if ctx.mode == Mode::Targeting && ctx.examining {
                format!("Examine {}", ctx.cursor.as_ref().map_or("here", |c| c.label()))
            } else {
                "Examine".into()
            }
        }
        Action::Fire => {
            if ctx.mode == Mode::Targeting && !ctx.examining {
                return match &ctx.cursor {
                    Some(c) if !c.is_none() => format!("Fire at {}", c.label()),
                    _ => "Fire".into(),
                };
            }
            ctx.readied_action.clone().unwrap_or_else(|| "Fire".into())
        }
        Action::Hold { tap, .. } => action_label(tap, ctx),
        Action::Menu { op } => {
            let shop = ctx.menu.as_ref().and_then(|m| m.shop.as_ref());
            // the letter switches, in the shop's own words; they read the same marked or not, as the footer does
            if let Some(shop) = shop {
                if *op == MenuOp::Select {
                    return if shop.mode == ShopMode::Buy { "mark item for purchase" } else { "examine item" }.into();
                }
                if *op == MenuOp::AltSelect {
                    return "put item on shopping list".into();
                }
            }
            // the bumpers in the pack name the page they turn to
            if let Some(pack) = ctx.menu.as_ref().and_then(|m| m.pack.as_ref()).filter(|p| p.next.is_some()) {
                if *op == MenuOp::Left || *op == MenuOp::Right {
                    return if *op == MenuOp::Left { pack.prev.clone() } else { pack.next.clone() }.unwrap_or_default();
                }
            }
            if *op == MenuOp::SectionNext || *op == MenuOp::SectionPrev {
                // the bumpers read as the jump they make: sections where the menu has headers, pages otherwise
                let sections = ctx.menu.as_ref().is_some_and(|m| menu_has_sections(&m.menu));
                return match (*op == MenuOp::SectionNext, sections) {
                    (true, true) => "Next section",
                    (true, false) => "Page down",
                    (false, true) => "Previous section",
                    (false, false) => "Page up",
                }
                .into();
            }
            // nothing under the cursor on a single-select arrows menu: A leaves the menu, and says so
            if *op == MenuOp::Select && ctx.menu.as_ref().is_some_and(|m| m.arrows_select && !m.multiselect && m.menu.last_hovered < 0) {
                return "exit".into();
            }
            // select, page down, page up, exit and toggle selected are the keyhelp's own words
            match op {
                MenuOp::Next => "Next",
                MenuOp::Prev => "Previous",
                MenuOp::PageNext => "page down",
                MenuOp::PagePrev => "page up",
                MenuOp::First => "First",
                MenuOp::Last => "Last",
                MenuOp::Select => "select",
                MenuOp::AltSelect => "List",
                MenuOp::Examine => "Examine",
                MenuOp::Toggle => "toggle selected",
                MenuOp::Cancel => "exit",
                MenuOp::Left => "Left",
                MenuOp::Right => "Right",
                MenuOp::SectionNext | MenuOp::SectionPrev => unreachable!(),
            }
            .into()
        }
        Action::Prompt { hotkey } => {
            if let Some(o) = prompt_of(ctx).and_then(|p| p.options.into_iter().find(|o| o.hotkey.to_lowercase() == hotkey.to_lowercase())) {
                // an answer read off the log wears the log's colour for it
                return match o.colour {
                    Some(c) => coloured_text(&o.label, c, false),
                    None => o.label,
                };
            }
            match hotkey.as_str() {
                "Y" | "y" => "Yes".into(),
                "N" | "n" => "No".into(),
                h => h.into(),
            }
        }
        Action::Focus { op } => {
            let f = ctx.focus.as_ref();
            let fb = focus_fallback(ctx);
            match op {
                FocusOp::Select => {
                    // a prompt's answer under the cursor keeps the colour the log gave it
                    match f.and_then(|f| f.label.as_ref().map(|l| (l, f.colour))) {
                        Some((l, Some(c))) => coloured_text(l, c, false),
                        Some((l, None)) => l.clone(),
                        None => fb.label.into(),
                    }
                }
                FocusOp::Cancel => f.and_then(|f| f.cancel_label.clone()).unwrap_or_else(|| fb.cancel_label.into()),
                FocusOp::AltSelect => f.and_then(|f| f.alt_label.clone()).unwrap_or_default(),
                FocusOp::Next => "Next".into(),
                FocusOp::Prev => "Previous".into(),
                FocusOp::Left => "Left".into(),
                FocusOp::Right => "Right".into(),
                FocusOp::PageNext => "Page down".into(),
                FocusOp::PagePrev => "Page up".into(),
                FocusOp::First => "First".into(),
                FocusOp::Last => "Last".into(),
            }
        }
        Action::Ui { op, arg, .. } => match op {
            UiOp::PopupAction => ctx.popup_actions.as_ref().and_then(|p| p.get(arg.unwrap_or(0).max(0) as usize)).map(|p| p.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| "Action".into()),
            // the bumpers on X's actions name the tab they turn to
            UiOp::ActionTab => {
                let actions = ctx.menu.as_ref().and_then(|m| m.actions.as_ref());
                actions.map(|a| if arg.unwrap_or(1) < 0 { a.prev.clone() } else { a.next.clone() }).unwrap_or_else(|| "Actions tab".into())
            }
            // X and Y name the first tab of what they open
            UiOp::Commands => "Spells".into(),
            UiOp::Equipment => "Gear".into(),
            UiOp::Palette => "Commands".into(),
            UiOp::System => "Character".into(),
            UiOp::Keyboard => "Keyboard".into(),
            UiOp::ResetView => "Reset view".into(),
            UiOp::Levelmap => "Level map".into(),
            UiOp::Bindings => "Gamepad".into(),
            UiOp::ScrollLog => "Log".into(),
            UiOp::Dock => "Hotbar".into(),
            UiOp::InstantExplore => "Explore at once".into(),
            UiOp::ExploreWhileHeld => "Explore while held".into(),
            UiOp::Interact => "Interact".into(),
        },
        Action::Osk { op, .. } => match op {
            OskOp::Move => "Move",
            OskOp::Type => "Type",
            OskOp::Backspace => "Backspace",
            OskOp::Space => "Space",
            OskOp::Submit => "Done",
            OskOp::Cancel => "Cancel",
            OskOp::Shift => "Shift",
        }
        .into(),
        Action::Step { .. } => "Step".into(),
        Action::Turn { .. } => "Turn".into(),
        Action::Look { .. } => "Look".into(),
        Action::Cursor { .. } => "Cursor".into(),
    }
}

/// Whether a menu has sections at all: a header (title or subtitle row) somewhere in its rows (menu-nav.ts).
pub fn menu_has_sections(menu: &isorun_webtiles::MenuState) -> bool {
    menu.items.iter().any(|it| it.as_ref().and_then(|it| it.level).unwrap_or(2) < 2)
}

/// Whether A must ask which of two underfoot interactions the player intends.
pub fn has_interaction_choice(ctx: &Context) -> bool {
    match &ctx.under {
        Target::Feature { feature, items: Some(_), .. } => matches!(feature, Feature::Stairs { .. } | Feature::Hatch { .. } | Feature::Portal | Feature::Shop | Feature::Transporter | Feature::Altar { .. }),
        _ => false,
    }
}

/// A contextual A with nothing to act on: the tap does nothing.
pub const NO_ACTION: &str = "Nothing";

/// A label the map names in lower case ("weapon shop"), as a prompt starts it.
fn sentence_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The A button: what is underfoot first, then the door ahead. Only actions
/// with a target: never a bare step (the stick) or an attack (RT). Items ahead
/// are not offered: crawl picks up from the cell you stand on.
///
/// `alt` names the pickup option when items share a feature's tile.
///
/// A pile underfoot is named, not verbed: the prompt is the server's own line
/// for it, colour tags kept. Every other label is the client's own word.
pub fn contextual_label(ctx: &Context, alt: bool) -> String {
    let u = &ctx.under;
    // underfoot the server named the pile ("You see here a +0 halberd."), so the prompt is that name
    if alt {
        return match u {
            Target::Feature { items: Some(items), .. } => items.clone(),
            _ => NO_ACTION.into(),
        };
    }
    if let Target::Feature { feature, label, items } = u {
        match feature {
            // the D:1 start square is the way out: `<` there asks to leave the game (main.cc _prompt_stairs)
            Feature::Stairs { exit: true, .. } => return "Leave dungeon".into(),
            Feature::Stairs { dir, .. } | Feature::Hatch { dir } => return if *dir == UpDown::Down { "Descend" } else { "Ascend" }.into(),
            Feature::Portal | Feature::Transporter => return "Enter".into(),
            // the shop as the tile names it ("weapon shop")
            Feature::Shop => return sentence_case(label),
            Feature::Altar { .. } => return "Altar".into(),
            _ => {
                if let Some(items) = items {
                    return items.clone();
                }
            }
        }
    }
    if let Target::Item { label } = u {
        return label.clone();
    }
    match &ctx.ahead {
        Target::DoorClosed { .. } => "Open door".into(),
        Target::Feature { feature: Feature::Door { state: DoorState::Open }, .. } => "Close door".into(),
        // items ahead get no prompt: crawl picks up from the cell you stand on
        _ => NO_ACTION.into(),
    }
}

/// Whether a press of `button` now opens a tap-or-hold decision. The decision
/// is taken on release (or after HOLD_MS), by which time the server may have
/// changed mode. Only a press that began as a tap-or-hold may fire a tap or a
/// hold, so a button that already acted on press does nothing more (game.ts `pad`).
pub fn arms_tap_or_hold(button: Button, ctx: &Context) -> bool {
    matches!(binding_table(ctx).get(button), Some(Action::Hold { .. }))
}

/// The hold half of a tap-or-hold binding, for the frame loop to fire once HOLD_MS has passed.
pub fn hold_action(button: Button, ctx: &Context) -> Option<Action> {
    match binding_table(ctx).get(button) {
        Some(Action::Hold { hold, .. }) => Some((**hold).clone()),
        _ => None,
    }
}

/// Whether a held button sends this action again and again, as a held key does
/// on a keyboard: autofight, and the level map's zoom. Every repeat is resolved
/// against the context of the moment, so whatever the last swing opened binds the
/// button to something else and the run stops there rather than answering it.
fn repeats_held(a: &Action) -> bool {
    matches!(a, Action::Fight | Action::Keys { repeats: true, .. })
}

/// Resolve a pad event into an action, or None.
pub fn resolve(ev: &PadEvent, ctx: &Context) -> Option<Action> {
    let t = binding_table(ctx);
    match ev {
        PadEvent::Press { button, .. } => {
            // a tap-or-hold binding is decided on release
            match t.get(*button)? {
                Action::Hold { .. } => None,
                a => Some(a.clone()),
            }
        }
        // all but a handful of buttons fire once a press
        PadEvent::Repeat { button, .. } => t.get(*button).filter(|a| repeats_held(a)).cloned(),
        PadEvent::Release { button, held, .. } => {
            // the hold half fires from the frame loop as soon as HOLD_MS passes; a release before that is the tap
            match t.get(*button) {
                Some(Action::Hold { tap, .. }) if *held < HOLD_MS => Some((**tap).clone()),
                _ => None,
            }
        }
        PadEvent::Dir { source, dir } => resolve_dir(*source, (*dir)?, false, ctx),
        PadEvent::DirRepeat { source, dir, .. } => resolve_dir(*source, *dir, true, ctx),
        PadEvent::Look { dx, dy, .. } => Some(Action::Look { dx: *dx, dy: *dy }),
    }
}

fn resolve_dir(source: DirSource, dir: Dir8, repeat: bool, ctx: &Context) -> Option<Action> {
    if is_focus_mode(ctx) {
        return match dir {
            0 => Some(Action::focus(FocusOp::Prev)),
            4 => Some(Action::focus(FocusOp::Next)),
            6 => Some(Action::focus(FocusOp::Left)),
            2 => Some(Action::focus(FocusOp::Right)),
            _ => None,
        };
    }
    match ctx.mode {
        Mode::Command => {
            // the left stick is a d-pad: every direction steps, as it is on the screen.
            // held: one more single step per tick, paced by the runner; never a run.
            // the right stick's directions are only ever its turn (gamepad.rs `right_stick_turns`)
            let turns = source == DirSource::RStick;
            Some(Action::Step { dir, run: false, attack: false, turns, held: repeat })
        }
        Mode::Menu => match dir {
            0 => Some(Action::menu(MenuOp::Prev)),
            4 => Some(Action::menu(MenuOp::Next)),
            // X's actions: left and right turn the tabs, as they turn the pack's pages
            6 | 2 if ctx.menu.as_ref().is_some_and(|m| m.actions.is_some()) => Some(Action::ui_arg(UiOp::ActionTab, if dir == 6 { -1 } else { 1 })),
            6 => Some(Action::menu(MenuOp::Left)),
            2 => Some(Action::menu(MenuOp::Right)),
            _ => None,
        },
        Mode::Targeting | Mode::Levelmap => Some(Action::Cursor { dir }),
        Mode::Text => Some(Action::Osk { op: OskOp::Move, dir: Some(dir) }),
        Mode::Spectating => {
            if repeat {
                None
            } else {
                Some(Action::ui_arg(UiOp::ScrollLog, if dir == 0 { -1 } else if dir == 4 { 1 } else { 0 }))
            }
        }
        _ => None,
    }
}

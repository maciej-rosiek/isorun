//! Keyboard navigation of a server menu (menu-nav.ts), ported from the official client's menu.js
//! (`menu_keydown_handler`, `menu_keypress_handler`, `next_hoverable_item`, `paging`, `line_up`,
//! `line_down`, `snap_hover_in_page`, `get_relative_hover`, `set_relative_hover`). The official
//! client never sends arrows, PgUp / PgDn, Home or End to the server while a menu is up: it moves
//! the hover and the scroll itself and tells the server with `menu_hover` and `menu_scroll`. (The
//! server would move its own hover on a raw arrow and reply with a non-forced `menu_scroll`,
//! which both clients ignore, so a raw arrow moves nothing on screen.)
//!
//! Everything here is pure: the caller measures what is visible and applies the scrolls; the
//! decisions are made from the menu's items, flags and tag.

use isorun_webtiles::{MenuFlag, MenuState};
use once_cell::sync::Lazy;
use regex::Regex;

/// The rows the caller can see in full: `menu.first_visible` / `menu.last_visible`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    pub first: i64,
    pub last: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuNavKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    LineUp,
    LineDown,
}

/// A key as the navigation sees it: the named keys a keydown carries, or the character it types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyName {
    PageUp,
    PageDown,
    Home,
    End,
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    NumpadAdd,
    NumpadSubtract,
    Char(char),
    Other,
}

/// The subset of a keydown the intent needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavKey {
    pub key: KeyName,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl NavKey {
    pub fn plain(key: KeyName) -> Self {
        NavKey { key, shift: false, ctrl: false, alt: false }
    }
}

fn level(menu: &MenuState, i: i64) -> i64 {
    if i < 0 {
        return 2;
    }
    menu.items.get(i as usize).and_then(|it| it.as_ref()).and_then(|it| it.level).unwrap_or(2)
}

fn is_header(menu: &MenuState, i: i64) -> bool {
    level(menu, i) < 2
}

/// menu.js `item_selectable`: a level-2 row with a hotkey, or any row of the use_item menu (relettering).
pub fn item_selectable(menu: &MenuState, index: i64) -> bool {
    let Some(Some(it)) = (index >= 0).then(|| menu.items.get(index as usize)).flatten() else { return false };
    it.level.unwrap_or(2) == 2 && (menu.tag == "use_item" || it.hotkeys.as_ref().is_some_and(|h| !h.is_empty()))
}

/// menu.js `next_hoverable_item`: the next selectable row from `start` in the given direction,
/// wrapping only with the WRAP flag; -1 when there is none ("up arrow on no hover does nothing").
pub fn next_hoverable_item(menu: &MenuState, reverse: bool, start: i64, start_at_start: bool) -> i64 {
    let n = menu.items.len() as i64;
    let wrap = menu.flags & MenuFlag::WRAP != 0;
    let mut max = if wrap {
        n
    } else if reverse {
        menu.last_hovered
    } else {
        n - start.max(0)
    };
    if start_at_start && n > 0 {
        max = max.max(1);
    }
    if max <= 0 {
        return -1;
    }
    let mut hover = start;
    if reverse && hover < 0 {
        hover = 0;
    }
    if !start_at_start {
        hover += if reverse { -1 } else { 1 };
    }
    let mut tried = 0;
    while tried < max {
        tried += 1;
        if wrap {
            hover = hover.rem_euclid(n);
        }
        hover = hover.clamp(0, n - 1);
        if item_selectable(menu, hover) {
            return hover;
        }
        hover += if reverse { -1 } else { 1 };
    }
    -1
}

/// menu.cc `get_header_block`: the run of headers (level below 2) leading into `index`, and the row it heads.
fn header_block(menu: &MenuState, index: i64) -> (i64, i64) {
    let mut first = index;
    let mut last = index;
    while first >= 1 && is_header(menu, first - 1) {
        first -= 1;
    }
    while last + 1 < menu.items.len() as i64 && is_header(menu, last) {
        last += 1;
    }
    (first, last)
}

/// menu.cc `next_block_from` with `wrap`: the first row of the block after (or before) the one holding `index`.
fn next_block_from(menu: &MenuState, index: i64, forward: bool) -> i64 {
    let n = menu.items.len() as i64;
    let cur = header_block(menu, index);
    let next = if forward { cur.1 + 1 } else { cur.0 - 1 };
    header_block(menu, next.rem_euclid(n)).0
}

/// Whether a menu has sections at all: a header (title or subtitle row) somewhere in its rows.
pub fn menu_has_sections(menu: &MenuState) -> bool {
    (0..menu.items.len() as i64).any(|i| is_header(menu, i))
}

/// menu.cc `Menu::cycle_headers` (bound to `,`, forward only there; the pad's bumpers take both
/// directions): from the block of the hover (or of the first visible row without one), the header
/// of the next block in the given direction, wrapping round the menu; -1 when the menu has no
/// other header. The server has no client-side port of this, so Isorun runs it on its own hover
/// and reports it with `menu_hover`.
pub fn cycle_headers_target(menu: &MenuState, forward: bool, vis: Visible) -> i64 {
    if menu.items.is_empty() {
        return -1;
    }
    let arrows = menu.flags & MenuFlag::ARROWS_SELECT != 0;
    let start = header_block(menu, if arrows { menu.last_hovered.max(0) } else { vis.first }).0;
    let mut cur = next_block_from(menu, start, forward);
    while cur != start {
        if is_header(menu, cur) {
            return cur;
        }
        cur = next_block_from(menu, cur, forward);
    }
    -1
}

/// menu.js `get_relative_hover`: the hover's place in the page, headers skipped; negative counts
/// from the end, 0 with no hover.
pub fn relative_hover(menu: &MenuState, vis: Visible) -> i64 {
    if menu.last_hovered < 0 || menu.items.is_empty() {
        return 0;
    }
    if vis.last == menu.last_hovered {
        return -1;
    }
    let rel = menu.last_hovered - vis.first;
    let headers = (0..=rel).filter(|i| is_header(menu, vis.first + i)).count() as i64;
    (rel - headers).max(0)
}

/// menu.js `set_relative_hover`: the row to hover for a place in the (new) page, or -1.
pub fn relative_hover_target(menu: &MenuState, vis: Visible, rel: i64) -> i64 {
    let mut target;
    if rel >= 0 {
        // headers in the page do not count toward the relative position
        let mut rel = rel;
        let mut i = 0;
        while i <= rel && vis.first + i < menu.items.len() as i64 {
            if is_header(menu, vis.first + i) {
                rel += 1;
            }
            i += 1;
        }
        target = vis.first + rel;
    } else {
        target = vis.last + rel + 1;
    }
    if target > vis.last {
        target = vis.last;
    } else if target < vis.first {
        target = vis.first;
    }
    next_hoverable_item(menu, false, target, true)
}

/// menu.js `line_down`: the row to put at the top; a header and the row under it count as one.
pub fn line_down_target(menu: &MenuState, vis: Visible) -> i64 {
    let mut next = vis.first + 1;
    if is_header(menu, vis.first) {
        next += 1;
    }
    next.min(menu.items.len() as i64 - 1)
}

/// menu.js `line_up`
pub fn line_up_target(vis: Visible) -> i64 {
    (vis.first - 1).max(0)
}

/// menu.js `snap_hover_in_page`: where the hover must move to stay on the page, or -1 to leave it.
pub fn snap_hover_target(last_hovered: i64, vis: Visible) -> i64 {
    if last_hovered < 0 {
        -1
    } else if last_hovered < vis.first {
        vis.first
    } else if last_hovered > vis.last {
        vis.last
    } else {
        -1
    }
}

/// menu.js `raw_arrow_keys`: the macro editor takes arrows raw while it is reading a key (the
/// `.raw_input_mode` field is up) or has no rows yet (a new binding is being typed).
pub fn raw_arrow_keys(menu: &MenuState) -> bool {
    menu.tag == "macro_mapping" && (menu.title_prompt.as_ref().is_some_and(|t| t.raw) || menu.items.is_empty())
}

/// menu.js `menu_has_custom_dash`: menus where `-` means something of its own, so it is not "page up".
pub fn menu_has_custom_dash(menu: &MenuState) -> bool {
    matches!(menu.tag.as_str(), "inventory" | "stash" | "actions" | "macros" | "macro_mapping" | "use_item")
}

/// A switch the more line prints: `[<w>!</w>] buy|examine items`, in the plain text of the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoreSwitch {
    /// what the brackets held: a character, or the name of a key (`Esc`, `Enter`)
    pub key: String,
    pub label: String,
    /// offsets (in characters) of the whole switch (bracket to end of label) in the text handed in
    pub start: usize,
    pub end: usize,
}

/// Bracketed keys of a more line. `[XXX]` (the scroll position menu.js rewrites) holds no key.
static MORE_SWITCH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\[\]\n]{1,5})\]").unwrap());

/// Character offset of a byte offset.
pub(crate) fn char_at(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// The switches of a menu's more line, read from its plain text. The label is what follows the
/// brackets up to the next switch, a run of two spaces (the more line is padded into columns by
/// `pad_more_with`) or the end of the line; a switch with no label of its own keeps its key as one.
pub fn parse_more_switches(text: &str) -> Vec<MoreSwitch> {
    let chars: Vec<char> = text.chars().collect();
    let hits: Vec<(String, usize, usize)> = MORE_SWITCH_RE
        .captures_iter(text)
        .map(|c| (c[1].to_string(), char_at(text, c.get(0).unwrap().start()), char_at(text, c.get(0).unwrap().end())))
        .collect();
    let mut out = Vec::new();
    for (i, (key, start, end)) in hits.iter().enumerate() {
        if more_switch_keycode(key) == 0 && key.chars().count() != 1 {
            continue;
        }
        let next = hits.get(i + 1).map_or(chars.len(), |h| h.1);
        let line_end = chars[*end..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| end + p);
        let limit = next.min(line_end);
        let rest: String = chars[*end..limit.max(*end)].iter().collect();
        let stop = rest.find("  ").map(|b| char_at(&rest, b));
        let label_part: String = match stop {
            Some(s) => rest.chars().take(s).collect(),
            None => rest.clone(),
        };
        let label = label_part.trim();
        let span = match stop {
            Some(s) => s,
            None => rest.trim_end().chars().count(),
        };
        out.push(MoreSwitch { key: key.clone(), label: if label.is_empty() { key.clone() } else { label.to_string() }, start: *start, end: end + span });
    }
    out
}

/// The keycode a named switch sends (`[Esc]`, `[Enter]`), or 0 when the switch is a plain character.
pub fn more_switch_keycode(key: &str) -> i32 {
    match key {
        "Esc" => 27,
        "Enter" => 13,
        "Space" => 32,
        "Tab" => 9,
        _ => 0,
    }
}

/// Every menu is walked with a cursor of Isorun's own, whether or not the server set
/// ARROWS_SELECT. Without that flag the official client leaves the arrows scrolling and offers no
/// way to walk the rows (the shop, `ShopMenu`, is the worst of them). Isorun moves the hover with
/// up / down on all of them and tells the server where it is with `menu_hover` — which is what the
/// official client sends when the mouse crosses a row, so no menu is surprised by it.
pub fn menu_client_hover(_menu: &MenuState) -> bool {
    true
}

/// What a keydown means to the menu, or None when the key is the server's (menu.js
/// `menu_keydown_handler` and `menu_keypress_handler`, in one). Alt and Ctrl combinations are
/// never the menu's. Plain left / right are the server's (they switch inventory categories or the
/// action mode); with Shift they scroll a line.
pub fn menu_key_intent(menu: &MenuState, ev: NavKey) -> Option<MenuNavKey> {
    if ev.alt || ev.ctrl {
        return None;
    }
    let raw = raw_arrow_keys(menu);
    let arrows = menu_client_hover(menu);
    let unless_raw = |k: MenuNavKey| if raw { None } else { Some(k) };
    match ev.key {
        KeyName::PageUp => return unless_raw(MenuNavKey::PageUp),
        KeyName::PageDown => return unless_raw(MenuNavKey::PageDown),
        KeyName::End => return unless_raw(MenuNavKey::End),
        KeyName::Home => return unless_raw(MenuNavKey::Home),
        KeyName::Up => return unless_raw(if arrows && !ev.shift { MenuNavKey::Up } else { MenuNavKey::LineUp }),
        KeyName::Down => return unless_raw(if arrows && !ev.shift { MenuNavKey::Down } else { MenuNavKey::LineDown }),
        KeyName::Left => return (!raw && ev.shift).then_some(MenuNavKey::LineUp),
        KeyName::Right => return (!raw && ev.shift).then_some(MenuNavKey::LineDown),
        KeyName::NumpadSubtract => return if raw || menu_has_custom_dash(menu) { None } else { Some(MenuNavKey::PageUp) },
        KeyName::NumpadAdd => return unless_raw(MenuNavKey::PageDown),
        _ => {}
    }
    // characters: the keypress block (raw input in the macro editor is the server's)
    if menu.tag == "macro_mapping" {
        return None;
    }
    let KeyName::Char(mut ch) = ev.key else { return None };
    // in a multiselect arrows menu, space is the toggle (sent as `.` by the official client, so it does not page)
    if ch == ' ' && menu.flags & MenuFlag::MULTISELECT != 0 && arrows {
        ch = '.';
    }
    match ch {
        '-' => (!menu_has_custom_dash(menu)).then_some(MenuNavKey::PageUp),
        '<' => (menu.tag != "travel").then_some(MenuNavKey::PageUp),
        ';' => Some(MenuNavKey::PageUp),
        '>' => (menu.tag != "travel").then_some(MenuNavKey::PageDown),
        '+' | ' ' => Some(MenuNavKey::PageDown),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// popups: ui-layouts.js `scroller_handle_key`

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKey {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Popup types whose text the official client scrolls itself (ui-layouts.js: every layout that
/// calls `scroller_handle_key`). The rest (progress bar, seed selection, new game, message line)
/// leave every key to the server.
pub const SCROLLER_POPUPS: [&str; 10] = [
    "describe-generic",
    "describe-feature-wide",
    "describe-item",
    "describe-spell",
    "describe-cards",
    "describe-god",
    "describe-monster",
    "version",
    "game-over",
    "formatted-scroller",
];

pub fn is_scroller(kind: &str) -> bool {
    SCROLLER_POPUPS.contains(&kind)
}

/// What a key does to a popup's scroller, or None when the key is the server's. Alt and Ctrl
/// combinations are never the scroller's (Shift is not looked at). The feature popup leaves `<`
/// and `>` alone (its own keys), and the god and monster popups leave space to the server.
pub fn scroll_key_intent(popup_type: &str, ev: NavKey) -> Option<ScrollKey> {
    if !is_scroller(popup_type) || ev.alt || ev.ctrl {
        return None;
    }
    match ev.key {
        KeyName::PageUp => return Some(ScrollKey::PageUp),
        KeyName::PageDown => return Some(ScrollKey::PageDown),
        KeyName::End => return Some(ScrollKey::End),
        KeyName::Home => return Some(ScrollKey::Home),
        KeyName::Up => return Some(ScrollKey::LineUp),
        KeyName::Down => return Some(ScrollKey::LineDown),
        _ => {}
    }
    let KeyName::Char(ch) = ev.key else { return None };
    if (popup_type == "describe-god" || popup_type == "describe-monster") && ch == ' ' {
        return None;
    }
    if popup_type == "describe-feature-wide" && (ch == '<' || ch == '>') {
        return None;
    }
    match ch {
        ' ' | '>' | '+' | '\'' => Some(ScrollKey::PageDown),
        '-' | '<' | ';' => Some(ScrollKey::PageUp),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use isorun_webtiles::{reduce, GameState};

    /// A menu recorded from a real server (native/fixtures/menus), through the reducer.
    pub fn fixture_menu(json: &str) -> MenuState {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut st = GameState::default();
        // the fixture's first message is the menu (what follows it closes it again)
        reduce(&mut st, &v["msgs"][0]);
        st.menus.last().unwrap().clone()
    }
    pub fn pickup() -> MenuState {
        fixture_menu(include_str!("../../../fixtures/menus/menu-pickup-1.json"))
    }
    pub fn use_item() -> MenuState {
        fixture_menu(include_str!("../../../fixtures/menus/menu-use_item-1.json"))
    }
    fn inventory() -> MenuState {
        fixture_menu(include_str!("../../../fixtures/menus/menu-inventory-1.json"))
    }
    fn travel() -> MenuState {
        fixture_menu(include_str!("../../../fixtures/menus/menu-travel-1.json"))
    }
    fn key(k: KeyName) -> NavKey {
        NavKey::plain(k)
    }
    fn shift(k: KeyName) -> NavKey {
        NavKey { shift: true, ..NavKey::plain(k) }
    }
    fn ch(c: char) -> NavKey {
        NavKey::plain(KeyName::Char(c))
    }
    fn with_hover(m: &MenuState, h: i64) -> MenuState {
        MenuState { last_hovered: h, ..m.clone() }
    }
    fn vis(first: i64, last: i64) -> Visible {
        Visible { first, last }
    }

    #[test]
    fn selectable() {
        let m = pickup();
        assert!(!item_selectable(&m, 0));
        assert!(item_selectable(&m, 1));
        assert!(!item_selectable(&m, 3));
        let u = use_item();
        assert!(!item_selectable(&u, 0));
        let row = Some(isorun_webtiles::MenuItem { text: "x".into(), level: Some(2), ..Default::default() });
        assert!(item_selectable(&MenuState { tag: "use_item".into(), items: vec![row.clone()], ..Default::default() }, 0));
        assert!(!item_selectable(&MenuState { tag: "inventory".into(), items: vec![row], ..Default::default() }, 0));
    }

    #[test]
    fn next_hoverable() {
        let m = pickup();
        assert_eq!(m.flags & MenuFlag::WRAP, 0);
        assert_eq!(next_hoverable_item(&m, false, -1, false), 1);
        assert_eq!(next_hoverable_item(&m, false, 2, false), 4);
        assert_eq!(next_hoverable_item(&with_hover(&m, 6), false, 6, false), -1);
        let inv = inventory();
        assert_eq!(inv.last_hovered, -1);
        assert_eq!(next_hoverable_item(&inv, true, -1, false), -1);
        assert_eq!(next_hoverable_item(&inv, false, -1, false), 1);
        let h = with_hover(&m, 1);
        assert_eq!(next_hoverable_item(&h, true, 1, false), -1);
        let wrap = MenuState { flags: h.flags | MenuFlag::WRAP, ..h.clone() };
        assert_eq!(next_hoverable_item(&wrap, true, 1, false), 4);
        assert_eq!(next_hoverable_item(&wrap, false, 4, false), 1);
        assert_eq!(next_hoverable_item(&m, false, 0, true), 1);
        assert_eq!(next_hoverable_item(&m, false, 4, true), 4);
        assert_eq!(next_hoverable_item(&with_hover(&m, 6), true, 6, true), 4);
        assert_eq!(next_hoverable_item(&m, true, 6, true), -1);
    }

    #[test]
    fn paging_helpers() {
        let m = pickup();
        assert_eq!(relative_hover(&with_hover(&m, -1), vis(0, 6)), 0);
        assert_eq!(relative_hover(&with_hover(&m, 2), vis(0, 6)), 1);
        assert_eq!(relative_hover(&with_hover(&m, 4), vis(0, 6)), 2);
        assert_eq!(relative_hover(&with_hover(&m, 6), vis(0, 6)), -1);
        assert_eq!(relative_hover_target(&m, vis(0, 6), 0), 1);
        assert_eq!(relative_hover_target(&m, vis(0, 6), 1), 2);
        assert_eq!(relative_hover_target(&m, vis(3, 6), 0), 4);
        assert_eq!(relative_hover_target(&m, vis(0, 6), -1), -1);
        assert_eq!(relative_hover_target(&m, vis(0, 4), -1), 4);
        assert_eq!(relative_hover_target(&m, vis(0, 2), 5), 2);
        assert_eq!(line_down_target(&m, vis(0, 3)), 2);
        assert_eq!(line_down_target(&m, vis(1, 3)), 2);
        assert_eq!(line_down_target(&m, vis(6, 6)), 6);
        assert_eq!(line_up_target(vis(0, 3)), 0);
        assert_eq!(line_up_target(vis(4, 6)), 3);
        assert_eq!(snap_hover_target(-1, vis(2, 5)), -1);
        assert_eq!(snap_hover_target(3, vis(2, 5)), -1);
        assert_eq!(snap_hover_target(1, vis(2, 5)), 2);
        assert_eq!(snap_hover_target(6, vis(2, 5)), 5);
        assert!(menu_has_sections(&m));
        // pickup: header, a, b, header, c, header, corpse
        assert_eq!(cycle_headers_target(&with_hover(&m, 1), true, vis(0, 6)), 3);
        assert_eq!(cycle_headers_target(&with_hover(&m, 4), true, vis(0, 6)), 5);
        assert_eq!(cycle_headers_target(&with_hover(&m, 1), false, vis(0, 6)), 5);
    }

    #[test]
    fn key_intents() {
        let arrows = use_item();
        let multi = pickup();
        let plain = MenuState { tag: "help".into(), flags: arrows.flags & !MenuFlag::ARROWS_SELECT, ..arrows.clone() };
        assert!(arrows.flags & MenuFlag::ARROWS_SELECT != 0);
        assert!(multi.flags & MenuFlag::MULTISELECT != 0);
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Up)), Some(MenuNavKey::Up));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Down)), Some(MenuNavKey::Down));
        assert_eq!(menu_key_intent(&arrows, shift(KeyName::Down)), Some(MenuNavKey::LineDown));
        assert_eq!(menu_key_intent(&plain, key(KeyName::Up)), Some(MenuNavKey::Up));
        assert_eq!(menu_key_intent(&plain, shift(KeyName::Up)), Some(MenuNavKey::LineUp));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Left)), None);
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Right)), None);
        assert_eq!(menu_key_intent(&arrows, shift(KeyName::Left)), Some(MenuNavKey::LineUp));
        assert_eq!(menu_key_intent(&arrows, shift(KeyName::Right)), Some(MenuNavKey::LineDown));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::PageUp)), Some(MenuNavKey::PageUp));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::PageDown)), Some(MenuNavKey::PageDown));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Home)), Some(MenuNavKey::Home));
        assert_eq!(menu_key_intent(&arrows, key(KeyName::End)), Some(MenuNavKey::End));
        assert_eq!(menu_key_intent(&plain, key(KeyName::NumpadAdd)), Some(MenuNavKey::PageDown));
        assert_eq!(menu_key_intent(&plain, key(KeyName::NumpadSubtract)), Some(MenuNavKey::PageUp));
        assert_eq!(menu_key_intent(&arrows, NavKey { ctrl: true, ..key(KeyName::Down) }), None);
        assert_eq!(menu_key_intent(&arrows, NavKey { alt: true, ..key(KeyName::Down) }), None);
        for (c, k) in [
            (' ', MenuNavKey::PageDown),
            ('+', MenuNavKey::PageDown),
            ('>', MenuNavKey::PageDown),
            ('-', MenuNavKey::PageUp),
            ('<', MenuNavKey::PageUp),
            (';', MenuNavKey::PageUp),
        ] {
            assert_eq!(menu_key_intent(&plain, ch(c)), Some(k));
        }
        assert_eq!(menu_key_intent(&multi, ch(' ')), None);
        assert_eq!(menu_key_intent(&arrows, ch(' ')), Some(MenuNavKey::PageDown));
        for tag in ["inventory", "stash", "actions", "macros", "macro_mapping", "use_item"] {
            assert!(menu_has_custom_dash(&MenuState { tag: tag.into(), ..Default::default() }));
        }
        assert_eq!(menu_key_intent(&arrows, ch('-')), None);
        assert_eq!(menu_key_intent(&arrows, key(KeyName::NumpadSubtract)), None);
        let t = travel();
        assert_eq!(t.tag, "travel");
        assert_eq!(menu_key_intent(&t, ch('<')), None);
        assert_eq!(menu_key_intent(&t, ch('>')), None);
        assert_eq!(menu_key_intent(&t, ch(';')), Some(MenuNavKey::PageUp));
        assert_eq!(menu_key_intent(&arrows, ch('a')), None);
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Enter)), None);
        assert_eq!(menu_key_intent(&arrows, key(KeyName::Escape)), None);
        let macro_ = MenuState {
            tag: "macro_mapping".into(),
            title_prompt: Some(isorun_webtiles::TitlePrompt { prompt: "Input trigger key:".into(), raw: true }),
            ..arrows.clone()
        };
        assert!(raw_arrow_keys(&macro_));
        assert_eq!(menu_key_intent(&macro_, key(KeyName::Down)), None);
        assert_eq!(menu_key_intent(&macro_, key(KeyName::PageDown)), None);
        let listing = MenuState { tag: "macro_mapping".into(), title_prompt: None, ..arrows.clone() };
        assert!(!raw_arrow_keys(&listing));
        assert_eq!(menu_key_intent(&listing, key(KeyName::Down)), Some(MenuNavKey::Down));
        assert_eq!(menu_key_intent(&listing, ch(' ')), None);
        assert!(raw_arrow_keys(&MenuState { items: vec![], ..listing }));
    }

    #[test]
    fn scroller_keys() {
        for t in SCROLLER_POPUPS {
            assert!(is_scroller(t));
        }
        for t in ["newgame-choice", "seed-selection", "progress-bar", "msgwin-get-line", "newgame-random-combo"] {
            assert_eq!(scroll_key_intent(t, key(KeyName::Down)), None);
        }
        let d = "describe-item";
        assert_eq!(scroll_key_intent(d, key(KeyName::Up)), Some(ScrollKey::LineUp));
        assert_eq!(scroll_key_intent(d, shift(KeyName::Down)), Some(ScrollKey::LineDown));
        assert_eq!(scroll_key_intent(d, key(KeyName::PageUp)), Some(ScrollKey::PageUp));
        assert_eq!(scroll_key_intent(d, key(KeyName::End)), Some(ScrollKey::End));
        assert_eq!(scroll_key_intent(d, NavKey { ctrl: true, ..key(KeyName::Down) }), None);
        assert_eq!(scroll_key_intent(d, key(KeyName::Left)), None);
        assert_eq!(scroll_key_intent(d, key(KeyName::Enter)), None);
        for c in [' ', '>', '+', '\''] {
            assert_eq!(scroll_key_intent("formatted-scroller", ch(c)), Some(ScrollKey::PageDown));
        }
        for c in ['-', '<', ';'] {
            assert_eq!(scroll_key_intent("formatted-scroller", ch(c)), Some(ScrollKey::PageUp));
        }
        assert_eq!(scroll_key_intent("formatted-scroller", ch('d')), None);
        assert_eq!(scroll_key_intent("describe-feature-wide", ch('<')), None);
        assert_eq!(scroll_key_intent("describe-feature-wide", ch(' ')), Some(ScrollKey::PageDown));
        assert_eq!(scroll_key_intent("describe-god", ch(' ')), None);
        assert_eq!(scroll_key_intent("describe-monster", ch('-')), Some(ScrollKey::PageUp));
    }

    #[test]
    fn more_switches() {
        let shop = "You have 100 gold pieces.\n[Esc] exit          [!] buy|examine items    a-c mark item for purchase   \n[/] sort (default)         [Enter] buy marked items   A-C put item on shopping list";
        let sw = parse_more_switches(shop);
        assert_eq!(
            sw.iter().map(|s| format!("{}={}", s.key, s.label)).collect::<Vec<_>>(),
            ["Esc=exit", "!=buy|examine items", "/=sort (default)", "Enter=buy marked items"]
        );
        let chars: Vec<char> = shop.chars().collect();
        assert_eq!(chars[sw[1].start..sw[1].end].iter().collect::<String>(), "[!] buy|examine items");
        assert!(parse_more_switches("[Up|Down] select  [ 42%]").is_empty());
    }
}

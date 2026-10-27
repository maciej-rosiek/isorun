//! The Find screen's conversation with crawl: its own stash search (stash.cc `search_stashes`),
//! run behind the screen, so the results are crawl's on a server and offline alike, and its
//! travel is crawl's.
//!
//! - crawl's prompt for the search (`init_input` tagged `stash_search`, opened by Ctrl-F from a
//!   key, the dock, the pad or the palette) is answered at once with `..`, a regular expression
//!   that every result matches and that crawl lists unhighlighted (`nohl`): everything the
//!   stash tracker knows, on every level, the pack's items too. The prompt is never drawn.
//! - crawl answers with its results menu (tag `stash`), which stays up under the screen,
//!   hidden: the screen lists its rows (`entry::parse_menu`).
//! - a row is picked as crawl's own menu picks it: hovered (`menu_hover`) and Enter. crawl then
//!   shows its level map at the place, in travel mode (`show_map(lp, true, true)`), where `.`
//!   travels there (`CMD_MAP_GOTO_TARGET`); both keys go at once, crawl reads them in order.
//! - more than 400 results (`SEARCH_SPAM_THRESHOLD`, after crawl folds duplicates) and crawl
//!   lists none ("Too many matches"): the search is asked again for the player's level only
//!   (`@..`), and the screen says so.
//! - `=` shows or hides useless items and duplicates (crawl lists its results again);
//!   another search (the screen's field) closes the menu and asks crawl again with the words.
//! - Escape closes crawl's menu with the screen.

use crate::entry::{is_stash_menu, parse_menu, Entry};
use isorun_webtiles::{cm, formatted_string_to_text, GameState, MouseMode, TileRef};
use serde_json::Value;

/// What crawl is asked for when the screen opens: every result (see the module's notes).
pub const EVERYTHING: &str = "..";
/// Every result on the player's level only (`@` searches the current level, stash.cc).
pub const THIS_LEVEL: &str = "@..";

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Idle,
    /// Ctrl-F sent for a search of ours; waiting for crawl's prompt
    Asking,
    /// the prompt answered; waiting for the results
    Answered,
    /// the screen is up (crawl's menu under it, unless crawl listed nothing)
    Shown,
    /// the screen was closed before crawl's prompt came: Escape it when it does
    Dismissing,
}

/// The screen's side of crawl's stash search.
#[derive(Clone, Debug)]
pub struct Driver {
    phase: Phase,
    /// what crawl was asked for last
    pub query: String,
    /// when the phase began (ms): a prompt that never comes is given up on
    since: f64,
    /// the message lines there were when the query went: crawl's word on a search with no menu
    /// comes after them
    lines_at: usize,
    /// `=` sent: crawl's menu goes and comes back
    refreshing: bool,
    /// the menu last read (its place in the ui stack, the state's ui revision)
    read: (i64, u64),
    pub entries: Vec<Entry>,
    /// crawl's menu is up under the screen
    pub menu_up: bool,
    /// crawl hides useless items and duplicates (its title's `hide`), as it does at first
    pub hiding_useless: bool,
    /// crawl's count of matches (its title's)
    pub matches: Option<String>,
    /// what the screen says over the list: crawl's word when it listed nothing
    pub notice: Option<String>,
}

impl Default for Driver {
    fn default() -> Self {
        Driver {
            phase: Phase::Idle,
            query: EVERYTHING.into(),
            since: 0.0,
            lines_at: 0,
            refreshing: false,
            read: (-1, u64::MAX),
            entries: Vec::new(),
            menu_up: false,
            hiding_useless: true,
            matches: None,
            notice: None,
        }
    }
}

/// crawl's prompt for a stash search is up.
pub fn stash_prompt(st: &GameState) -> bool {
    st.text_input.as_ref().is_some_and(|t| t.tag.as_deref() == Some("stash_search"))
}

fn stash_menu(st: &GameState) -> Option<&isorun_webtiles::MenuState> {
    st.menus.last().filter(|m| is_stash_menu(m))
}

/// The answer to crawl's text prompt (what overlays' field sends: Ctrl-U and Ctrl-K wipe a
/// prefill, then the line).
fn answer(query: &str, out: &mut Vec<Value>) {
    out.push(cm::key(21));
    out.push(cm::key(11));
    out.push(cm::text_input(&format!("{query}\r")));
}

impl Driver {
    /// The screen is up.
    pub fn open(&self) -> bool {
        matches!(self.phase, Phase::Asking | Phase::Answered | Phase::Shown)
    }

    /// Waiting on crawl (the screen says so).
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Asking | Phase::Answered) || self.refreshing
    }

    /// What the overlays leave undrawn while the screen drives crawl: its stash prompt and menu.
    pub fn hides(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Follow crawl's state: call every frame, with the time in ms. `name_of` names a tile
    /// (`entry::tile_name` over the game's gamedata). What it sends crawl goes to `out`.
    pub fn update(&mut self, st: &GameState, watching: bool, now: f64, name_of: &dyn Fn(&TileRef) -> Option<String>, out: &mut Vec<Value>) {
        if watching {
            // a spectator sees crawl's own prompt and menu: nothing of theirs is answered
            if self.phase != Phase::Idle {
                *self = Driver::default();
            }
            return;
        }
        let prompt = stash_prompt(st);
        match self.phase {
            Phase::Idle if prompt => {
                // crawl asks, from whatever opened it: the screen takes over with everything
                self.query = EVERYTHING.into();
                self.notice = None;
                self.ask(st, now, out);
            }
            Phase::Asking if prompt => self.ask(st, now, out),
            Phase::Asking | Phase::Dismissing if now - self.since > 5000.0 => {
                // the prompt never came (another key took it): give the search up
                self.phase = if self.phase == Phase::Dismissing { Phase::Idle } else { Phase::Shown };
                self.notice.get_or_insert_with(|| "crawl did not answer the search.".into());
            }
            Phase::Dismissing if prompt => {
                out.push(cm::key(27));
                self.phase = Phase::Idle;
            }
            Phase::Answered => {
                if stash_menu(st).is_some() {
                    self.phase = Phase::Shown;
                } else if !prompt && st.menus.is_empty() && st.input_mode == MouseMode::COMMAND {
                    // crawl is back at its commands with no menu: its message says why
                    let said: Vec<String> =
                        st.messages.lines.iter().skip(self.lines_at.min(st.messages.lines.len())).map(|l| formatted_string_to_text(&l.text)).collect();
                    let said = said.join(" ");
                    self.entries.clear();
                    self.menu_up = false;
                    self.matches = None;
                    if said.contains("Too many matches") && self.query == EVERYTHING {
                        // too much to list at once: this level's, and the words to narrow it
                        self.query = THIS_LEVEL.into();
                        self.notice = Some("crawl knows too much to list it all: this level only. Search for the rest.".into());
                        out.push(cm::key(6));
                        self.phase = Phase::Asking;
                        self.since = now;
                    } else {
                        self.notice = Some(if said.contains("Too many matches") {
                            "Too many matches: search for something narrower.".into()
                        } else if said.contains("invalid") {
                            "crawl cannot read that search.".into()
                        } else if self.query == EVERYTHING || self.query == THIS_LEVEL {
                            "Nothing found yet: explore, and what you see is listed here.".into()
                        } else {
                            format!("Nothing matches \u{201c}{}\u{201d}.", self.query)
                        });
                        self.phase = Phase::Shown;
                    }
                }
            }
            _ => {}
        }
        if self.phase == Phase::Shown {
            match stash_menu(st) {
                Some(m) => {
                    self.menu_up = true;
                    let key = (m.stack_order, st.rev.ui);
                    if key != self.read {
                        if key.0 != self.read.0 {
                            self.refreshing = false;
                        }
                        self.read = key;
                        self.entries = parse_menu(m, name_of);
                        let title = formatted_string_to_text(&m.title.text);
                        self.hiding_useless = !title.contains("show useless");
                        self.matches = title.split(':').next().map(|s| s.trim().to_string()).filter(|s| s.contains("match"));
                        if !self.entries.is_empty() {
                            self.notice = None;
                        }
                    }
                }
                None if self.menu_up && !self.refreshing => {
                    // crawl put its menu away (the game moved on): the screen goes with it
                    self.close_quietly();
                }
                None => {}
            }
        }
    }

    fn ask(&mut self, st: &GameState, now: f64, out: &mut Vec<Value>) {
        answer(&self.query, out);
        self.phase = Phase::Answered;
        self.lines_at = st.messages.lines.len();
        self.since = now;
        self.refreshing = false;
    }

    fn close_quietly(&mut self) {
        let hiding = self.hiding_useless;
        *self = Driver::default();
        self.hiding_useless = hiding;
    }

    /// Open the screen from the client (a button of its own): Ctrl-F, answered when it comes.
    pub fn start(&mut self, now: f64, out: &mut Vec<Value>) {
        if self.phase != Phase::Idle {
            return;
        }
        self.query = EVERYTHING.into();
        self.notice = None;
        out.push(cm::key(6));
        self.phase = Phase::Asking;
        self.since = now;
    }

    /// Go to a result: crawl's own pick, then `.` on its map. The screen closes.
    pub fn travel(&mut self, e: &Entry, out: &mut Vec<Value>) {
        if !self.menu_up {
            return;
        }
        out.push(cm::menu_hover(e.index as i64, false));
        out.push(cm::key(13));
        out.push(cm::key('.' as i32));
        self.close_quietly();
    }

    /// Search crawl again for `words` (empty: everything).
    pub fn search(&mut self, words: &str, now: f64, out: &mut Vec<Value>) {
        if !self.open() {
            return;
        }
        let q = words.trim();
        self.query = if q.is_empty() { EVERYTHING.into() } else { q.to_string() };
        self.notice = None;
        if self.menu_up {
            out.push(cm::key(27));
        }
        self.menu_up = false;
        self.read = (-1, u64::MAX);
        out.push(cm::key(6));
        self.phase = Phase::Asking;
        self.since = now;
    }

    /// Show or hide useless items and duplicates (crawl's `=`).
    pub fn toggle_useless(&mut self, out: &mut Vec<Value>) {
        if self.menu_up && !self.refreshing {
            out.push(cm::key('=' as i32));
            self.refreshing = true;
        }
    }

    /// Close the screen, and crawl's menu or prompt under it.
    pub fn close(&mut self, st: &GameState, now: f64, out: &mut Vec<Value>) {
        match self.phase {
            Phase::Idle | Phase::Dismissing => return,
            Phase::Asking if !stash_prompt(st) => {
                // the prompt is still on its way: Escape it when it comes
                *self = Driver { phase: Phase::Dismissing, since: now, hiding_useless: self.hiding_useless, ..Driver::default() };
                return;
            }
            _ => {}
        }
        if self.menu_up || stash_prompt(st) || self.phase == Phase::Answered {
            out.push(cm::key(27));
        }
        self.close_quietly();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::tests::names;
    use isorun_webtiles::reduce;
    use serde_json::json;

    fn prompt(st: &mut GameState) {
        reduce(st, &json!({"msg": "input_mode", "mode": 7}));
        reduce(st, &json!({"msg": "init_input", "type": "messages", "tag": "stash_search", "prompt": "Search for what [? for help]? "}));
    }
    fn results(st: &mut GameState) {
        reduce(st, &json!({"msg": "close_input"}));
        let v: Value = serde_json::from_str(include_str!("../../../fixtures/menus/menu-stash-1.json")).unwrap();
        reduce(st, &v["msgs"][0]);
    }

    #[test]
    fn crawls_prompt_is_answered_and_its_results_read() {
        let mut st = GameState::default();
        let mut d = Driver::default();
        let mut out = Vec::new();
        prompt(&mut st);
        d.update(&st, false, 0.0, &names, &mut out);
        assert!(d.open() && d.hides() && d.busy());
        assert_eq!(out, vec![cm::key(21), cm::key(11), cm::text_input("..\r")]);
        out.clear();
        // the prompt is still up a frame later: it is answered once
        d.update(&st, false, 16.0, &names, &mut out);
        assert!(out.is_empty());
        results(&mut st);
        d.update(&st, false, 32.0, &names, &mut out);
        assert!(d.menu_up && !d.busy());
        assert_eq!(d.entries.len(), 26);
        assert!(d.hiding_useless);
        assert_eq!(d.matches.as_deref(), Some("26 matches"));
        // a pick is crawl's: hover, Enter, then `.` on its map
        let darts = d.entries.iter().find(|e| e.name == "4 poisoned darts").unwrap().clone();
        d.travel(&darts, &mut out);
        assert_eq!(out, vec![cm::menu_hover(9, false), cm::key(13), cm::key(46)]);
        assert!(!d.open() && !d.hides());
    }

    #[test]
    fn too_many_asks_for_this_level() {
        let mut st = GameState::default();
        let mut d = Driver::default();
        let mut out = Vec::new();
        prompt(&mut st);
        d.update(&st, false, 0.0, &names, &mut out);
        out.clear();
        reduce(&mut st, &json!({"msg": "close_input"}));
        reduce(&mut st, &json!({"msg": "msgs", "messages": [{"text": "Too many matches; use a more specific search."}]}));
        reduce(&mut st, &json!({"msg": "input_mode", "mode": 1}));
        d.update(&st, false, 10.0, &names, &mut out);
        assert_eq!(out, vec![cm::key(6)]);
        assert!(d.notice.as_deref().unwrap().contains("this level"));
        out.clear();
        prompt(&mut st);
        d.update(&st, false, 20.0, &names, &mut out);
        assert_eq!(out.last(), Some(&cm::text_input("@..\r")));
    }

    #[test]
    fn closing_escapes_crawls_menu_and_a_prompt_on_its_way() {
        let mut st = GameState::default();
        let mut d = Driver::default();
        let mut out = Vec::new();
        prompt(&mut st);
        d.update(&st, false, 0.0, &names, &mut out);
        results(&mut st);
        d.update(&st, false, 1.0, &names, &mut out);
        out.clear();
        d.close(&st, 2.0, &mut out);
        assert_eq!(out, vec![cm::key(27)]);
        assert!(!d.open());
        // a search of the client's: Ctrl-F, and closed before crawl asks
        let mut st = GameState::default();
        let mut d = Driver::default();
        out.clear();
        d.start(0.0, &mut out);
        assert_eq!(out, vec![cm::key(6)]);
        d.close(&st, 1.0, &mut out);
        assert!(!d.open() && d.hides());
        out.clear();
        prompt(&mut st);
        d.update(&st, false, 2.0, &names, &mut out);
        assert_eq!(out, vec![cm::key(27)]);
        assert!(!d.hides());
    }

    #[test]
    fn a_spectator_is_left_crawls_own() {
        let mut st = GameState::default();
        let mut d = Driver::default();
        let mut out = Vec::new();
        prompt(&mut st);
        d.update(&st, true, 0.0, &names, &mut out);
        assert!(out.is_empty() && !d.open());
    }
}

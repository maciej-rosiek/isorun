//! The focus layer (focus.ts): one cursor over whatever the top overlay offers, driven by the
//! d-pad or the arrow keys, with A / Enter to activate and B / Escape to cancel. Every overlay
//! registers its [`Focusable`]s in visual order; [`FocusNav`] owns the cursor and reports what it
//! is on. The items are plain data: what an item does is a list of client messages
//! ([`Act`]), so the layer is testable without a screen.
//!
//! Server menus are not routed through here: their hover is the server's (`menu_hover`), and
//! `Overlays::menu_op` keeps the two in step.

use serde_json::Value;

/// What activating a stop does. Mostly messages to crawl; a few stops act on the overlay itself.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Act {
    #[default]
    None,
    /// Send these, in order.
    Send(Vec<Value>),
    /// Bring up the field's keyboard (the seed field): the pad types into it.
    Keyboard,
}

impl Act {
    pub fn send(msgs: impl IntoIterator<Item = Value>) -> Act {
        Act::Send(msgs.into_iter().collect())
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Focusable {
    pub label: String,
    /// the terminal colour the label wears where it came from (a prompt's word in the message log), for the action bar
    pub colour: Option<u8>,
    pub act: Act,
    /// column-like grouping: left / right move within a row, so a group is what a row spans
    pub group: String,
    /// stable identity across rebuilds of the same screen (default: the label)
    pub id: Option<String>,
    /// visual row; items without one are each their own row, in order
    pub row: Option<i32>,
    /// visual column within the row (default: order of registration in the row)
    pub col: Option<i32>,
    /// what B / Escape means on this screen (a "No", a "Close"); at most one
    pub cancel: bool,
    /// a second thing the item can do (a skill row's "Set target"), on Y
    pub alt: Option<(String, Act)>,
    /// sent when the cursor arrives by a move (not by a rebuild): the new-game screen tells the server where it is
    pub on_focus: Vec<Value>,
}

impl Focusable {
    pub fn new(label: impl Into<String>, act: Act) -> Self {
        Focusable { label: label.into(), act, ..Default::default() }
    }
    pub fn at(mut self, row: i32, col: i32) -> Self {
        self.row = Some(row);
        self.col = Some(col);
        self
    }
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    /// The identity the cursor follows across rebuilds.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusOp {
    Next,
    Prev,
    Left,
    Right,
    Select,
    AltSelect,
    Cancel,
    PageNext,
    PagePrev,
    First,
    Last,
}

/// The cursor as the bindings see it: what A and B would do, for the bar.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FocusInfo {
    pub label: Option<String>,
    pub colour: Option<u8>,
    pub cancel_label: Option<String>,
    pub alt_label: Option<String>,
    pub index: i32,
    pub count: usize,
}

/// A slot in the pure layout: what [`focus_step`] needs to know about an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusSlot {
    pub row: i32,
    pub col: i32,
}

/// Resolve the row / col of each item: rows default to registration order, cols to order within the row.
pub fn focus_slots(items: &[(Option<i32>, Option<i32>)]) -> Vec<FocusSlot> {
    let mut per_row = std::collections::HashMap::new();
    items
        .iter()
        .enumerate()
        .map(|(i, &(row, col))| {
            let row = row.unwrap_or(i as i32);
            let n = per_row.entry(row).or_insert(0);
            let slot = FocusSlot { row, col: col.unwrap_or(*n) };
            *n += 1;
            slot
        })
        .collect()
}

/// Options of the step: `columns` lets left and right cross to the next column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepOpts {
    pub columns: bool,
    pub keep_column: bool,
}

/// Where the cursor goes from `index` in `dir`. Up and down go to the nearest row above or below,
/// keeping the column as close as possible; left and right stay on the row, unless the screen is
/// laid out in columns (`columns`), where they cross to the nearest row of the next column — the
/// skills screen prints rows the other column has no entry on, and the cursor must still leave
/// them sideways or the key falls through to the server. None at an edge, so the caller can wrap
/// or hand the key to the server (a popup scrolls its text on up / down).
pub fn focus_step(slots: &[FocusSlot], index: i32, dir: FocusDir, opts: StepOpts) -> Option<usize> {
    let Some(cur) = (index >= 0).then(|| slots.get(index as usize)).flatten().copied() else {
        return if slots.is_empty() { None } else { Some(0) };
    };
    let index = index as usize;
    if matches!(dir, FocusDir::Left | FocusDir::Right) {
        let forward = dir == FocusDir::Right;
        let mut best: Option<usize> = None;
        for (i, s) in slots.iter().enumerate() {
            if i == index || (!opts.columns && s.row != cur.row) {
                continue;
            }
            if if forward { s.col <= cur.col } else { s.col >= cur.col } {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) => {
                    let b = slots[b];
                    // the nearest column that way; within it, the nearest row
                    let better = if s.col == b.col {
                        (s.row - cur.row).abs() < (b.row - cur.row).abs()
                    } else if forward {
                        s.col < b.col
                    } else {
                        s.col > b.col
                    };
                    if better {
                        best = Some(i);
                    }
                }
            }
        }
        return best;
    }
    // the nearest row in that direction with an item in the same column (down a column of the
    // skills screen), else the nearest row at all, nearest column
    let down = dir == FocusDir::Down;
    let nearest_row = |same_col: bool| -> Option<i32> {
        let mut row: Option<i32> = None;
        for s in slots {
            if same_col && s.col != cur.col {
                continue;
            }
            if if down { s.row > cur.row } else { s.row < cur.row } {
                if row.is_none_or(|r| if down { s.row < r } else { s.row > r }) {
                    row = Some(s.row);
                }
            }
        }
        row
    };
    let row = nearest_row(true).or_else(|| nearest_row(false))?;
    let mut best: Option<usize> = None;
    for (i, s) in slots.iter().enumerate() {
        if s.row != row {
            continue;
        }
        if best.is_none_or(|b| (s.col - cur.col).abs() < (slots[b].col - cur.col).abs()) {
            best = Some(i);
        }
    }
    best
}

/// Past an edge: the start of the first row going down, the end of the last row going up (with
/// `keep_column`, the far row's nearest column), the far end of the row (or, in `columns`, of the
/// screen) sideways.
pub fn focus_wrap(slots: &[FocusSlot], index: i32, dir: FocusDir, opts: StepOpts) -> Option<usize> {
    if slots.is_empty() {
        return None;
    }
    let Some(cur) = (index >= 0).then(|| slots.get(index as usize)).flatten().copied() else { return Some(0) };
    if matches!(dir, FocusDir::Left | FocusDir::Right) {
        // the far end of the same row; in columns, the far column, nearest row
        let mut best: Option<usize> = None;
        for (i, s) in slots.iter().enumerate() {
            if !opts.columns && s.row != cur.row {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) => {
                    let b = slots[b];
                    let better = if s.col == b.col {
                        opts.columns && (s.row - cur.row).abs() < (b.row - cur.row).abs()
                    } else if dir == FocusDir::Right {
                        s.col < b.col
                    } else {
                        s.col > b.col
                    };
                    if better {
                        best = Some(i);
                    }
                }
            }
        }
        return match best {
            Some(b) if b as i32 == index || slots[b].col == cur.col => None,
            other => other,
        };
    }
    let down = dir == FocusDir::Down;
    let mut row: Option<i32> = None;
    for s in slots {
        if row.is_none_or(|r| if down { s.row < r } else { s.row > r }) {
            row = Some(s.row);
        }
    }
    let row = row?;
    if row == cur.row {
        return None;
    }
    let mut best: Option<usize> = None;
    for (i, s) in slots.iter().enumerate() {
        if s.row != row {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => {
                if opts.keep_column {
                    (s.col - cur.col).abs() < (slots[b].col - cur.col).abs()
                } else if down {
                    s.col < slots[b].col
                } else {
                    s.col > slots[b].col
                }
            }
        };
        if better {
            best = Some(i);
        }
    }
    best
}

/// The cursor `n` rows on, clamped: what a bumper does on a client-paged list.
pub fn focus_page(slots: &[FocusSlot], index: i32, n: i32) -> Option<usize> {
    let mut i = index;
    let step = if n > 0 { FocusDir::Down } else { FocusDir::Up };
    for _ in 0..n.abs() {
        match focus_step(slots, i, step, StepOpts::default()) {
            Some(nx) => i = nx as i32,
            None => break,
        }
    }
    (i != index).then_some(i as usize)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusOptions {
    /// past an edge, continue from the other end (else the caller forwards the key)
    pub wrap: bool,
    /// bumpers move the cursor by this many rows (else the caller forwards page keys)
    pub page_rows: i32,
    /// a single row of chips: up and down walk it too
    pub linear: bool,
    /// the screen is laid out in columns: left and right cross to the next column, not only along a row
    pub columns: bool,
    /// wrapping up or down lands in the far row's nearest column, not at its far end
    pub keep_column: bool,
}

/// The cursor over one screen's focusables. `set` is called on every rebuild of the overlay; the
/// same `screen` key keeps the cursor on the item with the same id, a new screen starts at
/// `initial` (or the first item).
#[derive(Default)]
pub struct FocusNav {
    items: Vec<Focusable>,
    slots: Vec<FocusSlot>,
    index: i32,
    screen: String,
    opts: FocusOptions,
    /// messages and acts the last move asked for, for the owner to carry out
    fired: Vec<Act>,
}

impl FocusNav {
    pub fn new() -> Self {
        FocusNav { index: -1, ..Default::default() }
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn options(&self) -> FocusOptions {
        self.opts
    }

    pub fn screen(&self) -> &str {
        &self.screen
    }

    pub fn items(&self) -> &[Focusable] {
        &self.items
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn current(&self) -> Option<&Focusable> {
        (self.index >= 0).then(|| self.items.get(self.index as usize)).flatten()
    }

    /// Whether the cursor is on the item with this id.
    pub fn is_current(&self, id: &str) -> bool {
        self.current().is_some_and(|f| f.key() == id)
    }

    pub fn info(&self) -> FocusInfo {
        let cur = self.current();
        FocusInfo {
            label: cur.map(|c| c.label.clone()),
            colour: cur.and_then(|c| c.colour),
            cancel_label: self.items.iter().find(|f| f.cancel).map(|f| f.label.clone()),
            alt_label: cur.and_then(|c| c.alt.as_ref().map(|a| a.0.clone())),
            index: self.index,
            count: self.items.len(),
        }
    }

    /// `initial` -1: a new screen starts with nothing lit, and the first move lights its first item.
    pub fn set(&mut self, items: Vec<Focusable>, screen: &str, opts: FocusOptions, initial: Option<i32>, force_initial: bool) {
        let prev = self.current().map(|p| p.key().to_string());
        let same = screen == self.screen && prev.is_some() && !force_initial;
        self.slots = focus_slots(&items.iter().map(|f| (f.row, f.col)).collect::<Vec<_>>());
        self.items = items;
        self.screen = screen.to_string();
        self.opts = opts;
        let mut idx = -1;
        if !self.items.is_empty() {
            if same {
                let id = prev.unwrap();
                idx = self.items.iter().position(|f| f.key() == id).map_or(-1, |i| i as i32);
            }
            if idx < 0 && initial != Some(-1) {
                idx = match initial {
                    Some(i) if i >= 0 && (i as usize) < self.items.len() => i,
                    _ => 0,
                };
            }
        }
        self.index = idx;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.slots.clear();
        self.index = -1;
        self.screen.clear();
    }

    /// Put the cursor on item `i`; false if there is none. The item's `on_focus` is queued.
    pub fn focus(&mut self, i: i32) -> bool {
        if i < 0 || i as usize >= self.items.len() {
            return false;
        }
        self.index = i;
        let msgs = self.items[i as usize].on_focus.clone();
        if !msgs.is_empty() {
            self.fired.push(Act::Send(msgs));
        }
        true
    }

    /// Move the cursor. True when it moved (or wrapped); false when the edge was hit without
    /// wrap, so the caller can forward the key.
    pub fn step(&mut self, dir: FocusDir) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let dir = if self.opts.linear {
            match dir {
                FocusDir::Up => FocusDir::Left,
                FocusDir::Down => FocusDir::Right,
                d => d,
            }
        } else {
            dir
        };
        let cols = StepOpts { columns: self.opts.columns, keep_column: self.opts.keep_column };
        let mut nx = focus_step(&self.slots, self.index, dir, cols);
        if nx.is_none() && self.opts.wrap {
            nx = focus_wrap(&self.slots, self.index, dir, cols);
        }
        match nx {
            Some(n) => self.focus(n as i32),
            None => false,
        }
    }

    /// A bumper: `page_rows` rows on, when the screen pages on the client.
    pub fn page(&mut self, dir: i32) -> bool {
        let n = self.opts.page_rows;
        if n == 0 || self.items.is_empty() {
            return false;
        }
        match focus_page(&self.slots, self.index, dir * n) {
            Some(nx) => self.focus(nx as i32),
            None => false,
        }
    }

    pub fn first(&mut self) -> bool {
        !self.items.is_empty() && self.focus(0)
    }

    pub fn last(&mut self) -> bool {
        !self.items.is_empty() && self.focus(self.items.len() as i32 - 1)
    }

    /// Fire the focused item. False when there is none.
    pub fn activate(&mut self) -> bool {
        let Some(act) = self.current().map(|c| c.act.clone()) else { return false };
        self.fired.push(act);
        true
    }

    /// Fire the focused item's second action, if it has one.
    pub fn alt_activate(&mut self) -> bool {
        let Some(act) = self.current().and_then(|c| c.alt.as_ref().map(|a| a.1.clone())) else { return false };
        self.fired.push(act);
        true
    }

    /// Fire the screen's cancel item if it has one.
    pub fn cancel(&mut self) -> bool {
        let Some(act) = self.items.iter().find(|f| f.cancel).map(|f| f.act.clone()) else { return false };
        self.fired.push(act);
        true
    }

    /// What the last operations fired, oldest first.
    pub fn take_fired(&mut self) -> Vec<Act> {
        std::mem::take(&mut self.fired)
    }

    /// One focus op; false when it did nothing (the caller falls back to crawl's own key).
    pub fn op(&mut self, op: FocusOp) -> bool {
        match op {
            FocusOp::Select => self.activate(),
            FocusOp::AltSelect => self.alt_activate(),
            FocusOp::Cancel => self.cancel(),
            FocusOp::Prev => self.step(FocusDir::Up),
            FocusOp::Next => self.step(FocusDir::Down),
            FocusOp::Left => self.step(FocusDir::Left),
            FocusOp::Right => self.step(FocusDir::Right),
            FocusOp::PageNext => self.page(1),
            FocusOp::PagePrev => self.page(-1),
            FocusOp::First => self.first(),
            FocusOp::Last => self.last(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(v: &[(i32, i32)]) -> Vec<FocusSlot> {
        focus_slots(&v.iter().map(|&(r, c)| (Some(r), Some(c))).collect::<Vec<_>>())
    }
    fn item(label: &str) -> Focusable {
        Focusable::new(label, Act::send([Value::String(label.into())]))
    }

    #[test]
    fn layout_defaults() {
        assert_eq!(focus_slots(&[(None, None); 3]), vec![FocusSlot { row: 0, col: 0 }, FocusSlot { row: 1, col: 0 }, FocusSlot { row: 2, col: 0 }]);
        assert_eq!(
            focus_slots(&[(Some(5), None), (Some(5), None), (Some(7), None)]),
            vec![FocusSlot { row: 5, col: 0 }, FocusSlot { row: 5, col: 1 }, FocusSlot { row: 7, col: 0 }]
        );
    }

    const SKILLS: [(i32, i32); 7] = [(1, 0), (1, 1), (3, 0), (3, 1), (4, 0), (6, 1), (7, 0)];

    #[test]
    fn up_down_keep_the_column() {
        let s = slots(&SKILLS);
        let o = StepOpts::default();
        assert_eq!(focus_step(&s, 0, FocusDir::Down, o), Some(2));
        assert_eq!(focus_step(&s, 1, FocusDir::Down, o), Some(3));
        assert_eq!(focus_step(&s, 3, FocusDir::Down, o), Some(5));
        assert_eq!(focus_step(&s, 4, FocusDir::Down, o), Some(6));
        assert_eq!(focus_step(&s, 5, FocusDir::Down, o), Some(6));
        assert_eq!(focus_step(&s, 6, FocusDir::Down, o), None);
        assert_eq!(focus_step(&s, 6, FocusDir::Up, o), Some(4));
        assert_eq!(focus_step(&s, 5, FocusDir::Up, o), Some(3));
        assert_eq!(focus_step(&s, 0, FocusDir::Up, o), None);
        assert_eq!(focus_step(&s, 0, FocusDir::Right, o), Some(1));
        assert_eq!(focus_step(&s, 1, FocusDir::Right, o), None);
        assert_eq!(focus_step(&s, 1, FocusDir::Left, o), Some(0));
        assert_eq!(focus_step(&s, 4, FocusDir::Right, o), None);
    }

    #[test]
    fn columns_cross_sideways() {
        let s = slots(&SKILLS);
        let c = StepOpts { columns: true, keep_column: false };
        assert_eq!(focus_step(&s, 4, FocusDir::Right, c), Some(3));
        assert_eq!(focus_step(&s, 6, FocusDir::Right, c), Some(5));
        assert_eq!(focus_step(&s, 5, FocusDir::Left, c), Some(6));
        assert_eq!(focus_step(&s, 0, FocusDir::Right, c), Some(1));
        assert_eq!(focus_step(&s, 1, FocusDir::Right, c), None);
        assert_eq!(focus_wrap(&s, 1, FocusDir::Right, c), Some(0));
        assert_eq!(focus_wrap(&s, 5, FocusDir::Right, c), Some(6));
        assert_eq!(focus_wrap(&s, 4, FocusDir::Left, c), Some(3));
    }

    #[test]
    fn wraps() {
        let s = focus_slots(&[(Some(0), None), (Some(0), None), (Some(1), None)]);
        let o = StepOpts::default();
        assert_eq!(focus_wrap(&s, 2, FocusDir::Down, o), Some(0));
        assert_eq!(focus_wrap(&s, 0, FocusDir::Up, o), Some(2));
        assert_eq!(focus_wrap(&s, 1, FocusDir::Right, o), Some(0));
        assert_eq!(focus_wrap(&s, 0, FocusDir::Left, o), Some(1));
        assert_eq!(focus_wrap(&s, 2, FocusDir::Right, o), None);
        let t = focus_slots(&[(Some(0), None), (Some(1), None), (Some(200), Some(0)), (Some(200), Some(1))]);
        assert_eq!(focus_wrap(&t, 0, FocusDir::Up, o), Some(3));
        assert_eq!(focus_wrap(&t, 0, FocusDir::Up, StepOpts { keep_column: true, columns: false }), Some(2));
        assert_eq!(focus_wrap(&t, 3, FocusDir::Down, StepOpts { keep_column: true, columns: false }), Some(0));
    }

    #[test]
    fn pages() {
        let s = focus_slots(&[(None, None); 5]);
        assert_eq!(focus_page(&s, 0, 3), Some(3));
        assert_eq!(focus_page(&s, 3, 3), Some(4));
        assert_eq!(focus_page(&s, 4, 3), None);
        assert_eq!(focus_page(&s, 4, -10), Some(0));
    }

    #[test]
    fn nav_basics() {
        let mut nav = FocusNav::new();
        let mut no = item("No");
        no.cancel = true;
        nav.set(vec![item("Yes"), no], "prompt", FocusOptions::default(), None, false);
        assert_eq!(nav.info(), FocusInfo { label: Some("Yes".into()), cancel_label: Some("No".into()), index: 0, count: 2, ..Default::default() });
        assert!(nav.step(FocusDir::Down));
        assert!(nav.activate());
        assert_eq!(nav.take_fired(), vec![Act::send([Value::String("No".into())])]);
        assert!(nav.cancel());
        nav.set(vec![item("Wield"), item("Drop")], "popup", FocusOptions::default(), None, false);
        assert!(!nav.cancel());
    }

    #[test]
    fn edges_wrap_and_rebuilds() {
        let mut nav = FocusNav::new();
        nav.set(vec![item("Wield").at(0, 0), item("Drop").at(0, 1)], "popup", FocusOptions::default(), None, false);
        assert!(!nav.step(FocusDir::Up));
        assert!(!nav.step(FocusDir::Down));
        assert!(nav.step(FocusDir::Right));
        assert!(!nav.step(FocusDir::Right));
        nav.set(vec![item("a"), item("b")], "s", FocusOptions { wrap: true, ..Default::default() }, None, false);
        assert!(nav.step(FocusDir::Up));
        assert_eq!(nav.info().label.as_deref(), Some("b"));
        // the same screen keeps the item by id; another starts over
        nav.set(vec![item("Fighting").id("row:a"), item("Dodging").id("row:g")], "crt:skills", FocusOptions::default(), None, false);
        nav.step(FocusDir::Down);
        nav.set(vec![item("Dodging").id("row:g"), item("Fighting").id("row:a")], "crt:skills", FocusOptions::default(), None, false);
        assert_eq!(nav.index(), 0);
        nav.set(vec![item("Wield"), item("Drop")], "popup:describe-item", FocusOptions::default(), None, false);
        assert_eq!(nav.current().unwrap().label, "Wield");
        // the server's word wins when forced
        nav.set(vec![item("Fighter"), item("Wizard")], "popup:newgame-choice", FocusOptions::default(), Some(0), false);
        nav.set(vec![item("Fighter"), item("Wizard")], "popup:newgame-choice", FocusOptions::default(), Some(1), true);
        assert_eq!(nav.index(), 1);
        // linear
        nav.set(vec![item("Yes").at(0, 0), item("No").at(0, 1)], "p", FocusOptions { wrap: true, linear: true, ..Default::default() }, None, false);
        assert!(nav.step(FocusDir::Down));
        assert_eq!(nav.info().label.as_deref(), Some("No"));
        assert!(nav.step(FocusDir::Down));
        assert_eq!(nav.info().label.as_deref(), Some("Yes"));
        // nothing lit
        nav.set(vec![item("x")], "q", FocusOptions::default(), Some(-1), false);
        assert_eq!(nav.index(), -1);
        assert!(nav.step(FocusDir::Down));
        assert_eq!(nav.index(), 0);
        nav.set(vec![], "empty", FocusOptions::default(), None, false);
        assert_eq!(nav.info(), FocusInfo { index: -1, ..Default::default() });
        assert!(!nav.step(FocusDir::Down) && !nav.activate() && !nav.page(1));
    }
}

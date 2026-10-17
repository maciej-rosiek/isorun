//! The phone's Back as Escape (the port of `back-button.ts`).
//!
//! On the web, Back walks the page's history: the game pushes an entry of its own (`back`) over
//! one it marks (`game`), so a Back lands on `game`, which is taken as the game's Back
//! (`GameScreen.back`), and the entry is pushed again for the next one. Natively there is no
//! history: Android's Back reaches egui as `Key::BrowserBack` ([`back_pressed`]), which is the
//! game's Back directly.

/// Which of the game's two history entries a state is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackEntry {
    Game,
    Back,
}

/// What a Back did: taken as the game's own, a stale one (the game no longer wants it), or not ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackOutcome {
    Taken,
    Stale,
}

/// The page's history, as far as the Back button needs it.
pub trait History {
    fn state(&self) -> Option<BackEntry>;
    fn replace(&mut self, entry: BackEntry);
    fn push(&mut self, entry: BackEntry);
}

#[derive(Clone, Debug, Default)]
pub struct BackButton {
    armed: bool,
}

impl BackButton {
    /// The game is up on a touch screen: put the two entries in place.
    pub fn arm(&mut self, history: &mut dyn History, touch_screen: bool) {
        if self.armed || !touch_screen {
            return;
        }
        self.armed = true;
        if history.state() == Some(BackEntry::Back) {
            return;
        }
        history.replace(BackEntry::Game);
        history.push(BackEntry::Back);
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// A history move landed on `state`: the game's Back when it is the game's entry.
    pub fn take(&mut self, state: Option<BackEntry>, history: &mut dyn History, on_back: impl FnOnce()) -> Option<BackOutcome> {
        if state != Some(BackEntry::Game) {
            return None;
        }
        if !self.armed {
            return Some(BackOutcome::Stale);
        }
        on_back();
        history.push(BackEntry::Back);
        Some(BackOutcome::Taken)
    }
}

/// Whether the phone's Back was pressed this frame (Android's Back reaches egui as BrowserBack).
pub fn back_pressed(events: &[egui::Event]) -> bool {
    events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::BrowserBack, pressed: true, .. }))
}

/// The browser's history (`window.history`), with the entries as `{isorun: 'game' | 'back'}`.
#[cfg(target_arch = "wasm32")]
pub struct WebHistory;

#[cfg(target_arch = "wasm32")]
impl WebHistory {
    fn entry(e: BackEntry) -> wasm_bindgen::JsValue {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"isorun".into(), &(if e == BackEntry::Game { "game" } else { "back" }).into());
        o.into()
    }

    /// Which entry a history state (a popstate's) is.
    pub fn entry_of(state: &wasm_bindgen::JsValue) -> Option<BackEntry> {
        let v = js_sys::Reflect::get(state, &"isorun".into()).ok()?.as_string()?;
        match v.as_str() {
            "game" => Some(BackEntry::Game),
            "back" => Some(BackEntry::Back),
            _ => None,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl History for WebHistory {
    fn state(&self) -> Option<BackEntry> {
        let h = web_sys::window()?.history().ok()?;
        Self::entry_of(&h.state().ok()?)
    }
    fn replace(&mut self, entry: BackEntry) {
        if let Some(h) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = h.replace_state(&Self::entry(entry), "");
        }
    }
    fn push(&mut self, entry: BackEntry) {
        if let Some(h) = web_sys::window().and_then(|w| w.history().ok()) {
            let _ = h.push_state(&Self::entry(entry), "");
        }
    }
}

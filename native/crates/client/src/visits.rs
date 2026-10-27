//! Visit counts for the hosted page's `/stats` (the port of the TypeScript client's `visits.ts`
//! and `visit-beacon.ts`; the Worker that takes them is `worker/`): a beacon to `/api/visit` for a
//! visit, each game started, and a beat every 30 s while the window is shown, with the seconds a
//! game was on screen. One random id a run, no cookie. The web build sends them to its own
//! origin, the desktop and Android apps to the hosted page. Never sent from a debug build (as the
//! web build sent none from its dev server), nor with `ISORUN_NO_VISITS` set.
//!
//! The Worker records a POST only (`worker/index.ts`): on the web the beacon is
//! `navigator.sendBeacon`, which also goes out while the page is closing.

use std::cell::RefCell;
use std::rc::Rc;

pub const VISIT_PATH: &str = "/api/visit";
pub const BEAT_SECONDS: f64 = 30.0;
/// The most seconds one beat counts (the Worker clamps to the same, `MAX_BEAT_SECONDS`).
pub const MAX_BEAT_SECONDS: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitEvent {
    Visit,
    Game,
    Beat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    Play,
    Watch,
    Offline,
}

impl GameMode {
    fn word(self) -> &'static str {
        match self {
            GameMode::Play => "play",
            GameMode::Watch => "watch",
            GameMode::Offline => "offline",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisitBeacon {
    pub id: String,
    pub event: VisitEvent,
    pub mode: Option<GameMode>,
    pub server: Option<String>,
    pub seconds: u32,
}

/// The beacon's address under the page's origin (`visitUrl`; the Worker's `parseVisitBeacon`
/// reads it back).
pub fn visit_url(b: &VisitBeacon) -> String {
    let e = match b.event {
        VisitEvent::Visit => "visit",
        VisitEvent::Game => "game",
        VisitEvent::Beat => "beat",
    };
    let mut url = format!("{VISIT_PATH}?id={}&e={e}", b.id);
    if let Some(m) = b.mode {
        url.push_str(&format!("&m={}", m.word()));
    }
    // a server id is [a-z0-9-] (the Worker drops any other), so it needs no escaping
    if let Some(s) = &b.server {
        url.push_str(&format!("&s={s}"));
    }
    if b.seconds > 0 {
        url.push_str(&format!("&t={}", b.seconds));
    }
    url
}

/// 64 random bits: the browser's `Math.random` on the web (std's hasher keys are fixed there),
/// the per-process random keys of std's hasher elsewhere.
fn random_u64(salt: u64) -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = salt;
        let hi = (js_sys::Math::random() * 4294967296.0) as u64;
        let lo = (js_sys::Math::random() * 4294967296.0) as u64;
        (hi << 32) | lo
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.finish()
    }
}

/// A random version 4 UUID, the shape the Worker takes (`crypto.randomUUID`).
pub fn random_id() -> String {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&random_u64(1).to_le_bytes());
    bytes[8..].copy_from_slice(&random_u64(2).to_le_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The game on screen, for the beats: what kind, on which server (its id).
#[derive(Clone, Debug, PartialEq)]
pub struct GameOnScreen {
    pub mode: GameMode,
    pub server: String,
}

impl GameOnScreen {
    /// What a session's game is: a spectate, a game on this device, or one on a server.
    pub fn of(session: &crate::session::Session) -> Self {
        let mode = if session.watching() {
            GameMode::Watch
        } else if session.server.offline {
            GameMode::Offline
        } else {
            GameMode::Play
        };
        GameOnScreen { mode, server: session.server.id.clone() }
    }
}

struct State {
    id: String,
    /// Since when the seconds of the next beat are counted.
    counted_since: f64,
    last_beat: f64,
    started: bool,
    shown: bool,
    game: Option<GameOnScreen>,
    /// What was sent, in a test (nothing goes out then).
    #[cfg(test)]
    sent: Vec<VisitBeacon>,
}

impl State {
    /// A beat: the seconds since the last one, sent only when the window was shown through them
    /// (`beat(wasVisible)`); the count starts again either way.
    fn beat(&mut self, now: f64, was_shown: bool) {
        let seconds = (now - self.counted_since).round().max(0.0) as u32;
        self.counted_since = now;
        if !was_shown {
            return;
        }
        let game = self.game.clone();
        let secs = if game.is_some() { seconds.min(MAX_BEAT_SECONDS) } else { 0 };
        self.send(VisitEvent::Beat, game, secs);
    }

    fn send(&mut self, event: VisitEvent, game: Option<GameOnScreen>, seconds: u32) {
        let beacon = VisitBeacon { id: self.id.clone(), event, mode: game.as_ref().map(|g| g.mode), server: game.map(|g| g.server), seconds };
        #[cfg(test)]
        {
            self.sent.push(beacon);
        }
        #[cfg(not(test))]
        {
            if cfg!(debug_assertions) || std::env::var_os("ISORUN_NO_VISITS").is_some() {
                return;
            }
            crate::platform::http::beacon(&format!("{}{}", crate::servers::web_origin(), visit_url(&beacon)));
        }
    }
}

/// The visit of this run. `tick` once a frame; `game_started` when a game screen opens.
pub struct Visits {
    state: Rc<RefCell<State>>,
}

impl Default for Visits {
    fn default() -> Self {
        Self::new()
    }
}

impl Visits {
    pub fn new() -> Self {
        Self::at(crate::platform::seconds())
    }

    fn at(now: f64) -> Self {
        let state = State {
            id: random_id(),
            counted_since: now,
            last_beat: now,
            started: false,
            shown: true,
            game: None,
            #[cfg(test)]
            sent: Vec::new(),
        };
        Visits { state: Rc::new(RefCell::new(state)) }
    }

    /// Once a frame, with whether the window is shown (not minimised) and the game on screen:
    /// the first call sends the visit; then a beat every 30 s. A window hidden sends the beat
    /// so far; one shown again starts the count afresh (`visibilitychange`). On the web no frame
    /// runs while the tab is hidden, so the page's own events do that there (`listen`).
    pub fn tick(&mut self, shown: bool, game: Option<GameOnScreen>) {
        self.tick_at(crate::platform::seconds(), shown, game);
    }

    fn tick_at(&mut self, now: f64, shown: bool, game: Option<GameOnScreen>) {
        let first = {
            let mut s = self.state.borrow_mut();
            s.game = game;
            if !s.started {
                s.started = true;
                s.send(VisitEvent::Visit, None, 0);
                true
            } else {
                false
            }
        };
        if first {
            self.listen();
        }
        let mut s = self.state.borrow_mut();
        if s.shown != shown {
            s.shown = shown;
            s.beat(now, !shown);
        }
        if now - s.last_beat >= BEAT_SECONDS {
            s.last_beat = now;
            s.beat(now, shown);
        }
    }

    pub fn game_started(&mut self, game: GameOnScreen) {
        self.game_started_at(crate::platform::seconds(), game);
    }

    fn game_started_at(&mut self, now: f64, game: GameOnScreen) {
        let mut s = self.state.borrow_mut();
        s.counted_since = now;
        s.game = Some(game.clone());
        s.send(VisitEvent::Game, Some(game), 0);
    }

    /// The page's `visibilitychange` and `pagehide`, as visits.ts listens to them: a beat for the
    /// time shown when the tab is hidden or closed, a fresh count when it is shown again.
    #[cfg(all(target_arch = "wasm32", not(test)))]
    fn listen(&self) {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let Some(window) = web_sys::window() else { return };
        let Some(document) = window.document() else { return };
        let hidden = || web_sys::window().and_then(|w| w.document()).map(|d| d.hidden()).unwrap_or(false);
        let state = self.state.clone();
        let on_visibility = Closure::<dyn FnMut()>::new(move || {
            let now = crate::platform::seconds();
            let mut s = state.borrow_mut();
            let h = hidden();
            s.shown = !h;
            s.beat(now, h);
        });
        let state = self.state.clone();
        let on_pagehide = Closure::<dyn FnMut()>::new(move || {
            let now = crate::platform::seconds();
            let mut s = state.borrow_mut();
            let shown = !hidden();
            s.beat(now, shown);
        });
        let _ = document.add_event_listener_with_callback("visibilitychange", on_visibility.as_ref().unchecked_ref());
        let _ = window.add_event_listener_with_callback("pagehide", on_pagehide.as_ref().unchecked_ref());
        // for the page's whole life
        on_visibility.forget();
        on_pagehide.forget();
    }

    #[cfg(not(all(target_arch = "wasm32", not(test))))]
    fn listen(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(v: &Visits) -> Vec<String> {
        v.state.borrow().sent.iter().map(|b| visit_url(&VisitBeacon { id: "x".into(), ..b.clone() })).collect()
    }

    #[test]
    fn beacon_url() {
        let b = VisitBeacon { id: "x".into(), event: VisitEvent::Beat, mode: Some(GameMode::Offline), server: Some("offline".into()), seconds: 30 };
        assert_eq!(visit_url(&b), "/api/visit?id=x&e=beat&m=offline&s=offline&t=30");
        let b = VisitBeacon { id: "x".into(), event: VisitEvent::Visit, mode: None, server: None, seconds: 0 };
        assert_eq!(visit_url(&b), "/api/visit?id=x&e=visit");
    }

    #[test]
    fn ids_are_random_uuids() {
        let id = random_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_ne!(random_id(), id);
    }

    #[test]
    fn a_visit_then_a_beat_every_30_seconds_with_the_game_time() {
        let mut v = Visits::at(0.0);
        let game = GameOnScreen { mode: GameMode::Play, server: "cue".into() };
        v.tick_at(0.0, true, None);
        v.tick_at(10.0, true, None);
        v.game_started_at(12.0, game.clone());
        v.tick_at(29.0, true, Some(game.clone()));
        v.tick_at(30.0, true, Some(game.clone()));
        v.tick_at(60.0, true, None);
        assert_eq!(
            sent(&v),
            ["/api/visit?id=x&e=visit", "/api/visit?id=x&e=game&m=play&s=cue", "/api/visit?id=x&e=beat&m=play&s=cue&t=18", "/api/visit?id=x&e=beat"]
        );
    }

    #[test]
    fn a_hidden_window_sends_its_beat_so_far_and_counts_afresh_when_shown() {
        let mut v = Visits::at(0.0);
        let game = GameOnScreen { mode: GameMode::Offline, server: "offline".into() };
        v.tick_at(0.0, true, Some(game.clone()));
        v.tick_at(5.0, false, Some(game.clone()));
        // hidden through the 30 s beat: nothing sent
        v.tick_at(30.0, false, Some(game.clone()));
        v.tick_at(40.0, true, Some(game.clone()));
        v.tick_at(60.0, true, Some(game.clone()));
        assert_eq!(sent(&v), ["/api/visit?id=x&e=visit", "/api/visit?id=x&e=beat&m=offline&s=offline&t=5", "/api/visit?id=x&e=beat&m=offline&s=offline&t=20"]);
    }
}

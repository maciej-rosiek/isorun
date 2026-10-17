//! Visit counts (the port of `visits.ts` and `visit-beacon.ts`): a beacon to the hosted page's
//! `/api/visit` for a visit, each game started, and a beat every 30 s while the window is shown,
//! with the seconds a game was on screen. One random id a run, no cookie. Never sent from a debug
//! build, as the web build sends none from its dev server.

pub const VISIT_PATH: &str = "/api/visit";
pub const BEAT_SECONDS: f64 = 30.0;
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

/// The beacon's address under the hosted page.
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
    if let Some(s) = &b.server {
        url.push_str(&format!("&s={s}"));
    }
    if b.seconds > 0 {
        url.push_str(&format!("&t={}", b.seconds));
    }
    url
}

/// A random id in the shape the Worker takes (`crypto.randomUUID`).
pub fn random_id() -> String {
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (crate::splash::random() * 256.0) as u8 ^ (i as u8).wrapping_mul(37);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The game on screen, for the beats: what kind, on which server.
#[derive(Clone, Debug, PartialEq)]
pub struct GameOnScreen {
    pub mode: GameMode,
    pub server: String,
}

pub struct Visits {
    id: String,
    counted_since: f64,
    last_beat: f64,
    started: bool,
}

impl Default for Visits {
    fn default() -> Self {
        Self::new()
    }
}

impl Visits {
    pub fn new() -> Self {
        let now = isorun_client::platform::seconds();
        Visits { id: random_id(), counted_since: now, last_beat: now, started: false }
    }

    /// Once a frame: the first call sends the visit, then a beat every 30 s while `shown`.
    pub fn tick(&mut self, shown: bool, game: Option<GameOnScreen>) {
        let now = isorun_client::platform::seconds();
        if !self.started {
            self.started = true;
            self.send(VisitEvent::Visit, None, 0);
        }
        if now - self.last_beat < BEAT_SECONDS {
            return;
        }
        self.last_beat = now;
        let seconds = (now - self.counted_since).round() as u32;
        self.counted_since = now;
        if !shown {
            return;
        }
        let secs = if game.is_some() { seconds.min(MAX_BEAT_SECONDS) } else { 0 };
        self.send(VisitEvent::Beat, game, secs);
    }

    pub fn game_started(&mut self, game: GameOnScreen) {
        self.counted_since = isorun_client::platform::seconds();
        self.send(VisitEvent::Game, Some(game), 0);
    }

    fn send(&self, event: VisitEvent, game: Option<GameOnScreen>, seconds: u32) {
        let beacon = VisitBeacon { id: self.id.clone(), event, mode: game.as_ref().map(|g| g.mode), server: game.map(|g| g.server), seconds };
        if cfg!(debug_assertions) || std::env::var_os("ISORUN_NO_VISITS").is_some() {
            return;
        }
        let url = format!("{}{}", isorun_client::servers::HOSTED_ORIGIN, visit_url(&beacon));
        let _ = isorun_client::platform::http::get(&url, || {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_url() {
        let b = VisitBeacon { id: "x".into(), event: VisitEvent::Beat, mode: Some(GameMode::Offline), server: Some("offline".into()), seconds: 30 };
        assert_eq!(visit_url(&b), "/api/visit?id=x&e=beat&m=offline&s=offline&t=30");
        let b = VisitBeacon { id: "x".into(), event: VisitEvent::Visit, mode: None, server: None, seconds: 0 };
        assert_eq!(visit_url(&b), "/api/visit?id=x&e=visit");
        let id = random_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
    }
}

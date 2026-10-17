//! One connection to one server (the port of `session.ts`): the state the messages build, the
//! gamedata load (the messages wait while it is in flight, so none is lost), and the scene, built
//! again once a frame when the map or the player changed.

use crate::platform::http;
use crate::platform::net::{ConnEvent, Connection, RemoteTransport};
use crate::servers::{gamedata_base_for, get_token, set_token, ServerInfo};
use crate::tiles::{assemble, LoadedGamedata};
use isorun_scene::Scene;
use isorun_scene_webtiles::{build_scene, BuildOptions, BuildRecord, Dirty};
use isorun_webtiles::{cm, reduce, GameState};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A close with no reason of its own, in words (RFC 6455 §7.4.1).
pub fn close_word(code: u16) -> String {
    match code {
        1000 => "closed by the server".into(),
        1001 => "the server went away".into(),
        1005 => "closed without a reason".into(),
        1006 => "the connection dropped".into(),
        1011 => "the server hit an error".into(),
        1012 => "the server is restarting".into(),
        1013 => "the server is busy".into(),
        c => format!("code {c}"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GamedataStatus {
    Loading { done: usize, total: usize },
    Ready,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum SessionEvent {
    /// A message reached the state.
    State(Value),
    Scene,
    Gamedata(GamedataStatus),
    Closed(String),
    Open,
    Sent(Value),
}

/// A load in flight: the files' requests, then their decoding.
struct GamedataLoad {
    version: String,
    requests: Vec<(String, http::Request)>,
    files: HashMap<String, Vec<u8>>,
    total: usize,
    assembling: Option<Arc<Mutex<Option<Result<LoadedGamedata, String>>>>>,
    started: f64,
    progress_at: f64,
}

/// A load that makes no progress for this long is given up as stalled.
const LOAD_STALL_SECONDS: f64 = 30.0;

pub struct Session {
    pub server: ServerInfo,
    /// The account this connection is for; none while an account is being added.
    pub username: Option<String>,
    /// A token login is out and not yet answered.
    pub logging_in: bool,
    pub conn: Connection,
    pub state: GameState,
    pub gamedata: Option<Arc<LoadedGamedata>>,
    pub scene: Arc<Scene>,
    queue: Vec<Value>,
    load: Option<GamedataLoad>,
    scene_dirty: bool,
    last_scene_rev: (u64, u64),
    record: BuildRecord,
    pub diagnostics: Vec<String>,
    pub closed: bool,
    rc_asks: Vec<(String, Option<Arc<Mutex<Option<Option<String>>>>>)>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Session {
    pub fn new(server: ServerInfo, username: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let base = gamedata_base_for(&server);
        let transport: Box<dyn crate::platform::net::Transport> = if server.offline { crate::engine::connect(wake.clone()) } else { Box::new(RemoteTransport::new(&server.ws, wake.clone())) };
        Session {
            server,
            username,
            logging_in: false,
            conn: Connection::new(transport, base),
            state: GameState::default(),
            gamedata: None,
            scene: Arc::new(Scene::default()),
            queue: Vec::new(),
            load: None,
            scene_dirty: false,
            last_scene_rev: (u64::MAX, u64::MAX),
            record: BuildRecord::default(),
            diagnostics: Vec::new(),
            closed: false,
            rc_asks: Vec::new(),
            wake,
        }
    }

    pub fn diag(&mut self, text: impl Into<String>) {
        self.diagnostics.push(text.into());
        if self.diagnostics.len() > 100 {
            self.diagnostics.remove(0);
        }
    }

    pub fn send(&mut self, msg: Value) -> SessionEvent {
        self.conn.send(&msg);
        SessionEvent::Sent(msg)
    }

    pub fn close(&mut self) {
        self.conn.close();
    }

    /// Open the connection again after it dropped, as this same session; the state starts blank.
    pub fn reconnect(&mut self) -> bool {
        if !self.closed {
            return false;
        }
        self.state = GameState::default();
        self.scene = Arc::new(Scene::default());
        self.record = BuildRecord::default();
        self.last_scene_rev = (u64::MAX, u64::MAX);
        self.scene_dirty = false;
        self.queue.clear();
        self.closed = false;
        self.conn.reopen();
        true
    }

    /// Ask for the account's rc file for `game_id`; the answer lands in the returned slot.
    pub fn read_rc(&mut self, game_id: &str) -> Arc<Mutex<Option<Option<String>>>> {
        let slot = Arc::new(Mutex::new(None));
        if !self.conn.open || self.state.lobby.username.is_none() {
            *slot.lock().unwrap() = Some(None);
            return slot;
        }
        self.rc_asks.push((game_id.to_string(), Some(slot.clone())));
        self.conn.send(&cm::get_rc(game_id));
        slot
    }

    pub fn write_rc(&mut self, game_id: &str, contents: &str) {
        self.conn.send(&cm::set_rc(game_id, contents));
    }

    pub fn loading(&self) -> bool {
        self.load.is_some()
    }

    pub fn playing(&self) -> bool {
        self.state.phase == isorun_webtiles::Phase::Playing
    }

    pub fn watching(&self) -> bool {
        self.state.phase == isorun_webtiles::Phase::Watching || self.state.watching.is_some()
    }

    /// Everything that happened since the last frame: the socket's events, the messages applied,
    /// the gamedata load moved on.
    pub fn poll(&mut self) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        for e in self.conn.poll() {
            match e {
                ConnEvent::Open => {
                    let who = self.username.clone();
                    let token = who.as_ref().and_then(|w| if self.server.offline { Some(format!("offline:{w}")) } else { get_token(&self.server.id, w) });
                    if let (Some(token), Some(who)) = (token, who) {
                        out.push(self.send(cm::token_login(&token)));
                        self.logging_in = true;
                        // the server forgets a token as it is used; `login_cookie` brings the next one
                        set_token(&self.server.id, &who, None, None);
                    }
                    out.push(SessionEvent::Open);
                }
                ConnEvent::Message(m) => self.handle(m, &mut out),
                ConnEvent::Closed { code, reason } => {
                    self.closed = true;
                    self.logging_in = false;
                    for (_, slot) in self.rc_asks.drain(..) {
                        if let Some(slot) = slot {
                            *slot.lock().unwrap() = Some(None);
                        }
                    }
                    self.load = None;
                    self.queue.clear();
                    out.push(SessionEvent::Closed(if reason.is_empty() { close_word(code) } else { reason }));
                }
                ConnEvent::Diagnostic(d) => self.diag(d),
            }
        }
        self.progress_load(&mut out);
        out
    }

    fn handle(&mut self, m: Value, out: &mut Vec<SessionEvent>) {
        let kind = m.get("msg").and_then(Value::as_str).unwrap_or("").to_string();
        if kind == "rcfile_contents" && !self.rc_asks.is_empty() {
            let game = m.get("game_id").and_then(Value::as_str).unwrap_or("");
            let at = self.rc_asks.iter().position(|(g, _)| g == game).unwrap_or(0);
            let (_, slot) = self.rc_asks.remove(at);
            if let Some(slot) = slot {
                *slot.lock().unwrap() = Some(Some(m.get("contents").and_then(Value::as_str).unwrap_or("").to_string()));
            }
            return;
        }
        if kind == "login_success" || kind == "login_fail" {
            self.logging_in = false;
        }
        if kind == "login_success" {
            if let Some(u) = m.get("username").and_then(Value::as_str).filter(|u| !u.is_empty()) {
                self.username = Some(u.to_string());
            }
            // a token, so the password is never stored
            out.push(self.send(cm::set_login_cookie()));
        }
        if kind == "login_cookie" {
            let who = self.state.lobby.username.clone().or(self.username.clone());
            if let (Some(who), Some(cookie)) = (who, m.get("cookie").and_then(Value::as_str)) {
                set_token(&self.server.id, &who, Some(cookie), m.get("expires").and_then(Value::as_f64));
            }
        }
        if kind == "login_fail" {
            if let Some(who) = &self.username {
                set_token(&self.server.id, who, None, None);
            }
        }
        if kind == "logout" {
            if let Some(who) = self.state.lobby.username.clone().or(self.username.clone()) {
                set_token(&self.server.id, &who, None, None);
            }
        }
        if kind == "game_client" {
            reduce(&mut self.state, &m);
            out.push(SessionEvent::State(m.clone()));
            let version = m.get("version").and_then(Value::as_str).unwrap_or("").to_string();
            let have = self.gamedata.as_ref().is_some_and(|g| g.version == version);
            let loading = self.load.as_ref().is_some_and(|l| l.version == version);
            if !have && !loading {
                self.start_load(version, out);
            }
            return;
        }
        if self.load.is_some() {
            self.queue.push(m);
            return;
        }
        self.apply(m, out);
    }

    fn apply(&mut self, m: Value, out: &mut Vec<SessionEvent>) {
        reduce(&mut self.state, &m);
        if matches!(m.get("msg").and_then(Value::as_str), Some("map" | "player" | "game_client" | "go_lobby")) {
            self.scene_dirty = true;
        }
        out.push(SessionEvent::State(m));
    }

    fn start_load(&mut self, version: String, out: &mut Vec<SessionEvent>) {
        let base = self.conn.gamedata_base.clone();
        let files = isorun_gamedata::gamedata_files();
        let now = crate::platform::seconds();
        let mut load = GamedataLoad { version: version.clone(), requests: Vec::new(), files: HashMap::new(), total: files.len(), assembling: None, started: now, progress_at: now };
        if self.server.offline {
            // the engine's files are on this device
            for f in &files {
                match crate::engine::gamedata_file(&version, f) {
                    Ok(bytes) => {
                        load.files.insert(f.clone(), bytes);
                    }
                    Err(e) => {
                        out.push(SessionEvent::Gamedata(GamedataStatus::Error(e)));
                        return;
                    }
                }
            }
        } else {
            let root = isorun_gamedata::gamedata_root(&base, &version);
            for f in &files {
                let wake = self.wake.clone();
                load.requests.push((f.clone(), http::get(&format!("{root}/{f}"), move || wake())));
            }
        }
        self.load = Some(load);
        out.push(SessionEvent::Gamedata(GamedataStatus::Loading { done: 0, total: files.len() }));
    }

    fn progress_load(&mut self, out: &mut Vec<SessionEvent>) {
        let Some(load) = self.load.as_mut() else { return };
        let now = crate::platform::seconds();
        let mut failed = None;
        let before = load.files.len();
        load.requests.retain(|(name, request)| match request.take() {
            Some(Ok(bytes)) => {
                load.files.insert(name.clone(), bytes);
                false
            }
            Some(Err(e)) => {
                failed = Some(e);
                false
            }
            None => true,
        });
        if load.files.len() != before {
            load.progress_at = now;
            out.push(SessionEvent::Gamedata(GamedataStatus::Loading { done: load.files.len(), total: load.total }));
        }
        let mut result = None;
        if let Some(e) = failed {
            result = Some(Err(e));
        } else if now - load.progress_at > LOAD_STALL_SECONDS && load.assembling.is_none() {
            result = Some(Err(format!("no progress in {}s", LOAD_STALL_SECONDS)));
        } else if load.assembling.is_some() || load.requests.is_empty() && load.files.len() == load.total {
            match &load.assembling {
                None => {
                    let slot = Arc::new(Mutex::new(None));
                    load.assembling = Some(slot.clone());
                    let files = std::mem::take(&mut load.files);
                    let version = load.version.clone();
                    let wake = self.wake.clone();
                    let work = move || {
                        let r = assemble(&version, files);
                        *slot.lock().unwrap() = Some(r);
                        wake();
                    };
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = std::thread::Builder::new().name("gamedata".into()).spawn(work);
                    }
                    #[cfg(target_arch = "wasm32")]
                    work();
                }
                Some(slot) => {
                    if let Some(r) = slot.lock().unwrap().take() {
                        result = Some(r);
                    }
                }
            }
        }
        let Some(result) = result else { return };
        let _ = load.started;
        self.load = None;
        match result {
            Ok(gd) => {
                self.gamedata = Some(Arc::new(gd));
                out.push(SessionEvent::Gamedata(GamedataStatus::Ready));
            }
            Err(e) => {
                self.diag(format!("gamedata load failed: {e}"));
                out.push(SessionEvent::Gamedata(GamedataStatus::Error(e)));
            }
        }
        for m in std::mem::take(&mut self.queue) {
            self.apply(m, out);
        }
        if self.flush_scene() {
            out.push(SessionEvent::Scene);
        }
    }

    /// Build the scene again if the map or the player changed. Call once a frame.
    pub fn flush_scene(&mut self) -> bool {
        if !self.scene_dirty {
            return false;
        }
        self.scene_dirty = false;
        let Some(gd) = self.gamedata.clone() else { return false };
        let r = (self.state.rev.map, self.state.rev.player);
        if r == self.last_scene_rev {
            return false;
        }
        self.last_scene_rev = r;
        let scene = build_scene(&self.state, &gd.gd, BuildOptions { previous: Some(&self.scene), dirty: Some(Dirty::of(&self.state)), record: Some(&mut self.record) });
        self.state.clear_dirty();
        self.scene = Arc::new(scene);
        true
    }
}

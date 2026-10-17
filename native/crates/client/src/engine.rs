//! The offline engine: crawl on this device, with no server (the port of `packages/offline`
//! `server.ts`). The [`OfflineServer`] is the lobby and the relay a server's `ws_handler.py` and
//! `process_handler.py` are, a [`Transport`] like a server's socket; how crawl runs is the
//! platform's:
//!
//! - Linux, Android, macOS: a child process on crawl's own WebTiles socket, a Unix datagram socket
//!   (`unix`).
//! - Windows: a child process on its stdin and stdout, which
//!   `engine/patches/13-windows-webtiles.patch` gives crawl there.
//! - The web: the WebAssembly crawl of `engine/dist` in a Web Worker, run by
//!   `native/web/engine/worker.ts` through the page's `isorunEngine` module (`native/web`;
//!   `web`).
//!
//! An engine natively is a folder (`engine.json`, the `crawl` binary, its `dat`, `docs` and
//! `settings`, and the client's `gamedata/<version>/`, as `engine/native.sh` stages it):
//! `ISORUN_ENGINE_DIR`, `engine/` beside the executable, on Android `engine/` in the app's files
//! (unpacked from the APK's assets, the binary in the native library folder), or in a checkout
//! the native build in `engine/.work/stable`.

use crate::platform::net::{ConnEvent, Transport};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The option defaults of a server build (initfile.cc `USING_DGL`) that a player notices: travel
/// and rest arrive at their end in one update, and a travel leaves its trail.
const SERVER_DEFAULTS: [&str; 3] = ["travel_delay=-1", "rest_delay=-1", "show_travel_trail=true"];

/// A token names its profile and lasts ten years: the account is only ever this device's.
const TOKEN_PREFIX: &str = "offline:";
const TOKEN_DAYS: i64 = 3650;

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const SAVE_NOTE_KEY: &str = "isorun.offline.save:";
const RC_KEY: &str = "isorun.offline.rc:";

/// A profile's name from what the login form was given: what crawl accepts in a name, no longer
/// than a server lets an account be. It names the profile's save folder too.
pub fn profile_name(name: &str) -> Option<String> {
    let n: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(20).collect();
    (!n.is_empty()).then_some(n)
}

/// A character the way a server's lobby names a save: "Kai, a level 3 Minotaur Berserker of Trog".
pub fn save_desc(f: &Value) -> Option<String> {
    let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if s("name").is_empty() || s("xl").is_empty() || s("race").is_empty() {
        return None;
    }
    let kind = [s("race"), s("cls")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    let god = s("god");
    Some(format!("{}, a level {} {kind}{}", s("name"), s("xl"), if god.is_empty() { String::new() } else { format!(" of {god}") }))
}

fn escape_html(s: &str) -> String {
    s.chars().map(|c| if "&<>\"'".contains(c) { format!("&#{};", c as u32) } else { c.to_string() }).collect()
}

/// A stored text: JSON text, as this app keeps values, or the raw text the web build's
/// `localStorage` holds for the same key (`rc.ts`, `saves.ts`), so both builds read each other's.
fn stored_text(raw: String) -> String {
    serde_json::from_str::<String>(&raw).unwrap_or(raw)
}

/// The profile's rc file, one for every game (`rc.ts`).
pub fn read_rc(profile: &str) -> String {
    crate::platform::storage::get_raw(&format!("{RC_KEY}{}", profile.to_lowercase())).map(stored_text).unwrap_or_default()
}

pub fn write_rc(profile: &str, contents: &str) {
    let key = format!("{RC_KEY}{}", profile.to_lowercase());
    if contents.is_empty() {
        crate::platform::storage::set_raw(&key, None);
    } else if cfg!(target_arch = "wasm32") {
        // raw, as the web build keeps it on the same origin
        crate::platform::storage::set_raw(&key, Some(contents));
    } else {
        crate::platform::storage::set_raw(&key, Some(&serde_json::to_string(contents).unwrap_or_default()));
    }
}

/// One engine build, as the lobby offers it.
#[derive(Clone, Debug)]
pub struct Build {
    /// The gamedata version `game_client` announces: the crawl commit.
    pub version: String,
    /// crawl's own version string, `0.34.1-4-g0e95e087e2`.
    pub crawl_version: String,
}

impl Build {
    /// The release, `0.34`, or `trunk` (channels.ts `slotOf`).
    pub fn slot(&self) -> String {
        let v = &self.crawl_version;
        let mut parts = v.split(|c: char| !c.is_ascii_digit());
        match (parts.next(), parts.next()) {
            (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() && !v.contains("-a0") => format!("{a}.{b}"),
            _ => "trunk".into(),
        }
    }
    pub fn game_id(&self) -> String {
        format!("offline-{}", self.slot())
    }
    pub fn label(&self) -> String {
        let s = self.slot();
        if s == "trunk" { "DCSS trunk".into() } else { format!("DCSS {s}") }
    }
}

/// What crawl sent: a flush of its output (newline-ended JSON lines), or a failure of the channel.
enum Output {
    Text(String),
    Error(String),
}

/// How a running crawl is asked to stop with its game saved.
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum Hangup {
    /// It saves and exits on its own, as on a server's SIGHUP.
    Exits,
    /// It saves where it stands and says so with a `*checkpoint` line, and is then terminated (the
    /// web build, which never exits on its own: OfflineServer.stop in server.ts).
    Checkpoint,
}

/// A running crawl, however it runs.
trait Process {
    /// A control message, as a server sends it down the socket.
    fn control(&mut self, json: &str);
    /// Typed text, as a server writes it to the terminal.
    fn keys(&mut self, text: &str) {
        self.control(&json!({ "msg": "text_input", "text": text }).to_string());
    }
    fn poll(&mut self, out: &mut Vec<Output>);
    /// Its exit code, once it has exited.
    fn exited(&mut self) -> Option<i32>;
    fn hangup(&mut self) -> Hangup;
    fn kill(&mut self);
    /// Wait (natively) up to `seconds` for it to exit after a hangup: the app saves on its way
    /// out. The web cannot wait; its worker is retired in the background (`retire`).
    fn wait_exit(&mut self, _seconds: f64) {}
    /// The connection is going and the process with it: on the web a stop's save still finishes.
    fn retire(&mut self) {
        self.kill();
    }
    /// What it left behind when it stopped badly: how it stopped (a signal), what it wrote to its
    /// error output and crawl's own crash file. None when there is nothing to tell.
    fn crash_report(&mut self) -> Option<String> {
        None
    }
}

/// The line of a crash report the dialog shows: the first that says something. crawl's crash
/// file and its error output open with the error itself ("ASSERT(...) in 'x.cc' at line N
/// failed", "Cannot find ..."), or with the signal line this side writes.
fn crash_headline(report: &str) -> Option<String> {
    let line = report.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("---") && !l.starts_with("==="))?;
    let mut cut: String = line.chars().take(160).collect();
    if cut.len() < line.len() {
        cut.push('\u{2026}');
    }
    Some(cut)
}

/// How long a stop waits for crawl to save after its hangup before it is killed.
const STOP_SAVE: f64 = 5.0;

struct Game {
    process: Box<dyn Process>,
    /// The key its save's words are kept under (`saves.ts`).
    save_dir: String,
    partial: String,
    queueing: bool,
    pending: Vec<Value>,
    exit_reason: Option<(String, Option<String>)>,
    stopping: Option<(f64, Hangup)>,
    /// The messages crawl sent before the client was told its gamedata (the web fetches it first).
    held: Option<Vec<Value>>,
    /// Whether crawl has drawn a map yet (logged once: the log tells how far a game got).
    mapped: bool,
}

/// The lobby and the relay around crawl (server.ts `OfflineServer`).
pub struct OfflineServer {
    wake: Wake,
    greeted: bool,
    /// What goes to the client; filled from callbacks too (the web's save info).
    out: Arc<Mutex<Vec<ConnEvent>>>,
    user: String,
    logged_in: bool,
    game: Option<Game>,
    closed: bool,
}

impl OfflineServer {
    pub fn new(wake: Wake) -> Self {
        OfflineServer { wake, greeted: false, out: Arc::default(), user: crate::servers::DEFAULT_PROFILE.into(), logged_in: false, game: None, closed: false }
    }

    fn push(&self, e: ConnEvent) {
        if let Ok(mut o) = self.out.lock() {
            o.push(e);
        }
    }

    fn emit(&mut self, m: Value) {
        match self.game.as_mut().and_then(|g| g.held.as_mut()) {
            Some(held) if is_game_output(&m) => held.push(m),
            _ => self.push(ConnEvent::Message(m)),
        }
    }

    fn diag(&mut self, text: String) {
        self.push(ConnEvent::Diagnostic(text));
    }

    /// The lobby's "Play now" line (game_links.html): the build's link, on the save in brackets
    /// after its name when the profile has one waiting.
    fn send_game_links(&mut self) {
        let Some(b) = host::build() else { return };
        let href = format!("#play-{}", b.game_id());
        let (out, wake) = (self.out.clone(), self.wake.clone());
        let label = b.label();
        host::save_info(
            &b,
            &self.user,
            Box::new(move |save: Option<String>| {
                let content = match save {
                    Some(save) => format!("<span>{} <span><a href=\"{href}\">[{}]</a></span></span>", escape_html(&label), escape_html(&save)),
                    None => format!("<span><a href=\"{href}\">{}</a></span>", escape_html(&label)),
                };
                if let Ok(mut o) = out.lock() {
                    o.push(ConnEvent::Message(json!({ "msg": "set_game_links", "content": content })));
                }
                wake();
            }),
        );
    }

    fn receive(&mut self, msg: Value) {
        let kind = msg.get("msg").and_then(Value::as_str).unwrap_or("").to_string();
        match kind.as_str() {
            "login" | "token_login" => {
                let name = if kind == "login" {
                    msg.get("username").and_then(Value::as_str).map(str::to_string)
                } else {
                    msg.get("cookie").and_then(Value::as_str).and_then(|c| c.strip_prefix(TOKEN_PREFIX)).map(str::to_string)
                };
                if let Some(n) = name.as_deref().and_then(profile_name) {
                    self.user = n;
                }
                self.logged_in = true;
                let user = self.user.clone();
                self.emit(json!({ "msg": "login_success", "username": user }));
                self.send_game_links();
                return;
            }
            "set_login_cookie" => {
                let cookie = format!("{TOKEN_PREFIX}{}", self.user);
                self.emit(json!({ "msg": "login_cookie", "cookie": cookie, "expires": TOKEN_DAYS }));
                return;
            }
            "register" => {
                self.emit(json!({ "msg": "register_fail", "reason": "Offline play needs no account." }));
                return;
            }
            "get_rc" => {
                let contents = read_rc(&self.user);
                self.emit(json!({ "msg": "rcfile_contents", "game_id": msg.get("game_id").cloned().unwrap_or(Value::Null), "contents": contents }));
                return;
            }
            "set_rc" => {
                if let Some(c) = msg.get("contents").and_then(Value::as_str) {
                    write_rc(&self.user, c);
                }
                return;
            }
            "play" => {
                let id = msg.get("game_id").and_then(Value::as_str).unwrap_or("").to_string();
                self.play(&id);
                return;
            }
            "go_lobby" => {
                if self.game.is_some() {
                    self.stop();
                } else {
                    self.emit(json!({ "msg": "go_lobby" }));
                }
                return;
            }
            "pong" | "watch" | "chat_msg" | "forget_login_cookie" | "force_terminate" => return,
            _ => {}
        }
        let Some(g) = &mut self.game else { return };
        // process_handler.py handle_input: `input` goes to the terminal, which here is the
        // socket's text_input; every other message goes down the socket as it is
        if kind == "input" {
            let mut text: String = msg.get("data").and_then(Value::as_array).map(|a| a.iter().filter_map(|b| b.as_u64()).map(|b| b as u8 as char).collect()).unwrap_or_default();
            text.push_str(msg.get("text").and_then(Value::as_str).unwrap_or(""));
            if !text.is_empty() {
                g.process.keys(&text);
            }
        } else {
            g.process.control(&msg.to_string());
        }
    }

    fn play(&mut self, id: &str) {
        if self.game.is_some() {
            return;
        }
        let Some(b) = host::build().filter(|b| b.game_id() == id) else {
            self.emit(json!({ "msg": "game_ended", "reason": "error", "message": format!("No offline game \"{id}\".") }));
            self.emit(json!({ "msg": "go_lobby" }));
            return;
        };
        // read before the rc file, so the player's own options win
        let mut extra: Vec<String> = Vec::new();
        for opt in SERVER_DEFAULTS {
            extra.push("-extra-opt-first".into());
            extra.push(opt.into());
        }
        let rc = read_rc(&self.user);
        match host::launch(&b, &self.user, extra, &rc, self.wake.clone()) {
            Ok((mut process, save_dir)) => {
                log::info!("the offline game started ({}), saves in {save_dir}", b.label());
                // the socket connection a server makes to the new process (connection.py)
                process.control(&json!({ "msg": "attach", "primary": true }).to_string());
                // A server sends game_client when crawl names its client_path, which only an
                // installed build does. The build's gamedata is known before the engine starts, so
                // it goes out now (natively) or once it is fetched (the web), and the client loads
                // it while the engine boots.
                let ready = host::gamedata_ready(&b.version, self.wake.clone());
                self.game = Some(Game { process, save_dir, partial: String::new(), queueing: false, pending: Vec::new(), exit_reason: None, stopping: None, held: None, mapped: false });
                self.emit(json!({ "msg": "game_started" }));
                if ready {
                    self.emit(json!({ "msg": "game_client", "version": b.version, "content": "" }));
                } else if let Some(g) = &mut self.game {
                    g.held = Some(Vec::new());
                }
            }
            Err(e) => {
                log::error!("the offline engine did not start: {e}");
                self.emit(json!({ "msg": "game_ended", "reason": "error", "message": format!("The engine did not start: {e}") }));
                self.emit(json!({ "msg": "go_lobby" }));
            }
        }
    }

    /// End the game with its state saved, as a server's SIGHUP does.
    fn stop(&mut self) {
        let Some(g) = &mut self.game else { return };
        if g.stopping.is_none() {
            let how = g.process.hangup();
            g.stopping = Some((crate::platform::seconds(), how));
        }
    }

    fn pump(&mut self) {
        // the web's gamedata, fetched while crawl boots
        if let Some(g) = &mut self.game {
            if g.held.is_some() {
                match host::gamedata_status() {
                    Some(Ok(version)) => {
                        let held = g.held.take().unwrap_or_default();
                        self.push(ConnEvent::Message(json!({ "msg": "game_client", "version": version, "content": "" })));
                        for m in held {
                            self.push(ConnEvent::Message(m));
                        }
                    }
                    Some(Err(e)) => {
                        g.held = None;
                        g.exit_reason = Some(("error".into(), Some(format!("The game's files did not load: {e}"))));
                        g.process.kill();
                        self.ended(-1);
                        return;
                    }
                    None => {}
                }
            }
        }
        let Some(g) = &mut self.game else { return };
        let mut chunks = Vec::new();
        g.process.poll(&mut chunks);
        let mut lines = Vec::new();
        let mut errors = Vec::new();
        for o in chunks {
            match o {
                Output::Text(t) => {
                    let all = std::mem::take(&mut g.partial) + &t;
                    let mut parts: Vec<&str> = all.split('\n').collect();
                    g.partial = parts.pop().unwrap_or("").to_string();
                    lines.extend(parts.into_iter().filter(|l| !l.is_empty()).map(str::to_string));
                }
                Output::Error(e) => errors.push(e),
            }
        }
        for e in errors {
            self.diag(e);
        }
        for line in lines {
            self.line(&line);
            if self.game.is_none() {
                return;
            }
        }
        let Some(g) = &mut self.game else { return };
        let exited = g.process.exited();
        let timed_out = g.stopping.is_some_and(|(t, _)| crate::platform::seconds() - t > STOP_SAVE);
        if timed_out {
            g.process.kill();
            g.exit_reason.get_or_insert(("error".into(), Some("The game could not be saved before it stopped.".into())));
        }
        if let Some(code) = exited.or(timed_out.then_some(-1)) {
            self.ended(code);
        }
    }

    fn line(&mut self, line: &str) {
        let (special, body) = match line.strip_prefix('*') {
            Some(b) => (true, b),
            None => (false, line),
        };
        let m: Value = match serde_json::from_str(body) {
            Ok(m) => m,
            Err(_) => {
                let cut: String = line.chars().take(200).collect();
                self.diag(format!("offline engine sent a line that is not JSON: {cut}"));
                return;
            }
        };
        let Some(g) = &mut self.game else { return };
        if special {
            match m.get("msg").and_then(Value::as_str).unwrap_or("") {
                "flush_messages" => {
                    g.queueing = true;
                    let batch = std::mem::take(&mut g.pending);
                    for m in batch {
                        self.emit(m);
                    }
                }
                "exit_reason" => {
                    g.exit_reason = Some((m.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string(), m.get("message").and_then(Value::as_str).map(str::to_string)));
                }
                "milestone" => host::note_save(&g.save_dir, &m),
                // a stop's save is done: the web's engine is terminated with its game saved
                "checkpoint" => {
                    if g.stopping.is_some_and(|(_, how)| how == Hangup::Checkpoint) {
                        g.process.kill();
                        g.exit_reason = Some(("saved".into(), None));
                        self.ended(0);
                    }
                }
                _ => {}
            }
            return;
        }
        if !g.mapped && m.get("msg").and_then(Value::as_str) == Some("map") {
            g.mapped = true;
            log::info!("the offline game drew its first map");
        }
        if g.queueing {
            g.pending.push(m);
        } else {
            self.emit(m);
        }
    }

    fn ended(&mut self, code: i32) {
        let Some(mut g) = self.game.take() else { return };
        let mut batch: Vec<Value> = g.held.take().unwrap_or_default();
        batch.append(&mut g.pending);
        for m in batch {
            self.push(ConnEvent::Message(m));
        }
        let (kind, mut message) = match (g.stopping.is_some(), g.exit_reason.take()) {
            // a hangup's save ends the game as crawl's own "saved"
            (true, Some((k, m))) if k != "unknown" && k != "error" => (k, m),
            (true, None) | (true, Some(_)) if code == 0 => ("saved".into(), None),
            (_, Some((k, m))) if k != "unknown" => (k, m),
            _ => (if code == 0 { "quit".into() } else { "crash".into() }, None),
        };
        // a crash or an error goes to the log whole (logcat on Android, where nothing else shows
        // it), and the dialog shows its first line when crawl said nothing itself
        if kind == "crash" || kind == "error" {
            if let Some(report) = g.process.crash_report() {
                log::error!("the offline game stopped ({kind}, exit {code}):\n{report}");
                if message.as_deref().is_none_or(|m| m.trim().is_empty()) {
                    message = crash_headline(&report);
                }
            } else {
                log::error!("the offline game stopped ({kind}, exit {code}) with no report");
            }
        }
        log::info!("the offline game ended: {kind}{}", message.as_deref().map(|m| format!(" ({})", m.trim())).unwrap_or_default());
        let mut ended = json!({ "msg": "game_ended", "reason": kind });
        if let Some(m) = message {
            ended["message"] = m.into();
        }
        self.emit(json!({ "msg": "go_lobby" }));
        self.emit(ended);
        drop(g);
        if self.logged_in {
            self.send_game_links();
        }
    }
}

/// What crawl itself says, held back until the client has its gamedata; the lobby's own
/// messages are not.
fn is_game_output(m: &Value) -> bool {
    !matches!(m.get("msg").and_then(Value::as_str), Some("game_started" | "game_client" | "game_ended" | "go_lobby" | "set_game_links" | "login_success" | "login_cookie" | "rcfile_contents" | "register_fail"))
}

impl Transport for OfflineServer {
    fn send(&mut self, text: String) {
        if self.closed {
            return;
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(m) => self.receive(m),
            Err(e) => self.diag(format!("offline: a message that is not JSON: {e}")),
        }
    }

    fn close(&mut self) {
        self.stop();
        // the game saves on its own time; the app waits for it on the way out (shutdown)
        if let Some(mut g) = self.game.take() {
            g.process.wait_exit(STOP_SAVE);
            g.process.retire();
        }
        self.closed = true;
        self.push(ConnEvent::Closed { code: 1000, reason: String::new() });
    }

    fn poll(&mut self, out: &mut Vec<ConnEvent>) {
        if !self.greeted && !self.closed {
            self.greeted = true;
            out.push(ConnEvent::Open);
            self.emit(json!({ "msg": "lobby_clear" }));
            self.emit(json!({ "msg": "lobby_complete" }));
        }
        self.pump();
        if let Ok(mut o) = self.out.lock() {
            out.append(&mut o);
        }
        if self.game.as_ref().is_some_and(|g| g.stopping.is_some() || g.held.is_some()) {
            (self.wake)();
        }
    }

    fn reopen(&mut self) {
        self.closed = false;
        self.greeted = false;
    }
}

impl Drop for OfflineServer {
    fn drop(&mut self) {
        if self.game.is_some() && !self.closed {
            self.close();
        }
    }
}

/// Whether this build plays offline: the engine is on this device (or, on the web, served beside
/// the page and its pointer read: [`init`]).
pub fn available() -> bool {
    host::build().is_some()
}

/// Offline gamedata is read from the engine (or the page's memory), not fetched by the session.
pub fn gamedata_base() -> String {
    "offline:".into()
}

pub fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
    host::gamedata_file(version, file)
}

pub fn connect(wake: Wake) -> Box<dyn Transport> {
    if host::build().is_none() {
        return Box::new(Missing { told: false });
    }
    Box::new(OfflineServer::new(wake))
}

/// Find the engine before the app starts. Natively that is a look at the disk; on the web the
/// page's engine pointer is fetched, so call it before the front end asks [`available`].
pub async fn init() {
    #[cfg(target_arch = "wasm32")]
    web::init().await;
}

/// A connection where this build has no engine: it closes at once, saying so.
struct Missing {
    told: bool,
}

impl Transport for Missing {
    fn send(&mut self, _text: String) {}
    fn close(&mut self) {}
    fn poll(&mut self, out: &mut Vec<ConnEvent>) {
        if !self.told {
            self.told = true;
            out.push(ConnEvent::Closed { code: 1011, reason: "this build has no offline engine".into() });
        }
    }
    fn reopen(&mut self) {
        self.told = false;
    }
}

type SaveReply = Box<dyn FnOnce(Option<String>) + Send>;

#[cfg(not(target_arch = "wasm32"))]
use native as host;
#[cfg(target_arch = "wasm32")]
use web as host;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{install, Install};
#[cfg(target_arch = "wasm32")]
pub use web::{install, Install};

/// The engine build this device plays, if any.
pub fn build() -> Option<Build> {
    host::build()
}

/// The desktop and Android: crawl as a child process.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::servers::DEFAULT_PROFILE;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    /// One engine build on this device.
    #[derive(Clone, Debug)]
    pub struct Install {
        /// Where crawl runs: its `dat`, `docs` and `settings` are here.
        pub dir: PathBuf,
        pub crawl: PathBuf,
        /// The client's files for this build (`gamedata/<version>` of a server).
        pub gamedata: PathBuf,
        pub build: Build,
    }

    fn read_install(dir: &Path) -> Option<Install> {
        let info: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("engine.json")).ok()?).ok()?;
        let version = info.get("gamedata").or(info.get("commit"))?.as_str()?.to_string();
        let crawl = crawl_binary(dir);
        let gamedata = dir.join("gamedata").join(&version);
        (crawl.exists() && gamedata.exists()).then(|| Install { dir: dir.to_path_buf(), crawl, gamedata, build: Build { crawl_version: info.get("version").and_then(Value::as_str).unwrap_or(&version).to_string(), version } })
    }

    /// Where an install's binary is. Android runs no file from the app's data (its folder is
    /// mounted noexec), only from the native library folder the APK's `lib/<abi>/` is unpacked
    /// into: crawl is shipped there as `libcrawl.so`, beside this library.
    fn crawl_binary(dir: &Path) -> PathBuf {
        #[cfg(target_os = "android")]
        if let Some(lib) = own_library_dir() {
            let _ = dir;
            return lib.join("libcrawl.so");
        }
        dir.join(if cfg!(windows) { "crawl.exe" } else { "crawl" })
    }

    /// The folder this library was loaded from (the APK's native library folder).
    #[cfg(target_os = "android")]
    fn own_library_dir() -> Option<PathBuf> {
        let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
        let here = own_library_dir as fn() -> Option<PathBuf> as *const libc::c_void;
        if unsafe { libc::dladdr(here, &mut info) } == 0 || info.dli_fname.is_null() {
            return None;
        }
        let path = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) }.to_string_lossy().into_owned();
        Path::new(&path).parent().map(Path::to_path_buf)
    }

    /// A checkout's native build (`engine/native.sh`, in `engine/.work/stable`).
    fn checkout_install() -> Option<Install> {
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine");
        // the staged folder first: it is what a package ships
        if let Some(i) = read_install(&engine.join("dist/native/host/engine")) {
            return Some(i);
        }
        let source = engine.join(".work/stable/crawl-ref/source");
        let crawl = source.join("crawl");
        if !crawl.exists() {
            return None;
        }
        let commit = std::fs::read_to_string(engine.join(".work/stable.commit")).ok()?.trim().to_string();
        let info: Value = std::fs::read_to_string(engine.join("dist/stable/engine.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let crawl_version = info.get("version").and_then(Value::as_str).unwrap_or("0.34").to_string();
        Some(Install { dir: source.clone(), crawl, gamedata: source.join("webserver/game_data/static"), build: Build { version: commit, crawl_version } })
    }

    /// The engine on this device, if there is one.
    pub fn install() -> Option<&'static Install> {
        static INSTALL: OnceLock<Option<Install>> = OnceLock::new();
        INSTALL
            .get_or_init(|| {
                if let Some(d) = std::env::var_os("ISORUN_ENGINE_DIR") {
                    return read_install(Path::new(&d));
                }
                #[cfg(target_os = "android")]
                if let Some(i) = crate::platform::storage::data_dir().and_then(|d| read_install(&d.join("engine"))) {
                    return Some(i);
                }
                let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("engine")));
                beside.and_then(|d| read_install(&d)).or_else(checkout_install)
            })
            .as_ref()
    }

    pub(super) fn build() -> Option<Build> {
        install().map(|i| i.build.clone())
    }

    pub(super) fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
        let i = install().ok_or("no offline engine")?;
        if version != i.build.version || file.contains("..") {
            return Err(format!("no gamedata {version}/{file}"));
        }
        std::fs::read(i.gamedata.join(file)).map_err(|e| format!("{file}: {e}"))
    }

    /// The gamedata is on the disk: always ready.
    pub(super) fn gamedata_ready(_version: &str, _wake: Wake) -> bool {
        true
    }

    pub(super) fn gamedata_status() -> Option<Result<String, String>> {
        None
    }

    /// Where a profile keeps its saves for a release (`channels.ts` `profileSaveDir`).
    fn save_dir(slot: &str, profile: &str) -> PathBuf {
        let base = crate::platform::storage::data_dir().unwrap_or_else(std::env::temp_dir).join("saves");
        let p = profile.to_lowercase();
        base.join(if p == DEFAULT_PROFILE.to_lowercase() { format!("crawl-{slot}") } else { format!("crawl-{slot}~{p}") })
    }

    /// The save waiting in a profile's folder, in the lobby's words; none when there is no save.
    pub(super) fn save_info(b: &Build, user: &str, reply: SaveReply) {
        let dir = save_dir(&b.slot(), user);
        let found = (|| {
            let file = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "cs"))?;
            let note = crate::platform::storage::get_raw(&format!("{SAVE_NOTE_KEY}{}", dir.display())).map(stored_text);
            Some(note.unwrap_or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()))
        })();
        reply(found);
    }

    pub(super) fn note_save(save_dir: &str, fields: &Value) {
        if let Some(desc) = save_desc(fields) {
            crate::platform::storage::set_raw(&format!("{SAVE_NOTE_KEY}{save_dir}"), Some(&serde_json::to_string(&desc).unwrap_or_default()));
        }
    }

    /// The channel crawl talks over: its own Unix datagram socket, or (Windows) loopback UDP.
    trait Socket: Send {
        fn send(&self, bytes: &[u8]) -> std::io::Result<()>;
        fn reader(&self) -> std::io::Result<Box<dyn FnMut(&mut [u8]) -> std::io::Result<usize> + Send>>;
    }

    #[cfg(unix)]
    struct UnixSocket {
        socket: std::os::unix::net::UnixDatagram,
        crawl: Peer,
    }

    /// Where crawl's socket is: a socket file, or (Android) a name in the abstract namespace.
    #[cfg(unix)]
    #[derive(Clone)]
    enum Peer {
        Path(PathBuf),
        #[cfg(any(target_os = "linux", target_os = "android"))]
        Abstract(std::os::unix::net::SocketAddr),
    }

    #[cfg(unix)]
    impl Peer {
        fn send(&self, socket: &std::os::unix::net::UnixDatagram, bytes: &[u8]) -> std::io::Result<usize> {
            match self {
                Peer::Path(p) => socket.send_to(bytes, p),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                Peer::Abstract(a) => socket.send_to_addr(bytes, a),
            }
        }
        /// Whether crawl has bound it: its file is there, or a connect to the name is taken.
        fn bound(&self) -> bool {
            match self {
                Peer::Path(p) => p.exists(),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                Peer::Abstract(a) => std::os::unix::net::UnixDatagram::unbound().and_then(|s| s.connect_addr(a)).is_ok(),
            }
        }
    }

    /// Whether crawl's channel is in the abstract namespace: on Android, whose SELinux policy
    /// refuses a socket file in the shell's data folder (EACCES on the emulator) and may in an
    /// app's; ISORUN_SOCKET=abstract or =fs chooses elsewhere (Linux).
    #[cfg(unix)]
    fn abstract_socket() -> bool {
        match std::env::var("ISORUN_SOCKET").as_deref() {
            Ok("abstract") => cfg!(any(target_os = "linux", target_os = "android")),
            Ok("fs") => false,
            _ => cfg!(target_os = "android"),
        }
    }

    /// An abstract socket name as crawl binds it (15-abstract-socket.patch): the whole of
    /// sun_path after its leading zero, padded with zeros, since crawl's sends name the server
    /// with sizeof(sockaddr_un) and so the padded name.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn abstract_addr(name: &str) -> std::io::Result<std::os::unix::net::SocketAddr> {
        #[cfg(target_os = "android")]
        use std::os::android::net::SocketAddrExt;
        #[cfg(target_os = "linux")]
        use std::os::linux::net::SocketAddrExt;
        let mut padded = [0u8; 107];
        let n = name.len().min(padded.len());
        padded[..n].copy_from_slice(&name.as_bytes()[..n]);
        std::os::unix::net::SocketAddr::from_abstract_name(padded)
    }

    #[cfg(unix)]
    impl Socket for UnixSocket {
        fn send(&self, bytes: &[u8]) -> std::io::Result<()> {
            self.crawl.send(&self.socket, bytes).map(|_| ())
        }
        fn reader(&self) -> std::io::Result<Box<dyn FnMut(&mut [u8]) -> std::io::Result<usize> + Send>> {
            let r = self.socket.try_clone()?;
            Ok(Box::new(move |buf| r.recv(buf)))
        }
    }

    /// Windows has no Unix datagram sockets: crawl's webtiles channel is its stdin and stdout there
    /// (13-windows-webtiles.patch), a control message a line of JSON, its output as the socket
    /// carries it.
    #[cfg(windows)]
    struct PipeChannel {
        stdin: std::sync::Mutex<std::process::ChildStdin>,
        stdout: std::sync::Mutex<Option<std::process::ChildStdout>>,
    }

    #[cfg(windows)]
    impl Socket for PipeChannel {
        fn send(&self, bytes: &[u8]) -> std::io::Result<()> {
            use std::io::Write;
            let mut w = self.stdin.lock().map_err(|_| std::io::Error::other("poisoned"))?;
            w.write_all(bytes)?;
            w.write_all(b"\n")?;
            w.flush()
        }
        fn reader(&self) -> std::io::Result<Box<dyn FnMut(&mut [u8]) -> std::io::Result<usize> + Send>> {
            use std::io::Read;
            let mut r = self.stdout.lock().ok().and_then(|mut o| o.take()).ok_or_else(|| std::io::Error::other("crawl's output is taken"))?;
            Ok(Box::new(move |buf| match r.read(buf)? {
                0 => Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "crawl closed its output")),
                n => Ok(n),
            }))
        }
    }

    struct Child_ {
        child: Child,
        socket: Box<dyn Socket>,
        run_dir: PathBuf,
        /// Where crawl writes its crash files (its morgue folder), and when it started: a crash
        /// file newer than that is this run's.
        morgue: PathBuf,
        started: std::time::SystemTime,
        status: Option<std::process::ExitStatus>,
        output: Receiver<Output>,
        /// The pipe kept open as crawl's stdin, which it never reads from.
        _stdin: Option<std::process::ChildStdin>,
    }

    impl Drop for Child_ {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.run_dir);
        }
    }

    impl Process for Child_ {
        fn control(&mut self, json: &str) {
            if let Err(e) = self.socket.send(json.as_bytes()) {
                log::warn!("crawl socket send: {e}");
            }
        }

        fn poll(&mut self, out: &mut Vec<Output>) {
            while let Ok(o) = self.output.try_recv() {
                out.push(o);
            }
        }

        fn exited(&mut self) -> Option<i32> {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.status = Some(status);
                    Some(status.code().unwrap_or(-1))
                }
                Ok(None) => None,
                Err(_) => Some(-1),
            }
        }

        fn crash_report(&mut self) -> Option<String> {
            let mut parts: Vec<String> = Vec::new();
            #[cfg(unix)]
            if let Some(sig) = self.status.and_then(|s| std::os::unix::process::ExitStatusExt::signal(&s)) {
                parts.push(format!("crawl was stopped by signal {sig} ({})", signal_name(sig)));
            }
            let log = std::fs::read_to_string(self.run_dir.join("crawl.log")).unwrap_or_default();
            let log = log.trim();
            if !log.is_empty() {
                // the end of it: what came last is what went wrong
                let tail: Vec<&str> = log.lines().rev().take(60).collect();
                parts.push(tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
            if let Some(file) = newest_crash_file(&self.morgue, self.started) {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    let head: Vec<&str> = text.lines().take(80).collect();
                    parts.push(format!("--- {}\n{}", file.display(), head.join("\n")));
                }
            }
            (!parts.is_empty()).then(|| parts.join("\n"))
        }

        fn hangup(&mut self) -> Hangup {
            #[cfg(unix)]
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGHUP);
            }
            // crawl on Windows takes the hangup as a control message (13-windows-webtiles.patch):
            // there are no signals to send it
            #[cfg(windows)]
            self.control(r#"{"msg":"hangup"}"#);
            Hangup::Exits
        }

        fn kill(&mut self) {
            let _ = self.child.kill();
        }

        fn wait_exit(&mut self, seconds: f64) {
            let deadline = Instant::now() + Duration::from_secs_f64(seconds);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = self.child.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    /// The crash file crawl wrote (`crash-<name>-<time>.txt` in its morgue folder) after `since`.
    fn newest_crash_file(morgue: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
        std::fs::read_dir(morgue)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .filter(|(t, _)| *t >= since)
            .max_by_key(|(t, _)| *t)
            .map(|(_, p)| p)
    }

    #[cfg(unix)]
    fn signal_name(sig: i32) -> &'static str {
        match sig {
            libc::SIGSEGV => "SIGSEGV, a bad memory access",
            libc::SIGABRT => "SIGABRT, an abort",
            libc::SIGBUS => "SIGBUS",
            libc::SIGILL => "SIGILL",
            libc::SIGFPE => "SIGFPE",
            libc::SIGSYS => "SIGSYS, a system call the system forbids",
            libc::SIGKILL => "SIGKILL",
            libc::SIGHUP => "SIGHUP",
            libc::SIGTERM => "SIGTERM",
            libc::SIGPIPE => "SIGPIPE",
            _ => "a signal",
        }
    }

    /// Start crawl for `user`: the process and the folder its save is kept in.
    pub(super) fn launch(b: &Build, user: &str, extra: Vec<String>, rc: &str, wake: Wake) -> Result<(Box<dyn Process>, String), String> {
        let i = install().ok_or("no offline engine")?;
        let base = crate::platform::storage::data_dir().unwrap_or_else(std::env::temp_dir);
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // short: a socket's path has room for 107 bytes
        let mut run_dir = base.join("run").join(format!("{}-{n}", std::process::id()));
        if run_dir.as_os_str().len() > 80 {
            run_dir = std::env::temp_dir().join(format!("isorun-{}-{n}", std::process::id()));
        }
        let _ = std::fs::remove_dir_all(&run_dir);
        std::fs::create_dir_all(&run_dir).map_err(|e| e.to_string())?;
        let save_dir = save_dir(&b.slot(), user);
        std::fs::create_dir_all(&save_dir).map_err(|e| e.to_string())?;
        let morgue = save_dir.join("morgue");
        // made here, so crawl finds it there: crawl makes a missing folder from the root down,
        // and an Android app may not read the folders above its own (14-android-webtiles.patch)
        let _ = std::fs::create_dir_all(&morgue);
        let crawl_sock = run_dir.join("crawl.sock");
        #[cfg(unix)]
        let (socket, peer, sock_arg) = {
            let (socket, peer, arg) = if abstract_socket() {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    let tag = format!("isorun-{}-{n}", std::process::id());
                    let own = abstract_addr(&format!("{tag}-srv")).map_err(|e| format!("socket: {e}"))?;
                    let socket = std::os::unix::net::UnixDatagram::bind_addr(&own).map_err(|e| format!("socket: {e}"))?;
                    let name = format!("{tag}-crawl");
                    (socket, Peer::Abstract(abstract_addr(&name).map_err(|e| format!("socket: {e}"))?), format!("@{name}"))
                }
                #[cfg(not(any(target_os = "linux", target_os = "android")))]
                unreachable!()
            } else {
                let socket = std::os::unix::net::UnixDatagram::bind(run_dir.join("srv.sock")).map_err(|e| format!("socket {}: {e}", run_dir.join("srv.sock").display()))?;
                (socket, Peer::Path(crawl_sock.clone()), crawl_sock.display().to_string())
            };
            let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
            (socket, peer, arg)
        };
        // on Windows the name is only a token: the channel is the pipes
        #[cfg(windows)]
        let sock_arg = "pipe".to_string();
        let mut args: Vec<String> = vec!["-headless".into(), "-name".into(), user.into(), "-webtiles-socket".into(), sock_arg, "-await-connection".into()];
        args.extend(extra);
        for opt in [format!("save_dir={}", save_dir.display()), format!("morgue_dir={}", morgue.display())] {
            args.push("-extra-opt-first".into());
            args.push(opt);
        }
        let rc_path = run_dir.join("init.txt");
        std::fs::write(&rc_path, rc).map_err(|e| e.to_string())?;
        args.push("-rc".into());
        args.push(rc_path.display().to_string());
        let log = std::fs::File::create(run_dir.join("crawl.log")).ok();
        let started = std::time::SystemTime::now();
        let mut command = Command::new(&i.crawl);
        // crawl hangs up with the app, as with a server: it saves and stops, and holds no save
        // locked for the next start
        #[cfg(any(target_os = "linux", target_os = "android"))]
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGHUP);
                Ok(())
            });
        }
        // no console window for crawl on Windows (CREATE_NO_WINDOW)
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .args(&args)
            .current_dir(&i.dir)
            // crawl's webtiles want a UTF-8 locale; C.UTF-8 is there on every glibc and bionic
            .env("LC_ALL", "C.UTF-8")
            .env("LANG", "C.UTF-8")
            // where crawl finds its dat, docs and settings when its binary is elsewhere (Android)
            .env("CRAWL_DIR", format!("{}{}", i.dir.display(), std::path::MAIN_SEPARATOR))
            .stdin(Stdio::piped())
            // the webtiles channel on Windows; nothing natively
            .stdout(if cfg!(windows) { Stdio::piped() } else { Stdio::null() })
            .stderr(log.map(Stdio::from).unwrap_or(Stdio::null()))
            .spawn()
            .map_err(|e| format!("{}: {e}", i.crawl.display()))?;
        let stdin = child.stdin.take();
        let start = Instant::now();
        // the pipes are there from the start; crawl's socket once it has bound it
        #[cfg(unix)]
        let ready = || peer.bound();
        #[cfg(windows)]
        let ready = || true;
        while !ready() {
            if start.elapsed() > Duration::from_secs(20) {
                let _ = child.kill();
                return Err("crawl did not open its socket".into());
            }
            if let Ok(Some(status)) = child.try_wait() {
                let log = std::fs::read_to_string(run_dir.join("crawl.log")).unwrap_or_default();
                let _ = std::fs::remove_dir_all(&run_dir);
                return Err(format!("crawl stopped ({status}): {}", log.trim()));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        #[cfg(unix)]
        let socket: Box<dyn Socket> = Box::new(UnixSocket { socket, crawl: peer });
        #[cfg(windows)]
        let (socket, stdin): (Box<dyn Socket>, Option<std::process::ChildStdin>) = {
            let input = stdin.ok_or("crawl's input")?;
            let output = child.stdout.take().ok_or("crawl's output")?;
            (Box::new(PipeChannel { stdin: std::sync::Mutex::new(input), stdout: std::sync::Mutex::new(Some(output)) }), None)
        };
        let (tx, rx) = channel();
        let mut read = socket.reader().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("crawl output".into())
            .spawn(move || {
                let mut buf = vec![0u8; 1 << 20];
                loop {
                    match read(&mut buf) {
                        Ok(n) => {
                            if tx.send(Output::Text(String::from_utf8_lossy(&buf[..n]).into_owned())).is_err() {
                                return;
                            }
                            wake();
                        }
                        Err(e) => {
                            let _ = tx.send(Output::Error(format!("crawl socket: {e}")));
                            wake();
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let process = Child_ { child, socket, run_dir, morgue, started, status: None, output: rx, _stdin: stdin };
        Ok((Box::new(process), save_dir.display().to_string()))
    }
}

/// The web: the WebAssembly crawl in a Web Worker, through the page's `isorunEngine` module
/// (`native/web/engine-bridge.ts`, which runs `native/web/engine/worker.ts`).
#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    #[wasm_bindgen]
    extern "C" {
        /// The build the page serves (`<engine base>/stable/engine.json`), as JSON: `{id, label,
        /// commit, build, gamedata, saveDir, version, base}`; null when there is none.
        #[wasm_bindgen(catch, js_namespace = isorunEngine, js_name = channel)]
        fn js_channel() -> Result<js_sys::Promise, JsValue>;
        /// The save waiting in a save folder (an IndexedDB database), in the lobby's words, or null.
        #[wasm_bindgen(catch, js_namespace = isorunEngine, js_name = saveInfo)]
        fn js_save_info(dir: &str) -> Result<js_sys::Promise, JsValue>;
        #[wasm_bindgen(catch, js_namespace = isorunEngine, js_name = noteSave)]
        fn js_note_save(dir: &str, fields: &str) -> Result<(), JsValue>;
        /// Start crawl in a worker: `{control(json), keys(text), terminate(), retire()}`.
        #[wasm_bindgen(catch, js_namespace = isorunEngine, js_name = launch)]
        fn js_launch(save_dir: &str, args: &str, rc: &str, on_output: &js_sys::Function, on_exit: &js_sys::Function, on_error: &js_sys::Function) -> Result<JsValue, JsValue>;
    }

    struct Channel {
        build: Build,
        /// Where its gamedata is served: `<engine base>gamedata/<version>/`.
        gamedata_url: String,
        /// The slot's save folder (`/crawl-0.34`), before the profile's suffix.
        save_dir: String,
    }

    // The page has one thread: the state lives in thread-locals, the platform's Send bounds are
    // met by what is behind them.
    thread_local! {
        static CHANNEL: RefCell<Option<Rc<Channel>>> = const { RefCell::new(None) };
        static GAMEDATA: RefCell<Gamedata> = RefCell::new(Gamedata::default());
    }

    #[derive(Default)]
    struct Gamedata {
        version: String,
        files: HashMap<String, Vec<u8>>,
        requests: Vec<(String, crate::platform::http::Request)>,
        error: Option<String>,
    }

    pub(super) async fn init() {
        let Ok(promise) = js_channel() else {
            log::info!("no isorunEngine on the page: no offline play");
            return;
        };
        let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else { return };
        let Some(text) = value.as_string() else { return };
        let Ok(c) = serde_json::from_str::<Value>(&text) else { return };
        let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let build = Build { version: s("gamedata"), crawl_version: s("version") };
        let channel = Channel { gamedata_url: format!("{}gamedata/{}/", s("base"), build.version), save_dir: s("saveDir"), build };
        CHANNEL.with(|ch| *ch.borrow_mut() = Some(Rc::new(channel)));
    }

    fn channel() -> Option<Rc<Channel>> {
        CHANNEL.with(|c| c.borrow().clone())
    }

    pub(super) fn build() -> Option<Build> {
        channel().map(|c| c.build.clone())
    }

    /// The build the page serves, as the desktop's install names it (its folder is the server's).
    #[derive(Clone, Debug)]
    pub struct Install {
        pub build: Build,
    }

    pub fn install() -> Option<Install> {
        build().map(|build| Install { build })
    }

    /// The profile's save folder (channels.ts `profileSaveDir`).
    fn profile_save_dir(c: &Channel, user: &str) -> String {
        let p = user.to_lowercase();
        if p == crate::servers::DEFAULT_PROFILE.to_lowercase() { c.save_dir.clone() } else { format!("{}~{p}", c.save_dir) }
    }

    pub(super) fn save_info(_b: &Build, user: &str, reply: SaveReply) {
        let Some(c) = channel() else { return reply(None) };
        let Ok(promise) = js_save_info(&profile_save_dir(&c, user)) else { return reply(None) };
        wasm_bindgen_futures::spawn_local(async move {
            let found = wasm_bindgen_futures::JsFuture::from(promise).await.ok().and_then(|v| v.as_string());
            reply(found);
        });
    }

    pub(super) fn note_save(save_dir: &str, fields: &Value) {
        let _ = js_note_save(save_dir, &fields.to_string());
    }

    /// Fetch the build's gamedata into memory (the session reads it with `gamedata_file`); false
    /// while it is on its way.
    pub(super) fn gamedata_ready(version: &str, wake: Wake) -> bool {
        let Some(c) = channel() else { return true };
        GAMEDATA.with(|g| {
            let mut g = g.borrow_mut();
            if g.version == version && g.error.is_none() && g.requests.is_empty() && !g.files.is_empty() {
                return true;
            }
            *g = Gamedata { version: version.to_string(), ..Default::default() };
            for f in isorun_gamedata::gamedata_files() {
                let wake = wake.clone();
                let request = crate::platform::http::get(&format!("{}{f}", c.gamedata_url), move || wake());
                g.requests.push((f, request));
            }
            false
        })
    }

    /// The fetch's end: the version once every file is in, or why it failed; none while it runs.
    pub(super) fn gamedata_status() -> Option<Result<String, String>> {
        GAMEDATA.with(|g| {
            let mut g = g.borrow_mut();
            let g = &mut *g;
            let mut i = 0;
            while i < g.requests.len() {
                match g.requests[i].1.take() {
                    Some(Ok(bytes)) => {
                        let (name, _) = g.requests.swap_remove(i);
                        g.files.insert(name, bytes);
                    }
                    Some(Err(e)) => {
                        g.requests.swap_remove(i);
                        g.error.get_or_insert(e);
                    }
                    None => i += 1,
                }
            }
            if let Some(e) = &g.error {
                return Some(Err(e.clone()));
            }
            g.requests.is_empty().then(|| Ok(g.version.clone()))
        })
    }

    pub(super) fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
        GAMEDATA.with(|g| {
            let g = g.borrow();
            if g.version != version {
                return Err(format!("no gamedata {version}"));
            }
            g.files.get(file).cloned().ok_or_else(|| format!("no gamedata {version}/{file}"))
        })
    }

    #[derive(Default)]
    struct Inbox {
        output: VecDeque<Output>,
        exit: Option<i32>,
    }

    struct Worker {
        handle: JsValue,
        inbox: Rc<RefCell<Inbox>>,
        _callbacks: [Closure<dyn FnMut(JsValue)>; 3],
    }

    impl Worker {
        fn call(&self, name: &str, arg: Option<&str>) {
            let Ok(f) = js_sys::Reflect::get(&self.handle, &name.into()) else { return };
            let Ok(f) = f.dyn_into::<js_sys::Function>() else { return };
            let _ = match arg {
                Some(a) => f.call1(&self.handle, &a.into()),
                None => f.call0(&self.handle),
            };
        }
    }

    impl Process for Worker {
        fn control(&mut self, json: &str) {
            self.call("control", Some(json));
        }
        fn keys(&mut self, text: &str) {
            self.call("keys", Some(text));
        }
        fn poll(&mut self, out: &mut Vec<Output>) {
            out.extend(self.inbox.borrow_mut().output.drain(..));
        }
        fn exited(&mut self) -> Option<i32> {
            self.inbox.borrow().exit
        }
        fn hangup(&mut self) -> Hangup {
            // the engine holds the request until the player has control (tileweb.cc
            // `_maybe_checkpoint`), and its commit reaches IndexedDB before the `*checkpoint` line
            self.control(r#"{"msg":"checkpoint"}"#);
            Hangup::Checkpoint
        }
        fn kill(&mut self) {
            self.call("terminate", None);
        }
        fn retire(&mut self) {
            // the worker outlives this connection until its save is in (or a few seconds pass)
            self.call("retire", None);
        }
    }

    pub(super) fn launch(_b: &Build, user: &str, extra: Vec<String>, rc: &str, wake: Wake) -> Result<(Box<dyn Process>, String), String> {
        let c = channel().ok_or("no offline engine")?;
        let save_dir = profile_save_dir(&c, user);
        // the rc file is put in place by the bridge (rc.ts RC_PATH) when there is one
        let mut args: Vec<String> = vec!["-headless".into(), "-webtiles-socket".into(), "bridge".into(), "-name".into(), user.into()];
        args.extend(extra);
        let inbox = Rc::new(RefCell::new(Inbox::default()));
        let (i, w) = (inbox.clone(), wake.clone());
        let on_output = Closure::<dyn FnMut(JsValue)>::new(move |text: JsValue| {
            if let Some(t) = text.as_string() {
                i.borrow_mut().output.push_back(Output::Text(t));
            }
            w();
        });
        let (i, w) = (inbox.clone(), wake.clone());
        let on_exit = Closure::<dyn FnMut(JsValue)>::new(move |code: JsValue| {
            i.borrow_mut().exit = Some(code.as_f64().unwrap_or(-1.0) as i32);
            w();
        });
        let (i, w) = (inbox.clone(), wake);
        let on_error = Closure::<dyn FnMut(JsValue)>::new(move |message: JsValue| {
            let mut i = i.borrow_mut();
            i.output.push_back(Output::Error(format!("offline engine: {}", message.as_string().unwrap_or_default())));
            i.output.push_back(Output::Text(format!("*{}\n", json!({ "msg": "exit_reason", "type": "error", "message": message.as_string().unwrap_or_default() }))));
            i.exit = Some(-1);
            w();
        });
        let args = serde_json::to_string(&args).unwrap_or_default();
        let handle = js_launch(&save_dir, &args, rc, on_output.as_ref().unchecked_ref(), on_exit.as_ref().unchecked_ref(), on_error.as_ref().unchecked_ref()).map_err(|e| format!("{e:?}"))?;
        Ok((Box::new(Worker { handle, inbox, _callbacks: [on_output, on_exit, on_error] }), save_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::crash_headline;

    #[test]
    fn a_crash_report_is_headed_by_its_first_line_that_says_something() {
        assert_eq!(crash_headline("\n  \ncrawl was stopped by signal 11 (SIGSEGV)\nmore").as_deref(), Some("crawl was stopped by signal 11 (SIGSEGV)"));
        assert_eq!(crash_headline("--- /x/crash-a.txt\nASSERT(foo) in 'x.cc' at line 3 failed.").as_deref(), Some("ASSERT(foo) in 'x.cc' at line 3 failed."));
        assert_eq!(crash_headline(" \n"), None);
        let long = "x".repeat(400);
        assert_eq!(crash_headline(&long).map(|l| l.chars().count()), Some(161));
    }
}

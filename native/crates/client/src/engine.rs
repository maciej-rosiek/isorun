//! The offline engine: crawl on this device, with no server (the port of `packages/offline`
//! `server.ts`). Natively crawl runs as a child process and speaks its own WebTiles socket
//! protocol, a Unix datagram socket as a server's `process_handler.py` speaks it; the
//! [`OfflineServer`] is the lobby and the relay around it, a [`Transport`] like a server's socket.
//!
//! An engine is a folder (`engine.json`, the `crawl` binary, its `dat`, `docs` and `settings`,
//! and the client's `gamedata/<version>/`): `ISORUN_ENGINE_DIR`, `engine/` beside the
//! executable, or in a checkout the native build in `engine/.work/stable`.

use crate::platform::net::{ConnEvent, Transport};
use serde_json::{json, Value};
use std::sync::Arc;

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The option defaults of a server build (initfile.cc `USING_DGL`) that a player notices: travel
/// and rest arrive at their end in one update, and a travel leaves its trail.
const SERVER_DEFAULTS: [&str; 3] = ["travel_delay=-1", "rest_delay=-1", "show_travel_trail=true"];

/// A token names its profile and lasts ten years: the account is only ever this device's.
const TOKEN_PREFIX: &str = "offline:";
const TOKEN_DAYS: i64 = 3650;

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

/// The profile's rc file, one for every game (`rc.ts`).
pub fn read_rc(profile: &str) -> String {
    crate::platform::storage::get_raw(&format!("{RC_KEY}{}", profile.to_lowercase())).and_then(|v| serde_json::from_str::<String>(&v).ok()).unwrap_or_default()
}

pub fn write_rc(profile: &str, contents: &str) {
    let key = format!("{RC_KEY}{}", profile.to_lowercase());
    if contents.is_empty() {
        crate::platform::storage::set_raw(&key, None);
    } else {
        crate::platform::storage::set_raw(&key, Some(&serde_json::to_string(contents).unwrap_or_default()));
    }
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
pub use native::*;

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod native {
    use super::*;
    use crate::servers::DEFAULT_PROFILE;
    use std::io::Write;
    use std::os::unix::net::UnixDatagram;
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
        /// The gamedata version `game_client` announces: the crawl commit.
        pub version: String,
        /// crawl's own version string, `0.34.1-4-g0e95e087e2`.
        pub crawl_version: String,
    }

    impl Install {
        /// The release, `0.34`, or `trunk`.
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

    fn read_install(dir: &Path) -> Option<Install> {
        let info: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("engine.json")).ok()?).ok()?;
        let version = info.get("gamedata").or(info.get("commit"))?.as_str()?.to_string();
        let crawl = dir.join(if cfg!(windows) { "crawl.exe" } else { "crawl" });
        let gamedata = dir.join("gamedata").join(&version);
        (crawl.exists() && gamedata.exists()).then(|| Install { dir: dir.to_path_buf(), crawl, gamedata, crawl_version: info.get("version").and_then(Value::as_str).unwrap_or(&version).to_string(), version })
    }

    /// A checkout's native build (`engine/native.sh`, in `engine/.work/stable`).
    fn checkout_install() -> Option<Install> {
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine");
        let source = engine.join(".work/stable/crawl-ref/source");
        let crawl = source.join("crawl");
        if !crawl.exists() {
            return None;
        }
        let commit = std::fs::read_to_string(engine.join(".work/stable.commit")).ok()?.trim().to_string();
        let info: Value = std::fs::read_to_string(engine.join("dist/stable/engine.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let crawl_version = info.get("version").and_then(Value::as_str).unwrap_or("0.34").to_string();
        Some(Install { dir: source.clone(), crawl, gamedata: source.join("webserver/game_data/static"), version: commit, crawl_version })
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

    pub fn available() -> bool {
        install().is_some()
    }

    /// Offline gamedata is read from the install, not fetched.
    pub fn gamedata_base() -> String {
        "offline:".into()
    }

    pub fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
        let i = install().ok_or("no offline engine")?;
        if version != i.version || file.contains("..") {
            return Err(format!("no gamedata {version}/{file}"));
        }
        std::fs::read(i.gamedata.join(file)).map_err(|e| format!("{file}: {e}"))
    }

    pub fn connect(wake: Wake) -> Box<dyn Transport> {
        Box::new(OfflineServer::new(wake))
    }

    /// Where a profile keeps its saves for a release (`channels.ts` `profileSaveDir`).
    fn save_dir(slot: &str, profile: &str) -> PathBuf {
        let base = crate::platform::storage::data_dir().unwrap_or_else(std::env::temp_dir).join("saves");
        let p = profile.to_lowercase();
        base.join(if p == DEFAULT_PROFILE.to_lowercase() { format!("crawl-{slot}") } else { format!("crawl-{slot}~{p}") })
    }

    /// The save waiting in a folder, in the lobby's words; none when there is no save.
    fn save_info(dir: &Path) -> Option<String> {
        let file = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "cs"))?;
        let note = crate::platform::storage::get_raw(&format!("{SAVE_NOTE_KEY}{}", dir.display())).and_then(|v| serde_json::from_str::<String>(&v).ok());
        Some(note.unwrap_or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()))
    }

    enum Output {
        Text(String),
        Error(String),
    }

    struct Game {
        child: Child,
        socket: UnixDatagram,
        crawl_sock: PathBuf,
        run_dir: PathBuf,
        output: Receiver<Output>,
        save_dir: PathBuf,
        partial: String,
        queueing: bool,
        pending: Vec<Value>,
        exit_reason: Option<(String, Option<String>)>,
        stopping: Option<Instant>,
        ended: bool,
        /// The pipe kept open as crawl's stdin, which it never reads from.
        _stdin: Option<std::process::ChildStdin>,
    }

    impl Drop for Game {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.run_dir);
        }
    }

    pub struct OfflineServer {
        wake: Wake,
        greeted: bool,
        out: Vec<ConnEvent>,
        user: String,
        logged_in: bool,
        game: Option<Game>,
        closed: bool,
    }

    /// How long a stop waits for crawl to save after its hangup before it is killed.
    const STOP_SAVE: Duration = Duration::from_secs(5);

    impl OfflineServer {
        pub fn new(wake: Wake) -> Self {
            OfflineServer { wake, greeted: false, out: Vec::new(), user: DEFAULT_PROFILE.into(), logged_in: false, game: None, closed: false }
        }

        fn emit(&mut self, m: Value) {
            self.out.push(ConnEvent::Message(m));
        }

        fn diag(&mut self, text: String) {
            self.out.push(ConnEvent::Diagnostic(text));
        }

        fn send_game_links(&mut self) {
            let Some(i) = install() else { return };
            let href = format!("#play-{}", i.game_id());
            let content = match save_info(&save_dir(&i.slot(), &self.user)) {
                Some(save) => format!("<span>{} <span><a href=\"{href}\">[{}]</a></span></span>", escape_html(&i.label()), escape_html(&save)),
                None => format!("<span><a href=\"{href}\">{}</a></span>", escape_html(&i.label())),
            };
            self.emit(json!({ "msg": "set_game_links", "content": content }));
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
                    g.control(&json!({ "msg": "text_input", "text": text }));
                }
            } else {
                g.control(&msg);
            }
        }

        fn play(&mut self, id: &str) {
            if self.game.is_some() {
                return;
            }
            let Some(i) = install().filter(|i| i.game_id() == id) else {
                self.emit(json!({ "msg": "game_ended", "reason": "error", "message": format!("No offline game \"{id}\".") }));
                self.emit(json!({ "msg": "go_lobby" }));
                return;
            };
            match Game::start(i, &self.user, self.wake.clone()) {
                Ok(g) => {
                    self.game = Some(g);
                    self.emit(json!({ "msg": "game_started" }));
                    self.emit(json!({ "msg": "game_client", "version": i.version, "content": "" }));
                }
                Err(e) => {
                    self.emit(json!({ "msg": "game_ended", "reason": "error", "message": format!("The engine did not start: {e}") }));
                    self.emit(json!({ "msg": "go_lobby" }));
                }
            }
        }

        /// End the game with its state saved, as a server's SIGHUP does.
        fn stop(&mut self) {
            let Some(g) = &mut self.game else { return };
            if g.stopping.is_none() {
                g.stopping = Some(Instant::now());
                hangup(&g.child);
            }
        }

        fn pump(&mut self) {
            let Some(g) = &mut self.game else { return };
            let mut lines = Vec::new();
            let mut errors = Vec::new();
            while let Ok(o) = g.output.try_recv() {
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
            }
            let Some(g) = &mut self.game else { return };
            let exited = match g.child.try_wait() {
                Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
                Ok(None) => None,
                Err(_) => Some(-1),
            };
            let timed_out = g.stopping.is_some_and(|t| t.elapsed() > STOP_SAVE);
            if timed_out {
                let _ = g.child.kill();
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
                    self.diag(format!("offline engine sent a line that is not JSON: {}", &line[..line.len().min(200)]));
                    return;
                }
            };
            let Some(g) = &mut self.game else { return };
            if special {
                match m.get("msg").and_then(Value::as_str).unwrap_or("") {
                    "flush_messages" => {
                        g.queueing = true;
                        let batch = std::mem::take(&mut g.pending);
                        self.out.extend(batch.into_iter().map(ConnEvent::Message));
                    }
                    "exit_reason" => {
                        g.exit_reason = Some((m.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string(), m.get("message").and_then(Value::as_str).map(str::to_string)));
                    }
                    "milestone" => {
                        if let Some(desc) = save_desc(&m) {
                            crate::platform::storage::set_raw(&format!("{SAVE_NOTE_KEY}{}", g.save_dir.display()), Some(&serde_json::to_string(&desc).unwrap_or_default()));
                        }
                    }
                    _ => {}
                }
                return;
            }
            if g.queueing {
                g.pending.push(m);
            } else {
                self.emit(m);
            }
        }

        fn ended(&mut self, code: i32) {
            let Some(mut g) = self.game.take() else { return };
            g.ended = true;
            let batch = std::mem::take(&mut g.pending);
            self.out.extend(batch.into_iter().map(ConnEvent::Message));
            let (kind, message) = match (g.stopping.is_some(), g.exit_reason.take()) {
                // a hangup's save ends the game as crawl's own "saved"
                (true, Some((k, m))) if k != "unknown" && k != "error" => (k, m),
                (true, None) | (true, Some(_)) if code == 0 => ("saved".into(), None),
                (_, Some((k, m))) if k != "unknown" => (k, m),
                _ => (if code == 0 { "quit".into() } else { "crash".into() }, None),
            };
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

    fn hangup(child: &Child) {
        unsafe {
            libc::kill(child.id() as i32, libc::SIGHUP);
        }
    }

    impl Game {
        fn start(i: &Install, user: &str, wake: Wake) -> Result<Game, String> {
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
            let save_dir = save_dir(&i.slot(), user);
            std::fs::create_dir_all(&save_dir).map_err(|e| e.to_string())?;
            let morgue = save_dir.join("morgue");
            let srv_sock = run_dir.join("srv.sock");
            let crawl_sock = run_dir.join("crawl.sock");
            let socket = UnixDatagram::bind(&srv_sock).map_err(|e| format!("socket: {e}"))?;
            let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
            let mut args: Vec<String> = vec!["-headless".into(), "-name".into(), user.into(), "-webtiles-socket".into(), crawl_sock.display().to_string(), "-await-connection".into()];
            for opt in SERVER_DEFAULTS.iter().map(|s| s.to_string()).chain([format!("save_dir={}", save_dir.display()), format!("morgue_dir={}", morgue.display())]) {
                args.push("-extra-opt-first".into());
                args.push(opt);
            }
            let rc = read_rc(user);
            let rc_path = run_dir.join("init.txt");
            std::fs::write(&rc_path, &rc).map_err(|e| e.to_string())?;
            args.push("-rc".into());
            args.push(rc_path.display().to_string());
            let log = std::fs::File::create(run_dir.join("crawl.log")).ok();
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
            let mut child = command
                .args(&args)
                .current_dir(&i.dir)
                // crawl's webtiles want a UTF-8 locale; C.UTF-8 is there on every glibc and bionic
                .env("LC_ALL", "C.UTF-8")
                .env("LANG", "C.UTF-8")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(log.map(Stdio::from).unwrap_or(Stdio::null()))
                .spawn()
                .map_err(|e| format!("{}: {e}", i.crawl.display()))?;
            let stdin = child.stdin.take();
            let start = Instant::now();
            while !crawl_sock.exists() {
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
            let (tx, rx) = channel();
            let reader = socket.try_clone().map_err(|e| e.to_string())?;
            std::thread::Builder::new()
                .name("crawl output".into())
                .spawn(move || {
                    let mut buf = vec![0u8; 1 << 20];
                    loop {
                        match reader.recv(&mut buf) {
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
            let mut g = Game { child, socket, crawl_sock, run_dir, output: rx, save_dir, partial: String::new(), queueing: false, pending: Vec::new(), exit_reason: None, stopping: None, ended: false, _stdin: stdin };
            g.control(&json!({ "msg": "attach", "primary": true }));
            Ok(g)
        }

        fn control(&mut self, m: &Value) {
            let text = m.to_string();
            if let Err(e) = self.socket.send_to(text.as_bytes(), &self.crawl_sock) {
                log::warn!("crawl socket send: {e}");
            }
            let _ = std::io::stderr().flush();
        }
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
            if let Some(g) = &mut self.game {
                let deadline = Instant::now() + STOP_SAVE;
                while Instant::now() < deadline {
                    if let Ok(Some(_)) = g.child.try_wait() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            self.game = None;
            self.closed = true;
            self.out.push(ConnEvent::Closed { code: 1000, reason: String::new() });
        }

        fn poll(&mut self, out: &mut Vec<ConnEvent>) {
            if !self.greeted && !self.closed {
                self.greeted = true;
                out.push(ConnEvent::Open);
                self.emit(json!({ "msg": "lobby_clear" }));
                self.emit(json!({ "msg": "lobby_complete" }));
            }
            self.pump();
            out.append(&mut self.out);
            if self.game.as_ref().is_some_and(|g| g.stopping.is_some()) {
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
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
mod missing {
    use super::*;

    pub fn available() -> bool {
        false
    }

    pub fn gamedata_base() -> String {
        String::new()
    }

    pub fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
        Err(format!("no offline engine for {version}/{file}"))
    }

    pub fn connect(_wake: Wake) -> Box<dyn Transport> {
        Box::new(Missing { told: false })
    }

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
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
pub use missing::*;

//! The front end (the port of `menu.ts` and the routing of `main.ts` that is not the game
//! screen): readable menus over the room, in the console face, one cursor driven by the pad, the
//! keyboard, the mouse or touch. The home screen is the chosen account and the things to do:
//! Continue or Play, Watch, Settings; Play, Watch and Settings are the sections the shoulder
//! buttons walk. The account row leads to the accounts on this device, adding one (a server, then
//! its login or register form), and logging out. Every screen remembers the row it was left on,
//! so Back lands where the player was.
//!
//! The front end keeps the chosen account's connection warm while it is up, so its token login
//! has already happened when its lobby comes up; a game or a spectate that starts on it is handed
//! over to the game screen ([`FrontAction::Play`], [`FrontAction::Watch`]) with the session, and
//! comes back when the game is over ([`FrontEnd::game_over`]).
//!
//! Text is never attached to the scenery: the room is looked around with the right stick or a
//! drag on it, and the menus stand still.

use crate::focus::{focus_move, focus_slots, FocusDir, FocusSlot};
use crate::room::{next_home_place, RoomView};
use crate::settings::{self, Settings};
use crate::whereis::{self, Whereis};
use crate::words::{self, NavDir, RcState};
use eframe::egui_wgpu::RenderState;
use isorun_client::platform::http;
use isorun_client::servers::{self, Account, LoginState, ServerInfo};
use isorun_client::session::{Session, SessionEvent};
use isorun_webtiles::{cm, GameExit, Phase};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// What to do once a fresh connection is up: start (or continue) a game, or watch a player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Play(String),
    Watch(String),
}

/// What the front end asks of the app.
pub enum FrontAction {
    /// A game started on `session`: show the game screen on it. `game_id` is the game played.
    Play { session: Box<Session>, account: Option<Account>, game_id: String },
    /// A spectate started on `session`: show the game screen watching `username`.
    Watch { session: Box<Session>, username: String },
    /// Open the test bed.
    Testbed,
    /// Close the app.
    Quit,
    /// Fill the screen, or leave it.
    Fullscreen(bool),
    /// Share the log (the app's: the share sheet on Android, the clipboard elsewhere).
    ShareLog,
}

/// How a game came back to the front end.
pub enum GameEnd {
    /// The server put its lobby up (`go_lobby`: a death, a save, a quit): this account's home
    /// screen, on the same connection; out of a spectate, the Watch screen it was picked from.
    Lobby(Box<Session>),
    /// The player left from the game's menu: the connection is dropped (crawl keeps the game
    /// across it) and the account's home screen logs in again on a fresh one.
    Leave(Box<Session>),
    /// The connection dropped under the game: the game is gone back into, as a reload of its
    /// address does, a few times; then the home screen says the game is saved.
    Dropped(Box<Session>),
}

/// A pad's buttons as the front end reads them (the app maps its pad to these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    Start,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadEvent {
    /// The d-pad or the left stick, once or repeating.
    Dir(NavDir),
    Press(PadButton),
    /// The right stick, -1 to 1 each way; zero lets go.
    Look {
        dx: f32,
        dy: f32,
    },
}

/// What the app tells the front end about where it runs.
#[derive(Clone, Debug)]
pub struct FrontOptions {
    /// The window can be closed from the home screen (a window of its own: the desktop, a Deck).
    pub can_quit: bool,
    /// The window can go full screen, and whether it is.
    pub can_fullscreen: bool,
    pub fullscreen: bool,
    /// Offer Get the app (the web build).
    pub app_downloads: bool,
    /// Offer the test bed on the home screen's footer.
    pub testbed: bool,
    /// A controller is connected: the legend and the hints name its buttons.
    pub pad: bool,
    /// The pointer is a mouse (not touch): the legend says a drag looks around.
    pub mouse: bool,
}

impl Default for FrontOptions {
    fn default() -> Self {
        FrontOptions {
            can_quit: !cfg!(target_arch = "wasm32"),
            can_fullscreen: true,
            fullscreen: false,
            app_downloads: cfg!(target_arch = "wasm32"),
            testbed: false,
            pad: false,
            mouse: true,
        }
    }
}

/// The front end's screens. Play (home), Watch and Settings are the sections the shoulder
/// buttons walk; the rest are the flows off them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Home,
    Watch,
    Settings,
    SettingsGroup,
    Controls,
    Accounts,
    Versions,
    Rc,
    Servers,
    AddServer,
    Login,
    Register,
    Exit,
    Doc,
    News,
    Boot,
}

/// The way off a screen.
pub const BACK: &str = "back";

/// How long a server has to answer for an rc file before the editor says it did not.
/// The log's lines the Share the log page shows (the newest; all of it is shared).
const LOG_PAGE_LINES: usize = 400;
const RC_SECONDS: f64 = 10.0;
/// The wait before a retry of the warm connection doubles from this up to the most.
const RETRY_SECONDS: f64 = 1.0;
const RETRY_MAX_SECONDS: f64 = 30.0;
/// The waits before each try at a game whose connection dropped, after the first, which goes at once.
const REJOIN_SECONDS: [f64; 5] = [1.0, 2.0, 4.0, 8.0, 15.0];
/// How often the home screen asks again what waits in a save.
const WHEREIS_SECONDS: f64 = 60.0;

pub const LATEST_RELEASE_DOWNLOAD: &str = "https://github.com/maciej-rosiek/isorun/releases/latest/download/";
/// The app's packages in every release (native/desktop/package.sh, native/android/build.sh):
/// file, label, note, hint.
pub const APP_DOWNLOADS: [(&str, &str, &str, &str); 4] = [
    ("Isorun-x86_64.AppImage", "Linux", "AppImage", "For a Steam Deck, a ROG Ally or any Linux PC. AppImageUpdate or Gear Lever keep it up to date."),
    ("Isorun-linux-x86_64.tar.gz", "Linux", "folder", "The same app as a plain folder. Run Isorun/isorun."),
    ("Isorun-win-x64.zip", "Windows", "zip", "Unzip it and run Isorun.exe."),
    ("Isorun.apk", "Android", "APK", "For a phone or a tablet. A newer APK installs over it, saves kept."),
];

/// Where the chosen account's connection stands: a dot after its name only when it is not ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    Up,
    Wait,
    Off,
    Down,
}

/// What a row does when it is chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    None,
    Back,
    Home,
    Accounts,
    News,
    About,
    App,
    Steam,
    Settings,
    SettingsGroup(String),
    Controls,
    Fullscreen,
    Quit,
    Testbed,
    ShareLog,
    Play(Account, String),
    Watch,
    ServersWatch,
    ServersAdd,
    Login,
    ChooseAccount(Account),
    Rc(Account, Option<String>),
    Versions(Account),
    LoginAs(Account),
    Remove(Account),
    AskDelete(Account),
    Delete(Account),
    AddPlayer,
    PickServer(ServerInfo),
    RemoveServer(String),
    AddServer,
    SubmitAddServer,
    SubmitLogin,
    SubmitRegister,
    SubmitNewProfile,
    Register,
    ExitClose,
    ExitDump {
        title: String,
        url: Option<String>,
        href: String,
    },
    Open(String),
    Terminate(bool),
    RcSave,
    RcRevert,
    WatchPlayer(String),
    TakeArrivals,
    /// Turn a setting (its key) by a step; a click turns it forward.
    Setting(String),
    ResetSettings(String),
    ReplayTips,
}

/// A text field a row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Password,
    Password2,
    Email,
    Address,
}

/// Something standing beside a row: an account's remove beside its name.
#[derive(Clone, Debug)]
pub struct Beside {
    pub id: String,
    pub label: String,
    pub title: String,
    pub act: Act,
}

/// One line of a menu: what it says, what it does, what the message line says while the cursor is on it.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub sub: Option<String>,
    pub conn: Option<Conn>,
    /// The glyph in the margin, as the game marks a feature: `>` stairs, `?` a scroll, `{` the pool, `+` a door.
    pub marker: &'static str,
    pub hint: String,
    /// Small print after the hint, fainter than it.
    pub fine: Option<String>,
    pub act: Act,
    pub also: Vec<Beside>,
    pub input: Option<Field>,
    /// The rc file's box.
    pub text: bool,
    /// The way in: where the cursor starts on this screen.
    pub main: bool,
    /// A row that cannot be taken right now.
    pub off: bool,
    /// A group starts here: a little air above.
    pub gap: bool,
    /// A group's name over its rows: read, never taken.
    pub heading: bool,
    /// Something here not yet seen: a dot after the label.
    pub dot: bool,
}

impl Row {
    pub fn new(id: impl Into<String>, label: impl Into<String>, marker: &'static str, hint: impl Into<String>, act: Act) -> Self {
        Row {
            id: id.into(),
            label: label.into(),
            sub: None,
            conn: None,
            marker,
            hint: hint.into(),
            fine: None,
            act,
            also: Vec::new(),
            input: None,
            text: false,
            main: false,
            off: false,
            gap: false,
            heading: false,
            dot: false,
        }
    }

    pub fn sub(mut self, s: impl Into<String>) -> Self {
        self.sub = Some(s.into());
        self
    }

    pub fn main(mut self) -> Self {
        self.main = true;
        self
    }

    pub fn gap(mut self) -> Self {
        self.gap = true;
        self
    }

    pub fn back(hint: &str) -> Self {
        Row::new(BACK, "Back", "<", hint, Act::Back)
    }

    pub(crate) fn heading(id: &str, label: &str) -> Self {
        let mut r = Row::new(id, label, "", "", Act::None);
        r.heading = true;
        r.gap = true;
        r
    }

    pub(crate) fn field(id: &str, label: &str, field: Field, hint: String) -> Self {
        let mut r = Row::new(id, label, "", hint, Act::None);
        r.input = Some(field);
        r
    }
}

/// A notice over the rows: the server's word, the connection, or an error.
#[derive(Clone, Debug)]
pub struct Notice {
    pub text: String,
    pub error: bool,
    /// A plain hint line ("Connecting…"), not a notice with its mark.
    pub hint: bool,
    /// Yes / No under it (a stale process to force off).
    pub ask: bool,
}

/// What stands under a screen's rows.
#[derive(Clone, Debug)]
pub enum Below {
    None,
    Doc(DocBody),
    News,
    Roster,
    Settings(String),
    Controls,
}

#[derive(Clone, Debug)]
pub enum DocBody {
    Markdown(&'static str),
    /// A file read from a server (a morgue file): its text once it came.
    Fetched,
}

/// One screen to draw: the list it is, as `menu.ts list()` takes it.
#[derive(Clone, Debug)]
pub struct Screen {
    pub cls: Cls,
    pub title: String,
    pub lede: Option<String>,
    pub splash: Option<&'static str>,
    pub rows: Vec<Row>,
    pub utilities: Vec<Row>,
    pub notices: Vec<Notice>,
    pub below: Below,
    pub focus: Option<String>,
    /// A dialog laid over the home screen (the exit report).
    pub over: bool,
    /// Links out under a document, once reading it here failed.
    pub links: Vec<(String, String)>,
}

/// The kind of screen, for its layout (`.home-list`, `.doc-list`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cls {
    Home,
    List,
    Doc,
    News,
    Settings,
    Watch,
    Rc,
    Exit,
    Form,
}

/// The rc file on show: whose, and which. A server keeps one file per account and rc directory,
/// named by the first game that reads it; a player on this device has one for every game.
pub struct RcEdit {
    pub account: Account,
    pub game_id: Option<String>,
    pub text: String,
    /// The file as the server has it, once read.
    pub saved: Option<String>,
    pub state: RcState,
    pub asked: bool,
    pub note: Option<(String, bool)>,
    /// Back was pressed once with changes unsaved: once more leaves without them.
    pub leaving: bool,
    /// A read or save under way: the answer's slot and when it was asked; a save's text.
    pub pending: Option<(Arc<Mutex<Option<Option<String>>>>, f64, Option<String>)>,
    /// The box is being typed in.
    pub editing: bool,
}

/// A game whose connection dropped, being gone back into.
struct Rejoin {
    account: Option<Account>,
    server: ServerInfo,
    intent: Intent,
    tries: usize,
    at: Option<f64>,
}

/// A `.where` read in flight.
struct WhereRead {
    key: String,
    server: ServerInfo,
    who: String,
    dirs: Vec<String>,
    known: bool,
    at: usize,
    request: http::Request,
}

/// The message line's state: what it says, and since when (it fades in).
#[derive(Default)]
struct Said {
    text: String,
    fine: Option<String>,
    at: f64,
}

pub struct FrontEnd {
    pub(crate) rs: RenderState,
    pub(crate) wake: Wake,
    pub options: FrontOptions,
    pub(crate) room: RoomView,
    pub(crate) settings: Settings,
    pub(crate) view: View,
    pub(crate) session: Option<Session>,
    pub(crate) intent: Option<Intent>,
    pub(crate) out: Option<FrontAction>,
    pub(crate) splash: &'static str,
    pub(crate) error: Option<String>,
    pub(crate) lost: bool,
    // the cursor
    pub(crate) cursor: Option<String>,
    pub(crate) marks: HashMap<View, String>,
    pub(crate) want: Option<String>,
    said: Said,
    pub(crate) arrived_at: f64,
    // the flows
    pub(crate) settings_from: Option<View>,
    pub(crate) settings_group: Option<String>,
    pub(crate) login_name: String,
    pub(crate) fields: HashMap<Field, String>,
    pub(crate) adding: Option<ServerInfo>,
    pub(crate) deleting: Option<Account>,
    pub(crate) versions_of: Option<Account>,
    pub(crate) rc: Option<RcEdit>,
    /// the pad's on-screen keyboard over a field or the rc file
    pub(crate) osk: crate::osk::FrontOsk,
    pub(crate) servers_from: View,
    pub(crate) servers_mode_watch: bool,
    pub(crate) watch_on: Option<ServerInfo>,
    pub(crate) doc: Option<(String, Option<String>, DocBody, View)>,
    pub(crate) doc_text: Option<Result<String, String>>,
    doc_request: Option<http::Request>,
    pub(crate) doc_links: Vec<(String, String)>,
    pub(crate) doc_scroll: Option<f32>,
    pub(crate) doc_offset: f32,
    pub(crate) doc_height: f32,
    pub(crate) news_since: Option<Option<String>>,
    // the roster
    pub(crate) roster_seen: HashSet<i64>,
    pub(crate) roster_first: bool,
    pub(crate) details_open: HashSet<i64>,
    // the connection
    retry_at: Option<f64>,
    retries: u32,
    rejoining: Option<Rejoin>,
    whereis: HashMap<String, Option<Whereis>>,
    asked: HashMap<String, f64>,
    where_reads: Vec<WhereRead>,
    pub(crate) pings: crate::ping::Pings,
    pub(crate) gamepad_tips_reset: bool,
    pub(crate) scroll_to_cursor: bool,
    pub(crate) cursor_rect: Option<egui::Rect>,
    pub(crate) news_glide_for: Option<String>,
    pub(crate) news_tops: Vec<(String, f32, f32)>,
    /// The player a spectate was asked for, until the server starts it.
    watching: Option<String>,
    pub(crate) boot_text: String,
    pub(crate) last_frame: f64,
}

impl FrontEnd {
    /// The front end in an eframe app: `rs` draws the room, `wake` asks for a frame when a
    /// connection's news lands.
    pub fn new(ctx: &egui::Context, rs: RenderState, options: FrontOptions) -> Self {
        let c = ctx.clone();
        let wake: Wake = Arc::new(move || c.request_repaint());
        let settings = settings::get();
        let mut room = RoomView::new(next_home_place());
        room.reduced_motion = std::env::var_os("ISORUN_REDUCED_MOTION").is_some();
        let splash = crate::splash::pick_splash(crate::splash::random(), &crate::splash::XOM_SPLASHES);
        let mut f = FrontEnd {
            rs,
            wake,
            options,
            room,
            settings,
            view: View::Home,
            session: None,
            intent: None,
            out: None,
            splash,
            error: None,
            lost: false,
            cursor: None,
            marks: HashMap::new(),
            want: None,
            said: Said::default(),
            arrived_at: -1.0,
            settings_from: None,
            settings_group: None,
            login_name: String::new(),
            fields: HashMap::new(),
            adding: None,
            deleting: None,
            versions_of: None,
            rc: None,
            osk: Default::default(),
            servers_from: View::Accounts,
            servers_mode_watch: false,
            watch_on: None,
            doc: None,
            doc_text: None,
            doc_request: None,
            doc_links: Vec::new(),
            doc_scroll: None,
            doc_offset: 0.0,
            doc_height: 0.0,
            news_since: None,
            roster_seen: HashSet::new(),
            roster_first: true,
            details_open: HashSet::new(),
            retry_at: None,
            retries: 0,
            rejoining: None,
            whereis: HashMap::new(),
            asked: HashMap::new(),
            where_reads: Vec::new(),
            pings: crate::ping::Pings::default(),
            gamepad_tips_reset: false,
            scroll_to_cursor: false,
            cursor_rect: None,
            news_glide_for: None,
            news_tops: Vec::new(),
            watching: None,
            boot_text: String::new(),
            last_frame: isorun_client::platform::seconds(),
        };
        f.show_home();
        f
    }

    /// The screen on show.
    pub fn view(&self) -> &View {
        &self.view
    }

    /// The settings as the front end keeps them (the game screen reads the same).
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether the front end is resting on the home screen with nothing over it.
    pub fn resting(&self) -> bool {
        self.view == View::Home && self.intent.is_none()
    }

    /// The window's title for the screen on show (servers.ts `screenTitle`).
    pub fn title(&self) -> &'static str {
        if self.options.can_quit {
            servers::APP_NAME
        } else {
            servers::HOME_TITLE
        }
    }

    // ------------------------------------------------------------------ the connection

    fn chosen(&self) -> Option<(Account, ServerInfo)> {
        let a = servers::get_chosen_account()?;
        let s = servers::find_server(&a.server_id)?;
        Some((a, s))
    }

    /// The open session on `server` for `username`, when that is what it is: the warm connection.
    pub(crate) fn session_on(&self, server: &ServerInfo, username: Option<&str>) -> Option<&Session> {
        self.session.as_ref().filter(|s| !s.closed && s.server.id == server.id && s.username.as_deref() == username)
    }

    fn has_session_on(&self, server: &ServerInfo, username: Option<&str>) -> bool {
        self.session_on(server, username).is_some()
    }

    /// Open a session on `server` for `username` (none while an account is being added),
    /// replacing whatever was open; `intent` says what to do once it is up.
    fn open_session(&mut self, server: ServerInfo, username: Option<String>, intent: Option<Intent>) {
        if let Some(mut s) = self.session.take() {
            s.close();
        }
        self.intent = intent;
        self.session = Some(Session::new(server, username, self.wake.clone()));
    }

    /// Connect as the chosen account while the front end is up, so its lobby comes up logged in.
    fn warm(&mut self) {
        let Some((account, server)) = self.chosen() else { return };
        if self.has_session_on(&server, Some(&account.username)) {
            return;
        }
        // a connection waiting to be tried again is the retry's to open
        if self.retry_at.is_some() && self.session.as_ref().is_some_and(|s| s.server.id == server.id && s.username.as_deref() == Some(&account.username)) {
            return;
        }
        self.open_session(server, Some(account.username), None);
    }

    /// The chosen account's connection again, when another has taken its place (the login of an
    /// account being added, which is nobody's until it logs in). A connection that is somebody's
    /// is left be, closed or not, and so is a spectate.
    fn rewarm(&mut self) {
        let Some((account, server)) = self.chosen() else { return };
        if let Some(s) = &self.session {
            if s.username.is_some() || s.state.watching.is_some() {
                return;
            }
        }
        self.open_session(server, Some(account.username), None);
    }

    /// A session on `server` for `username` to log in to, register on, or watch from: the warm
    /// one, or a fresh one.
    fn ensure_session(&mut self, server: &ServerInfo, username: Option<&str>) {
        if self.has_session_on(server, username) {
            return;
        }
        self.open_session(server.clone(), username.map(str::to_string), None);
    }

    /// Start (or continue) `game_id` on the session: which version was played last is kept, so the
    /// home screen leads with it.
    fn play(&mut self, game_id: &str) {
        let Some(s) = &mut self.session else { return };
        servers::set_last(&servers::LastPlayed {
            server_id: s.server.id.clone(),
            game_id: Some(game_id.to_string()),
            username: s.state.lobby.username.clone(),
            gamedata_version: servers::get_last().and_then(|l| l.gamedata_version),
        });
        s.send(cm::play(game_id));
        self.intent = Some(Intent::Play(game_id.to_string()));
    }

    /// Open a session for `account` and show its home screen. With an intent (Continue, Play) the
    /// game starts on its own once the login is through, and the screen stays up meanwhile.
    /// `played` (the way back from a game) forgets what the server said was waiting.
    pub(crate) fn connect_to(&mut self, account: &Account, intent: Option<Intent>, played: bool) {
        if intent.is_some() {
            self.lost = false;
        }
        let Some(server) = servers::find_server(&account.server_id) else { return self.show_home() };
        let same = self.has_session_on(&server, Some(&account.username));
        if same {
            let s = self.session.as_mut().unwrap();
            match &intent {
                Some(Intent::Watch(who)) if s.conn.open => {
                    s.send(cm::watch(who));
                    self.intent = None;
                }
                Some(Intent::Play(game)) if s.state.lobby.username.is_some() => {
                    let g = game.clone();
                    self.play(&g);
                }
                _ => self.intent = intent,
            }
        } else {
            self.open_session(server.clone(), Some(account.username.clone()), intent);
        }
        if servers::get_last().is_none_or(|l| l.server_id != server.id) {
            servers::set_last(&servers::LastPlayed { server_id: server.id.clone(), username: Some(account.username.clone()), ..Default::default() });
        }
        if same && self.view == View::Home {
            return;
        }
        if played {
            self.asked.clear();
            self.whereis.clear();
        }
        self.show_home();
    }

    /// A game came back: the home screen (or the Watch screen out of a spectate) on its session.
    pub fn game_over(&mut self, end: GameEnd) {
        match end {
            GameEnd::Lobby(s) => {
                let watching = s.state.watching.is_some() || s.state.phase == Phase::Watching;
                let watch_on = if s.username.is_none() { Some(s.server.clone()) } else { None };
                self.session = Some(*s);
                self.intent = None;
                self.asked.clear();
                self.whereis.clear();
                if watching {
                    self.watch_on = watch_on;
                    self.roster_first = true;
                    self.show_watch();
                } else {
                    self.show_home();
                }
            }
            GameEnd::Leave(mut s) => {
                let account = account_of(&s);
                s.close();
                self.intent = None;
                match account {
                    Some(a) => self.connect_to(&a, None, true),
                    None => self.show_home(),
                }
            }
            GameEnd::Dropped(mut s) => {
                let account = account_of(&s);
                let live = s.state.phase != Phase::Ended;
                let watching = s.state.watching.clone();
                let game = s.state.version.game_id.clone().or_else(|| servers::get_last().and_then(|l| l.game_id));
                let server = s.server.clone();
                s.close();
                let intent = match (watching, game) {
                    (Some(w), _) => Some(Intent::Watch(w)),
                    (None, Some(g)) if live && account.is_some() => Some(Intent::Play(g)),
                    _ => None,
                };
                match intent {
                    Some(i) => {
                        self.rejoining = Some(Rejoin { account: account.filter(|_| s.username.is_some()), server, intent: i, tries: 0, at: None });
                        self.boot("Reconnecting…");
                        self.rejoin();
                    }
                    None => {
                        match account {
                            Some(a) => self.connect_to(&a, None, true),
                            None => self.show_home(),
                        }
                        self.dropped();
                    }
                }
            }
        }
    }

    /// A game's connection dropped on its way here: the home screen says so, and that the game is saved.
    pub fn dropped(&mut self) {
        self.lost = true;
    }

    fn boot(&mut self, text: &str) {
        self.view = View::Boot;
        self.boot_text = text.to_string();
    }

    /// The next try at the game `rejoining` names; with no token left to log in with, the menu.
    fn rejoin(&mut self) {
        let Some(j) = &mut self.rejoining else { return };
        j.at = None;
        if let Intent::Play(_) = j.intent {
            let user = j.account.as_ref().map(|a| a.username.clone()).unwrap_or_default();
            if !j.server.offline && servers::get_token(&j.server.id, &user).is_none() {
                let account = j.account.clone();
                self.rejoining = None;
                match account {
                    Some(a) => self.connect_to(&a, None, true),
                    None => self.show_home(),
                }
                self.dropped();
                return;
            }
        }
        j.tries += 1;
        let (server, user, intent) = (j.server.clone(), j.account.as_ref().map(|a| a.username.clone()), j.intent.clone());
        self.open_session(server, user, Some(intent));
    }

    /// A drop of the warm connection: out of a rejoin, the next try or the menu; on a front-end
    /// screen, the same connection again after a wait that doubles for each unanswered try.
    fn schedule_retry(&mut self, now: f64) {
        if let Some(j) = &mut self.rejoining {
            if j.tries <= REJOIN_SECONDS.len() {
                j.at = Some(now + REJOIN_SECONDS[j.tries.saturating_sub(1).min(REJOIN_SECONDS.len() - 1)]);
                return;
            }
            let account = j.account.clone();
            self.rejoining = None;
            self.session = None;
            match account {
                Some(a) => self.connect_to(&a, None, true),
                None => self.show_home(),
            }
            self.dropped();
            return;
        }
        let Some(s) = &self.session else { return };
        let Some(account) = account_of(s) else { return };
        let Some(chosen) = servers::get_chosen_account() else { return };
        if chosen.server_id != account.server_id || chosen.username != account.username {
            return;
        }
        let wait = (RETRY_SECONDS * 2f64.powi(self.retries as i32)).min(RETRY_MAX_SECONDS);
        self.retries += 1;
        self.retry_at = Some(now + wait);
    }

    /// The session's news since the last frame: what main.ts and the front end's `follow` do with it.
    fn poll_session(&mut self, now: f64) {
        if let Some(at) = self.retry_at {
            if now >= at {
                self.retry_at = None;
                if let Some(s) = &mut self.session {
                    s.reconnect();
                }
            }
        }
        if let Some(at) = self.rejoining.as_ref().and_then(|j| j.at) {
            if now >= at {
                self.rejoin();
            }
        }
        let Some(s) = &mut self.session else { return };
        s.flush_scene();
        let events = s.poll();
        for e in events {
            match e {
                SessionEvent::Open => {
                    self.retries = 0;
                    if let Some(Intent::Watch(who)) = self.intent.clone() {
                        // spectating needs no account: go as soon as the socket is up
                        if let Some(s) = &mut self.session {
                            s.send(cm::watch(&who));
                        }
                        self.watching = Some(who);
                        self.intent = None;
                    }
                }
                SessionEvent::State(m) => self.on_message(&m),
                SessionEvent::Closed(reason) => {
                    // only a screen that stands on the connection says it
                    let says = matches!(self.view, View::Login | View::Register | View::Home | View::Watch | View::Accounts | View::Versions | View::Rc);
                    self.schedule_retry(now);
                    if says {
                        self.error =
                            Some(if self.retry_at.is_some() { "Connection lost. Reconnecting…".into() } else { format!("Connection closed: {reason}") });
                    }
                    if matches!(self.view, View::Login | View::Register) {
                        self.show_home();
                    }
                }
                _ => {}
            }
            if self.out.is_some() {
                return;
            }
        }
    }

    fn on_message(&mut self, m: &Value) {
        let kind = m.get("msg").and_then(Value::as_str).unwrap_or("");
        match kind {
            "login_success" => {
                if let Some(Intent::Play(g)) = self.intent.clone() {
                    // the server refuses `play` before login; the token login just finished
                    self.play(&g);
                }
                if matches!(self.view, View::Login | View::Register) {
                    self.logged_in();
                }
            }
            "login_fail" => {
                if matches!(self.intent, Some(Intent::Play(_))) {
                    // a Continue whose token was refused: the login form comes up, and the intent waits for it
                    self.show_login();
                }
            }
            "register_fail" if self.view == View::Register => {}
            "set_game_links" => {
                if let Some(s) = &self.session {
                    servers::set_games(&s.server.id, &s.state.lobby.games);
                }
            }
            "game_client" => {
                if let (Some(s), Some(v)) = (&self.session, m.get("version").and_then(Value::as_str)) {
                    if let Some(mut last) = servers::get_last() {
                        if last.server_id == s.server.id && last.gamedata_version.as_deref() != Some(v) {
                            last.gamedata_version = Some(v.to_string());
                            servers::set_last(&last);
                        }
                    }
                }
            }
            "game_ended" => {
                if let Some(s) = &self.session {
                    let who = s.state.lobby.username.clone().or(s.username.clone());
                    if let (Some(who), Some(dump)) = (who, m.get("dump").and_then(Value::as_str)) {
                        if let Some(dir) = whereis::morgue_dir_of(&s.server, dump) {
                            servers::set_morgue_dir(&s.server.id, &who, &dir);
                        }
                    }
                }
            }
            "stale_processes" if self.view == View::Boot => {
                let t = m.get("timeout").and_then(Value::as_i64).filter(|t| *t > 0).unwrap_or(10);
                self.boot_text = format!("Closing your last session… about {t} s");
            }
            "force_terminate?" if self.view == View::Boot => {
                self.rejoining = None;
                self.show_home();
            }
            _ => {}
        }
        // a game or a spectate is on: the game screen takes the session
        if matches!(kind, "game_client" | "watching_started" | "game_started") {
            self.start_game();
        }
    }

    fn start_game(&mut self) {
        let Some(s) = self.session.take() else { return };
        self.rejoining = None;
        let watching = s
            .state
            .watching
            .clone()
            .or(match &self.intent {
                Some(Intent::Watch(w)) => Some(w.clone()),
                _ => None,
            })
            .or(self.watching.take())
            .filter(|_| s.state.phase != Phase::Playing);
        self.intent = None;
        self.boot("Entering the dungeon…");
        self.out = Some(match watching {
            Some(username) => FrontAction::Watch { session: Box::new(s), username },
            None => {
                let game_id = s.state.version.game_id.clone().or_else(|| servers::get_last().and_then(|l| l.game_id)).unwrap_or_default();
                let account = account_of(&s);
                FrontAction::Play { session: Box::new(s), account, game_id }
            }
        });
    }

    /// Through Log in or Register: the account is kept, chosen, and its screen comes up.
    fn logged_in(&mut self) {
        let Some(s) = &self.session else { return };
        let Some(username) = s.state.lobby.username.clone() else { return };
        let account = Account { server_id: s.server.id.clone(), username: username.clone() };
        servers::add_account(&account);
        servers::set_chosen_account(Some(&account));
        servers::set_last(&servers::LastPlayed { server_id: account.server_id.clone(), username: Some(username), ..Default::default() });
        self.adding = None;
        self.login_name.clear();
        self.fields.clear();
        self.asked.clear();
        self.whereis.clear();
        self.show_home();
    }

    // ------------------------------------------------------------------ what waits in a save

    /// What the server says is waiting in an account's save, read in the background; asked again
    /// no more than once a minute while the home screen stands.
    pub(crate) fn where_for(&mut self, server: &ServerInfo, who: Option<&str>) -> Option<Whereis> {
        let who = who?;
        if server.offline {
            return None;
        }
        let key = format!("{}/{}", server.id, who.to_lowercase());
        let now = isorun_client::platform::seconds();
        let stale = self.asked.get(&key).is_none_or(|at| now - at > WHEREIS_SECONDS);
        if stale {
            self.asked.insert(key.clone(), now);
            let known = servers::get_morgue_dir(&server.id, who);
            if known.as_deref() != Some("") {
                let dirs = match &known {
                    Some(d) => vec![d.clone()],
                    None => whereis::morgue_dir_guesses(who),
                };
                self.start_where(WhereRead {
                    key: key.clone(),
                    server: server.clone(),
                    who: who.to_string(),
                    dirs,
                    known: known.is_some(),
                    at: 0,
                    request: http::get("about:blank", || {}),
                });
            }
        }
        self.whereis.get(&key).cloned().flatten()
    }

    fn start_where(&mut self, mut r: WhereRead) {
        while r.at < r.dirs.len() {
            if let Some(url) = whereis::whereis_url(&r.server, &r.dirs[r.at]) {
                let wake = self.wake.clone();
                r.request = http::get(&url, move || wake());
                self.where_reads.push(r);
                return;
            }
            r.at += 1;
        }
        if !r.known {
            servers::set_morgue_dir(&r.server.id, &r.who, "");
        }
    }

    fn poll_where(&mut self) {
        let reads = std::mem::take(&mut self.where_reads);
        for mut r in reads {
            match r.request.take() {
                None => self.where_reads.push(r),
                Some(Ok(bytes)) => match whereis::parse_whereis(&String::from_utf8_lossy(&bytes)) {
                    Some(w) => {
                        if !r.known {
                            servers::set_morgue_dir(&r.server.id, &r.who, &r.dirs[r.at]);
                        }
                        self.whereis.insert(r.key.clone(), Some(w));
                    }
                    None => {
                        r.at += 1;
                        self.start_where(r);
                    }
                },
                Some(Err(e)) => {
                    // a status the server answered with is no answer from this directory; no network is
                    // no answer at all, and the next redraw asks again
                    if e.contains("status") {
                        r.at += 1;
                        self.start_where(r);
                    } else {
                        self.asked.remove(&r.key);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ the frame

    /// Draw the front end filling the window; what the app is asked to do comes back.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<FrontAction> {
        CTX.with(|c| *c.borrow_mut() = Some(ui.ctx().clone()));
        let now = isorun_client::platform::seconds();
        self.last_frame = now;
        self.poll_session(now);
        if self.out.is_some() {
            return self.out.take();
        }
        self.poll_where();
        self.poll_doc();
        self.poll_rc(now);
        // a menu that needs the connection is drawn from its session every frame, so keep the
        // chosen account's warm while one is up
        let screen = self.screen();
        self.draw(ui, &screen);
        self.draw_osk(ui.ctx());
        if self.session.as_ref().is_some_and(|s| !s.closed) || self.retry_at.is_some() || !self.where_reads.is_empty() || self.doc_request.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        self.out.take()
    }

    /// A pad event: the stick and d-pad move the cursor, A chooses, B goes back, X opens the
    /// settings on the home screen, Y brings the roster's arrivals in, the shoulders walk the
    /// sections, and Start confirms like Enter.
    pub fn pad(&mut self, ev: PadEvent) {
        // the on-screen keyboard takes the pad while it is up (menu.ts `pad`)
        if self.osk_pad(ev) {
            return;
        }
        match ev {
            PadEvent::Look { dx, dy } => {
                if matches!(self.view, View::Doc | View::News) {
                    self.scroll_doc(dy * 12.0);
                } else {
                    self.room.look(dx);
                }
            }
            PadEvent::Dir(d) => self.nav(d),
            PadEvent::Press(b) => match b {
                // a field or the rc file is gone into with the on-screen keyboard from a pad
                PadButton::A => {
                    if !self.open_osk() {
                        self.activate();
                    }
                }
                PadButton::B => self.back(),
                PadButton::X => {
                    // nothing to type into on the front screen: X is the settings there, as it opens the keyboard everywhere else
                    if self.view == View::Home {
                        self.show_settings(Some(View::Home));
                    } else {
                        self.open_osk();
                    }
                }
                PadButton::Y => self.take_arrivals(),
                PadButton::LB => self.section(-1),
                PadButton::RB => self.section(1),
                PadButton::Start => self.confirm(),
                PadButton::Select => {}
            },
        }
    }

    // ------------------------------------------------------------------ the cursor

    pub(crate) fn current(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    /// The focus items of a screen, in the order and grid the cursor walks.
    pub(crate) fn items(&self, s: &Screen) -> Vec<(String, FocusSlot, Act)> {
        let mut raw: Vec<(String, Option<i32>, Option<i32>, Act)> = Vec::new();
        for (i, r) in s.rows.iter().enumerate() {
            if r.heading {
                continue;
            }
            raw.push((r.id.clone(), Some(i as i32), Some(0), if r.input.is_some() { Act::None } else { r.act.clone() }));
            for (k, a) in r.also.iter().enumerate() {
                raw.push((a.id.clone(), Some(i as i32), Some(k as i32 + 1), a.act.clone()));
            }
        }
        for n in &s.notices {
            if n.ask {
                raw.push(("terminate:yes".into(), Some(100), Some(0), Act::Terminate(true)));
                raw.push(("terminate:no".into(), Some(100), Some(1), Act::Terminate(false)));
            }
        }
        match &s.below {
            Below::Roster => {
                for (i, e) in self.roster_entries().iter().enumerate() {
                    raw.push((format!("watch:{}", e.id), Some(300 + i as i32), Some(0), Act::WatchPlayer(e.username.clone())));
                }
            }
            Below::Settings(group) => {
                for (i, (id, act)) in self.settings_items(group).into_iter().enumerate() {
                    raw.push((id, Some(300 + i as i32), Some(0), act));
                }
            }
            Below::News => {
                for (i, r) in self.releases().iter().enumerate() {
                    raw.push((format!("release:{}", r.version), Some(300 + i as i32), Some(0), Act::None));
                }
            }
            _ => {}
        }
        for (k, (label, href)) in s.links.iter().enumerate() {
            raw.push((format!("link:{k}"), Some(400 + k as i32), Some(0), Act::Open(href.clone())));
            let _ = label;
        }
        for (i, u) in s.utilities.iter().enumerate() {
            raw.push((u.id.clone(), Some(500), Some(i as i32), u.act.clone()));
        }
        let slots = focus_slots(&raw.iter().map(|(_, r, c, _)| (*r, *c)).collect::<Vec<_>>());
        raw.into_iter().zip(slots).map(|((id, _, _, act), slot)| (id, slot, act)).collect()
    }

    /// Put the cursor where the screen wants it: the row it stood on, else the one asked for, else
    /// the screen's way in, never Back on its own.
    pub(crate) fn settle_cursor(&mut self, s: &Screen) {
        let items = self.items(s);
        let has = |id: &str| items.iter().any(|(i, _, _)| i == id);
        if let Some(c) = &self.cursor {
            if has(c) && self.want.is_none() {
                return;
            }
        }
        let want = self.want.take().or_else(|| s.focus.clone()).or_else(|| self.marks.get(&self.view).cloned());
        let pick = want
            .filter(|w| w != BACK && has(w))
            .or_else(|| s.rows.iter().find(|r| r.main && has(&r.id)).map(|r| r.id.clone()))
            .or_else(|| items.iter().find(|(i, _, _)| i != BACK).map(|(i, _, _)| i.clone()))
            .or_else(|| items.first().map(|(i, _, _)| i.clone()));
        if pick != self.cursor {
            self.cursor = pick;
            self.on_cursor(s);
        }
    }

    /// The cursor landed on a row: the message line says what it is, unless the row says it already.
    pub(crate) fn on_cursor(&mut self, s: &Screen) {
        let Some(c) = self.cursor.clone() else { return };
        if self.view == View::Rc && self.rc.as_ref().is_some_and(|r| r.leaving) {
            return self.say("Your changes are not saved. Back again leaves without them; Save keeps them.", None);
        }
        let mut said = None;
        for r in s.rows.iter().chain(s.utilities.iter()) {
            if r.id == c {
                said = Some((if r.sub.as_deref() == Some(r.hint.as_str()) { String::new() } else { r.hint.clone() }, r.fine.clone()));
            }
            for a in &r.also {
                if a.id == c {
                    said = Some((a.title.clone(), None));
                }
            }
        }
        if said.is_none() {
            if c.starts_with("watch:") {
                if let Some(e) = self.roster_entries().iter().find(|e| format!("watch:{}", e.id) == c) {
                    said = Some((words::roster_here(e), None));
                }
            } else if c.starts_with("release:") {
                said = Some(("Up and down step through the releases.".into(), None));
            } else if c.starts_with("link:") {
                said = Some(("Opens in a new window. Your game stays here.".into(), None));
            } else if let Below::Settings(group) = &s.below {
                said = self.settings_hint(group, &c).map(|h| (h, None));
            }
        }
        if let Some((t, f)) = said {
            self.say(&t, f);
        }
    }

    /// The message line: what the row under the cursor is, said the way the game says what is underfoot.
    pub(crate) fn say(&mut self, text: &str, fine: Option<String>) {
        if self.said.text == text && self.said.fine == fine {
            return;
        }
        // a redraw handing the line the same words lets them stand without fading in again
        if self.said.text != text {
            self.said.at = isorun_client::platform::seconds();
        }
        self.said.text = text.to_string();
        self.said.fine = fine;
    }

    pub(crate) fn said(&self) -> (&str, Option<&str>, f64) {
        (&self.said.text, self.said.fine.as_deref(), self.said.at)
    }

    /// Move the cursor (up and down; left and right along a row, or turn a setting).
    pub(crate) fn nav(&mut self, d: NavDir) {
        if matches!(self.view, View::Doc) && matches!(d, NavDir::Up | NavDir::Down) {
            let step = self.doc_step();
            self.scroll_doc(if d == NavDir::Up { -step } else { step });
            return;
        }
        let s = self.screen();
        if matches!(d, NavDir::Left | NavDir::Right) {
            let dir = if d == NavDir::Right { 1 } else { -1 };
            if let Some(c) = self.cursor.clone() {
                if let Some(key) = c.strip_prefix("setting:") {
                    let key = key.to_string();
                    self.turn_setting(&key, dir);
                    let group = self.settings_group.clone().unwrap_or_default();
                    if let Some(h) = self.settings_hint(&group, &c) {
                        self.say(&h, None);
                    }
                    return;
                }
                if let Some(id) = c.strip_prefix("watch:").and_then(|i| i.parse::<i64>().ok()) {
                    if dir > 0 {
                        self.details_open.insert(id);
                    } else {
                        self.details_open.remove(&id);
                    }
                    return;
                }
            }
        }
        let items = self.items(&s);
        if items.is_empty() {
            return;
        }
        let slots: Vec<FocusSlot> = items.iter().map(|(_, s, _)| *s).collect();
        let at = self.cursor.as_ref().and_then(|c| items.iter().position(|(i, _, _)| i == c)).unwrap_or(0);
        let dir = match d {
            NavDir::Up => FocusDir::Up,
            NavDir::Down => FocusDir::Down,
            NavDir::Left => FocusDir::Left,
            NavDir::Right => FocusDir::Right,
        };
        if let Some(n) = focus_move(&slots, at, dir, true) {
            self.cursor = Some(items[n].0.clone());
            self.on_cursor(&s);
            self.scroll_to_cursor = true;
        }
    }

    /// Choose the row under the cursor.
    pub(crate) fn activate(&mut self) {
        let s = self.screen();
        let Some(c) = self.cursor.clone() else { return };
        if c == "rc:text" {
            if let Some(rc) = &mut self.rc {
                if rc.state == RcState::Ready {
                    rc.editing = true;
                }
            }
            return;
        }
        if let Some(act) = self.items(&s).into_iter().find(|(i, _, _)| *i == c).map(|(_, _, a)| a) {
            self.perform(act);
        }
    }

    /// Start is Enter: submit a form rather than open its keyboard.
    fn confirm(&mut self) {
        let s = self.screen();
        if self.cursor.as_ref().is_some_and(|c| s.rows.iter().any(|r| &r.id == c && r.input.is_some())) {
            self.submit_form();
        } else {
            self.activate();
        }
    }

    /// The form of the screen on show, sent (Enter in a field).
    pub(crate) fn submit_form(&mut self) {
        match self.view {
            View::Login => {
                if self.adding.as_ref().is_some_and(|a| a.offline) {
                    self.perform(Act::SubmitNewProfile)
                } else {
                    self.perform(Act::SubmitLogin)
                }
            }
            View::Register => self.perform(Act::SubmitRegister),
            View::AddServer => self.perform(Act::SubmitAddServer),
            _ => self.activate(),
        }
    }

    /// The next or previous section: Play (the home screen), Watch, Settings. Inert in a flow.
    pub(crate) fn section(&mut self, d: i32) {
        let v = match self.view {
            View::Controls | View::SettingsGroup => View::Settings,
            ref v => v.clone(),
        };
        let sections = [View::Home, View::Watch, View::Settings];
        let Some(i) = sections.iter().position(|s| *s == v) else { return };
        let mut next = &sections[((i as i32 + d).rem_euclid(3)) as usize];
        // nobody else plays on this device: Watch is not a section of it
        if *next == View::Watch && self.chosen().is_some_and(|(_, s)| s.offline) {
            next = &sections[((i as i32 + 2 * d).rem_euclid(3)) as usize];
        }
        match next {
            View::Home => self.go_home(),
            View::Watch => {
                if self.chosen().is_some() {
                    self.show_watch()
                } else {
                    self.show_servers(true, None)
                }
            }
            _ => self.show_settings(Some(View::Home)),
        }
    }

    pub(crate) fn back(&mut self) {
        self.close_osk();
        match self.view {
            View::Home | View::Boot => {}
            View::Settings => {
                let from = self.settings_from.take();
                match from {
                    Some(View::Watch) => self.show_watch(),
                    _ => self.show_home(),
                }
            }
            View::Controls => self.show_settings_group("Controls"),
            View::SettingsGroup => {
                let from = self.settings_from.clone();
                self.show_settings(from)
            }
            View::News => self.show_home(),
            View::Doc => {
                let from = self.doc.as_ref().map(|d| d.3.clone());
                match from {
                    Some(View::Doc) => self.show_app(),
                    _ => self.show_home(),
                }
            }
            View::Watch => self.go_home(),
            View::Accounts => self.show_home(),
            View::Versions => {
                let a = self.versions_of.take();
                self.show_accounts(a.map(|a| format!("versions:{}/{}", a.server_id, a.username)));
            }
            View::Rc => self.leave_rc(),
            View::Exit => {
                if let Some(s) = &mut self.session {
                    s.state.exit = None;
                }
                self.show_home();
            }
            View::Servers => {
                if self.watch_on.is_some() || self.servers_from == View::Home {
                    self.show_home()
                } else {
                    self.show_accounts(None)
                }
            }
            View::AddServer => {
                self.fields.remove(&Field::Address);
                let watch = self.servers_mode_watch;
                self.show_servers(watch, Some("add-server".into()));
            }
            View::Login => self.cancel_auth(),
            View::Register => self.show_login(),
        }
    }

    /// Leave the screen on show for `v`: the row the cursor was on is remembered.
    pub(crate) fn set_view(&mut self, v: View) {
        if let Some(c) = &self.cursor {
            self.marks.insert(self.view.clone(), c.clone());
        }
        if v != self.view {
            self.cursor = None;
            self.arrived_at = isorun_client::platform::seconds();
            self.doc_scroll = None;
        }
        if v != View::Watch {
            self.roster_first = true;
        }
        self.view = v;
    }

    pub(crate) fn go_home(&mut self) {
        self.show_home();
    }

    // ------------------------------------------------------------------ the screens' doors

    pub fn show_home(&mut self) {
        servers::ensure_device_account();
        self.adding = None;
        self.watch_on = None;
        if self.view == View::Boot && self.rejoining.is_some() {
            return;
        }
        self.rewarm();
        self.warm();
        let had = self.view == View::Home;
        self.set_view(View::Home);
        if !had {
            self.want = self.marks.get(&View::Home).cloned();
        }
    }

    /// Open the screen an address names (menu.ts `open`, the web build's paths: `settings`,
    /// `settings/graphics`, `settings/controls/gamepad`, `accounts`, `accounts/add`,
    /// `accounts/add/server`, `watch`, `watch/add`, `whats-new`, `about`, `app`, `app/steam`,
    /// `login/<server>`, `register/<server>`). A name it does not know is the home screen.
    pub fn open(&mut self, path: &str) {
        let parts: Vec<String> = path.trim_matches('/').split('/').map(|p| p.to_lowercase()).collect();
        let part = |i: usize| parts.get(i).map(String::as_str).unwrap_or("");
        match (part(0), part(1), part(2)) {
            ("settings", "", _) => self.show_settings(Some(View::Home)),
            ("settings", "controls", "gamepad") => {
                self.settings_from = Some(View::Home);
                self.settings_group = Some("Controls".into());
                self.set_view(View::Controls);
            }
            ("settings", g, _) => match settings::SETTING_GROUPS.iter().find(|(n, _)| n.to_lowercase() == g) {
                Some((n, _)) => {
                    self.settings_from = Some(View::Home);
                    self.show_settings_group(n)
                }
                None => self.show_settings(Some(View::Home)),
            },
            ("accounts", "add", "server") => {
                self.servers_mode_watch = false;
                self.perform(Act::AddServer)
            }
            ("accounts", "add", _) => self.show_servers(false, None),
            ("accounts", _, _) => self.show_accounts(None),
            ("watch", "add", _) => {
                self.servers_mode_watch = true;
                self.perform(Act::AddServer)
            }
            ("watch", _, _) => self.show_servers(true, None),
            ("whats-new", _, _) => self.show_news(),
            ("about", _, _) => self.perform(Act::About),
            ("app", "steam", _) => self.perform(Act::Steam),
            ("app", _, _) => self.show_app(),
            ("login" | "register", server, _) => match servers::find_server(server) {
                Some(sv) => {
                    self.adding = Some(sv);
                    if part(0) == "register" {
                        self.show_register()
                    } else {
                        self.show_login()
                    }
                }
                None => self.show_home(),
            },
            _ => self.show_home(),
        }
    }

    pub(crate) fn show_settings(&mut self, from: Option<View>) {
        if self.view == View::Settings {
            return;
        }
        let back = from.or_else(|| match self.view {
            View::Watch => Some(View::Watch),
            View::Controls | View::SettingsGroup => self.settings_from.clone(),
            _ => Some(View::Home),
        });
        self.settings_from = back;
        self.set_view(View::Settings);
    }

    pub(crate) fn show_settings_group(&mut self, group: &str) {
        self.settings_group = Some(group.to_string());
        self.set_view(View::SettingsGroup);
    }

    pub(crate) fn show_accounts(&mut self, focus: Option<String>) {
        if focus.is_none() && self.view != View::Accounts {
            self.deleting = None;
        }
        self.adding = None;
        self.watch_on = None;
        self.rewarm();
        self.set_view(View::Accounts);
        if focus.is_some() {
            self.want = focus;
        }
    }

    pub(crate) fn show_servers(&mut self, watch: bool, focus: Option<String>) {
        if matches!(self.view, View::Home | View::Accounts) {
            self.servers_from = self.view.clone();
        }
        self.servers_mode_watch = watch;
        self.adding = None;
        self.watch_on = None;
        self.set_view(View::Servers);
        if focus.is_some() {
            self.want = focus;
        }
    }

    pub(crate) fn show_login(&mut self) {
        let chosen = if self.adding.is_some() { None } else { self.chosen() };
        let Some(server) = self.adding.clone().or(chosen.as_ref().map(|c| c.1.clone())) else { return self.show_servers(false, None) };
        if !server.offline {
            self.ensure_session(&server, chosen.as_ref().map(|c| c.0.username.as_str()));
        }
        if self.login_name.is_empty() {
            if let Some((a, _)) = &chosen {
                self.login_name = a.username.clone();
            }
        }
        if !self.fields.contains_key(&Field::Name) {
            self.fields.insert(Field::Name, self.login_name.clone());
        }
        self.set_view(View::Login);
        self.want = Some(if self.login_name.is_empty() || server.offline { "username".into() } else { "password".into() });
    }

    pub(crate) fn show_register(&mut self) {
        let Some(server) = self.adding.clone().or(self.chosen().map(|c| c.1)) else { return self.show_servers(false, None) };
        let user = if self.adding.is_some() { None } else { self.chosen().map(|c| c.0.username) };
        self.ensure_session(&server, user.as_deref());
        self.set_view(View::Register);
        self.want = Some("username".into());
    }

    pub(crate) fn show_watch(&mut self) {
        let chosen = if self.watch_on.is_some() { None } else { self.chosen() };
        let Some(server) = self.watch_on.clone().or(chosen.as_ref().map(|c| c.1.clone())) else { return self.show_servers(true, None) };
        if server.offline {
            return self.show_home();
        }
        let username = chosen.map(|c| c.0.username);
        self.ensure_session(&server, username.as_deref());
        if self.view != View::Watch {
            self.roster_first = true;
            self.details_open.clear();
        }
        self.set_view(View::Watch);
    }

    pub(crate) fn show_news(&mut self) {
        if self.news_since.is_none() {
            self.news_since = Some(crate::news::news_seen());
        }
        crate::news::mark_news_seen();
        self.set_view(View::News);
        self.doc_scroll = Some(0.0);
    }

    pub(crate) fn show_doc(&mut self, title: &str, lede: Option<String>, body: DocBody, from: View) {
        self.doc = Some((title.to_string(), lede, body, from));
        self.doc_links.clear();
        self.set_view(View::Doc);
        self.want = Some(BACK.into());
        self.doc_scroll = Some(0.0);
    }

    pub(crate) fn show_app(&mut self) {
        self.doc = None;
        self.set_view(View::Doc);
        self.want = Some(format!("app:{}", APP_DOWNLOADS[0].0));
    }

    pub(crate) fn show_versions(&mut self, a: Account) {
        let Some(server) = servers::find_server(&a.server_id).filter(|s| !s.offline) else { return self.show_accounts(None) };
        let again = self.view == View::Versions && self.versions_of.as_ref().is_some_and(|v| servers::same_account(Some(v), Some(&a)));
        if !again {
            servers::set_chosen_account(Some(&a));
        }
        self.versions_of = Some(a.clone());
        self.ensure_session(&server, Some(&a.username));
        self.set_view(View::Versions);
    }

    pub(crate) fn show_rc(&mut self, a: Account, game_id: Option<String>) {
        let Some(server) = servers::find_server(&a.server_id) else { return self.show_accounts(None) };
        if !server.offline && game_id.is_none() {
            return self.show_accounts(None);
        }
        let again = self.rc.as_ref().is_some_and(|r| servers::same_account(Some(&r.account), Some(&a)) && r.game_id == game_id);
        if !again {
            servers::set_chosen_account(Some(&a));
            self.rc = Some(RcEdit {
                account: a.clone(),
                game_id,
                text: String::new(),
                saved: None,
                state: RcState::Reading,
                asked: false,
                note: None,
                leaving: false,
                pending: None,
                editing: false,
            });
        }
        self.ensure_session(&server, Some(&a.username));
        self.set_view(View::Rc);
        self.want = Some("rc:text".into());
    }

    fn cancel_auth(&mut self) {
        self.login_name.clear();
        self.fields.clear();
        if self.adding.as_ref().is_some_and(|a| a.offline) {
            self.show_accounts(Some("add-player".into()))
        } else if self.adding.is_some() {
            self.show_servers(false, None)
        } else {
            self.go_home()
        }
    }

    // ------------------------------------------------------------------ doing

    pub(crate) fn perform(&mut self, act: Act) {
        match act {
            Act::None => {}
            Act::Back => self.back(),
            Act::Home => self.go_home(),
            Act::Accounts => self.show_accounts(None),
            Act::News => self.show_news(),
            Act::About => self.show_doc("About & credits", None, DocBody::Markdown(crate::docs::ABOUT), View::Home),
            Act::App => self.show_app(),
            Act::Steam => {
                let from = if self.view == View::Doc { View::Doc } else { View::Home };
                self.show_doc("Add to Steam", None, DocBody::Markdown(crate::docs::STEAM), from)
            }
            Act::Settings => self.show_settings(Some(View::Home)),
            Act::SettingsGroup(g) => self.show_settings_group(&g),
            Act::Controls => self.set_view(View::Controls),
            Act::Fullscreen => {
                self.options.fullscreen = !self.options.fullscreen;
                self.out = Some(FrontAction::Fullscreen(self.options.fullscreen));
            }
            Act::Quit => {
                // the game lives on the server, which keeps it across a dropped socket
                if let Some(mut s) = self.session.take() {
                    s.close();
                }
                self.out = Some(FrontAction::Quit);
            }
            Act::Testbed => self.out = Some(FrontAction::Testbed),
            Act::ShareLog => {
                // the page shows the newest of it; the app hands all of it on
                let lede = if isorun_client::platform::android() {
                    "What the app wrote down, newest last. All of it went to the share sheet: send it with the report."
                } else {
                    "What the app wrote down, newest last. All of it is on the clipboard: paste it into the report."
                };
                self.show_doc("The app's log", Some(lede.into()), DocBody::Fetched, View::Home);
                let text = isorun_client::platform::diag::text();
                let lines: Vec<&str> = text.lines().collect();
                self.doc_text = Some(Ok(lines[lines.len().saturating_sub(LOG_PAGE_LINES)..].join("\n")));
                self.out = Some(FrontAction::ShareLog);
            }
            Act::Play(a, g) => self.connect_to(&a, Some(Intent::Play(g)), true),
            Act::Watch => self.show_watch(),
            Act::ServersWatch => self.show_servers(true, None),
            Act::ServersAdd => self.show_servers(false, None),
            Act::Login => self.show_login(),
            Act::ChooseAccount(a) => {
                servers::set_chosen_account(Some(&a));
                // a switch, not a return from a game: what the server said is waiting still stands
                self.connect_to(&a, None, false);
            }
            Act::Rc(a, g) => self.show_rc(a, g),
            Act::Versions(a) => self.show_versions(a),
            Act::LoginAs(a) => {
                servers::set_chosen_account(Some(&a));
                self.login_name = a.username.clone();
                self.fields.insert(Field::Name, a.username.clone());
                self.show_login();
            }
            Act::Remove(a) => {
                // the account goes first, so nothing opens it again on the way out
                servers::remove_account(&a);
                self.forget(&a);
                self.show_home();
            }
            Act::AskDelete(a) => {
                let id = format!("delete:{}/{}", a.server_id, a.username);
                self.deleting = Some(a);
                self.show_accounts(Some(id));
            }
            Act::Delete(a) => {
                self.deleting = None;
                servers::remove_account(&a);
                self.forget(&a);
                delete_profile_saves(&a.username);
                self.show_accounts(None);
            }
            Act::AddPlayer => {
                self.adding = Some(servers::offline_server());
                self.fields.clear();
                self.show_login();
            }
            Act::PickServer(sv) => {
                if self.servers_mode_watch {
                    self.watch_on = Some(sv);
                    self.show_watch();
                } else {
                    self.adding = Some(sv);
                    self.fields.clear();
                    self.login_name.clear();
                    self.show_login();
                }
            }
            Act::RemoveServer(id) => {
                servers::remove_server(&id);
                let w = self.servers_mode_watch;
                self.show_servers(w, Some("add-server".into()));
            }
            Act::AddServer => {
                self.set_view(View::AddServer);
                self.want = Some("address".into());
            }
            Act::SubmitAddServer => {
                let text = self.fields.get(&Field::Address).cloned().unwrap_or_default();
                match servers::add_server(&text) {
                    Ok(sv) => {
                        self.fields.remove(&Field::Address);
                        let w = self.servers_mode_watch;
                        self.show_servers(w, Some(format!("server:{}", sv.id)));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Act::SubmitLogin => {
                let name = self.fields.get(&Field::Name).cloned().unwrap_or_default();
                let pass = self.fields.get(&Field::Password).cloned().unwrap_or_default();
                self.login_name = name.clone();
                if let Some(s) = &mut self.session {
                    s.send(cm::login(&name, &pass));
                    s.username = Some(name.clone()).filter(|n| !n.is_empty());
                    let id = s.server.id.clone();
                    servers::set_last(&servers::LastPlayed { server_id: id, username: Some(name), ..Default::default() });
                }
                self.fields.remove(&Field::Password);
            }
            Act::SubmitRegister => {
                let get = |f| self.fields.get(&f).cloned().unwrap_or_default();
                let (name, pass, pass2, email) = (get(Field::Name), get(Field::Password), get(Field::Password2), get(Field::Email));
                if pass != pass2 {
                    self.error = Some("Passwords do not match.".into());
                    return;
                }
                if let Some(s) = &mut self.session {
                    s.send(cm::register(&name, &pass, &email));
                    let id = s.server.id.clone();
                    servers::set_last(&servers::LastPlayed { server_id: id, username: Some(name), ..Default::default() });
                }
            }
            Act::SubmitNewProfile => {
                let text = self.fields.get(&Field::Name).cloned().unwrap_or_default();
                self.login_name = text.clone();
                let account = isorun_client::engine::profile_name(&text).map(|n| Account { server_id: "offline".into(), username: n });
                let taken = account.as_ref().and_then(|a| servers::list_accounts().into_iter().find(|x| servers::same_account(Some(x), Some(a))));
                match (account, taken) {
                    (Some(a), None) => {
                        servers::add_account(&a);
                        servers::set_chosen_account(Some(&a));
                        self.adding = None;
                        self.login_name.clear();
                        self.fields.clear();
                        self.connect_to(&a, None, true);
                    }
                    (_, Some(t)) => self.error = Some(format!("{} is already on this device.", t.username)),
                    (None, None) => self.error = Some("A name needs a letter or a digit.".into()),
                }
            }
            Act::Register => self.show_register(),
            Act::ExitClose => {
                if let Some(s) = &mut self.session {
                    s.state.exit = None;
                }
                self.show_home();
            }
            Act::ExitDump { title, url, href } => {
                let lede = self.session.as_ref().map(|s| format!("{} keeps it; it reads here.", s.server.name));
                self.show_doc(&title, lede, DocBody::Fetched, View::Home);
                self.doc_links = vec![("Open in a new window".into(), href)];
                self.doc_text = None;
                match url {
                    Some(u) => {
                        let wake = self.wake.clone();
                        self.doc_request = Some(http::get(&u, move || wake()));
                    }
                    None => self.doc_text = Some(Err("not a URL this app reads".into())),
                }
            }
            Act::Open(url) => self.open_url(&url),
            Act::Terminate(yes) => {
                if let Some(s) = &mut self.session {
                    s.send(cm::force_terminate(yes));
                    s.state.lobby.force_terminate = false;
                }
            }
            Act::RcSave => self.save_rc(),
            Act::RcRevert => self.read_rc(true),
            Act::WatchPlayer(who) => {
                if let Some(s) = &mut self.session {
                    s.send(cm::watch(&who));
                }
                self.watching = Some(who);
            }
            Act::TakeArrivals => self.take_arrivals(),
            Act::Setting(key) => self.turn_setting(&key, 1),
            Act::ResetSettings(group) => {
                if !settings::group_at_defaults(&group, &self.settings) {
                    settings::reset_group(&group, &mut self.settings);
                    self.settings_changed();
                }
            }
            Act::ReplayTips => {
                self.settings.insert("hints".into(), "adaptive".into());
                self.gamepad_tips_reset = true;
                self.settings_changed();
            }
        }
    }

    pub(crate) fn open_url(&self, url: &str) {
        if let Some(ctx) = self.ctx() {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
    }

    fn ctx(&self) -> Option<egui::Context> {
        CTX.with(|c| c.borrow().clone())
    }

    /// Close a removed account's connection: its token, the server's cookie, the socket.
    fn forget(&mut self, account: &Account) {
        servers::set_token(&account.server_id, &account.username, None, None);
        if let Some(server) = servers::find_server(&account.server_id) {
            if self.has_session_on(&server, Some(&account.username)) {
                let mut s = self.session.take().unwrap();
                if let Some(cookie) = s.state.lobby.login_cookie.clone() {
                    s.send(cm::forget_login_cookie(&cookie.cookie));
                }
                s.close();
            }
        }
        self.retry_at = None;
        self.intent = None;
    }

    pub(crate) fn turn_setting(&mut self, key: &str, d: i32) {
        if let Some(r) = settings::rows().into_iter().find(|r| r.key == key) {
            settings::adjust(&r, &mut self.settings, d);
            self.settings_changed();
        }
    }

    /// A setting changed: it is kept, and shows on the room behind the panel at once.
    pub(crate) fn settings_changed(&mut self) {
        settings::save_settings(&self.settings);
        let dpr = self.ctx().map(|c| c.pixels_per_point()).unwrap_or(1.0);
        self.room.apply_settings(&self.settings, dpr);
    }

    /// Y on the roster (or the line that counts them): the arrivals held back come into the table.
    pub(crate) fn take_arrivals(&mut self) {
        if self.view != View::Watch {
            return;
        }
        if let Some(s) = &self.session {
            for e in &s.state.lobby.entries {
                self.roster_seen.insert(e.id);
            }
        }
    }

    // ------------------------------------------------------------------ the roster

    /// The entries the roster shows, by name; arrivals are held back from the table until the
    /// player asks for them (Y), so rows never move under the cursor; whoever leaves leaves at once.
    pub(crate) fn roster_entries(&self) -> Vec<isorun_webtiles::LobbyEntry> {
        let Some(s) = &self.session else { return Vec::new() };
        let mut all: Vec<_> = s.state.lobby.entries.clone();
        all.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()).then(a.id.cmp(&b.id)));
        all.truncate(200);
        all.into_iter().filter(|e| self.roster_seen.contains(&e.id)).collect()
    }

    /// Keep the roster's set of entries shown: the first draw takes everyone; later, whoever is
    /// gone goes, and arrivals wait once the roster has loaded and has rows to keep still.
    pub(crate) fn update_roster(&mut self) {
        let Some(s) = &self.session else { return };
        let ids: Vec<i64> = s.state.lobby.entries.iter().map(|e| e.id).collect();
        let complete = s.state.lobby.complete;
        if self.roster_first {
            self.roster_seen = ids.iter().copied().collect();
            self.roster_first = false;
        }
        self.roster_seen.retain(|id| ids.contains(id));
        if !complete || self.roster_seen.is_empty() {
            self.roster_seen.extend(ids.iter().copied());
        }
    }

    pub(crate) fn roster_held(&self) -> usize {
        let Some(s) = &self.session else { return 0 };
        s.state.lobby.entries.iter().take(200).filter(|e| !self.roster_seen.contains(&e.id)).count()
    }

    // ------------------------------------------------------------------ documents

    pub(crate) fn doc_step(&self) -> f32 {
        (self.doc_height / 5.0).round().max(48.0)
    }

    pub(crate) fn scroll_doc(&mut self, by: f32) {
        self.doc_scroll = Some((self.doc_offset + by).max(0.0));
    }

    fn poll_doc(&mut self) {
        if let Some(r) = &self.doc_request {
            if let Some(body) = r.take() {
                self.doc_request = None;
                self.doc_text = Some(body.map(|b| String::from_utf8_lossy(&b).into_owned()));
            }
        }
    }

    // ------------------------------------------------------------------ the rc file

    /// Ask for the file; `revert` puts it in the box over what was typed.
    pub(crate) fn read_rc(&mut self, revert: bool) {
        let Some(rc) = &self.rc else { return };
        if rc.state == RcState::Saving || (rc.state == RcState::Reading && rc.asked && !revert) || rc.pending.is_some() {
            return;
        }
        let game = rc.game_id.clone().unwrap_or_default();
        let Some(s) = &mut self.session else { return };
        let slot = s.read_rc(&game);
        let rc = self.rc.as_mut().unwrap();
        rc.asked = true;
        rc.state = RcState::Reading;
        rc.note = None;
        rc.pending = Some((slot, isorun_client::platform::seconds(), None));
    }

    /// Save the box, then read it back: only a file that reads back the same is called saved (a
    /// server says nothing after `set_rc`, whether it wrote the file or not).
    pub(crate) fn save_rc(&mut self) {
        let Some(rc) = &mut self.rc else { return };
        let Some(server) = servers::find_server(&rc.account.server_id) else { return };
        if rc.state != RcState::Ready {
            return;
        }
        let place = if server.offline { "this device".to_string() } else { server.host.clone() };
        if Some(&rc.text) == rc.saved.as_ref() {
            rc.note = Some((format!("Nothing to save: this is the file as {place} has it."), false));
            return;
        }
        let text = rc.text.clone();
        let game = rc.game_id.clone().unwrap_or_default();
        rc.state = RcState::Saving;
        rc.note = None;
        let Some(s) = &mut self.session else { return };
        s.write_rc(&game, &text);
        let slot = s.read_rc(&game);
        self.rc.as_mut().unwrap().pending = Some((slot, isorun_client::platform::seconds(), Some(text)));
    }

    fn poll_rc(&mut self, now: f64) {
        if self.view != View::Rc {
            return;
        }
        let logged_in = self.session.as_ref().is_some_and(|s| s.conn.open && s.state.lobby.username.is_some());
        if self.rc.as_ref().is_some_and(|r| !r.asked) && logged_in {
            self.read_rc(false);
        }
        let Some(rc) = &mut self.rc else { return };
        let Some((slot, at, saving)) = &rc.pending else { return };
        let answer = slot.lock().unwrap().take();
        let timed_out = now - at > RC_SECONDS;
        if answer.is_none() && !timed_out {
            return;
        }
        let contents = answer.flatten();
        let saving = saving.clone();
        rc.pending = None;
        let server = servers::find_server(&rc.account.server_id);
        let offline = server.as_ref().is_some_and(|s| s.offline);
        let place = if offline { "this device".to_string() } else { server.as_ref().map(|s| s.host.clone()).unwrap_or_default() };
        match saving {
            Some(text) => {
                rc.state = RcState::Ready;
                if contents.as_deref() == Some(text.as_str()) {
                    rc.saved = Some(text);
                    rc.leaving = false;
                    rc.note = Some((
                        format!("Saved{}. It counts from the next game you start or continue.", if offline { String::new() } else { format!(" on {place}") }),
                        false,
                    ));
                } else {
                    rc.note = Some((
                        if contents.is_none() {
                            format!("No answer from {place}. Your text is still here: try Save again.")
                        } else {
                            format!("{place} did not keep it. Your text is still here: try Save again.")
                        },
                        true,
                    ));
                }
            }
            None => match contents {
                Some(c) => {
                    // the first read fills the box; a later one (revert) replaces what was typed
                    rc.saved = Some(c.clone());
                    rc.text = c;
                    rc.state = RcState::Ready;
                }
                None => {
                    rc.state = if rc.saved.is_none() { RcState::Failed } else { RcState::Ready };
                    rc.note = Some((format!("No answer from {place}: your rc file could not be read."), true));
                }
            },
        }
    }

    /// Back out of the rc file; changes not saved ask for a second Back first.
    fn leave_rc(&mut self) {
        let Some(rc) = &mut self.rc else { return self.show_accounts(None) };
        rc.editing = false;
        if rc.saved.is_some() && Some(&rc.text) != rc.saved.as_ref() && !rc.leaving {
            rc.leaving = true;
            self.say("Your changes are not saved. Back again leaves without them; Save keeps them.", None);
            return;
        }
        let rc = self.rc.take().unwrap();
        let offline = servers::find_server(&rc.account.server_id).is_some_and(|s| s.offline);
        if offline {
            self.show_accounts(Some(format!("rc:{}/{}", rc.account.server_id, rc.account.username)));
        } else {
            self.show_versions(rc.account.clone());
            self.want = rc.game_id.map(|g| format!("rc:{g}"));
        }
    }

    // ------------------------------------------------------------------ helpers for the screens

    /// The games a server offers: the live list, or the one kept from last time.
    pub(crate) fn games(&self, server: &ServerInfo) -> Option<Vec<isorun_webtiles::GameLink>> {
        let live = self.session.as_ref().filter(|s| s.conn.open && s.server.id == server.id).map(|s| s.state.lobby.games.clone()).filter(|g| !g.is_empty());
        live.or_else(|| {
            servers::get_games(&server.id).map(|v| {
                v.into_iter()
                    .map(|c| isorun_webtiles::GameLink { id: c.id, label: c.label, save: c.save, disabled: c.disabled, rc: c.rc, ..Default::default() })
                    .collect()
            })
        })
    }

    pub(crate) fn exit_of(&self) -> Option<GameExit> {
        self.session.as_ref().and_then(|s| s.state.exit.clone()).filter(|e| e.reason.is_some())
    }

    pub(crate) fn conn_of(&self, account: &Account, chosen: bool) -> Conn {
        let s = servers::find_server(&account.server_id)
            .and_then(|sv| self.session.as_ref().filter(|s| s.server.id == sv.id && s.username.as_deref() == Some(account.username.as_str())));
        account_conn(s, account, chosen)
    }

    pub(crate) fn login_state_of(&self, account: &Account) -> LoginState {
        let s = self.session.as_ref().filter(|s| s.server.id == account.server_id && s.username.as_deref() == Some(account.username.as_str()));
        let logged = s.filter(|s| s.conn.open).and_then(|s| s.state.lobby.username.clone());
        servers::login_state(Some(account), logged.as_deref(), s.is_some_and(|s| s.logging_in))
    }

    /// Ask for the pings of the servers listed.
    pub(crate) fn ping_of(&mut self, sv: &ServerInfo) -> Option<u32> {
        self.pings.get(sv, self.wake.clone())
    }
}

thread_local! {
    pub(crate) static CTX: std::cell::RefCell<Option<egui::Context>> = const { std::cell::RefCell::new(None) };
}

/// The account a session belongs to: the one it logged in as, or failing that the chosen one there.
pub fn account_of(s: &Session) -> Option<Account> {
    if let Some(u) = &s.username {
        return Some(Account { server_id: s.server.id.clone(), username: u.clone() });
    }
    servers::get_chosen_account().filter(|c| c.server_id == s.server.id)
}

/// The dot after the chosen account's name: green once its server knows it (an account on this
/// device, at once), gold on the way, red when the line is down or the server wants a password.
pub fn account_conn(s: Option<&Session>, account: &Account, chosen: bool) -> Conn {
    if account.server_id == "offline" {
        return if s.is_some_and(|s| s.closed) {
            Conn::Down
        } else if chosen {
            Conn::Up
        } else {
            Conn::Off
        };
    }
    let open = s.is_some_and(|s| s.conn.open);
    let logged = if open { s.and_then(|s| s.state.lobby.username.clone()) } else { None };
    if logged.is_some() {
        return Conn::Up;
    }
    if s.is_some_and(|s| s.closed) {
        return Conn::Down;
    }
    if s.is_none() && !chosen {
        return Conn::Off;
    }
    if open && servers::login_state(Some(account), None, s.is_some_and(|s| s.logging_in)) == LoginState::Out {
        Conn::Down
    } else {
        Conn::Wait
    }
}

/// Delete a player of this device's saved characters: its folders under the saves.
pub fn delete_profile_saves(profile: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(dir) = isorun_client::platform::storage::data_dir().map(|d| d.join("saves")) else { return };
        let p = profile.to_lowercase();
        let player = p == servers::DEFAULT_PROFILE.to_lowercase();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let Some(rest) = name.strip_prefix("crawl-") else { continue };
            let mine = match rest.split_once('~') {
                Some((_, who)) => who == p,
                None => player,
            };
            if mine {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    isorun_client::engine::write_rc(profile, "");
}

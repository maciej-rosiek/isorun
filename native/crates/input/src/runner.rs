//! Executes actions and owns the only path to crawl for game input (the port of `runner.ts`):
//! relative movement, sequencing with verification, and prompts.
//!
//! The runner sends nothing itself: every message goes into its [`Runner::outbox`], which the
//! game drains into its session each frame (`Runner::take_outbox`). What the situation is
//! comes in an [`Env`] on each call (the state, the scene, the context, the camera, the time);
//! what the runner asks of the screen goes to its [`RunnerHooks`]. A sequence that waits for the
//! server (a key held for its prompt, the direction for a door) is a pending wait, moved on by
//! [`Runner::on_state`] (a server message came in), [`Runner::on_scene`] (the map moved on) and
//! [`Runner::tick`] (its time ran out).

use crate::bindings::{autofight, has_interaction_choice, Action, CommandCategory, MenuOp, OskOp, RelDir, SeqItem, Stroke, UiOp};
use crate::context::{Context, Mode, Target};
use crate::focus::FocusOp;
use isorun_render::camera::{IsoCamera, TURN_DEGREES};
use isorun_scene::{dir_from_delta, rotate_dir, Attitude, BillboardKind, Dir8, DoorState, Feature, Scene, UpDown, DIR8_DX, DIR8_DY};
use isorun_webtiles::{cm, GameState, MouseMode, Msgch};
use serde_json::Value;

/// DCSS movement keys by absolute compass direction.
pub const MOVE_KEYS: [char; 8] = ['k', 'u', 'l', 'n', 'j', 'b', 'h', 'y'];

/// How long a first press of a dangerous action stays armed for its confirming second press.
const CONFIRM_WINDOW_MS: f64 = 2000.0;

/// How long after a step the runner sent a position change may still be that step's.
pub const OWN_STEP_WINDOW_MS: f64 = 1500.0;

/// How long after an `x` went out the targeting mode that follows may still be its look mode.
pub const LOOK_WINDOW_MS: f64 = 1500.0;

/// How long a cursor walk the server has not echoed yet is still walked from
/// (`orbit_step`). Past it the walk is taken to have been refused — crawl
/// keeps the cursor inside the level, so a ring that runs off the map stops
/// dead — and the server's own cursor is read again.
pub const ORBIT_WINDOW_MS: f64 = 1000.0;

/// How long a sequence waits for the server to open its prompt.
const AWAIT_MS: f64 = 1500.0;

/// The last step the runner initiated, so a move the server echoes is known to be ours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LastStep {
    /// the absolute direction sent
    pub dir: Dir8,
    pub t: f64,
}

/// Whether a move `dx, dy` echoed by the server is the runner's own step:
/// `last` was sent within OWN_STEP_WINDOW_MS *and* in the direction the feet
/// actually went. A step that went another way was not ours: an explore or
/// travel that stopped after one cell right after a keyed step must be treated as
/// path-driven, or the camera faces the road and never the threat.
///
/// The scene is rebuilt once per frame, so a stick held sideways can have two
/// or three echoed steps land in one update as a multi-cell displacement.
/// With `hops` (the one-cell moves the server sent since the last scene) that is
/// still ours when every hop went the step's way.
pub fn step_is_ours(last: Option<LastStep>, dx: i32, dy: i32, now: f64, hops: &[(i32, i32)]) -> bool {
    let Some(last) = last else { return false };
    if now - last.t >= OWN_STEP_WINDOW_MS {
        return false;
    }
    if !hops.is_empty() {
        return hops.iter().all(|&(hx, hy)| dir_from_delta(hx, hy) == Some(last.dir));
    }
    if dx.abs() > 1 || dy.abs() > 1 {
        return false;
    }
    dir_from_delta(dx, dy) == Some(last.dir)
}

/// What the runner asks of the screen around it. Every method has a default that does nothing.
pub trait RunnerHooks {
    fn ui(&mut self, _op: UiOp, _arg: Option<i32>, _category: Option<CommandCategory>, _section: Option<&str>) {}
    fn osk(&mut self, _op: OskOp, _dir: Option<Dir8>) {}
    fn menu(&mut self, _op: MenuOp) {}
    fn focus(&mut self, _op: FocusOp) {}
    fn status(&mut self, _text: &str) {}
    /// A melee attack went out at the monster on this cell: the player strikes at it.
    fn swing(&mut self, _x: i32, _y: i32) {}
    /// Keys that leave the level map for another screen went out: keep the map drawn till it is up (map_hold.rs).
    fn keep_map(&mut self) {}
}

/// Hooks that do nothing: for a runner driven without a screen.
pub struct NoHooks;
impl RunnerHooks for NoHooks {}

/// The situation a runner call reads.
pub struct Env<'a> {
    pub state: &'a GameState,
    pub scene: &'a Scene,
    pub ctx: &'a Context,
    pub camera: &'a mut IsoCamera,
    /// milliseconds, on the clock the pad's events use
    pub now: f64,
    /// a spectator sends nothing
    pub watching: bool,
    /// the "confirm dangerous actions" setting: stairs need a second press while hostiles are in view
    pub confirm_dangerous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Pending,
    Active,
}

#[derive(Clone, Copy, Debug)]
struct Track {
    state: Phase,
    t: f64,
}

impl Track {
    const IDLE: Track = Track { state: Phase::Idle, t: 0.0 };
}

/// A sequence waiting on the server.
#[derive(Clone, Debug)]
enum Wait {
    /// `await_prompt`: the rest of `seq` from `from` goes out once the server opens something
    Keys { seq: Vec<SeqItem>, from: usize, rev0: u64, deadline: f64 },
    /// `close_ahead`: `C` went out; the direction follows if the server asks for one
    Door { dir: char, deadline: f64 },
}

#[derive(Debug)]
pub struct Runner {
    /// The way the player last stepped or attacked: what is "ahead" for the contextual action.
    pub heading: Dir8,
    /// last step the runner initiated, so auto-facing knows it was ours
    pub last_step: Option<LastStep>,
    /// Recent intents, newest first: replies can arrive after another direction was sent.
    recent_steps: Vec<LastStep>,
    /// when the last wait (`.`, `s`) went out: a turn passed in place, which leaves the view where it is
    pub last_wait_at: f64,
    /// a dangerous action awaiting its confirming second press
    armed: Option<(&'static str, f64)>,
    /// look mode: `x` went out (pending), the server opened its targeting (active)
    look: Track,
    /// fire holds the view: `f` went out (pending), the server opened its aim (active). While it
    /// holds, the cursor's arrival on a default target turns nothing; the first cursor step releases it.
    hold: Track,
    /// autoexplore: `o` went out (pending), the server is walking it (active)
    explore: Track,
    /// The cells a walk round the player has sent the cursor to and the server has not echoed
    /// back yet, oldest first, with the time the oldest went out.
    orbit: Option<(Vec<(i32, i32)>, f64)>,
    wait: Option<Wait>,
    /// What went out, in order, for the game to send.
    pub outbox: Vec<Value>,
}

impl Default for Runner {
    fn default() -> Self {
        Runner::new()
    }
}

fn input_text(msg: &Value) -> Option<&str> {
    (msg.get("msg")?.as_str()? == "input").then(|| msg.get("text").and_then(|t| t.as_str())).flatten()
}

fn keycode(msg: &Value) -> Option<i64> {
    (msg.get("msg")?.as_str()? == "key").then(|| msg.get("keycode").and_then(|k| k.as_i64())).flatten()
}

/// A key, however it was typed: as text or as a keycode.
fn is_key(msg: &Value, keys: &[char]) -> bool {
    if let Some(t) = input_text(msg) {
        return keys.iter().any(|k| t.len() == 1 && t.starts_with(*k));
    }
    keycode(msg).is_some_and(|c| keys.iter().any(|k| c == *k as i64))
}

/// CMD_LOOK_AROUND, however it was typed: the `x` key as text or as a keycode.
fn is_look_around(msg: &Value) -> bool {
    is_key(msg, &['x'])
}
/// CMD_FIRE, however it was typed.
fn is_fire(msg: &Value) -> bool {
    is_key(msg, &['f'])
}
/// CMD_WAIT, however it was typed: `.` or `s`.
fn is_wait(msg: &Value) -> bool {
    is_key(msg, &['.', 's'])
}
/// CMD_EXPLORE, however it was typed.
fn is_explore(msg: &Value) -> bool {
    is_key(msg, &['o'])
}

/// The message the official client sends for a direction key with a modifier.
pub fn dir_message(abs: Dir8, run: bool, attack: bool) -> Value {
    let letter = MOVE_KEYS[abs as usize & 7];
    if attack {
        return cm::key(letter.to_ascii_uppercase() as i32 - 64);
    }
    if run {
        return cm::input(&letter.to_ascii_uppercase().to_string());
    }
    cm::input(&letter.to_string())
}

fn stroke_message(s: &SeqItem) -> Value {
    match &s.stroke {
        Stroke::Key(k) => cm::key(*k),
        Stroke::Text(t) => cm::input(t),
    }
}

/// Step options: Shift runs (or fires that way in an aim), Ctrl attacks; `turns` says left and
/// right turn the camera for the input that sent them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepOpts {
    pub run: bool,
    pub attack: bool,
    pub turns: bool,
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            heading: 0,
            last_step: None,
            recent_steps: Vec::new(),
            last_wait_at: f64::NEG_INFINITY,
            armed: None,
            look: Track::IDLE,
            hold: Track::IDLE,
            explore: Track::IDLE,
            orbit: None,
            wait: None,
            outbox: Vec::new(),
        }
    }

    /// The messages that went out since the last call, for the session.
    pub fn take_outbox(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    pub fn send(&mut self, msg: Value, env: &mut Env) {
        if env.watching {
            return;
        }
        // anything but the walk's own key may move the cursor itself (a click, a
        // grid direction, the key that ends the aim): the walk's unechoed path is
        // no longer what the cursor is doing. `step` re-records it after its send.
        self.orbit = None;
        if env.ctx.mode == Mode::Command {
            // explore with an unsafe monster in view: the server refuses with "X is nearby!" and
            // nothing moves, so the refusal is the event to face. With none in view, the steps
            // that follow drive the camera: they are the explore's, not ours.
            if is_explore(&msg) {
                self.last_step = None;
                self.recent_steps.clear();
                self.explore = Track { state: Phase::Pending, t: env.now };
            }
            // an aim's cursor is the server's: nothing is sent to place it. Look mode (`x`)
            // does not turn the view; the targeting that follows `x` is remembered as look
            // mode (`examining`). Fire holds the heading until the cursor is first stepped;
            // any other command drops a hold a refused `f` left behind.
            if is_wait(&msg) {
                self.last_wait_at = env.now;
            }
            if is_look_around(&msg) {
                self.look = Track { state: Phase::Pending, t: env.now };
            }
            self.hold = if is_fire(&msg) { Track { state: Phase::Pending, t: env.now } } else { Track::IDLE };
        }
        self.outbox.push(msg);
    }

    /// Whether the targeting up now is the look mode `x` opened. The server
    /// reports targeting alone; this pairs the `x` that went out with the targeting that
    /// follows it, and forgets it when that targeting ends, or when none follows within
    /// LOOK_WINDOW_MS. A --more-- or a popup over the look keeps it, so a look resumed
    /// after a --more-- still describes on A rather than travelling. Called once a frame.
    pub fn examining(&mut self, mode: Mode, now: f64) -> bool {
        Self::track(&mut self.look, mode, now)
    }

    /// Whether the aim up now is a fire whose view is held: no cursor the server places turns
    /// it. Paired with the `f` that went out the way `examining` pairs a look, and released by
    /// the first cursor step or the aim's end. Called once a frame.
    pub fn holding_view(&mut self, mode: Mode, now: f64) -> bool {
        Self::track(&mut self.hold, mode, now)
    }

    fn track(l: &mut Track, mode: Mode, now: f64) -> bool {
        if mode == Mode::Targeting {
            if l.state == Phase::Pending {
                l.state = Phase::Active;
            }
        } else if l.state == Phase::Active {
            if mode != Mode::More && mode != Mode::Popup {
                l.state = Phase::Idle;
            }
        } else if l.state == Phase::Pending && now - l.t > LOOK_WINDOW_MS {
            l.state = Phase::Idle;
        }
        l.state == Phase::Active && mode == Mode::Targeting
    }

    /// Whether an autoexplore is walking now. Crawl walks it without asking for a command, so
    /// the mouse mode stays NORMAL from the `o` until the walk ends (COMMAND), or something else
    /// comes up. An `o` the server never walks is forgotten after LOOK_WINDOW_MS. Called once a frame.
    pub fn exploring(&mut self, input_mode: i64, now: f64) -> bool {
        let e = &mut self.explore;
        if e.state == Phase::Pending {
            if input_mode == MouseMode::NORMAL {
                e.state = Phase::Active;
            } else if now - e.t > LOOK_WINDOW_MS {
                e.state = Phase::Idle;
            }
        } else if e.state == Phase::Active && input_mode != MouseMode::NORMAL {
            e.state = Phase::Idle;
        }
        e.state == Phase::Active
    }

    /// Whether an `o` went out and its walk has not ended yet (`exploring`).
    pub fn explore_open(&self) -> bool {
        self.explore.state != Phase::Idle
    }

    /// Whether a sequence is waiting on the server.
    pub fn waiting(&self) -> bool {
        self.wait.is_some()
    }

    /// Send a key sequence. A key marked `await_prompt` is held until the
    /// server leaves command mode for it (the travel prompt, a menu, a popup);
    /// if a --more-- shows instead, or nothing changes, the rest is dropped so
    /// a refused `G` never leaks its `>` into the dungeon.
    pub fn send_keys(&mut self, seq: &[SeqItem], env: &mut Env, hooks: &mut dyn RunnerHooks) {
        self.abort_sequence();
        self.send_from(seq.to_vec(), 0, false, env, hooks);
    }

    // `opened`: the wait for key i is over, so it goes out even if the mouse mode never left command (a prompt-channel line)
    fn send_from(&mut self, seq: Vec<SeqItem>, from: usize, mut opened: bool, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        for i in from..seq.len() {
            if seq[i].await_prompt && i > 0 && !opened && env.state.input_mode == MouseMode::COMMAND {
                self.await_prompt(Wait::Keys { seq, from: i, rev0: env.state.rev.messages, deadline: env.now + AWAIT_MS }, env, hooks);
                return;
            }
            opened = false;
            let m = stroke_message(&seq[i]);
            self.send(m, env);
        }
    }

    /// Send a message that opens something (an item's description), then the keys once it is up.
    pub fn send_then(&mut self, msg: Value, seq: &[SeqItem], env: &mut Env, hooks: &mut dyn RunnerHooks) {
        self.abort_sequence();
        let rev0 = env.state.rev.messages;
        self.send(msg, env);
        self.await_prompt(Wait::Keys { seq: seq.to_vec(), from: 0, rev0, deadline: env.now + AWAIT_MS }, env, hooks);
    }

    /// Wait until the server has opened something for input, or give up.
    /// "Opened" is a mouse mode other than command, a menu, a popup, a text
    /// field, or a new line on the prompt channel: `z` asks "Cast which spell?"
    /// as an MSGCH_PROMPT message with the mouse mode unchanged, so the mode alone
    /// would never say it is up.
    fn await_prompt(&mut self, wait: Wait, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        self.wait = Some(wait);
        self.check_wait(env, hooks);
    }

    /// A server message came in: a sequence waiting on it may go on, or give up.
    pub fn on_state(&mut self, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        self.check_wait(env, hooks);
    }

    /// The map moved on: a door closed on `C` alone asks for no direction.
    pub fn on_scene(&mut self) {
        if matches!(self.wait, Some(Wait::Door { .. })) {
            self.wait = None;
        }
    }

    /// The time moved on: a wait past its time gives up.
    pub fn tick(&mut self, now: f64, hooks: &mut dyn RunnerHooks) {
        match &self.wait {
            Some(Wait::Keys { deadline, .. }) if now >= *deadline => {
                self.wait = None;
                hooks.status("Aborted: no prompt");
            }
            Some(Wait::Door { deadline, .. }) if now >= *deadline => self.wait = None,
            _ => {}
        }
    }

    fn check_wait(&mut self, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        let st = env.state;
        let opened = st.input_mode != MouseMode::COMMAND || !st.menus.is_empty() || !st.ui.is_empty() || st.text_input.is_some();
        match self.wait.take() {
            None => {}
            Some(Wait::Keys { seq, from, rev0, deadline }) => {
                if env.now >= deadline {
                    hooks.status("Aborted: no prompt");
                    return;
                }
                if st.messages.more {
                    hooks.status("Aborted: --more--");
                    return;
                }
                let prompted = st.rev.messages != rev0 && st.messages.lines.last().is_some_and(|l| l.channel == Some(Msgch::PROMPT));
                if opened || prompted {
                    self.send_from(seq, from, true, env, hooks);
                } else {
                    self.wait = Some(Wait::Keys { seq, from, rev0, deadline });
                }
            }
            Some(Wait::Door { dir, deadline }) => {
                if env.now >= deadline {
                    return;
                }
                if st.input_mode == MouseMode::TARGET_DIR {
                    self.send(cm::input(&dir.to_string()), env);
                } else if !(st.messages.more || opened) {
                    self.wait = Some(Wait::Door { dir, deadline });
                }
            }
        }
    }

    fn remember_step(&mut self, step: LastStep) {
        if let Some(last) = self.last_step {
            self.recent_steps.insert(0, last);
        }
        self.recent_steps.retain(|s| step.t - s.t < OWN_STEP_WINDOW_MS);
        self.recent_steps.truncate(63);
        self.last_step = Some(step);
    }

    /// Attribute every confirmed hop using the same short direction/time window
    /// as `step_is_ours`, but retain older intents too. Otherwise forward, turn,
    /// forward makes the first reply look like travel and auto-facing undoes
    /// the turn. Returns the final hop's intent. These are hints, not protocol
    /// acknowledgements: blocked steps have no position echo.
    pub fn step_for_move(&self, dx: i32, dy: i32, now: f64, hops: &[(i32, i32)]) -> Option<LastStep> {
        let last = self.last_step?;
        let candidates: Vec<LastStep> = std::iter::once(last).chain(self.recent_steps.iter().copied()).collect();
        let one = [(dx, dy)];
        let hops = if hops.is_empty() { &one[..] } else { hops };
        let mut matched = None;
        for &(hx, hy) in hops {
            matched = candidates.iter().copied().find(|s| step_is_ours(Some(*s), hx, hy, now, &[]));
            matched?;
        }
        matched
    }

    /// Absolute direction for a screen-relative one: 0 is up the screen.
    pub fn absolute(&self, rel: RelDir, camera: &IsoCamera) -> Dir8 {
        rotate_dir(camera.facing(), rel as i32)
    }

    /// Absolute direction for a key in an aim: read against the grid the player sees.
    pub fn absolute_aim(&self, rel: RelDir, camera: &IsoCamera) -> Dir8 {
        self.absolute(rel, camera)
    }

    /// The compass step that walks an aim's cursor one cell round the player,
    /// to the right (1) or the left (-1): left and right keep the cursor the
    /// distance from the player it is already at (the ring of cells that far
    /// out, as crawl counts distance) and walk it round. It holds its heading along
    /// a side of that ring and changes it at the corners, the cells on a
    /// diagonal from the player, where the ring turns.
    ///
    /// The walk is stepped from where the runner's own unechoed steps have
    /// already sent the cursor (`orbit`), not from the cursor the server
    /// reports, which trails a held key by a round trip.
    ///
    /// None where there is no cursor to walk — a compass prompt, a cursor the
    /// server has not placed, or one standing on the player.
    fn orbit_step(&mut self, side: i32, env: &Env) -> Option<(Dir8, i32, i32)> {
        let scene = env.scene;
        if !scene.player_on_level {
            return None;
        }
        let (cx, cy) = self.orbit_from(env)?;
        let dx = cx - scene.player.0;
        let dy = cy - scene.player.1;
        // the ring the cursor stands on, and where on it: a side of the ring or a corner
        let r = dx.abs().max(dy.abs());
        if r == 0 {
            return None;
        }
        let at = dir_from_delta(if dx.abs() == r { dx.signum() } else { 0 }, if dy.abs() == r { dy.signum() } else { 0 })?;
        // a corner turns the walk a further eighth: it is stepping off one side of the ring onto the next
        let dir = rotate_dir(at, (if at % 2 == 1 { 3 } else { 2 }) * side);
        Some((dir, cx + DIR8_DX[dir as usize], cy + DIR8_DY[dir as usize]))
    }

    /// The cell the next walk steps from: the end of the runner's own unechoed
    /// path where it has one, else the server's cursor. The server's cursor
    /// confirms that path as it arrives, and a path it has not reached within
    /// ORBIT_WINDOW_MS is abandoned.
    fn orbit_from(&mut self, env: &Env) -> Option<(i32, i32)> {
        let server = env.state.cursors.first().copied().flatten().map(|c| (c.x, c.y));
        let Some((path, t)) = &mut self.orbit else { return server };
        if let Some(s) = server {
            if let Some(echoed) = path.iter().position(|&c| c == s) {
                path.drain(..=echoed);
                *t = env.now;
            }
        }
        if path.is_empty() || env.now - *t > ORBIT_WINDOW_MS {
            self.orbit = None;
            return server;
        }
        path.last().copied()
    }

    /// Record a walk's cell as sent but not echoed.
    fn orbit_sent(&mut self, cell: (i32, i32), now: f64) {
        match &mut self.orbit {
            Some((path, _)) => path.push(cell),
            None => self.orbit = Some((vec![cell], now)),
        }
    }

    /// Movement in command mode: forward turns the camera; the diagonals and
    /// back strafe. Left and right turn the camera or strafe as `opts.turns`
    /// says. Every step is remembered (`last_step`).
    /// In targeting the same key moves the cursor (plain), fires that way
    /// (Shift) or sends the Ctrl variant, read against the grid the player sees; a
    /// plain left or right walks the cursor round the player instead, keeping its
    /// distance (`orbit_step`). The level map is north-up: absolute.
    pub fn step(&mut self, rel: RelDir, opts: StepOpts, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        let mode = env.ctx.mode;
        if !matches!(mode, Mode::Command | Mode::Targeting | Mode::Levelmap | Mode::Prompt) {
            return;
        }
        if mode == Mode::Targeting || mode == Mode::Prompt {
            // read against the grid in view, never turn it. The first step of a fire's cursor releases the held view
            if mode == Mode::Targeting {
                self.hold = Track::IDLE;
            }
            // left and right walk the cursor round the player instead
            let orbit = if (rel == 2 || rel == 6) && !opts.run && !opts.attack { self.orbit_step(if rel == 2 { 1 } else { -1 }, env) } else { None };
            let dir = orbit.map_or_else(|| self.absolute_aim(rel, env.camera), |o| o.0);
            self.send(dir_message(dir, opts.run, opts.attack), env);
            // after the send, which drops the path any other cursor move invalidates
            if let Some((_, x, y)) = orbit {
                self.orbit_sent((x, y), env.now);
            }
            return;
        }
        if mode == Mode::Levelmap {
            // north-up surface: absolute
            self.send(dir_message(rel, opts.run, opts.attack), env);
            return;
        }
        // left / right on an input set to turn is a pure turn: nothing is sent and
        // no time passes. A run or an attack is always a step, whatever the setting says.
        if (rel == 2 || rel == 6) && opts.turns && !opts.attack && !opts.run {
            env.camera.turn(if rel == 2 { TURN_DEGREES } else { -TURN_DEGREES });
            return;
        }
        // spectating: a turn above still turns the view, but nothing else is a step of ours
        if env.watching {
            return;
        }
        let abs = self.absolute(rel, env.camera);
        self.walk(abs, opts, env, hooks);
    }

    /// One step in command mode the compass way `abs`: a swing when a monster stands there.
    pub fn walk(&mut self, abs: Dir8, opts: StepOpts, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        if env.watching || env.ctx.mode != Mode::Command {
            return;
        }
        // stepping into a monster is a swing: face it. Into a friendly it is a swap, so a
        // strafe back into a foxfire keeps its heading like any other strafe
        let target = self.monster_at(abs, env.scene);
        self.heading = abs;
        self.remember_step(LastStep { dir: abs, t: env.now });
        if let Some((x, y)) = target {
            self.swing_at(x, y, env.scene, hooks);
        }
        self.send(dir_message(abs, opts.run, opts.attack), env);
    }

    /// A click on a cell, as the official client sends it (`click_cell` with the cell and the
    /// button, 1 left, 2 middle, 3 right). The server decides what the click means. A left
    /// click on an adjacent monster in command mode is a swing, and the step is remembered as
    /// ours so the camera stays where the mouse left it.
    pub fn click_cell(&mut self, x: i32, y: i32, button: i32, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        if env.ctx.mode == Mode::Command && button == 1 {
            let (px, py) = env.scene.player;
            let (dx, dy) = (x - px, y - py);
            if let Some(dir) = dir_from_delta(dx.signum(), dy.signum()) {
                let adjacent = dx.abs() <= 1 && dy.abs() <= 1;
                self.heading = dir;
                self.remember_step(LastStep { dir, t: env.now });
                if let Some((tx, ty)) = if adjacent { self.monster_at(dir, env.scene) } else { None } {
                    self.swing_at(tx, ty, env.scene, hooks);
                }
            }
        }
        self.send(cm::click_cell(x, y, button), env);
    }

    /// The monster a step into `abs` would hit: anything but a friendly, which is swapped with instead.
    fn monster_at(&self, abs: Dir8, scene: &Scene) -> Option<(i32, i32)> {
        let x = scene.player.0 + DIR8_DX[abs as usize];
        let y = scene.player.1 + DIR8_DY[abs as usize];
        scene.billboards.iter().any(|b| b.kind == BillboardKind::Monster && b.attitude != Some(Attitude::Friendly) && b.x == x && b.y == y).then_some((x, y))
    }

    fn swing_at(&mut self, x: i32, y: i32, scene: &Scene, hooks: &mut dyn RunnerHooks) {
        let (px, py) = scene.player;
        if let Some(dir) = dir_from_delta((x - px).signum(), (y - py).signum()) {
            self.heading = dir;
        }
        hooks.swing(x, y);
    }

    pub fn execute(&mut self, a: &Action, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        match a {
            Action::Keys { seq, keeps_map, .. } => {
                if *keeps_map {
                    hooks.keep_map();
                }
                self.send_keys(seq, env, hooks);
            }
            Action::Step { dir, run, attack, turns, .. } => self.step(*dir, StepOpts { run: *run, attack: *attack, turns: *turns }, env, hooks),
            Action::Turn { right } => env.camera.turn(if *right { TURN_DEGREES } else { -TURN_DEGREES }),
            Action::Look { .. } => {}
            Action::Contextual { alt } => self.contextual(*alt, env, hooks),
            Action::Fight => self.fight(env, hooks),
            Action::Examine => self.examine(env),
            Action::Fire => self.fire(env),
            Action::Menu { op } => hooks.menu(*op),
            // targeting: facing-relative like the keyboard path; level map: absolute
            Action::Cursor { dir } => self.step(*dir, StepOpts::default(), env, hooks),
            Action::Prompt { hotkey } => self.send(cm::input(hotkey), env),
            Action::Focus { op } => hooks.focus(*op),
            Action::Ui { op, arg, category, section } => hooks.ui(*op, *arg, *category, section.as_deref()),
            Action::Osk { op, dir } => hooks.osk(*op, *dir),
            // tables hand the tap or hold half to resolve(); executing the pair itself means the tap
            Action::Hold { tap, .. } => self.execute(tap, env, hooks),
        }
    }

    /// RT always uses Crawl's autofight, including its HP cutoff and target selection.
    pub fn fight(&mut self, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        if env.ctx.mode != Mode::Command {
            return;
        }
        if let Action::Keys { seq, .. } = autofight() {
            self.send_keys(&seq, env, hooks);
        }
    }

    /// LT: fire. `f` in command mode, and `f` again inside the aim it opens,
    /// where it is CMD_TARGET_SELECT as `.` and Enter are: one key either side of the
    /// server's round trip, which is what lets the button be tapped as fast as `f` is
    /// spammed on a keyboard. Look mode is not an aim: nothing is sent there.
    pub fn fire(&mut self, env: &mut Env) {
        let ctx = env.ctx;
        if ctx.mode == Mode::Targeting {
            if !ctx.examining {
                self.send(cm::input("f"), env);
            }
            return;
        }
        if ctx.mode != Mode::Command {
            return;
        }
        self.send(cm::input("f"), env);
    }

    /// R3: examine. `x` in command mode, and inside the look it opens the same
    /// button describes the cell under the cursor (`v`, CMD_TARGET_DESCRIBE)
    /// rather than travelling there, which is what Enter would do.
    pub fn examine(&mut self, env: &mut Env) {
        let ctx = env.ctx;
        if ctx.mode == Mode::Targeting {
            if ctx.examining {
                self.send(cm::input("v"), env);
            }
            return;
        }
        if ctx.mode != Mode::Command {
            return;
        }
        self.send(cm::input("x"), env);
    }

    /// A acts underfoot, then on a door ahead. When a usable feature and items
    /// share the tile, ask instead of choosing. An explicit alt selects the
    /// feature (false) or the items (true) from that chooser.
    pub fn contextual(&mut self, alt: Option<bool>, env: &mut Env, hooks: &mut dyn RunnerHooks) {
        let ctx = env.ctx;
        if ctx.mode != Mode::Command {
            return;
        }
        if alt.is_none() && has_interaction_choice(ctx) {
            hooks.ui(UiOp::Interact, None, None, None);
            return;
        }
        // Explicit pickup from the interaction chooser.
        if alt == Some(true) {
            if matches!(&ctx.under, Target::Feature { items: Some(_), .. }) {
                self.send(cm::input(","), env);
            }
            return;
        }
        if let Target::Feature { feature, items, .. } = &ctx.under {
            match feature {
                Feature::Stairs { dir, .. } | Feature::Hatch { dir } => {
                    let key = if *dir == UpDown::Down { ">" } else { "<" };
                    if !self.confirmed("stairs", env, hooks) {
                        return;
                    }
                    return self.send(cm::input(key), env);
                }
                // altars: DCSS 0.29+ converts with '>' (there is no pray command)
                Feature::Portal | Feature::Shop | Feature::Transporter | Feature::Altar { .. } => return self.send(cm::input(">"), env),
                _ => {
                    if items.is_some() {
                        return self.send(cm::input(","), env);
                    }
                }
            }
        }
        if matches!(ctx.under, Target::Item { .. }) {
            return self.send(cm::input(","), env);
        }
        match &ctx.ahead {
            Target::Feature { feature: Feature::Door { state: DoorState::Open }, .. } => self.close_ahead(env),
            // moving into a closed door opens it without entering: the one "step" A keeps
            Target::DoorClosed { .. } => self.step(0, StepOpts::default(), env, hooks),
            _ => {}
        }
    }

    /// Close the open door ahead: CMD_CLOSE_DOOR, then the direction only if the
    /// server asks for one. With `easy_door` (on by default) and a single open
    /// door adjacent, crawl closes it on `C` alone; a direction sent anyway would be a
    /// step. With more doors, or `easy_door = false`, it prompts "Which direction?" in
    /// MOUSE_MODE_TARGET_DIR, and that mode is the cue to answer, rotated with facing
    /// like any step. Anything else (the door closed, a --more--) ends the wait.
    fn close_ahead(&mut self, env: &mut Env) {
        let dir = MOVE_KEYS[self.heading as usize & 7];
        self.abort_sequence();
        self.wait = Some(Wait::Door { dir, deadline: env.now + AWAIT_MS });
        self.send(cm::input("C"), env);
    }

    /// "Confirm dangerous actions": with the setting on and hostiles in view, a
    /// stairs action needs a second press within CONFIRM_WINDOW_MS. Returns whether to go ahead.
    fn confirmed(&mut self, what: &'static str, env: &Env, hooks: &mut dyn RunnerHooks) -> bool {
        let n = env.ctx.hostiles_in_view;
        if !env.confirm_dangerous || n == 0 {
            self.armed = None;
            return true;
        }
        if let Some((w, t)) = self.armed {
            if w == what && env.now - t <= CONFIRM_WINDOW_MS {
                self.armed = None;
                return true;
            }
        }
        self.armed = Some((what, env.now));
        hooks.status(&format!("{n} hostile{} in view: press again to confirm", if n == 1 { "" } else { "s" }));
        false
    }

    pub fn abort_sequence(&mut self) {
        self.wait = None;
    }
}

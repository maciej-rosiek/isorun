//! The game screen's input routing (the port of `game.ts`'s `onKeyDown`, `pad`, `fireHolds`,
//! `executePadAction`, `aimKey`, `stickAim`, `panMap`, `menuKey`, `back`, the input half of
//! `uiOp` and the pointer handlers): which key or button goes where, decided here so the game
//! screen only wires it.
//!
//! [`InputRouter`] holds the state game.ts kept for it (the buttons down and since when, the
//! taps armed, the aim arrow, the left stick's lean, the level map's pan); a [`GameHost`] is
//! the screen around it, which answers what is up (an overlay of the client's, the dock, the
//! chat) and does what is not input (open a screen, draw the aim arrow). Each call takes an
//! [`Io`]: the runner, the [`Env`] of the moment and the host.

use crate::bindings::{
    arms_tap_or_hold, button_action, hold_action, level_map, resolve, screen_key, stop_explore, Action, CommandCategory, MenuOp, OskOp, ScreenKey, SeqItem,
    UiOp, HOLD_MS,
};
use crate::context::{derive_mode, Context, Mode};
use crate::focus::FocusOp;
use crate::gamepad::{is_pad_activity, Button, DirSource, PadEvent};
use crate::gamepad_hints::{GamepadHints, PadHintEvidence};
use crate::keys::{direction_key, keydown_message, DirMod, KeyLike};
use crate::pack_tabs::is_pack;
use crate::runner::{Env, LastStep, Runner, RunnerHooks, StepOpts};
use crate::touch::Gesture;
use isorun_render::camera::{DEFAULT_ANGLES, TURN_DEGREES};
use isorun_render::stick_to_dir;
use isorun_scene::{dir_from_delta, Dir8, DIR8_DX, DIR8_DY};
use isorun_webtiles::cm;

/// How much the view zooms per second at full right stick.
pub const ZOOM_PER_STICK_SECOND: f32 = 1.5;
/// How much the view tilts for one press of Alt with Up or Down, in degrees.
pub const TILT_PER_KEY: f32 = 5.0;
/// level-map cells per unit of right-stick look while the map is open
pub const MAP_PAN_RATE: f32 = 0.12;

/// The device the player touched last; the prompt strip is drawn for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum InputDevice {
    Pad,
    #[default]
    Keyboard,
    Pointer,
}

/// The client's own screens the menu keys and buttons open and close (`Overlays.openMenu`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayMenu {
    /// X's actions (the frame an empty tab stands in)
    Battle,
    Levelmap,
    Equipment,
    System,
}

/// What a key or a button does to a client overlay up (`Overlays.clientOverlayInput`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayOp {
    Close,
    Select,
    Submit,
    Cancel,
    Keyboard,
    Alt,
    BumperPrev,
    BumperNext,
    Next,
    Prev,
    Left,
    Right,
    PageNext,
    PagePrev,
    First,
    Last,
}

/// The dock's screens a key opens in command mode (`i` the inventory, `@` the character).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockScreen {
    Inventory,
    Character,
}

/// The settings the routing reads (servers.ts `Settings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouterSettings {
    /// the Arrow keys setting on Aim: an arrow points the aim arrow, Space or Enter steps that way
    pub arrow_keys_aim: bool,
    /// the Left stick setting on Aim: the stick points the aim arrow, A steps that way
    pub left_stick_aim: bool,
    /// the LT explore setting on Hold
    pub explore_on_hold: bool,
}

impl Default for RouterSettings {
    fn default() -> Self {
        RouterSettings { arrow_keys_aim: true, left_stick_aim: true, explore_on_hold: true }
    }
}

impl RouterSettings {
    /// From the saved settings (`isorun_client::servers::Settings`).
    pub fn from_settings(s: &isorun_client::servers::Settings) -> Self {
        RouterSettings { arrow_keys_aim: s.arrow_keys == "aim", left_stick_aim: s.left_stick == "aim", explore_on_hold: s.explore_trigger == "hold" }
    }
}

/// The game screen around the router. Every method has a default that says nothing is up and
/// does nothing, so a host fills in what it has. The runner's hooks (`ui`, `menu`, `focus`,
/// `osk`, `status`, `swing`, `keep_map`) are the host's too; the router keeps the `ui` ops that
/// are input (explore, the level map, the view's reset) and hands the rest on.
pub trait GameHost: RunnerHooks {
    /// The frame loop should run (game.ts `wake`).
    fn wake(&mut self) {}
    /// The device that spoke (game.ts `inputFrom`); the router keeps its own copy too.
    fn input_from(&mut self, _device: InputDevice) {}
    /// A keyboard or pad action fired: the pointer's own selection and its tooltip go.
    fn input_acted(&mut self) {}
    /// A text field has the keys (the chat line, a menu filter, the seed box).
    fn text_entry_focused(&self) -> bool {
        false
    }
    fn look_panel_open(&self) -> bool {
        false
    }
    fn close_look_panel(&mut self) {}
    fn cell_menu_open(&self) -> bool {
        false
    }
    fn close_cell_menu(&mut self) {}
    /// Open the cell menu at the finger (logical px) on the cell picked there: false where there is none.
    fn open_cell_menu(&mut self, _at: egui::Pos2) -> bool {
        false
    }
    /// The cell under a point of the view (logical px), as the renderer picks it.
    fn pick(&mut self, _at: egui::Pos2) -> Option<(i32, i32)> {
        None
    }
    /// The pointer's cursor: on the cell under `at` (a hovering mouse, a finger), or off.
    fn hover(&mut self, _at: Option<egui::Pos2>, _by_touch: bool) {}
    /// `tile_web_mouse_control`: clicks on the view act.
    fn mouse_control(&self) -> bool {
        true
    }
    /// One of the client's own overlays is up (`Overlays.hasClientOverlay`).
    fn client_overlay(&self) -> bool {
        false
    }
    fn open_menu(&self) -> Option<OverlayMenu> {
        None
    }
    /// The overlay up is a screen of the dock's (inventory, character).
    fn screen_open(&self) -> bool {
        false
    }
    fn client_overlay_input(&mut self, _op: OverlayOp) -> bool {
        false
    }
    fn client_overlay_hotkey(&mut self, _key: &str) -> bool {
        false
    }
    /// The focus layer's cursor, for the hints' evidence.
    fn focus_index(&self) -> Option<i64> {
        None
    }
    fn dock_on(&self) -> bool {
        false
    }
    fn dock_focused(&self) -> bool {
        false
    }
    fn dock_key(&mut self, _key: &KeyLike) -> bool {
        false
    }
    fn dock_pad(&mut self, _ev: &PadEvent) -> bool {
        false
    }
    fn dock_focus_off(&mut self) {}
    fn dock_button(&mut self, _screen: DockScreen) {}
    fn chat_on(&self) -> bool {
        false
    }
    fn chat_capturing(&self) -> bool {
        false
    }
    fn chat_pad(&mut self, _ev: &PadEvent) {}
    fn chat_focus(&mut self) {}
    fn chat_close(&mut self) {}
    fn level_up_choosing(&self) -> bool {
        false
    }
    /// The level map's own canvas is up (crawl's or held: map_hold.rs).
    fn level_map_open(&self) -> bool {
        false
    }
    /// The popup up scrolls text (menu-nav.ts `scrollKeyIntent`): Space and the paging keys are its.
    fn popup_scrolls(&self) -> bool {
        false
    }
    /// A server menu's own keys on the client (Overlays.menuKey): true when taken.
    fn menu_key(&mut self, _key: &KeyLike) -> bool {
        false
    }
    /// A popup's own keys on the client (Overlays.popupKey): true when taken.
    fn popup_key(&mut self, _key: &KeyLike) -> bool {
        false
    }
    fn show_system(&mut self, _spectating: bool) {}
    fn show_palette(&mut self, _category: CommandCategory) {}
    /// An empty tab of X's actions takes Y and LT as the actions that are no list (game.ts `actionKey`).
    fn action_key(&mut self, _key: &str) {}
    /// The aim arrow on the ground: the way the held arrows or the stick point, or none.
    fn set_aim(&mut self, _dir: Option<Dir8>) {}
    /// Travel at the explore's pace (game.ts `paceTravel`).
    fn pace_travel(&mut self) {}
    /// Spectating: the log scrolls by lines.
    fn scroll_log(&mut self, _by: i32) {}
}

/// What a step the aim arrow points would do (game.ts `aimIntent`), for its colour: swing at a
/// monster there, step, or nothing where a wall stands.
pub fn aim_intent(scene: &isorun_scene::Scene, dir: Dir8) -> isorun_render::AimIntent {
    use isorun_scene::{Attitude, BillboardKind, CellKind};
    let x = scene.player.0 + DIR8_DX[dir as usize];
    let y = scene.player.1 + DIR8_DY[dir as usize];
    if scene.billboards.iter().any(|b| b.kind == BillboardKind::Monster && b.x == x && b.y == y && b.attitude != Some(Attitude::Friendly)) {
        return isorun_render::AimIntent::Attack;
    }
    match scene.cell(x, y) {
        Some(c) if !(c.occluder && c.kind != CellKind::Door) => isorun_render::AimIntent::Move,
        _ => isorun_render::AimIntent::Blocked,
    }
}

/// A host that is nothing but the runner's hooks: for a game screen that has no overlays yet.
pub struct BareHost<'a>(pub &'a mut dyn RunnerHooks);

impl RunnerHooks for BareHost<'_> {
    fn ui(&mut self, op: UiOp, arg: Option<i32>, category: Option<CommandCategory>, section: Option<&str>) {
        self.0.ui(op, arg, category, section)
    }
    fn osk(&mut self, op: OskOp, dir: Option<Dir8>) {
        self.0.osk(op, dir)
    }
    fn menu(&mut self, op: MenuOp) {
        self.0.menu(op)
    }
    fn focus(&mut self, op: FocusOp) {
        self.0.focus(op)
    }
    fn status(&mut self, text: &str) {
        self.0.status(text)
    }
    fn swing(&mut self, x: i32, y: i32) {
        self.0.swing(x, y)
    }
    fn keep_map(&mut self) {
        self.0.keep_map()
    }
}
impl GameHost for BareHost<'_> {}

/// What a router call works with: the runner, the situation, the screen.
pub struct Io<'a, 'b> {
    pub runner: &'b mut Runner,
    pub env: Env<'a>,
    pub host: &'b mut dyn GameHost,
}

/// The input half of `uiOp`: what a ui op the runner asked for does that is input. The rest go
/// to the host. Keys an op sends go out after the runner call that asked (`then`).
struct Hooks<'h> {
    host: &'h mut dyn GameHost,
    then: Vec<Action>,
    explore_while_held: bool,
    explore_at_once: bool,
    reset_view: bool,
}

impl RunnerHooks for Hooks<'_> {
    fn ui(&mut self, op: UiOp, arg: Option<i32>, category: Option<CommandCategory>, section: Option<&str>) {
        match op {
            UiOp::ExploreWhileHeld => {
                self.explore_while_held = true;
                self.then.push(Action::keys(vec![SeqItem::text("o")], "Explore while held"));
            }
            UiOp::InstantExplore => {
                self.explore_at_once = true;
                self.host.pace_travel();
                self.then.push(Action::keys(vec![SeqItem::text("o")], "Explore at once"));
            }
            UiOp::Levelmap => self.then.push(level_map()),
            UiOp::ResetView => self.reset_view = true,
            UiOp::ScrollLog => {
                if let Some(by) = arg.filter(|a| *a != 0) {
                    self.host.scroll_log(by);
                }
            }
            UiOp::Palette => self.host.show_palette(category.unwrap_or(CommandCategory::Command)),
            _ => self.host.ui(op, arg, category, section),
        }
    }
    fn osk(&mut self, op: OskOp, dir: Option<Dir8>) {
        self.host.osk(op, dir)
    }
    fn menu(&mut self, op: MenuOp) {
        self.host.menu(op)
    }
    fn focus(&mut self, op: FocusOp) {
        self.host.focus(op)
    }
    fn status(&mut self, text: &str) {
        self.host.status(text)
    }
    fn swing(&mut self, x: i32, y: i32) {
        self.host.swing(x, y)
    }
    fn keep_map(&mut self) {
        self.host.keep_map()
    }
}

/// The function keys that open Isorun's own menus: the pad's four, so a player can rebind a
/// button (or anything) to them. A bare F1 goes on to crawl (its game menu).
fn menu_key_of(key: &str) -> Option<OverlayMenu> {
    match key {
        "F2" => Some(OverlayMenu::Levelmap),
        "F3" => Some(OverlayMenu::Battle),
        "F4" => Some(OverlayMenu::Equipment),
        "F5" => Some(OverlayMenu::System),
        _ => None,
    }
}

fn arrow_vector(key: &str) -> Option<(i32, i32)> {
    match key {
        "ArrowUp" => Some((0, -1)),
        "ArrowDown" => Some((0, 1)),
        "ArrowLeft" => Some((-1, 0)),
        "ArrowRight" => Some((1, 0)),
        _ => None,
    }
}

#[derive(Default)]
pub struct InputRouter {
    pub settings: RouterSettings,
    /// The device that spoke last.
    pub last_input: InputDevice,
    /// When each pad button went down.
    press_times: Vec<(Button, f64)>,
    /// The buttons whose hold half has fired: their release is not also a tap.
    hold_fired: Vec<Button>,
    /// The buttons whose press opened a tap-or-hold, so their release may fire one. A press that
    /// already acted at once (A on a --more--) is not here, so its release cannot become a tap
    /// in whatever mode the server moved to meanwhile.
    tap_armed: Vec<Button>,
    /// A tap-or-hold under way, and how far along its hold is (0..1): its prompt shows it.
    pub holding: Option<(Button, f32)>,
    /// The button that opened the client overlay up: it puts it away whole.
    overlay_opener: Option<Button>,
    /// LT held on the Hold setting explores until it is let go.
    exploring_while_held: bool,
    /// LT tapped on the Hold setting explores at once, at the travel's pace.
    exploring_at_once: bool,
    /// The arrow keys held down while they aim, and the way they point.
    arrows_down: Vec<String>,
    pub aim_dir: Option<Dir8>,
    /// A pressed on the Left stick setting's aim stepped: its release is that step's, not a tap of its own
    stick_aim_stepped: bool,
    /// Where the left stick leans on the screen, kept while a menu is up so the aim comes back after it
    stick_lean: Option<Dir8>,
    /// the right stick's level-map pan, in cells, built up until a whole one
    map_pan: (f32, f32),
    /// the right stick is pushed: up and down zoom, in the frame loop
    pub pad_looking: bool,
    pad_zoom: f32,
    /// The pad step the runner took last: confirmed by the server, it teaches Move.
    pub pad_step: Option<LastStep>,
    pub hints: Option<GamepadHints>,
}

impl InputRouter {
    pub fn new(settings: RouterSettings) -> Self {
        InputRouter { settings, ..Default::default() }
    }

    /// Run a runner call with the router's hooks between it and the host, then what it asked for.
    fn with_hooks(&mut self, io: &mut Io, f: impl FnOnce(&mut Runner, &mut Env, &mut dyn RunnerHooks)) {
        self.with_hooks_dyn(io, Box::new(f));
    }

    fn with_hooks_dyn(&mut self, io: &mut Io, f: Box<dyn FnOnce(&mut Runner, &mut Env, &mut dyn RunnerHooks) + '_>) {
        let mut hooks = Hooks { host: &mut *io.host, then: Vec::new(), explore_while_held: false, explore_at_once: false, reset_view: false };
        f(io.runner, &mut io.env, &mut hooks);
        let Hooks { then, explore_while_held, explore_at_once, reset_view, .. } = hooks;
        self.exploring_while_held |= explore_while_held;
        self.exploring_at_once |= explore_at_once;
        if reset_view {
            io.env.camera.set_angles(DEFAULT_ANGLES);
        }
        for a in then {
            self.with_hooks_dyn(io, Box::new(move |r: &mut Runner, env: &mut Env, h: &mut dyn RunnerHooks| r.execute(&a, env, h)));
        }
    }

    /// Execute an action as the game does, through the router's hooks.
    pub fn execute(&mut self, a: &Action, io: &mut Io) {
        self.with_hooks(io, |r, env, h| r.execute(a, env, h));
    }

    /// A server message came in: a sequence the runner holds may go on (`Runner::on_state`).
    pub fn server_state(&mut self, io: &mut Io) {
        self.with_hooks(io, |r, env, h| r.on_state(env, h));
    }

    /// The time moved on: a sequence past its time gives up (`Runner::tick`).
    pub fn tick(&mut self, io: &mut Io) {
        let now = io.env.now;
        self.with_hooks(io, |r, _env, h| r.tick(now, h));
    }

    /// The interaction chooser's pick (game.ts `uiOp('interact')`'s two rows): the feature (false) or the pile (true).
    pub fn contextual(&mut self, alt: bool, io: &mut Io) {
        self.with_hooks(io, |r, env, h| r.contextual(Some(alt), env, h));
    }

    /// The context filled with what the runner and the settings know (game.ts `loop`): look
    /// mode, a walking explore, LT on Hold. Call once a frame on the context derived from the state.
    pub fn fill_context(&self, ctx: &mut Context, runner: &mut Runner, input_mode: i64, now: f64) {
        // the server reports targeting alone; the runner knows whether its `x` opened it
        if runner.examining(ctx.mode, now) {
            ctx.examining = true;
        }
        if runner.exploring(input_mode, now) {
            ctx.exploring = true;
        }
        if self.settings.explore_on_hold {
            ctx.explore_on_hold = true;
        }
    }

    /// The frame's input work (game.ts `loop`): the aim arrow goes outside command mode, the
    /// right stick turns only while walking the view, the holds fire, the explore at once ends,
    /// and a pushed right stick zooms. `dt` in seconds. Returns whether the right stick should
    /// turn (`GamepadInput::right_stick_turns`).
    pub fn frame(&mut self, io: &mut Io, pad_held: &dyn Fn(Button) -> bool, dt: f32) -> bool {
        let ctx = io.env.ctx;
        if self.aim_dir.is_some() && ctx.mode != Mode::Command {
            self.clear_aim(io.host);
        }
        let right_stick_turns = ctx.mode == Mode::Command && !io.host.client_overlay() && !io.host.chat_capturing();
        if self.pad_looking && self.last_input == InputDevice::Pad && ctx.mode == Mode::Command && !io.host.client_overlay() && !io.env.watching {
            io.env.camera.zoom_by((-self.pad_zoom * ZOOM_PER_STICK_SECOND * dt).exp());
            if let Some(h) = self.hints.as_mut() {
                h.looked_by(self.pad_zoom.abs() * dt);
            }
        }
        self.fire_holds(io, pad_held);
        if self.exploring_at_once && !io.runner.explore_open() {
            self.exploring_at_once = false;
            io.host.pace_travel();
        }
        right_stick_turns
    }

    /// Fire the hold half of any tap-or-hold button that has been down for HOLD_MS, without waiting for release.
    pub fn fire_holds(&mut self, io: &mut Io, pad_held: &dyn Fn(Button) -> bool) {
        self.holding = None;
        if io.host.chat_capturing() || io.host.client_overlay() {
            self.tap_armed.clear();
            return;
        }
        let now = io.env.now;
        for (b, t0) in self.press_times.clone() {
            if !self.tap_armed.contains(&b) {
                continue;
            }
            // the server may have moved the mode under the press: a button the new context has no tap-or-hold for
            // disarms, so neither half can fire where it would mean something else (LB pressed in a menu must not rest)
            if !arms_tap_or_hold(b, io.env.ctx) {
                self.tap_armed.retain(|x| *x != b);
                continue;
            }
            if self.hold_fired.contains(&b) || !pad_held(b) {
                continue;
            }
            let Some(a) = hold_action(b, io.env.ctx) else { continue };
            if now - t0 < HOLD_MS {
                // the button's own prompt shows how far along the hold is
                self.holding = Some((b, ((now - t0) / HOLD_MS) as f32));
                continue;
            }
            self.hold_fired.push(b);
            self.execute_pad_action(&a, io);
        }
    }

    fn pad_hint_evidence(&self, io: &Io) -> PadHintEvidence {
        let st = io.env.state;
        let cursor = st.cursors.first().copied().flatten().or(st.cursors.get(2).copied().flatten());
        PadHintEvidence {
            mode: derive_mode(st),
            turn: st.player.turn,
            x: st.player.pos.x,
            y: st.player.pos.y,
            cursor: cursor.map(|c| format!("{},{}", c.x, c.y)).unwrap_or_default(),
            focus: io.host.focus_index().or(st.menus.last().map(|m| m.last_hovered)).unwrap_or(-1),
            client_overlay: io.host.client_overlay(),
        }
    }

    fn execute_pad_action(&mut self, a: &Action, io: &mut Io) {
        let before_step = io.runner.last_step;
        if self.hints.is_some() {
            let ev = self.pad_hint_evidence(io);
            let ctx = io.env.ctx;
            let now = io.env.now;
            if let Some(h) = self.hints.as_mut() {
                h.attempt(a, ctx, ev, now);
            }
        }
        self.execute(a, io);
        self.pad_step =
            if matches!(a, Action::Step { .. }) && io.env.ctx.mode == Mode::Command && io.runner.last_step != before_step { io.runner.last_step } else { None };
        if self.hints.is_some() {
            let ev = self.pad_hint_evidence(io);
            let now = io.env.now;
            if let Some(h) = self.hints.as_mut() {
                h.observe(&ev, now);
            }
        }
    }

    /// Which device spoke last: the prompt strip is drawn in its terms.
    pub fn input_from(&mut self, device: InputDevice, host: &mut dyn GameHost) {
        if device != InputDevice::Pad {
            if let Some(h) = self.hints.as_mut() {
                h.cancel();
            }
            self.pad_step = None;
            self.pad_looking = false;
        }
        host.input_from(device);
        self.last_input = device;
    }

    fn input_acted(&mut self, host: &mut dyn GameHost) {
        self.tap_armed.clear();
        self.holding = None;
        host.input_acted();
    }

    /// One pad event (game.ts `pad`).
    pub fn pad(&mut self, ev: &PadEvent, io: &mut Io) {
        io.host.wake();
        // a press, a stick or the d-pad is the pad speaking; a stick settling back to centre is not
        if is_pad_activity(ev) {
            self.input_from(InputDevice::Pad, io.host);
        }
        if io.host.chat_capturing() {
            io.host.chat_pad(ev);
            return;
        }
        if io.host.dock_focused() && io.host.dock_pad(ev) {
            return;
        }
        if io.host.client_overlay() {
            match ev {
                PadEvent::Press { button, .. } => {
                    let b = *button;
                    let open = io.host.open_menu();
                    // the button that opened this screen puts it away whole, before anything else it would do here
                    if Some(b) == self.overlay_opener && !(io.host.screen_open() && b == Button::Y) {
                        io.host.client_overlay_input(OverlayOp::Close);
                    } else if open == Some(OverlayMenu::Battle) && (b == Button::Y || b == Button::LT) {
                        // an empty tab of X's actions keeps the two that are no list: Y swaps weapons, LT shouts
                        io.host.action_key(if b == Button::Y { crate::action_tabs::SWAP_WEAPONS_KEY } else { crate::action_tabs::SHOUT_KEY });
                    } else {
                        let op = match b {
                            Button::A => Some(OverlayOp::Select),
                            Button::Start => Some(OverlayOp::Submit),
                            Button::B => Some(OverlayOp::Cancel),
                            // Select is the put-away button even for a screen it did not open
                            Button::Select => Some(OverlayOp::Close),
                            Button::X => Some(OverlayOp::Keyboard),
                            Button::Y => Some(OverlayOp::Alt),
                            Button::LB => Some(OverlayOp::BumperPrev),
                            Button::RB => Some(OverlayOp::BumperNext),
                            _ => None,
                        };
                        if let Some(op) = op {
                            io.host.client_overlay_input(op);
                        }
                    }
                    // put away by any route: the next opener owns the next screen
                    if !io.host.client_overlay() {
                        self.overlay_opener = None;
                    }
                }
                PadEvent::Dir { dir: Some(d), .. } | PadEvent::DirRepeat { dir: d, .. } => {
                    let op = match d {
                        0 => Some(OverlayOp::Prev),
                        4 => Some(OverlayOp::Next),
                        6 => Some(OverlayOp::Left),
                        2 => Some(OverlayOp::Right),
                        _ => None,
                    };
                    if let Some(op) = op {
                        io.host.client_overlay_input(op);
                    }
                }
                _ => {}
            }
            return;
        }
        if self.stick_aim(ev, io) {
            return;
        }
        if let PadEvent::Look { dx, dy, .. } = ev {
            self.pad_looking = *dx != 0.0 || *dy != 0.0;
            if io.env.ctx.mode == Mode::Levelmap {
                // the level map is north-up: the right stick pans it instead of the camera
                self.pan_map(*dx, *dy, io);
                return;
            }
            self.map_pan = (0.0, 0.0);
            // the stick's left and right turn the view (`RStick` directions); up and down zoom it, in the frame loop
            self.pad_zoom = *dy;
            return;
        }
        if let PadEvent::Press { button, t } = ev {
            self.press_times.retain(|(b, _)| b != button);
            self.press_times.push((*button, *t));
            self.hold_fired.retain(|b| b != button);
            self.tap_armed.retain(|b| b != button);
            if io.env.ctx.mode != Mode::Spectating && arms_tap_or_hold(*button, io.env.ctx) {
                self.tap_armed.push(*button);
            }
        }
        if let PadEvent::Release { button, .. } = ev {
            if *button == Button::LT && self.exploring_while_held {
                self.exploring_while_held = false;
                if io.runner.explore_open() {
                    self.execute(&stop_explore(), io);
                }
            }
            self.press_times.retain(|(b, _)| b != button);
            self.holding = None;
            let armed = self.tap_armed.contains(button);
            self.tap_armed.retain(|b| b != button);
            let fired = self.hold_fired.contains(button);
            self.hold_fired.retain(|b| b != button);
            if fired || !armed {
                return;
            }
        }
        if io.env.ctx.mode == Mode::Spectating {
            if let PadEvent::Press { button: b @ (Button::B | Button::Start), .. } = ev {
                io.host.show_system(true);
                self.overlay_opener = Some(*b);
            }
            // a spectator's chat is one button away, as F12 is in the official client
            if let PadEvent::Press { button: Button::X, .. } = ev {
                io.host.chat_focus();
            }
            if let PadEvent::Dir { dir: Some(d), .. } | PadEvent::DirRepeat { dir: d, .. } = ev {
                io.host.scroll_log(if *d == 0 {
                    -1
                } else if *d == 4 {
                    1
                } else {
                    0
                });
            }
            return;
        }
        let Some(a) = resolve(ev, io.env.ctx) else { return };
        self.input_acted(io.host);
        let overlay_was_up = io.host.client_overlay();
        self.execute_pad_action(&a, io);
        // whichever button just opened a client overlay owns closing it; a screen
        // reached from inside one keeps the opener it came with
        if let PadEvent::Press { button, .. } = ev {
            if !io.host.client_overlay() {
                self.overlay_opener = None;
            } else if !overlay_was_up && io.host.open_menu() != Some(OverlayMenu::Battle) {
                // (the frame a turn of X's tabs stands in is no screen of the bumper's: the bumper turns it again)
                self.overlay_opener = Some(*button);
            }
        }
    }

    /// The Left stick setting on Aim: the stick points the aim arrow on the ground the way it
    /// leans on the screen, and A steps that way, again on each of its repeats while held; the
    /// stick back at centre puts the arrow away. Whether the event was one of these.
    fn stick_aim(&mut self, ev: &PadEvent, io: &mut Io) -> bool {
        if let PadEvent::Release { button: Button::A, .. } = ev {
            if self.stick_aim_stepped {
                self.stick_aim_stepped = false;
                return true;
            }
        }
        if let PadEvent::Dir { source: DirSource::LStick, dir } = ev {
            self.stick_lean = *dir;
        }
        if !self.settings.left_stick_aim || io.env.ctx.mode != Mode::Command || io.host.level_map_open() || io.env.watching {
            return false;
        }
        match ev {
            PadEvent::Dir { source: DirSource::LStick, dir } => {
                self.input_acted(io.host);
                if dir.is_none() {
                    self.clear_aim(io.host);
                } else {
                    self.refresh_aim(io);
                }
                true
            }
            PadEvent::DirRepeat { source: DirSource::LStick, .. } => true,
            PadEvent::Press { button: Button::A, .. } | PadEvent::Repeat { button: Button::A, .. } if self.stick_lean.is_some() => {
                self.input_acted(io.host);
                self.refresh_aim(io);
                let Some(dir) = self.aim_dir else { return true };
                self.stick_aim_stepped = true;
                self.with_hooks(io, |r, env, h| r.walk(dir, StepOpts::default(), env, h));
                true
            }
            _ => false,
        }
    }

    fn set_aim(&mut self, dir: Dir8, host: &mut dyn GameHost) {
        self.aim_dir = Some(dir);
        host.set_aim(Some(dir));
    }

    fn clear_aim(&mut self, host: &mut dyn GameHost) {
        self.arrows_down.clear();
        self.aim_dir = None;
        host.set_aim(None);
    }

    /// The way the held arrows point, as a compass direction on the ground, shown as the aim arrow.
    fn update_aim(&mut self, io: &mut Io) {
        let (mut x, mut y) = (0, 0);
        for k in &self.arrows_down {
            if let Some((vx, vy)) = arrow_vector(k) {
                x += vx;
                y += vy;
            }
        }
        if x == 0 && y == 0 {
            return self.clear_aim(io.host);
        }
        let d = stick_to_dir(x as f32, y as f32, io.env.camera.yaw_goal());
        self.set_aim(d, io.host);
    }

    /// The aim arrow again for a new scene or a turn of the camera: from the held arrows, or
    /// from where the left stick leans on the Left stick setting's aim.
    pub fn refresh_aim(&mut self, io: &mut Io) {
        match self.stick_lean.filter(|_| self.settings.left_stick_aim) {
            None => self.update_aim(io),
            Some(lean) => {
                let d = stick_to_dir(DIR8_DX[lean as usize] as f32, DIR8_DY[lean as usize] as f32, io.env.camera.yaw_goal());
                self.set_aim(d, io.host);
            }
        }
    }

    /// The arrow keys aim, as on the test bed (the Arrow keys setting on Aim): an arrow on the
    /// ground points the way the held arrows point on the screen, two of them a diagonal, and
    /// Space or Enter steps that way, again and again while it is held. Whether the key was one of these.
    fn aim_key(&mut self, key: &KeyLike, io: &mut Io) -> bool {
        if key.ctrl || key.alt || key.meta || key.shift {
            return false;
        }
        let mode = io.env.ctx.mode;
        if arrow_vector(&key.key).is_some() && self.settings.arrow_keys_aim && mode == Mode::Command && !io.host.level_map_open() {
            self.input_acted(io.host);
            if !self.arrows_down.contains(&key.key) {
                self.arrows_down.push(key.key.clone());
            }
            self.update_aim(io);
            return true;
        }
        if (key.key == " " || key.key == "Enter") && mode == Mode::Command {
            if let Some(dir) = self.aim_dir {
                self.input_acted(io.host);
                self.with_hooks(io, |r, env, h| r.walk(dir, StepOpts::default(), env, h));
                return true;
            }
        }
        if key.key == "Escape" && self.aim_dir.is_some() {
            self.clear_aim(io.host);
            return true;
        }
        false
    }

    /// A key let go: an aiming arrow no longer points.
    pub fn key_up(&mut self, key: &KeyLike, io: &mut Io) {
        let before = self.arrows_down.len();
        self.arrows_down.retain(|k| *k != key.key);
        if self.arrows_down.len() != before {
            self.update_aim(io);
        }
    }

    /// The window lost the keys: nothing is held any more.
    pub fn blur(&mut self, host: &mut dyn GameHost) {
        if !self.arrows_down.is_empty() {
            self.clear_aim(host);
        }
    }

    /// Right stick over the level map: accumulate deflection and step the map
    /// cursor (north-up, absolute) each time a whole cell's worth builds up.
    fn pan_map(&mut self, dx: f32, dy: f32, io: &mut Io) {
        if dx == 0.0 && dy == 0.0 {
            self.map_pan = (0.0, 0.0);
            return;
        }
        let p = &mut self.map_pan;
        p.0 += dx * MAP_PAN_RATE;
        p.1 += dy * MAP_PAN_RATE;
        let sx = if p.0.abs() >= 1.0 { p.0.signum() as i32 } else { 0 };
        let sy = if p.1.abs() >= 1.0 { p.1.signum() as i32 } else { 0 };
        if sx == 0 && sy == 0 {
            return;
        }
        p.0 -= sx as f32;
        p.1 -= sy as f32;
        if let Some(d) = dir_from_delta(sx, sy) {
            self.execute(&Action::Cursor { dir: d }, io);
        }
    }

    /// Alt with the arrows: left and right turn the view a step, up and down tilt it. Whether the key was one of them.
    fn view_key(&mut self, key: &str, io: &mut Io) -> bool {
        match key {
            "ArrowLeft" => io.env.camera.turn(-TURN_DEGREES),
            "ArrowRight" => io.env.camera.turn(TURN_DEGREES),
            "ArrowUp" => io.env.camera.tilt_by(TILT_PER_KEY),
            "ArrowDown" => io.env.camera.tilt_by(-TILT_PER_KEY),
            _ => return false,
        }
        true
    }

    /// One of the menu keys (F2-F5): it opens that menu, and closes it again as the button does.
    pub fn menu_key(&mut self, menu: OverlayMenu, io: &mut Io) {
        let open = io.host.open_menu();
        let ctx = io.env.ctx;
        let esc = || Action::keys(vec![SeqItem::key(27)], "Cancel");
        if io.env.watching {
            // a spectator has the Isorun menu only, as the pad's Start
            if menu != OverlayMenu::System {
                return;
            }
            if open == Some(OverlayMenu::System) {
                io.host.client_overlay_input(OverlayOp::Close);
            } else if !io.host.client_overlay() {
                io.host.show_system(true);
            }
            return;
        }
        let ui = |m: OverlayMenu| match m {
            OverlayMenu::Battle => UiOp::Commands,
            OverlayMenu::Levelmap => UiOp::Levelmap,
            OverlayMenu::Equipment => UiOp::Equipment,
            OverlayMenu::System => UiOp::System,
        };
        // Select's level map is crawl's own: F2 again puts it away, as Escape would
        if menu == OverlayMenu::Levelmap && ctx.mode == Mode::Levelmap {
            self.execute(&esc(), io);
        } else if open == Some(menu) {
            io.host.client_overlay_input(OverlayOp::Close);
        } else if menu == OverlayMenu::Equipment && ctx.mode == Mode::Menu && ctx.menu.as_ref().is_some_and(|m| is_pack(&m.menu)) {
            // the pack is crawl's own menu: F4 again puts it away, as Escape would
            self.execute(&esc(), io);
        } else if menu == OverlayMenu::Battle && ctx.mode == Mode::Menu && ctx.menu.as_ref().is_some_and(|m| m.actions.is_some()) {
            // and so are X's actions
            self.execute(&esc(), io);
        } else if ctx.mode == Mode::Command && (!io.host.client_overlay() || open.is_some()) {
            // from the map, or from another of the four, the key goes straight to its own menu;
            // the level map is crawl's, under ours: the menu up goes first
            if menu == OverlayMenu::Levelmap && open.is_some() {
                io.host.client_overlay_input(OverlayOp::Close);
            }
            self.execute(&Action::ui(ui(menu)), io);
        } else if io.host.client_overlay() {
            io.host.client_overlay_input(OverlayOp::Close);
        } else if menu == OverlayMenu::System {
            // elsewhere the Isorun menu opens where Start opens it, and nowhere Start means something else
            if matches!(button_action(Button::Start, ctx), Some(Action::Ui { op: UiOp::System, .. })) {
                self.execute(&Action::ui(UiOp::System), io);
            }
        } else if menu == OverlayMenu::Levelmap {
            // off the map F2 is the Select button there: the palette for the screen
            let category = match ctx.mode {
                Mode::Targeting => CommandCategory::Targeting,
                Mode::Menu => CommandCategory::Menu,
                _ => CommandCategory::Command,
            };
            io.host.show_palette(category);
        }
    }

    /// A key going down (game.ts `onKeyDown`). True when the key was taken, so it goes no further.
    pub fn key_down(&mut self, key: &KeyLike, repeat: bool, io: &mut Io) -> bool {
        if io.host.look_panel_open() && key.key == "Escape" {
            io.host.close_look_panel();
            return true;
        }
        // any key puts the cell menu away; Escape does nothing more
        if io.host.cell_menu_open() {
            io.host.close_cell_menu();
            if key.key == "Escape" {
                return true;
            }
        }
        io.host.wake();
        // a key typed into a field is still the keyboard speaking
        self.input_from(InputDevice::Keyboard, io.host);
        if io.host.text_entry_focused() {
            return false;
        }
        // F2–F5 are Isorun's own: the pad's four menus. A bare F1 goes on to Crawl, which binds it to its game menu
        if !key.shift && !key.ctrl && !key.alt && !key.meta {
            if let Some(menu) = menu_key_of(&key.key) {
                if !repeat {
                    self.menu_key(menu, io);
                }
                return true;
            }
        }
        if io.host.dock_focused() && io.host.dock_key(key) {
            return true;
        }
        if key.alt && !key.ctrl && !key.meta && !io.host.level_map_open() && self.view_key(&key.key, io) {
            return true;
        }
        if io.host.client_overlay() {
            let op = match key.key.as_str() {
                "Escape" => Some(OverlayOp::Cancel),
                "ArrowDown" => Some(OverlayOp::Next),
                "ArrowUp" => Some(OverlayOp::Prev),
                "ArrowLeft" => Some(OverlayOp::Left),
                "ArrowRight" => Some(OverlayOp::Right),
                "PageDown" => Some(OverlayOp::PageNext),
                "PageUp" => Some(OverlayOp::PagePrev),
                "Home" => Some(OverlayOp::First),
                "End" => Some(OverlayOp::Last),
                // Enter and space both fire the row the cursor is on, as they do in a server menu
                "Enter" | " " => Some(OverlayOp::Select),
                _ => None,
            };
            if let Some(op) = op {
                io.host.client_overlay_input(op);
            } else if io.host.open_menu() == Some(OverlayMenu::Battle)
                && (key.key == crate::action_tabs::SWAP_WEAPONS_KEY || key.key == crate::action_tabs::SHOUT_KEY)
            {
                // an empty tab of X's actions: crawl's own keys for the two that are no list
                io.host.action_key(&key.key);
            } else if !key.alt && !key.meta {
                if key.key == "Tab" {
                    io.host.client_overlay_hotkey("Tab");
                } else if key.key.chars().count() == 1 {
                    let k = if key.ctrl { format!("Ctrl-{}", key.key.to_uppercase()) } else { key.key.clone() };
                    io.host.client_overlay_hotkey(&k);
                }
            }
            return true;
        }
        if key.key == "F12" || key.code == "F12" {
            // client.js: F12 focuses chat
            if io.host.chat_on() {
                io.host.chat_focus();
            }
            return true;
        }
        if io.env.watching {
            if key.key == "Escape" {
                io.host.show_system(true);
            }
            return true;
        }
        if self.aim_key(key, io) {
            return true;
        }
        // Esc opens the Isorun menu where crawl takes commands, and is crawl's own everywhere else
        let mode = io.env.ctx.mode;
        let mods = key.ctrl || key.alt || key.meta;
        if key.key == "Escape" && mode == Mode::Command && !mods && !key.shift {
            self.input_acted(io.host);
            if !repeat {
                self.menu_key(OverlayMenu::System, io);
            }
            return true;
        }
        let screen = match key.key.as_str() {
            "i" => Some(DockScreen::Inventory),
            "@" => Some(DockScreen::Character),
            _ => None,
        };
        if let Some(screen) = screen.filter(|_| mode == Mode::Command && io.host.dock_on() && !mods) {
            self.input_acted(io.host);
            if !repeat {
                io.host.dock_button(screen);
            }
            return true;
        }
        // on a screen the keyboard is a pad: Enter, Escape, Space and the arrows are its buttons and its
        // d-pad, and do exactly what those do (bindings.rs screen_key), down the same path
        if !mods && !key.shift {
            if let Some(sk) = screen_key(&key.key, io.env.ctx, io.host.popup_scrolls()) {
                // a pad's buttons never repeat: a held Enter confirms once; the arrows repeat as the d-pad does
                if repeat && matches!(sk, ScreenKey::Button(_)) {
                    return true;
                }
                self.input_acted(io.host);
                let a = match sk {
                    ScreenKey::Button(b) => button_action(b, io.env.ctx),
                    ScreenKey::Dir(d) => resolve(&PadEvent::Dir { source: DirSource::Dpad, dir: Some(d) }, io.env.ctx),
                };
                if let Some(a) = a {
                    self.execute(&a, io);
                }
                return true;
            }
        }
        // a server menu: paging and the paging characters move its hover and page on the client
        if mode == Mode::Menu && !io.env.ctx.menu.as_ref().is_some_and(|m| m.is_crt()) && io.host.menu_key(key) {
            self.input_acted(io.host);
            return true;
        }
        // a popup that scrolls on the client: arrows, paging and the paging characters scroll it
        if (mode == Mode::Popup || mode == Mode::Ended) && io.host.popup_key(key) {
            self.input_acted(io.host);
            return true;
        }
        // direction keys are rewritten in command and targeting modes only; macro capture and everything else is raw
        if mode == Mode::Command || mode == Mode::Targeting {
            if let Some(d) = direction_key(key) {
                // the key's compass meaning is taken as relative to facing; the runner rotates it back
                // (plain: step / cursor move, Shift: run / fire that way, Ctrl: attack)
                self.input_acted(io.host);
                let opts = StepOpts { run: d.modifier == DirMod::Shift, attack: d.modifier == DirMod::Ctrl, turns: false };
                self.with_hooks(io, |r, env, h| r.step(d.abs, opts, env, h));
                return true;
            }
        }
        let Some(msg) = keydown_message(key) else { return false };
        self.input_acted(io.host);
        io.runner.send(msg, &mut io.env);
        true
    }

    /// The game's Back (game.ts `back`): the phone's Back button, a tap outside a menu.
    pub fn back(&mut self, io: &mut Io) {
        io.host.wake();
        self.input_acted(io.host);
        if io.host.look_panel_open() {
            return io.host.close_look_panel();
        }
        if io.host.cell_menu_open() {
            return io.host.close_cell_menu();
        }
        if io.host.level_up_choosing() {
            return;
        }
        if io.host.chat_capturing() {
            return io.host.chat_close();
        }
        if io.host.dock_focused() {
            return io.host.dock_focus_off();
        }
        if io.host.client_overlay() {
            io.host.client_overlay_input(OverlayOp::Cancel);
            return;
        }
        if io.env.watching {
            return io.host.show_system(true);
        }
        if self.aim_dir.is_some() {
            return self.clear_aim(io.host);
        }
        match io.env.ctx.mode {
            Mode::Command => self.menu_key(OverlayMenu::System, io),
            Mode::Text => io.runner.send(cm::key(27), &mut io.env),
            _ => {
                if let Some(cancel) = button_action(Button::B, io.env.ctx) {
                    self.execute(&cancel, io);
                }
            }
        }
    }

    /// What the pointer and the fingers did (touch.rs `Gestures`), carried out: a click on the
    /// cell picked under it, a drag, a pinch's zoom, turn and tilt, a long press's cell menu.
    pub fn gesture(&mut self, g: &Gesture, io: &mut Io) {
        match *g {
            Gesture::Pressed(kind) => {
                self.input_from(InputDevice::Pointer, io.host);
                let _ = kind;
            }
            Gesture::TouchHover(at) => io.host.hover(at, true),
            Gesture::MouseHover(at) => io.host.hover(Some(at), false),
            Gesture::MouseGone => io.host.hover(None, false),
            Gesture::Drag { dx, dy } => {
                if !io.host.level_map_open() {
                    io.env.camera.drag(dx, dy);
                }
            }
            Gesture::Click { pos, button } => self.click(pos, button, io),
            Gesture::LongPress { pos } => {
                // the cell menu where there is one; a description of the cell otherwise
                let menu =
                    !io.env.watching && io.env.ctx.mode == Mode::Command && !io.host.level_map_open() && io.host.mouse_control() && io.host.open_cell_menu(pos);
                if !menu {
                    self.click(pos, 3, io);
                }
            }
            Gesture::Zoom(f) => io.env.camera.zoom_by(f),
            Gesture::Turn(deg) => io.env.camera.turn(deg),
            Gesture::Tilt(deg) => io.env.camera.tilt_by(deg),
            Gesture::SnapTurn => io.env.camera.snap_turn(),
        }
        io.host.wake();
    }

    /// A click at a point of the view: crawl's `click_cell` on the cell under it, the button
    /// numbered as `ev.which` (1 left, 2 middle, 3 right). The camera is never turned.
    fn click(&mut self, pos: egui::Pos2, button: i32, io: &mut Io) {
        if io.env.watching || !io.host.mouse_control() {
            return;
        }
        let Some((x, y)) = io.host.pick(pos) else { return };
        self.with_hooks(io, |r, env, h| r.click_cell(x, y, button, env, h));
    }
}

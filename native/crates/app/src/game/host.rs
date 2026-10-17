//! The game screen around the input router (`GameHost`, and the runner's `RunnerHooks`): what is
//! up (the client's lists, the Isorun menu, the HUD's screens, the dock, the chat, the cell menu,
//! the level map) and what is not input (open a screen, draw the aim arrow, pick the cell under
//! a point). What needs the runner, which the router holds while it calls the host, waits in
//! `Parts::deferred` for the game screen to carry out after the call.

use super::client::{ClientEvent, ClientMenu};
use super::{convert, mouse_control, Parts};
use isorun_hud::dock::NavOp;
use isorun_hud::DockButtonId;
use isorun_input::bindings::{CommandCategory, MenuOp, OskOp, UiOp};
use isorun_input::context::Context;
use isorun_input::focus::FocusOp;
use isorun_input::gamepad::PadEvent;
use isorun_input::keys::KeyLike;
use isorun_input::router::{aim_intent, DockScreen, GameHost, InputDevice, OverlayMenu, OverlayOp};
use isorun_input::runner::RunnerHooks;
use isorun_input::Action;
use isorun_overlays::OverlayCx;
use isorun_render::IsoAim;
use isorun_scene::{Dir8, Scene};
use isorun_webtiles::GameState;
use serde_json::Value;
use std::sync::Arc;

/// What the host asked for that the game screen carries out after the router's call.
pub enum Deferred {
    Execute(Action),
    Send(Value),
    SendThen(Value, Vec<isorun_input::SeqItem>),
    Swing(i32, i32),
    ActionTab(isorun_input::action_tabs::ActionTabId),
    ActionTabStep(i32),
    ActionKey(String),
    Equipment,
    PaceTravel,
    Client(ClientEvent),
    MenuActions(Vec<isorun_front::MenuAction>),
    Hud(isorun_hud::HudCommand),
    DockScreen(DockButtonId),
    DockNav(NavOp),
    ExploreAtOnce,
    Fullscreen(bool),
}

pub struct Host<'a> {
    p: &'a mut Parts,
    state: &'a GameState,
    scene: &'a Arc<Scene>,
    ctx: &'a Context,
    watching: bool,
    /// ms
    now: f64,
    chat: bool,
}

impl<'a> Host<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(p: &'a mut Parts, state: &'a GameState, scene: &'a Arc<Scene>, ctx: &'a Context, watching: bool, now: f64, chat: bool, _online: bool) -> Self {
        Host { p, state, scene, ctx, watching, now, chat }
    }

    fn overlay_send(&mut self, f: impl FnOnce(&mut isorun_overlays::Overlays, &mut OverlayCx)) {
        let mut out = Vec::new();
        {
            let mut cx = OverlayCx::new(self.state, &mut self.p.icons, &mut out);
            cx.watching = self.watching;
            cx.device = convert::device(self.p.last_input).1;
            f(&mut self.p.overlays, &mut cx);
        }
        for m in out {
            self.p.deferred.push(Deferred::Send(m));
        }
    }

    fn client_result(&mut self, (e, stays): (Option<ClientEvent>, bool)) {
        if !stays {
            self.p.client = None;
        }
        if let Some(e) = e {
            self.p.deferred.push(Deferred::Client(e));
        }
    }

    fn secs(&self) -> f64 {
        isorun_client::platform::seconds()
    }
}

impl RunnerHooks for Host<'_> {
    fn ui(&mut self, op: UiOp, arg: Option<i32>, _category: Option<CommandCategory>, _section: Option<&str>) {
        match op {
            UiOp::Commands => self.p.deferred.push(Deferred::ActionTab(isorun_input::action_tabs::ActionTabId::Spells).with_last()),
            UiOp::ActionTab => self.p.deferred.push(Deferred::ActionTabStep(arg.unwrap_or(1))),
            UiOp::Dock => {
                if self.p.dock_on && !self.watching {
                    self.p.hud.dock_focus();
                }
            }
            UiOp::Equipment => self.p.deferred.push(Deferred::Equipment),
            UiOp::Interact => self.p.client = Some(ClientMenu::choices("Interact", super::GameScreen::interact_rows(self.ctx), "Interact", false)),
            UiOp::PopupAction => self.overlay_send(|o, cx| {
                o.trigger_popup_action(arg.unwrap_or(0).max(0) as usize, cx);
            }),
            UiOp::System => self.show_system(self.watching),
            // the controls sheet is a page of the settings (front's GameMenus)
            UiOp::Bindings => self.p.menus.open_settings(),
            UiOp::InstantExplore => self.p.deferred.push(Deferred::ExploreAtOnce),
            UiOp::Keyboard | UiOp::Palette | UiOp::ResetView | UiOp::Levelmap | UiOp::ScrollLog | UiOp::ExploreWhileHeld => {}
        }
    }

    fn osk(&mut self, _op: OskOp, _dir: Option<Dir8>) {
        // the pad's on-screen keyboard is not drawn yet: the text fields take the keyboard's keys
    }

    fn menu(&mut self, op: MenuOp) {
        if let Some(f) = convert::menu_as_focus(op) {
            self.overlay_send(|o, cx| o.focus_op(f, cx));
        }
    }

    fn focus(&mut self, op: FocusOp) {
        let f = convert::focus_op(op);
        self.overlay_send(|o, cx| o.focus_op(f, cx));
    }

    fn status(&mut self, text: &str) {
        let t = self.secs();
        self.p.hud.status(text.to_string(), t);
    }

    fn swing(&mut self, x: i32, y: i32) {
        self.p.deferred.push(Deferred::Swing(x, y));
    }

    fn keep_map(&mut self) {
        self.p.map_hold.begin(self.now);
    }
}

/// Commands opens the tab up last: the game screen keeps which (`action_tab_up`).
trait WithLast {
    fn with_last(self) -> Self;
}
impl WithLast for Deferred {
    fn with_last(self) -> Self {
        match self {
            Deferred::ActionTab(_) => Deferred::ActionTabStep(0),
            d => d,
        }
    }
}

impl GameHost for Host<'_> {
    fn input_from(&mut self, device: InputDevice) {
        self.p.last_input = device;
    }

    fn input_acted(&mut self) {
        self.p.pointer_live = false;
    }

    fn text_entry_focused(&self) -> bool {
        self.p.chat_capturing
    }

    fn cell_menu_open(&self) -> bool {
        self.p.cell_menu.is_some()
    }

    fn close_cell_menu(&mut self) {
        self.p.cell_menu = None;
    }

    fn open_cell_menu(&mut self, at: egui::Pos2) -> bool {
        let Some((x, y)) = self.p.view.pick(at) else { return false };
        match isorun_input::cell_menu::CellMenu::open(self.ctx, self.scene, x, y, at) {
            Some(m) => {
                self.p.cell_menu = Some(m);
                true
            }
            None => false,
        }
    }

    fn pick(&mut self, at: egui::Pos2) -> Option<(i32, i32)> {
        if self.p.map_shown {
            return self.p.level_map.pick(self.scene, at);
        }
        self.p.view.pick(at)
    }

    fn hover(&mut self, at: Option<egui::Pos2>, by_touch: bool) {
        self.p.hover = at;
        self.p.hover_by_touch = by_touch;
        if at.is_some() {
            self.p.pointer_live = true;
            self.p.hover_moved = true;
        }
    }

    fn mouse_control(&self) -> bool {
        mouse_control(self.state)
    }

    fn client_overlay(&self) -> bool {
        self.p.client.is_some() || self.p.menus.is_open() || self.p.menus.tuning() || self.p.hud.screen_open().is_some()
    }

    fn open_menu(&self) -> Option<OverlayMenu> {
        if self.p.menus.is_open() {
            return Some(OverlayMenu::System);
        }
        match self.p.client.as_ref().map(|c| &c.kind) {
            Some(super::client::Kind::Actions(..)) => return Some(OverlayMenu::Battle),
            Some(super::client::Kind::System { .. }) => return Some(OverlayMenu::System),
            _ => {}
        }
        if self.p.hud.screen_open() == Some(isorun_hud::screens::ScreenId::Inventory) {
            return Some(OverlayMenu::Equipment);
        }
        None
    }

    fn screen_open(&self) -> bool {
        self.p.hud.screen_open().is_some()
    }

    fn client_overlay_input(&mut self, op: OverlayOp) -> bool {
        if self.p.menus.is_open() {
            // the pad comes to the Isorun menu its own way (game screen `input`); a close from a key or button here
            if matches!(op, OverlayOp::Close) {
                self.p.menus.close();
            }
            return true;
        }
        if let Some(c) = self.p.client.as_mut() {
            let r = c.input(op);
            self.client_result(r);
            return true;
        }
        if self.p.hud.screen_open().is_some() {
            // the screens read their own keys; the pad's Back and the opener put them away
            if matches!(op, OverlayOp::Cancel | OverlayOp::Close) {
                self.p.hud.close_screen();
            }
            return true;
        }
        false
    }

    fn client_overlay_hotkey(&mut self, key: &str) -> bool {
        if let Some(c) = self.p.client.as_mut() {
            let r = c.hotkey(key);
            self.client_result(r);
            return true;
        }
        false
    }

    fn focus_index(&self) -> Option<i64> {
        self.p.overlays.focus_info(self.state).map(|f| f.index as i64)
    }

    fn dock_on(&self) -> bool {
        self.p.dock_on
    }

    fn dock_focused(&self) -> bool {
        self.p.hud.dock.focused()
    }

    fn dock_key(&mut self, key: &KeyLike) -> bool {
        let op = match key.key.as_str() {
            "ArrowUp" => NavOp::Up,
            "ArrowDown" => NavOp::Down,
            "ArrowLeft" => NavOp::Left,
            "ArrowRight" => NavOp::Right,
            "Enter" | " " => NavOp::Select,
            "Escape" => NavOp::Cancel,
            _ => {
                self.p.hud.dock.focus_off();
                return false;
            }
        };
        self.p.deferred.push(Deferred::DockNav(op));
        if op == NavOp::Select {
            self.p.hud.dock.focus_off();
        }
        true
    }

    fn dock_pad(&mut self, ev: &PadEvent) -> bool {
        use isorun_input::Button as B;
        match ev {
            PadEvent::Dir { dir: Some(d), .. } | PadEvent::DirRepeat { dir: d, .. } => {
                let op = match d {
                    0 => NavOp::Up,
                    4 => NavOp::Down,
                    6 => NavOp::Left,
                    2 => NavOp::Right,
                    _ => return true,
                };
                self.p.deferred.push(Deferred::DockNav(op));
                true
            }
            PadEvent::Press { button, .. } => {
                let op = match button {
                    B::DU | B::DD | B::DL | B::DR => return true,
                    B::A => NavOp::Select,
                    B::Y => NavOp::Assign,
                    B::B | B::L3 => NavOp::Cancel,
                    _ => {
                        self.p.hud.dock.focus_off();
                        return false;
                    }
                };
                self.p.deferred.push(Deferred::DockNav(op));
                true
            }
            PadEvent::Release { .. } | PadEvent::Repeat { .. } => true,
            _ => false,
        }
    }

    fn dock_focus_off(&mut self) {
        self.p.hud.dock.focus_off();
    }

    fn dock_button(&mut self, screen: DockScreen) {
        let id = match screen {
            DockScreen::Inventory => DockButtonId::Inventory,
            DockScreen::Character => DockButtonId::Character,
        };
        self.p.deferred.push(Deferred::DockScreen(id));
    }

    fn chat_on(&self) -> bool {
        self.chat
    }

    fn chat_capturing(&self) -> bool {
        self.p.chat_capturing
    }

    fn chat_focus(&mut self) {
        if self.chat {
            self.p.hud.chat.focus();
        }
    }

    fn level_up_choosing(&self) -> bool {
        self.p.overlays.level_up.choosing()
    }

    fn level_map_open(&self) -> bool {
        self.p.map_shown
    }

    fn popup_scrolls(&self) -> bool {
        self.state.ui.last().is_some_and(|p| isorun_overlays::menu_nav::is_scroller(&p.kind))
    }

    fn show_system(&mut self, _spectating: bool) {
        self.p.show_system();
    }

    fn show_palette(&mut self, category: CommandCategory) {
        self.p.client = Some(ClientMenu::palette(category));
    }

    fn action_key(&mut self, key: &str) {
        self.p.deferred.push(Deferred::ActionKey(key.to_string()));
    }

    fn set_aim(&mut self, dir: Option<Dir8>) {
        self.p.aim = dir;
        let aim = dir.map(|dir| IsoAim { dir, intent: aim_intent(self.scene, dir) });
        self.p.view.renderer.set_aim(aim);
    }

    fn pace_travel(&mut self) {
        self.p.deferred.push(Deferred::PaceTravel);
    }

    fn scroll_log(&mut self, by: i32) {
        self.p.hud.classic.scroll_log(by);
    }
}

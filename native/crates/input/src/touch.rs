//! Touch screens and phones (the port of `touch.ts` and the pointer handling of `game.ts`):
//! which window is compact, and [`Gestures`], the recogniser that turns the pointer and the
//! fingers into what the game does with them: a tap or click on a cell, a drag of the view, a
//! long press (the cell menu), two fingers that zoom, turn and tilt, the wheel's zoom and turn,
//! and a finger's hover that puts the cursor on its cell.

const COMPACT_WIDTH: f32 = 900.0;
const COMPACT_HEIGHT: f32 = 560.0;

/// css pixels a touch must travel before a press becomes a drag of the view
pub const DRAG_SLOP: f32 = 6.0;
pub const LONG_PRESS_MS: f64 = 450.0;
/// how long the cursor stays on a tapped cell after the finger lifts
pub const TAP_CURSOR_MS: f64 = 500.0;
/// how much the view zooms per css pixel of wheel scroll
pub const ZOOM_PER_WHEEL_PIXEL: f32 = 0.0015;
/// Wheel scroll with Shift that turns the view a step
pub const WHEEL_TURN_PIXELS: f32 = 60.0;
/// a wheel's line (a notch) in css pixels, as a browser reports one
pub const WHEEL_LINE_PIXELS: f32 = 100.0;
/// two fingers dragged across the view turn the camera this many degrees a css pixel, a half turn over a phone held sideways
pub const TURN_PER_TWO_FINGER_PIXEL: f32 = 0.25;
/// two fingers dragged up or down tilt the camera this many degrees a css pixel
pub const TILT_PER_TWO_FINGER_PIXEL: f32 = 0.2;
/// The walls drop to stubs over every floor cell this near the player on a touch screen (`setCutAwayReach`).
pub const TOUCH_CUT_AWAY_CELLS: i32 = 6;

/// A window under 900 x 560 css px gets the compact dock.
pub fn compact_window(size: egui::Vec2) -> bool {
    size.x < COMPACT_WIDTH || size.y < COMPACT_HEIGHT
}

pub fn landscape_window(size: egui::Vec2) -> bool {
    size.x > size.y
}

/// Whether the device's pointer is coarse (a finger): on the web `(pointer: coarse)`, natively
/// Android; a desktop touch screen says so with its first touch (`Gestures::seen_touch`).
pub fn touch_screen() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().and_then(|w| w.match_media("(pointer: coarse)").ok().flatten()).is_some_and(|m| m.matches())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(target_os = "android")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
}

/// What the pointer and the fingers did, for the game to carry out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// A finger on the view puts the cursor on its cell (Some), or takes it off (None).
    TouchHover(Option<egui::Pos2>),
    /// The mouse moved over the view, with no button held: the hovered cell is picked again.
    MouseHover(egui::Pos2),
    /// The mouse left the view.
    MouseGone,
    /// A press that moved past the slop drags the view: `IsoCamera::drag(dx, dy)`.
    Drag { dx: f32, dy: f32 },
    /// A press and release without moving: `click_cell` on the cell under it, `button` as
    /// `ev.which` numbers it (1 left, 2 middle, 3 right).
    Click { pos: egui::Pos2, button: i32 },
    /// A finger held still LONG_PRESS_MS: the cell menu there, or, where there is none, a
    /// description of the cell (a right click).
    LongPress { pos: egui::Pos2 },
    /// The zoom by a factor (`IsoCamera::zoom_by`).
    Zoom(f32),
    /// A turn by degrees (`IsoCamera::turn`).
    Turn(f32),
    /// A tilt by degrees (`IsoCamera::tilt_by`).
    Tilt(f32),
    /// The two fingers lifted: the turn ends on a whole step (`IsoCamera::snap_turn`), so the
    /// screen-relative directions stay the compass's.
    SnapTurn,
    /// A press began on the view (any pointer): the cell tooltip goes, the pointer is live again.
    Pressed(PointerKind),
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    id: u64,
    x: f32,
    y: f32,
    x0: f32,
    y0: f32,
    moved: bool,
    /// the DOM button: 0 left, 1 middle, 2 right
    button: i32,
}

/// Two fingers on the view: how far apart, where their midpoint is (px) and the angle of the line between them (radians).
#[derive(Clone, Copy, Debug)]
struct TwoFingers {
    span: f32,
    x: f32,
    y: f32,
    angle: f32,
}

const MOUSE_ID: u64 = u64::MAX;

/// The pointer recogniser. Feed it egui's events each frame ([`Gestures::feed`]) and let its
/// timers run ([`Gestures::tick`]); it answers with [`Gesture`]s. Positions are in the
/// window's logical pixels; `view` (set each frame) is the rectangle of the 3D view, outside
/// which a press is not the view's.
#[derive(Debug)]
pub struct Gestures {
    touches: Vec<(u64, egui::Pos2)>,
    drag: Option<Drag>,
    two_fingers: Option<TwoFingers>,
    long_press: Option<(f64, egui::Pos2)>,
    hover_by_touch: bool,
    hover_drop_at: Option<f64>,
    wheel_turn: f32,
    /// egui sends a touch's first finger again as mouse events: those are dropped
    emulated: bool,
    mouse_pos: Option<egui::Pos2>,
    mouse_buttons: u8,
    pub view: egui::Rect,
    /// What takes the pointer over the view (the dock, the touch bar, the buttons, a panel): a
    /// press there belongs to that, through its whole life (moves, the release, the tap), and
    /// never reaches the view's cell picking, hover, long press or pinch. Set each frame
    /// (egui's interactive rects of the last pass).
    pub ui: Vec<egui::Rect>,
    /// the level map is up: drags and pinches do not move the 3D view
    pub level_map: bool,
    /// the cell menu is up: the finger's cursor stays on the cell it is about until it goes
    pub cell_menu_open: bool,
    /// a touch has been seen: the screen is a touch screen, whatever the platform said
    pub seen_touch: bool,
    out: Vec<Gesture>,
}

impl Default for Gestures {
    fn default() -> Self {
        Gestures {
            touches: Vec::new(),
            drag: None,
            two_fingers: None,
            long_press: None,
            hover_by_touch: false,
            hover_drop_at: None,
            wheel_turn: 0.0,
            emulated: false,
            mouse_pos: None,
            mouse_buttons: 0,
            view: egui::Rect::EVERYTHING,
            ui: Vec::new(),
            level_map: false,
            cell_menu_open: false,
            seen_touch: false,
            out: Vec::new(),
        }
    }
}

impl Gestures {
    /// One frame of egui's events. `now` in ms.
    pub fn feed(&mut self, events: &[egui::Event], now: f64) -> Vec<Gesture> {
        for e in events {
            match e {
                egui::Event::Touch { id, phase, pos, .. } => {
                    self.seen_touch = true;
                    self.emulated = true;
                    let id = id.0;
                    match phase {
                        egui::TouchPhase::Start => self.down(id, PointerKind::Touch, *pos, 0, now),
                        egui::TouchPhase::Move => self.moved(id, PointerKind::Touch, *pos, true),
                        egui::TouchPhase::End => self.up(id, *pos, false, now),
                        egui::TouchPhase::Cancel => self.up(id, *pos, true, now),
                    }
                }
                egui::Event::PointerGone => {
                    if self.emulated && self.touches.is_empty() {
                        self.emulated = false;
                    } else if !self.emulated {
                        self.mouse_pos = None;
                        self.out.push(Gesture::MouseGone);
                    }
                }
                _ if self.emulated => {}
                egui::Event::PointerMoved(pos) => {
                    self.mouse_pos = Some(*pos);
                    self.moved(MOUSE_ID, PointerKind::Mouse, *pos, self.mouse_buttons != 0);
                }
                egui::Event::PointerButton { pos, button, pressed, .. } => {
                    let b = match button {
                        egui::PointerButton::Primary => 0,
                        egui::PointerButton::Middle => 1,
                        egui::PointerButton::Secondary => 2,
                        _ => continue,
                    };
                    if *pressed {
                        self.mouse_buttons |= 1 << b;
                        if self.view.contains(*pos) {
                            self.down(MOUSE_ID, PointerKind::Mouse, *pos, b, now);
                        }
                    } else {
                        self.mouse_buttons &= !(1 << b);
                        self.up(MOUSE_ID, *pos, false, now);
                    }
                }
                egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                    if self.mouse_pos.is_some_and(|p| !self.view.contains(p)) {
                        continue;
                    }
                    let scale = match unit {
                        egui::MouseWheelUnit::Line => WHEEL_LINE_PIXELS,
                        egui::MouseWheelUnit::Page => WHEEL_LINE_PIXELS * 10.0,
                        egui::MouseWheelUnit::Point => 1.0,
                    };
                    // the DOM's deltaY is down-positive, egui's up-positive
                    self.wheel(-delta.y * scale, -delta.x * scale, modifiers.shift);
                }
                _ => {}
            }
        }
        self.tick(now);
        std::mem::take(&mut self.out)
    }

    /// The timers: the long press, the tapped cell's cursor going. Called by `feed`; on frames
    /// without events call it and then `take`, or the long press waits for the next event.
    pub fn tick(&mut self, now: f64) {
        if let Some((at, pos)) = self.long_press {
            if now >= at {
                self.long_press = None;
                if let Some(d) = self.drag.as_mut().filter(|d| !d.moved) {
                    d.moved = true;
                    self.out.push(Gesture::LongPress { pos });
                }
            }
        }
        if self.hover_drop_at.is_some_and(|t| now >= t) {
            self.drop_touch_hover();
        }
    }

    /// The gestures found since the last `feed` or `take` (a lone `tick`'s).
    pub fn take(&mut self) -> Vec<Gesture> {
        std::mem::take(&mut self.out)
    }

    /// The wheel zooms the dungeon view; Shift with the wheel turns it. `dy`, `dx` in css pixels, down-positive.
    pub fn wheel(&mut self, dy: f32, dx: f32, shift: bool) {
        if self.level_map {
            return;
        }
        if shift {
            self.wheel_turn += if dy != 0.0 { dy } else { dx };
            if self.wheel_turn.abs() < WHEEL_TURN_PIXELS {
                return;
            }
            self.out.push(Gesture::Turn(self.wheel_turn.signum() * isorun_render::camera::TURN_DEGREES));
            self.wheel_turn = 0.0;
        } else {
            self.out.push(Gesture::Zoom((-dy * ZOOM_PER_WHEEL_PIXEL).exp()));
        }
    }

    pub fn down(&mut self, id: u64, kind: PointerKind, pos: egui::Pos2, button: i32, now: f64) {
        self.out.push(Gesture::Pressed(kind));
        // decided on the press: a gesture that starts on the UI is the UI's
        if self.ui.iter().any(|r| r.contains(pos)) {
            return;
        }
        if kind == PointerKind::Touch {
            if !self.view.contains(pos) {
                return;
            }
            self.touches.retain(|(t, _)| *t != id);
            self.touches.push((id, pos));
        }
        if self.touches.len() == 2 {
            return self.start_pinch();
        }
        if self.drag.is_some() {
            return;
        }
        self.drag = Some(Drag { id, x: pos.x, y: pos.y, x0: pos.x, y0: pos.y, moved: false, button });
        if kind == PointerKind::Touch {
            self.long_press = Some((now + LONG_PRESS_MS, pos));
            self.touch_hover(pos);
        }
    }

    pub fn moved(&mut self, id: u64, kind: PointerKind, pos: egui::Pos2, held: bool) {
        if kind == PointerKind::Mouse {
            self.hover_by_touch = false;
            self.hover_drop_at = None;
            if self.view.contains(pos) {
                self.out.push(Gesture::MouseHover(pos));
            }
        }
        if let Some(t) = self.touches.iter_mut().find(|(t, _)| *t == id) {
            t.1 = pos;
            if self.two_fingers.is_some() {
                return self.pinch();
            }
        }
        let Some(d) = self.drag.filter(|d| d.id == id) else { return };
        // the button came up somewhere the window could not see it: this move, with nothing
        // held, is that release. Drop the drag instead of orbiting a held button that is not held.
        if kind == PointerKind::Mouse && !held {
            return self.cancel_drag();
        }
        let (dx, dy) = (pos.x - d.x, pos.y - d.y);
        let mut d = d;
        d.x = pos.x;
        d.y = pos.y;
        if !d.moved && (pos.x - d.x0).hypot(pos.y - d.y0) < DRAG_SLOP {
            self.drag = Some(d);
            // the cursor follows the finger within the slop, onto the cell its tap would act on
            if kind == PointerKind::Touch {
                self.touch_hover(pos);
            }
            return;
        }
        if !d.moved {
            self.drag = Some(d);
            self.drop_touch_hover();
        }
        d.moved = true;
        self.long_press = None;
        self.drag = Some(d);
        // the drag grabs the map: it follows the pointer until the player moves
        if !self.level_map {
            self.out.push(Gesture::Drag { dx, dy });
        }
    }

    pub fn up(&mut self, id: u64, pos: egui::Pos2, cancelled: bool, now: f64) {
        self.touches.retain(|(t, _)| *t != id);
        if self.touches.len() < 2 && self.two_fingers.is_some() {
            self.two_fingers = None;
            self.out.push(Gesture::SnapTurn);
        }
        let Some(d) = self.drag.filter(|d| d.id == id) else { return };
        self.long_press = None;
        self.drag = None;
        if !d.moved && !cancelled {
            self.out.push(Gesture::Click { pos, button: d.button + 1 });
        }
        // a tap leaves the cursor on its cell a moment, so the player sees where it sent them
        if self.hover_by_touch && !d.moved && !cancelled {
            self.hover_drop_at = Some(now + TAP_CURSOR_MS);
        } else {
            self.drop_touch_hover();
        }
    }

    /// A drag does not survive leaving the window: the button released out there sends no up.
    pub fn cancel_drag(&mut self) {
        if self.drag.take().is_some() {
            self.long_press = None;
            self.drop_touch_hover();
        }
    }

    fn touch_hover(&mut self, pos: egui::Pos2) {
        self.hover_drop_at = None;
        self.hover_by_touch = true;
        self.out.push(Gesture::TouchHover(Some(pos)));
    }

    /// The finger's cursor goes (the cell menu put away calls this too).
    pub fn drop_touch_hover(&mut self) {
        self.hover_drop_at = None;
        // the cursor stays on the cell the menu is about until the menu goes
        if !self.hover_by_touch || self.cell_menu_open {
            return;
        }
        self.hover_by_touch = false;
        self.out.push(Gesture::TouchHover(None));
    }

    fn start_pinch(&mut self) {
        self.long_press = None;
        self.drop_touch_hover();
        if let Some(d) = self.drag.as_mut() {
            d.moved = true;
        }
        self.two_fingers = self.fingers_now();
    }

    /// The two fingers moved: the camera zooms by their spread, turns as they move across or
    /// twist, and tilts as they move up or down (down looks from higher).
    fn pinch(&mut self) {
        let was = self.two_fingers;
        let now = self.fingers_now();
        self.two_fingers = now;
        let (Some(was), Some(now)) = (was, now) else { return };
        if self.level_map {
            return;
        }
        if was.span > 0.0 && now.span > 0.0 {
            self.out.push(Gesture::Zoom(now.span / was.span));
        }
        let tau = std::f32::consts::TAU;
        let twist = ((now.angle - was.angle) % tau + 3.0 * std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
        self.out.push(Gesture::Turn((now.x - was.x) * TURN_PER_TWO_FINGER_PIXEL - twist.to_degrees()));
        self.out.push(Gesture::Tilt((now.y - was.y) * TILT_PER_TWO_FINGER_PIXEL));
    }

    fn fingers_now(&self) -> Option<TwoFingers> {
        let (a, b) = (self.touches.first()?.1, self.touches.get(1)?.1);
        Some(TwoFingers { span: (a.x - b.x).hypot(a.y - b.y), x: (a.x + b.x) / 2.0, y: (a.y + b.y) / 2.0, angle: (b.y - a.y).atan2(b.x - a.x) })
    }
}

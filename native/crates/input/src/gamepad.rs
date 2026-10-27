//! Gamepad polling -> PadEvent stream, in the W3C standard layout (the port of `gamepad.ts`).
//!
//! [`GamepadInput`] holds the logic: what is pressed and for how long, the repeats, the sticks'
//! sectors with their hysteresis, which pad is primary. It reads [`RawPad`]s, a pad's buttons
//! and axes as the browser's Gamepad API gives them, from a [`PadSource`]: [`GilrsSource`]
//! natively (feature `gilrs`), [`WebSource`] on the web, [`ScriptedPad`] for tests and demos.

use isorun_scene::Dir8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Select,
    Start,
    L3,
    R3,
    DU,
    DD,
    DL,
    DR,
    Home,
    /// Hints only, never pressed: the left stick pushed up (the forward attack's prompt)
    LStickUp,
    /// Hint only: the left stick
    LStick,
    /// Hint only: the right stick
    RStick,
    /// Hint only: the d-pad
    DPad,
}

/// The standard layout's buttons by index.
pub const BUTTON_INDEX: [Button; 17] = [
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::LB,
    Button::RB,
    Button::LT,
    Button::RT,
    Button::Select,
    Button::Start,
    Button::L3,
    Button::R3,
    Button::DU,
    Button::DD,
    Button::DL,
    Button::DR,
    Button::Home,
];

impl Button {
    /// The name the TypeScript uses ('A', 'SELECT', 'LSTICK_UP'), for logs and glyph lookups.
    pub fn name(self) -> &'static str {
        match self {
            Button::A => "A",
            Button::B => "B",
            Button::X => "X",
            Button::Y => "Y",
            Button::LB => "LB",
            Button::RB => "RB",
            Button::LT => "LT",
            Button::RT => "RT",
            Button::Select => "SELECT",
            Button::Start => "START",
            Button::L3 => "L3",
            Button::R3 => "R3",
            Button::DU => "DU",
            Button::DD => "DD",
            Button::DL => "DL",
            Button::DR => "DR",
            Button::Home => "HOME",
            Button::LStickUp => "LSTICK_UP",
            Button::LStick => "LSTICK",
            Button::RStick => "RSTICK",
            Button::DPad => "DPAD",
        }
    }

    pub fn from_name(name: &str) -> Option<Button> {
        BUTTON_INDEX
            .iter()
            .chain([Button::LStickUp, Button::LStick, Button::RStick, Button::DPad].iter())
            .copied()
            .find(|b| b.name().eq_ignore_ascii_case(name))
    }
}

/// Where a direction came from: the right stick only ever turns (`right_stick_turns`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirSource {
    Dpad,
    LStick,
    RStick,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadEvent {
    Press {
        button: Button,
        t: f64,
    },
    Release {
        button: Button,
        t: f64,
        held: f64,
    },
    Repeat {
        button: Button,
        n: u32,
    },
    Dir {
        source: DirSource,
        dir: Option<Dir8>,
    },
    DirRepeat {
        source: DirSource,
        dir: Dir8,
        n: u32,
    },
    /// `start`: true on the first frame of a deflection; false on the frames that follow and on the settle to 0,0
    Look {
        dx: f32,
        dy: f32,
        start: bool,
    },
}

/// Whether an event is the player choosing the controller: a button going
/// down, or a stick or the d-pad being pushed. A connection, a release or a
/// centred stick is not, and neither is the continuation of something already
/// counted (a held button's repeats, a held direction's repeats, the frames of
/// a look after its first): a stick that rests off centre must not keep the prompts
/// in pad glyphs against a keyboard that spoke since (game.ts `inputFrom`).
pub fn is_pad_activity(ev: &PadEvent) -> bool {
    matches!(ev, PadEvent::Press { .. } | PadEvent::Dir { dir: Some(_), .. } | PadEvent::Look { start: true, .. })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamepadOptions {
    pub deadzone_l: f32,
    pub enter_l: f32,
    pub leave_l: f32,
    pub deadzone_r: f32,
    /// Right stick starts a look only past this; it keeps looking down to deadzone_r.
    pub enter_r: f32,
    pub repeat_delay: f64,
    pub repeat_interval: f64,
    pub button_repeat_delay: f64,
    pub button_repeat_interval: f64,
}

impl Default for GamepadOptions {
    fn default() -> Self {
        GamepadOptions {
            deadzone_l: 0.25,
            enter_l: 0.55,
            leave_l: 0.4,
            deadzone_r: 0.15,
            enter_r: 0.3,
            repeat_delay: 350.0,
            repeat_interval: 90.0,
            // A held button repeats at the cadence a held key does, since what it repeats is
            // autofight, which the keyboard plays by leaning on Tab: macOS's own defaults, 25 and
            // 6 ticks of a 60Hz clock, are 417ms then every 100ms.
            button_repeat_delay: 420.0,
            button_repeat_interval: 100.0,
        }
    }
}

/// Whose button names the glyphs use. There is no Steam Deck kind: Steam
/// Input hands the game a virtual Xbox 360 pad whatever is in the player's
/// hands, and the Deck's own labels are Xbox's letters anyway.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PadKind {
    Xbox,
    Playstation,
    Nintendo,
    #[default]
    Generic,
}

pub fn detect_kind(id: &str) -> PadKind {
    let s = id.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(w));
    if has(&["xbox", "xinput", "045e", "valve", "steam"]) {
        PadKind::Xbox
    } else if has(&["playstation", "dualshock", "dualsense", "054c", "sony"]) {
        PadKind::Playstation
    } else if has(&["nintendo", "switch", "057e", "pro controller"]) {
        PadKind::Nintendo
    } else {
        PadKind::Generic
    }
}

/// One button of a raw pad, as the Gamepad API gives it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RawButton {
    pub pressed: bool,
    pub value: f32,
}

/// A pad as the browser's Gamepad API gives it: its id, its index, whether the browser mapped
/// it to the standard layout, its buttons and axes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RawPad {
    pub id: String,
    pub index: usize,
    pub standard: bool,
    pub buttons: Vec<RawButton>,
    pub axes: Vec<f32>,
}

impl RawPad {
    /// A standard-mapped pad with nothing pressed and these axes.
    pub fn standard(id: &str, index: usize, axes: Vec<f32>) -> Self {
        RawPad { id: id.into(), index, standard: true, buttons: vec![RawButton::default(); 17], axes }
    }

    pub fn press(&mut self, i: usize, down: bool) {
        if i >= self.buttons.len() {
            self.buttons.resize(i + 1, RawButton::default());
        }
        self.buttons[i] = RawButton { pressed: down, value: if down { 1.0 } else { 0.0 } };
    }
}

/// Where the pads are read from: once a frame, every pad there is (a `None` slot is a gap).
pub trait PadSource {
    fn pads(&mut self) -> Vec<Option<RawPad>>;
}

#[derive(Clone, Copy, Debug, Default)]
struct Repeat {
    next: f64,
    n: u32,
}

#[derive(Debug)]
pub struct GamepadInput {
    pressed: Vec<(Button, f64)>,
    repeat_state: Vec<(Button, Repeat)>,
    dpad_dir: Option<Dir8>,
    stick_dir: Option<Dir8>,
    /// the right stick's four-way sector on Turn: left and right turn, up and down tilt
    rstick_dir: Option<Dir8>,
    dir_repeat: Vec<(DirSource, Repeat)>,
    opts: GamepadOptions,
    last_active: Option<usize>,
    /// Each pad's axes as first seen, keyed by id and index: what "at rest" means for that pad.
    rest: Vec<(String, Vec<f32>)>,
    /// Buttons a key is standing in for, kept apart from the real pad's: `poll` reads a
    /// connected pad's buttons as the whole truth and would release them on the next frame.
    virtual_held: Vec<(Button, f64)>,
    looking: bool,
    events: Vec<PadEvent>,
    pub kind: PadKind,
    pub connected: bool,
    /// The left stick reads only the four straight ways, so a forward push
    /// pulled a little off true never steps diagonally.
    pub four_way: bool,
    /// The right stick's left and right are the left stick's turn, read the
    /// same way (reach, sectors, repeat) and sent as its `RStick` directions;
    /// only a push up or down is a look, and it only tilts (game.ts sets it on
    /// the Right stick setting's Turn, while walking the 3D view).
    pub right_stick_turns: bool,
}

impl Default for GamepadInput {
    fn default() -> Self {
        GamepadInput::new(GamepadOptions::default())
    }
}

/// Angular hysteresis, in sectors (1 = 45°): a held stick changes sector only
/// once it is this far past the boundary. 0.2 is 9°, so a sector is entered
/// from within ±13.5° of its centre and kept up to ±22.5°; on four ways
/// (sectors of 90°) it is 18°, entered within ±27° and kept up to ±45°.
const SECTOR_MARGIN: f32 = 0.2;

/// The 8-way sector of a deflection, angle 0 = up (north), clockwise, or with
/// `four_way` the nearest straight way, its sector 90° wide. With a `margin`,
/// None when the stick sits within that many sectors of a boundary.
fn sector_of(x: f32, y: f32, margin: f32, four_way: bool) -> Option<Dir8> {
    let per = if four_way { 2.0 } else { 1.0 };
    let a = x.atan2(-y) / (per * std::f32::consts::PI / 4.0);
    let q = a.round();
    if (a - q).abs() > 0.5 - margin {
        return None;
    }
    Some(((q * per) as i32).rem_euclid(8) as Dir8)
}

fn dir_from_delta(dx: i32, dy: i32) -> Dir8 {
    isorun_scene::dir_from_delta(dx, dy).unwrap_or(0)
}

impl GamepadInput {
    pub fn new(opts: GamepadOptions) -> Self {
        GamepadInput {
            pressed: Vec::new(),
            repeat_state: Vec::new(),
            dpad_dir: None,
            stick_dir: None,
            rstick_dir: None,
            dir_repeat: Vec::new(),
            opts,
            last_active: None,
            rest: Vec::new(),
            virtual_held: Vec::new(),
            looking: false,
            events: Vec::new(),
            kind: PadKind::Generic,
            connected: false,
            four_way: false,
            right_stick_turns: false,
        }
    }

    /// The events since the last call, in order.
    pub fn take_events(&mut self) -> Vec<PadEvent> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self, e: PadEvent) {
        self.events.push(e);
    }

    /// A pad is plugged in: a screen that sleeps between frames polls it while one is.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Some button is down (or a stick out of its centre): the frames keep coming for its hold or repeat.
    pub fn busy(&self) -> bool {
        !self.pressed.is_empty()
            || !self.virtual_held.is_empty()
            || self.dpad_dir.is_some()
            || self.stick_dir.is_some()
            || self.rstick_dir.is_some()
            || self.looking
    }

    pub fn is_held(&self, b: Button) -> bool {
        self.pressed.iter().any(|(x, _)| *x == b) || self.virtual_held.iter().any(|(x, _)| *x == b)
    }

    /// Press a button as if the pad had. The key's auto-repeat is dropped by the caller, so a held key is one long press.
    pub fn virtual_down(&mut self, b: Button, now: f64) {
        if self.virtual_held.iter().any(|(x, _)| *x == b) {
            return;
        }
        self.virtual_held.push((b, now));
        self.emit(PadEvent::Press { button: b, t: now });
    }

    pub fn virtual_up(&mut self, b: Button, now: f64) {
        let Some(i) = self.virtual_held.iter().position(|(x, _)| *x == b) else { return };
        let (_, t0) = self.virtual_held.remove(i);
        self.emit(PadEvent::Release { button: b, t: now, held: now - t0 });
    }

    /// Let go of every stand-in button (the window lost focus, and its key-up will never come).
    pub fn virtual_release(&mut self, now: f64) {
        for b in self.virtual_held.iter().map(|(b, _)| *b).collect::<Vec<_>>() {
            self.virtual_up(b, now);
        }
    }

    /// Whether the right stick is deflected (camera is being steered).
    pub fn is_looking(&self) -> bool {
        self.looking
    }

    /// Read the source: call every frame.
    pub fn poll_source(&mut self, now: f64, source: &mut dyn PadSource) {
        let pads = source.pads();
        self.poll(now, &pads);
    }

    /// One frame of every pad there is (a `None` is a gap where one was unplugged).
    pub fn poll(&mut self, now: f64, pads: &[Option<RawPad>]) {
        // most recently active pad is primary
        let mut pad: Option<&RawPad> = pads.iter().flatten().find(|p| Some(p.index) == self.last_active);
        if pad.is_none() {
            pad = pads.iter().flatten().next();
        }
        // An axis counts as moved against where that pad's axis rested when first seen, not
        // against 0: an unmapped pad (a Steam Deck's external controller as Chrome sees the raw
        // device) rests its triggers at -1, and would otherwise be "active" every frame and
        // take the primary from the pad actually in use. When several wake on the same frame,
        // as Steam Input's virtual pad and its raw twin do, the standard-mapped one wins.
        let mut woke: Option<&RawPad> = None;
        for p in pads.iter().flatten() {
            let key = format!("{}{}", p.id, p.index);
            let rest = match self.rest.iter().find(|(k, _)| *k == key) {
                Some((_, r)) => r.clone(),
                None => {
                    self.rest.push((key, p.axes.clone()));
                    p.axes.clone()
                }
            };
            let active = p.buttons.iter().any(|b| b.pressed) || p.axes.iter().enumerate().any(|(i, a)| (a - rest.get(i).copied().unwrap_or(0.0)).abs() > 0.5);
            if active && woke.is_none_or(|w| !w.standard && p.standard) {
                woke = Some(p);
            }
        }
        if let Some(w) = woke {
            self.last_active = Some(w.index);
            pad = Some(w);
        }
        self.connected = pad.is_some();
        let Some(pad) = pad else {
            // unplugged, or its link dropped: what it held is let go and its sticks come to
            // rest, or a held button would stay held and a pushed right stick turn the camera for good
            self.read(now, &[], &[]);
            return;
        };
        self.kind = detect_kind(&pad.id);
        let (buttons, axes) = standard_view(pad);
        self.read(now, &buttons, &axes);
    }

    /// One frame of a pad's buttons and sticks, in the standard layout.
    pub fn read(&mut self, now: f64, buttons: &[bool], axes: &[f32]) {
        for (i, &b) in BUTTON_INDEX.iter().enumerate() {
            let down = buttons.get(i).copied().unwrap_or(false);
            let was = self.pressed.iter().position(|(x, _)| *x == b);
            match (down, was) {
                (true, None) => {
                    self.pressed.push((b, now));
                    self.repeat_state.push((b, Repeat { next: now + self.opts.button_repeat_delay, n: 0 }));
                    self.emit(PadEvent::Press { button: b, t: now });
                }
                (false, Some(i)) => {
                    let (_, t0) = self.pressed.remove(i);
                    self.repeat_state.retain(|(x, _)| *x != b);
                    self.emit(PadEvent::Release { button: b, t: now, held: now - t0 });
                }
                (true, Some(_)) => {
                    let interval = self.opts.button_repeat_interval;
                    let fire = match self.repeat_state.iter_mut().find(|(x, _)| *x == b) {
                        Some((_, r)) if now >= r.next => {
                            r.n += 1;
                            r.next = now + interval;
                            Some(r.n)
                        }
                        _ => None,
                    };
                    if let Some(n) = fire {
                        self.emit(PadEvent::Repeat { button: b, n });
                    }
                }
                (false, None) => {}
            }
        }
        // dpad as direction
        let held = |b| self.pressed.iter().any(|(x, _)| *x == b);
        let dx = held(Button::DR) as i32 - held(Button::DL) as i32;
        let dy = held(Button::DD) as i32 - held(Button::DU) as i32;
        let new_dpad = if dx == 0 && dy == 0 { None } else { Some(dir_from_delta(dx, dy)) };
        self.dir_from(DirSource::Dpad, self.dpad_dir, new_dpad, now);
        self.dpad_dir = new_dpad;
        // left stick 8-way (or four-way) with hysteresis
        let ax = |i: usize| axes.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        let stick = self.sector(self.stick_dir, ax(0), ax(1), self.four_way);
        self.dir_from(DirSource::LStick, self.stick_dir, stick, now);
        self.stick_dir = stick;
        // right stick look, with hysteresis like the left stick: a worn stick
        // resting a little off centre must not steer the camera, nor count as the
        // pad speaking every frame
        let (rx, ry) = (ax(2), ax(3));
        let rm = rx.hypot(ry);
        let turn_of = |d: Option<Dir8>| d.filter(|&d| d == 2 || d == 6);
        let r = if self.right_stick_turns { self.sector(self.rstick_dir, rx, ry, true) } else { None };
        self.dir_from(DirSource::RStick, turn_of(self.rstick_dir), turn_of(r), now);
        self.rstick_dir = r;
        let dz = self.opts.deadzone_r;
        let k = ((rm - dz) / (1.0 - dz)).powi(2);
        if self.right_stick_turns {
            if r == Some(0) || r == Some(4) {
                let start = !self.looking;
                self.looking = true;
                self.emit(PadEvent::Look { dx: 0.0, dy: (ry / rm) * k, start });
            } else if self.looking {
                self.looking = false;
                self.emit(PadEvent::Look { dx: 0.0, dy: 0.0, start: false });
            }
        } else if rm > if self.looking { dz } else { self.opts.enter_r } {
            let start = !self.looking;
            self.looking = true;
            self.emit(PadEvent::Look { dx: (rx / rm) * k, dy: (ry / rm) * k, start });
        } else if self.looking {
            self.looking = false;
            self.emit(PadEvent::Look { dx: 0.0, dy: 0.0, start: false });
        }
    }

    /// A stick's next direction from `was`, with hysteresis in reach (enter_l
    /// to start, down to leave_l to keep) and in angle (SECTOR_MARGIN).
    fn sector(&self, was: Option<Dir8>, x: f32, y: f32, four_way: bool) -> Option<Dir8> {
        let mag = x.hypot(y);
        let Some(was) = was else {
            return if mag >= self.opts.enter_l { sector_of(x, y, 0.0, four_way) } else { None };
        };
        if mag < self.opts.leave_l {
            return None;
        }
        // switch sector only when the stick is clearly inside the new one: a
        // forward push that wobbles across the edge must not turn into a
        // diagonal, then back, restarting the repeat delay each time
        match sector_of(x, y, SECTOR_MARGIN, four_way) {
            Some(s) if s != was && mag >= self.opts.enter_l => Some(s),
            _ => Some(was),
        }
    }

    /// A direction source's change, or its repeat while it is held.
    fn dir_from(&mut self, source: DirSource, was: Option<Dir8>, dir: Option<Dir8>, now: f64) {
        if dir != was {
            self.emit(PadEvent::Dir { source, dir });
            self.dir_repeat.retain(|(s, _)| *s != source);
            if dir.is_some() {
                self.dir_repeat.push((source, Repeat { next: now + self.opts.repeat_delay, n: 0 }));
            }
        } else if let Some(d) = dir {
            let interval = self.opts.repeat_interval;
            let fire = match self.dir_repeat.iter_mut().find(|(s, _)| *s == source) {
                Some((_, r)) if now >= r.next => {
                    r.next = now + interval;
                    r.n += 1;
                    Some(r.n)
                }
                _ => None,
            };
            if let Some(n) = fire {
                self.emit(PadEvent::DirRepeat { source, dir: d, n });
            }
        }
    }
}

/// Standard-layout slot (index into BUTTON_INDEX) of each raw button, by raw index; -1 is a gap.
// Linux xpad (a wired Xbox pad, or Steam's virtual one read raw):
// A B X Y LB RB Back Start Guide L3 R3; axes LX LY LT RX RY RT, then the hat
const XPAD_BUTTONS: [i32; 11] = [0, 1, 2, 3, 4, 5, 8, 9, 16, 10, 11];
// Linux HID (an Xbox pad over Bluetooth): A B _ X Y _ LB RB _ _ Back Start Guide L3 R3;
// axes LX LY RX RY RT LT, then the hat
const BT_BUTTONS: [i32; 15] = [0, 1, -1, 2, 3, -1, 4, 5, -1, -1, 8, 9, 16, 10, 11];

/// A pad's buttons (down or not, in BUTTON_INDEX order) and its sticks (LX LY RX RY)
/// in the standard layout. The browser maps the pads it knows; one it does not (an Xbox
/// pad on a Steam Deck, read raw) comes in the order Linux lists its controls, with the
/// triggers on axes resting at -1 and the d-pad as a hat on the two axes after the rest.
pub fn standard_view(pad: &RawPad) -> (Vec<bool>, Vec<f32>) {
    let raw: Vec<bool> = pad.buttons.iter().map(|b| b.pressed || b.value > 0.5).collect();
    let a = &pad.axes;
    let first4 = || a.iter().take(4).copied().collect::<Vec<_>>();
    if pad.standard {
        return (raw, first4());
    }
    let bt = pad.buttons.len() >= BT_BUTTONS.len();
    let xpad = !bt && pad.buttons.len() == XPAD_BUTTONS.len();
    if !bt && !xpad {
        // an unknown layout: its buttons as they come, the d-pad from a hat if it has one
        let mut buttons = raw;
        if buttons.len() < 17 {
            buttons.resize(17, false);
        }
        if a.len() >= 8 {
            hat(&mut buttons, a[6], a[7]);
        }
        return (buttons, first4());
    }
    let mut buttons = vec![false; BUTTON_INDEX.len()];
    let slots: &[i32] = if bt { &BT_BUTTONS } else { &XPAD_BUTTONS };
    for (i, &slot) in slots.iter().enumerate() {
        if slot >= 0 && raw.get(i).copied().unwrap_or(false) {
            buttons[slot as usize] = true;
        }
    }
    let trigger = |v: Option<&f32>| v.is_some_and(|v| *v > 0.2);
    buttons[6] = buttons[6] || trigger(a.get(if bt { 5 } else { 2 }));
    buttons[7] = buttons[7] || trigger(a.get(if bt { 4 } else { 5 }));
    if a.len() >= 8 {
        hat(&mut buttons, a[6], a[7]);
    }
    let g = |i: usize| a.get(i).copied().unwrap_or(0.0);
    let axes = if bt { vec![g(0), g(1), g(2), g(3)] } else { vec![g(0), g(1), g(3), g(4)] };
    (buttons, axes)
}

fn hat(buttons: &mut [bool], hx: f32, hy: f32) {
    buttons[12] = buttons[12] || hy < -0.5;
    buttons[13] = buttons[13] || hy > 0.5;
    buttons[14] = buttons[14] || hx < -0.5;
    buttons[15] = buttons[15] || hx > 0.5;
}

/// Optional keyboard stand-ins for testing controller menus: Shift+F1 is RB, Shift+F2 Select.
/// Native Crawl punctuation stays untouched.
pub fn pad_key(code: &str) -> Option<Button> {
    match code {
        "F1" => Some(Button::RB),
        "F2" => Some(Button::Select),
        _ => None,
    }
}

/// Feed one key to the stand-ins (`installPadKeys`): Shift+F1 / Shift+F2 press RB / Select, their
/// release lets go (Shift may be let go first, so the release goes by the key alone). True when
/// the key was a stand-in's, so it goes no further. A repeat is swallowed: a held key is one long press.
pub fn pad_key_input(pad: &mut GamepadInput, input: &crate::keys::KeyInput, now: f64) -> bool {
    match input {
        crate::keys::KeyInput::Down { key, repeat } => {
            let Some(b) = pad_key(&key.code) else { return false };
            if !key.shift || key.ctrl || key.alt || key.meta {
                return false;
            }
            if !repeat {
                pad.virtual_down(b, now);
            }
            true
        }
        crate::keys::KeyInput::Up { key } => {
            let Some(b) = pad_key(&key.code) else { return false };
            let was = pad.is_held(b);
            pad.virtual_up(b, now);
            was
        }
    }
}

// ---------------------------------------------------------------------------
// sources

/// A pad that plays a script: at each time (ms since it started) the buttons down and the axes.
/// For tests, and for driving the game from a pad where there is none (the harness example).
#[derive(Clone, Debug, Default)]
pub struct ScriptedPad {
    steps: Vec<(f64, Vec<usize>, [f32; 4])>,
    /// the step the pad stands on, and the next one: one step a poll at most, so a slow frame
    /// never swallows a press and its release together
    current: Option<usize>,
    start: Option<f64>,
    pub now: f64,
}

impl ScriptedPad {
    /// Steps as `t:buttons:axes`, `;`-separated: `500:A`, `900:`, `1200::0,-1,0,0`, `1500:LB,RT`.
    /// Buttons by name (A B X Y LB RB LT RT SELECT START L3 R3 DU DD DL DR HOME); axes LX,LY,RX,RY.
    /// Each step holds until the next.
    pub fn parse(script: &str) -> Self {
        let mut steps = Vec::new();
        for part in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let mut f = part.splitn(3, ':');
            let t: f64 = f.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0.0);
            let buttons = f
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|b| Button::from_name(b.trim()))
                .filter_map(|b| BUTTON_INDEX.iter().position(|x| *x == b))
                .collect();
            let mut axes = [0.0; 4];
            if let Some(a) = f.next() {
                for (i, v) in a.split(',').take(4).enumerate() {
                    axes[i] = v.trim().parse().unwrap_or(0.0);
                }
            }
            steps.push((t, buttons, axes));
        }
        ScriptedPad { steps, current: None, start: None, now: 0.0 }
    }

    /// The time the step the pad stands on was scripted for (on the clock `now` runs on): poll
    /// the input at it, so a press of 200 ms is held 200 ms however slowly the frames come.
    pub fn clock(&self) -> Option<f64> {
        Some(self.start? + self.steps.get(self.current?)?.0)
    }

    /// Whether the script has played to its last step.
    pub fn done(&self) -> bool {
        self.start.is_some() && self.current.map_or(self.steps.is_empty(), |c| c + 1 >= self.steps.len())
    }
}

impl PadSource for ScriptedPad {
    fn pads(&mut self) -> Vec<Option<RawPad>> {
        let start = *self.start.get_or_insert(self.now);
        let t = self.now - start;
        let mut pad = RawPad::standard("Scripted Xbox pad (045e)", 0, vec![0.0; 4]);
        let next = self.current.map_or(0, |c| c + 1);
        if self.steps.get(next).is_some_and(|(at, _, _)| *at <= t) {
            self.current = Some(next);
        }
        if let Some((_, buttons, axes)) = self.current.and_then(|c| self.steps.get(c)) {
            for &b in buttons {
                pad.press(b, true);
            }
            pad.axes = axes.to_vec();
        }
        vec![Some(pad)]
    }
}

/// The pads gilrs reads (the desktop, Android), in the standard layout: gilrs maps every pad it
/// knows to the same buttons the Gamepad API's standard mapping has. Its sticks' y is up, the
/// standard's down, so it is turned over.
#[cfg(all(feature = "gilrs", not(target_arch = "wasm32")))]
pub struct GilrsSource {
    gilrs: gilrs::Gilrs,
}

#[cfg(all(feature = "gilrs", not(target_arch = "wasm32")))]
impl GilrsSource {
    pub fn new() -> Option<Self> {
        match gilrs::Gilrs::new() {
            Ok(gilrs) => Some(GilrsSource { gilrs }),
            Err(e) => {
                log::warn!("no gamepads: {e}");
                None
            }
        }
    }
}

#[cfg(all(feature = "gilrs", not(target_arch = "wasm32")))]
impl PadSource for GilrsSource {
    fn pads(&mut self) -> Vec<Option<RawPad>> {
        use gilrs::{Axis, Button as G};
        while self.gilrs.next_event().is_some() {}
        const ORDER: [G; 17] = [
            G::South,
            G::East,
            G::West,
            G::North,
            G::LeftTrigger,
            G::RightTrigger,
            G::LeftTrigger2,
            G::RightTrigger2,
            G::Select,
            G::Start,
            G::LeftThumb,
            G::RightThumb,
            G::DPadUp,
            G::DPadDown,
            G::DPadLeft,
            G::DPadRight,
            G::Mode,
        ];
        self.gilrs
            .gamepads()
            .map(|(id, g)| {
                let buttons = ORDER
                    .iter()
                    .map(|&b| {
                        let value = g.button_data(b).map_or(0.0, |d| d.value());
                        RawButton { pressed: g.is_pressed(b), value }
                    })
                    .collect();
                let axes = vec![g.value(Axis::LeftStickX), -g.value(Axis::LeftStickY), g.value(Axis::RightStickX), -g.value(Axis::RightStickY)];
                Some(RawPad { id: g.name().to_string(), index: usize::from(id), standard: true, buttons, axes })
            })
            .collect()
    }
}

/// The pads the browser's Gamepad API reads, as it gives them.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct WebSource;

#[cfg(target_arch = "wasm32")]
impl PadSource for WebSource {
    fn pads(&mut self) -> Vec<Option<RawPad>> {
        use wasm_bindgen::JsCast;
        let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return Vec::new() };
        let Ok(list) = nav.get_gamepads() else { return Vec::new() };
        list.iter()
            .map(|v| {
                let pad: web_sys::Gamepad = v.dyn_into().ok()?;
                let buttons = pad
                    .buttons()
                    .iter()
                    .filter_map(|b| b.dyn_into::<web_sys::GamepadButton>().ok())
                    .map(|b| RawButton { pressed: b.pressed(), value: b.value() as f32 })
                    .collect();
                let axes = pad.axes().iter().map(|a| a.as_f64().unwrap_or(0.0) as f32).collect();
                Some(RawPad { id: pad.id(), index: pad.index() as usize, standard: pad.mapping() == web_sys::GamepadMappingType::Standard, buttons, axes })
            })
            .collect()
    }
}

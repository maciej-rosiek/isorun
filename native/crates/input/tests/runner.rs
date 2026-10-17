//! The ports of `runner-step.test.ts`, `runner-controls.test.ts`, `runner-door.test.ts`,
//! `runner-cast.test.ts` and `runner-examine.test.ts`.

use isorun_input::bindings::{SeqItem, UiOp};
use isorun_input::context::{Context, Mode, Target};
use isorun_input::runner::*;
use isorun_render::camera::{CameraAngles, IsoCamera};
use isorun_scene::{cell_key, Attitude, Billboard, BillboardKind, CellKind, DoorState, Feature, Scene, SceneCell, UpDown, Visibility};
use isorun_webtiles::{cm, initial_state, GameMessage, GameState, MenuState, MouseMode, Msgch, Pos};
use serde_json::Value;

#[derive(Default)]
struct Rec {
    ui: Vec<UiOp>,
    swings: Vec<(i32, i32)>,
    status: Vec<String>,
}

impl RunnerHooks for Rec {
    fn ui(&mut self, op: UiOp, _arg: Option<i32>, _c: Option<isorun_input::bindings::CommandCategory>, _s: Option<&str>) {
        self.ui.push(op);
    }
    fn swing(&mut self, x: i32, y: i32) {
        self.swings.push((x, y));
    }
    fn status(&mut self, text: &str) {
        self.status.push(text.into());
    }
}

struct H {
    r: Runner,
    st: GameState,
    scene: Scene,
    ctx: Context,
    cam: IsoCamera,
    now: f64,
    watching: bool,
    hooks: Rec,
    sent: Vec<Value>,
}

impl H {
    fn new(facing: i32) -> Self {
        let mut st = initial_state();
        st.input_mode = MouseMode::COMMAND;
        H {
            r: Runner::new(),
            st,
            scene: Scene::default(),
            ctx: Context::default(),
            cam: IsoCamera::new(CameraAngles { rotation: facing as f32 * 45.0, tilt: 0.0, perspective: 0.0 }, 68.0),
            now: 1000.0,
            watching: false,
            hooks: Rec::default(),
            sent: Vec::new(),
        }
    }

    fn run<T>(&mut self, f: impl FnOnce(&mut Runner, &mut Env, &mut Rec) -> T) -> T {
        let mut env = Env { state: &self.st, scene: &self.scene, ctx: &self.ctx, camera: &mut self.cam, now: self.now, watching: self.watching, confirm_dangerous: false };
        let out = f(&mut self.r, &mut env, &mut self.hooks);
        self.sent.extend(self.r.take_outbox());
        out
    }

    /// what went out: a key's text, `^n` for a keycode, `@x,y` for a click
    fn keys(&self) -> Vec<String> {
        self.sent
            .iter()
            .map(|m| match m["msg"].as_str() {
                Some("input") => m["text"].as_str().unwrap_or("").to_string(),
                Some("key") => format!("^{}", m["keycode"]),
                Some("click_cell") => format!("@{},{}", m["x"], m["y"]),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn step(&mut self, rel: u8, opts: StepOpts) {
        self.run(|r, env, h| r.step(rel, opts, env, h));
    }

    fn send(&mut self, msg: Value) {
        self.run(|r, env, _| r.send(msg, env));
    }

    /// the server opens a mode: its cursor (id 0) lands where given; then a frame runs
    fn mode(&mut self, m: i64, cursor: Option<(i32, i32)>) {
        self.st.input_mode = m;
        self.st.cursors = cursor.map(|(x, y)| vec![Some(Pos { x, y })]).unwrap_or_default();
        self.ctx.mode = if m == MouseMode::COMMAND { Mode::Command } else { Mode::Targeting };
        self.run(|r, env, h| r.on_state(env, h));
        self.frame();
    }

    fn frame(&mut self) {
        self.ctx.examining = self.r.examining(self.ctx.mode, self.now);
    }

    /// the server's cursor, with the runner's own walk settled
    fn cursor(&mut self, x: i32, y: i32) {
        self.st.cursors = vec![Some(Pos { x, y })];
        self.now += ORBIT_WINDOW_MS + 1.0;
    }
}

fn floor(x: i32, y: i32) -> SceneCell {
    SceneCell { x, y, kind: CellKind::Floor, visibility: Visibility::Visible, floor_tile: 1, label: Some("floor".into()), ..Default::default() }
}

fn monster(x: i32, y: i32, attitude: Attitude) -> Billboard {
    let mut b = Billboard::new(x, y, 0, BillboardKind::Monster, 1.0);
    b.attitude = Some(attitude);
    b
}

fn last_step(dir: u8) -> Option<LastStep> {
    Some(LastStep { dir, t: 1000.0 })
}

// ---------------------------------------------------------------------------
// runner-step.test.ts

#[test]
fn step_is_ours_claims_a_one_cell_move_in_the_direction_sent() {
    assert!(step_is_ours(last_step(2), 1, 0, 1200.0, &[]));
}

#[test]
fn step_is_ours_does_not_claim_a_move_that_went_another_way() {
    assert!(!step_is_ours(last_step(2), 1, -1, 1200.0, &[]));
    assert!(!step_is_ours(last_step(0), 1, -1, 1200.0, &[]));
}

#[test]
fn step_is_ours_forgets_a_step_once_the_window_has_passed() {
    assert!(!step_is_ours(last_step(2), 1, 0, 1000.0 + OWN_STEP_WINDOW_MS, &[]));
    assert!(step_is_ours(last_step(2), 1, 0, 1000.0 + OWN_STEP_WINDOW_MS - 1.0, &[]));
}

#[test]
fn step_is_ours_claims_several_hops_that_all_went_the_steps_way() {
    assert!(step_is_ours(last_step(2), 2, 0, 1200.0, &[(1, 0), (1, 0)]));
    assert!(!step_is_ours(last_step(2), 2, -1, 1200.0, &[(1, 0), (1, -1)]));
    assert!(!step_is_ours(last_step(2), 2, 0, 1200.0, &[]));
}

#[test]
fn step_is_ours_never_claims_a_jump_or_no_step() {
    assert!(!step_is_ours(last_step(2), 5, -6, 1200.0, &[]));
    assert!(!step_is_ours(None, 1, 0, 1200.0, &[]));
}

#[test]
fn sending_o_forgets_the_last_step() {
    let mut h = H::new(1);
    h.r.last_step = last_step(2);
    h.send(cm::key('o' as i32));
    assert_eq!(h.r.last_step, None);
    assert_eq!(h.sent.len(), 1);
}

#[test]
fn other_keys_keep_the_last_step() {
    let mut h = H::new(1);
    h.r.last_step = last_step(2);
    h.send(cm::key('i' as i32));
    assert!(h.r.last_step.is_some());
}

#[test]
fn tab_only_sends_the_key_a_refused_autofight_swings_nothing() {
    let mut h = H::new(1);
    h.scene.player = (5, 5);
    h.scene.cells.insert(cell_key(6, 5), floor(6, 5));
    h.scene.billboards.push(monster(6, 5, Attitude::Hostile));
    h.send(cm::key(9));
    assert!(h.hooks.swings.is_empty());
    assert_eq!(h.sent.len(), 1);
}

fn lr(watching: bool) -> H {
    let mut h = H::new(1);
    h.scene.player = (5, 5);
    h.watching = watching;
    h
}

#[test]
fn an_input_set_to_strafe_steps_sideways() {
    let mut h = lr(false);
    h.step(2, StepOpts::default());
    h.step(6, StepOpts::default());
    assert_eq!(h.cam.facing(), 1);
    // screen-up is north-east: right is south-east (n), left is north-west (y)
    assert_eq!(h.keys(), vec!["n", "y"]);
    assert_eq!(h.r.last_step.map(|s| s.dir), Some(7));
}

#[test]
fn a_step_back_into_a_friendly_is_a_swap_into_a_hostile_a_swing() {
    let mut h = lr(false);
    // screen-down is south-west of 5,5: 4,6
    h.scene.billboards.push(monster(4, 6, Attitude::Friendly));
    h.step(4, StepOpts::default());
    assert_eq!(h.keys(), vec!["b"]);
    assert!(h.hooks.swings.is_empty());
    h.scene.billboards[0] = monster(4, 6, Attitude::Hostile);
    h.step(4, StepOpts::default());
    assert_eq!(h.hooks.swings, vec![(4, 6)]);
    assert_eq!(h.r.heading, 5);
}

#[test]
fn an_input_set_to_turn_turns_the_view_nothing_is_sent() {
    let mut h = lr(false);
    let turns = StepOpts { turns: true, ..Default::default() };
    h.step(2, turns);
    assert_eq!(h.cam.facing(), 2);
    h.step(6, turns);
    h.step(6, turns);
    assert_eq!(h.cam.facing(), 0);
    assert!(h.sent.is_empty());
}

#[test]
fn only_left_and_right_answer_to_the_setting() {
    let mut h = lr(false);
    for d in [0, 1, 3, 4, 5, 7] {
        h.step(d, StepOpts { turns: true, ..Default::default() });
    }
    assert_eq!(h.cam.facing(), 1);
    assert_eq!(h.sent.len(), 6);
}

#[test]
fn spectating_a_turn_still_turns_and_no_step_is_ours() {
    let mut h = lr(true);
    let turns = StepOpts { turns: true, ..Default::default() };
    h.step(2, turns);
    assert_eq!(h.cam.facing(), 2);
    h.step(0, turns);
    h.step(4, turns);
    assert!(h.sent.is_empty());
    assert_eq!(h.r.last_step, None);
}

#[test]
fn shift_and_ctrl_with_left_right_step_even_on_an_input_that_turns() {
    let mut h = lr(false);
    h.step(2, StepOpts { turns: true, run: true, ..Default::default() });
    h.step(6, StepOpts { turns: true, attack: true, ..Default::default() });
    assert_eq!(h.cam.facing(), 1);
    assert_eq!(h.sent.len(), 2);
}

// ---------------------------------------------------------------------------
// runner-controls.test.ts

fn controls() -> H {
    let mut h = H::new(0);
    h.ctx = Context { hostiles_in_view: 1, ahead: Target::Monster { id: None, hostile: true, label: "ogre".into() }, ..Default::default() };
    h
}

#[test]
fn autofight_sends_tab_even_with_a_hostile_directly_ahead() {
    let mut h = controls();
    h.run(|r, env, hk| r.fight(env, hk));
    assert_eq!(h.sent, vec![cm::key(9)]);
}

#[test]
fn refuses_autofight_outside_command_mode() {
    let mut h = controls();
    h.ctx.mode = Mode::Targeting;
    h.run(|r, env, hk| r.fight(env, hk));
    assert!(h.sent.is_empty());
}

#[test]
fn items_on_stairs_require_a_choice_with_no_key_sent_while_choosing() {
    let mut h = controls();
    h.ctx.under = Target::Feature { feature: Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }, label: "stairs".into(), items: Some("a potion".into()) };
    h.run(|r, env, hk| r.contextual(None, env, hk));
    assert_eq!(h.hooks.ui, vec![UiOp::Interact]);
    assert!(h.sent.is_empty());
    h.run(|r, env, hk| r.contextual(Some(true), env, hk));
    assert_eq!(h.sent.last(), Some(&cm::input(",")));
    h.run(|r, env, hk| r.contextual(Some(false), env, hk));
    assert_eq!(h.sent.last(), Some(&cm::input(">")));
}

#[test]
fn bare_stairs_still_work_with_one_press() {
    let mut h = controls();
    h.ctx.under = Target::Feature { feature: Feature::Stairs { dir: UpDown::Up, branch: false, exit: false }, label: "stairs".into(), items: None };
    h.run(|r, env, hk| r.contextual(None, env, hk));
    assert_eq!(h.sent, vec![cm::input("<")]);
    assert!(h.hooks.ui.is_empty());
}

// ---------------------------------------------------------------------------
// runner-door.test.ts

fn door(heading: u8) -> H {
    let mut h = H::new(0);
    h.ctx = Context { ahead: Target::Feature { feature: Feature::Door { state: DoorState::Open }, label: "open door".into(), items: None }, ..Default::default() };
    h.r.heading = heading;
    h
}

#[test]
fn a_closes_the_door_sending_c_alone_and_the_direction_once_asked() {
    let mut h = door(2);
    h.run(|r, env, hk| r.contextual(None, env, hk));
    assert_eq!(h.keys(), vec!["C"]);
    // "Which direction?": the server enters MOUSE_MODE_TARGET_DIR
    h.st.input_mode = MouseMode::TARGET_DIR;
    h.run(|r, env, hk| r.on_state(env, hk));
    // facing east: the direction is `l`, as a step would be
    assert_eq!(h.keys(), vec!["C", "l"]);
    h.st.input_mode = MouseMode::COMMAND;
    h.run(|r, env, hk| r.on_state(env, hk));
    h.r.on_scene();
    assert_eq!(h.keys(), vec!["C", "l"]);
}

#[test]
fn with_easy_door_the_door_closes_on_c_alone() {
    let mut h = door(0);
    h.run(|r, env, hk| r.contextual(None, env, hk));
    assert_eq!(h.keys(), vec!["C"]);
    h.run(|r, env, hk| r.on_state(env, hk));
    h.r.on_scene();
    // a later prompt is some other command's, not ours
    h.st.input_mode = MouseMode::TARGET_DIR;
    h.run(|r, env, hk| r.on_state(env, hk));
    assert_eq!(h.keys(), vec!["C"]);
}

#[test]
fn the_door_gives_up_quietly_when_nothing_comes_back() {
    let mut h = door(0);
    h.run(|r, env, hk| r.contextual(None, env, hk));
    h.now += 2000.0;
    h.r.tick(h.now, &mut h.hooks);
    h.st.input_mode = MouseMode::TARGET_DIR;
    h.run(|r, env, hk| r.on_state(env, hk));
    assert_eq!(h.keys(), vec!["C"]);
}

// ---------------------------------------------------------------------------
// runner-cast.test.ts

fn cast_a() -> Vec<SeqItem> {
    vec![SeqItem::text("z"), SeqItem::text("a").awaiting()]
}

fn message(h: &mut H, text: &str, channel: i64) {
    h.st.messages.lines.push(GameMessage { text: text.into(), channel: Some(channel), ..Default::default() });
    h.st.rev.messages += 1;
    h.run(|r, env, hk| r.on_state(env, hk));
}

#[test]
fn casting_sends_the_letter_once_the_prompt_is_printed() {
    let mut h = H::new(0);
    h.run(|r, env, hk| r.send_keys(&cast_a(), env, hk));
    assert_eq!(h.keys(), vec!["z"]);
    message(&mut h, "Cast which spell? (? or * to list) ", Msgch::PROMPT);
    assert_eq!(h.keys(), vec!["z", "a"]);
}

#[test]
fn a_prompt_line_already_on_screen_from_before_does_not_count() {
    let mut h = H::new(0);
    h.st.messages.lines.push(GameMessage { text: "Cast which spell? (? or * to list) ".into(), channel: Some(Msgch::PROMPT), ..Default::default() });
    h.run(|r, env, hk| r.send_keys(&cast_a(), env, hk));
    assert_eq!(h.keys(), vec!["z"]);
    message(&mut h, "Cast which spell? (? or * to list) ", Msgch::PROMPT);
    assert_eq!(h.keys(), vec!["z", "a"]);
}

#[test]
fn with_spell_menu_the_list_opens_as_a_menu_and_the_letter_goes_to_it() {
    let mut h = H::new(0);
    h.run(|r, env, hk| r.send_keys(&cast_a(), env, hk));
    h.st.menus.push(MenuState::default());
    h.run(|r, env, hk| r.on_state(env, hk));
    assert_eq!(h.keys(), vec!["z", "a"]);
}

#[test]
fn with_no_spells_the_letter_is_dropped() {
    let mut h = H::new(0);
    h.run(|r, env, hk| r.send_keys(&cast_a(), env, hk));
    message(&mut h, "You don't know any spells.", Msgch::PLAIN);
    assert_eq!(h.keys(), vec!["z"]);
    h.now += 2000.0;
    h.r.tick(h.now, &mut h.hooks);
    message(&mut h, "Cast which spell? (? or * to list) ", Msgch::PROMPT);
    assert_eq!(h.keys(), vec!["z"]);
}

// ---------------------------------------------------------------------------
// runner-examine.test.ts

fn examine_h(facing: i32, ahead: Target) -> H {
    let mut h = H::new(facing);
    h.scene.player = (10, 10);
    h.scene.player_on_level = true;
    h.ctx = Context { ahead, ..Default::default() };
    h
}

fn goblin() -> Target {
    Target::Monster { id: None, hostile: true, label: "goblin".into() }
}

fn delta(k: &str) -> (i32, i32) {
    match k {
        "k" => (0, -1),
        "u" => (1, -1),
        "l" => (1, 0),
        "n" => (1, 1),
        "j" => (0, 1),
        "b" => (-1, 1),
        "h" => (-1, 0),
        "y" => (-1, -1),
        _ => panic!("not a direction: {k}"),
    }
}

fn examine(h: &mut H) {
    h.run(|r, env, _| r.examine(env));
}
fn fire(h: &mut H) {
    h.run(|r, env, _| r.fire(env));
}
fn last(h: &H, n: usize) -> Vec<String> {
    let k = h.keys();
    k[k.len() - n..].to_vec()
}

#[test]
fn lb_sends_x_alone_the_cursor_crawl_opens_on_the_player_is_left_there() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    assert_eq!(h.keys(), vec!["x"]);
    assert_eq!(h.cam.facing(), 1);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    assert_eq!(h.keys(), vec!["x"]);
}

#[test]
fn never_sends_a_direction_or_target_of_its_own_whatever_stands_ahead() {
    for ahead in [Target::none("floor"), Target::Unknown { label: "unexplored".into() }, Target::Item { label: "a dagger".into() }, Target::DoorClosed { label: "closed door".into() }] {
        let mut h = examine_h(4, ahead);
        examine(&mut h);
        h.frame();
        h.mode(MouseMode::TARGET, Some((10, 10)));
        h.frame();
        assert_eq!(h.keys(), vec!["x"]);
    }
}

#[test]
fn in_look_mode_a_describes_the_cell_with_v() {
    let mut h = examine_h(0, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    assert!(h.r.examining(Mode::Targeting, h.now));
    h.ctx.examining = true;
    examine(&mut h);
    assert_eq!(h.keys(), vec!["x", "v"]);
}

#[test]
fn lt_sends_f_to_open_the_aim_and_f_again_to_confirm_it() {
    let mut h = examine_h(0, goblin());
    h.ctx.hostiles_in_view = 1;
    fire(&mut h);
    fire(&mut h);
    h.mode(MouseMode::TARGET_PATH, Some((13, 8)));
    fire(&mut h);
    assert_eq!(h.keys(), vec!["f", "f", "f"]);
    assert!(!h.r.examining(Mode::Targeting, h.now));
}

#[test]
fn fire_holds_the_view_and_releases_it_on_the_first_cursor_step() {
    let mut h = examine_h(1, goblin());
    h.ctx.hostiles_in_view = 1;
    fire(&mut h);
    assert_eq!(h.cam.facing(), 1);
    assert!(!h.r.holding_view(Mode::Command, h.now));
    h.mode(MouseMode::TARGET_PATH, Some((8, 12)));
    assert!(h.r.holding_view(Mode::Targeting, h.now));
    fire(&mut h);
    assert_eq!(h.cam.facing(), 1);
    // facing north-east, u is up the screen
    h.step(0, StepOpts::default());
    assert_eq!(h.cam.facing(), 1);
    assert_eq!(h.keys(), vec!["f", "f", "u"]);
    assert!(!h.r.holding_view(Mode::Targeting, h.now));
    h.step(1, StepOpts::default());
    // the cursor stands two cells south-west of the player, the ring's corner: right turns the
    // walk north up the ring's west side; left walks back down it from where our own step sent it
    h.step(2, StepOpts::default());
    h.step(6, StepOpts::default());
    assert_eq!(h.keys(), vec!["f", "f", "u", "l", "k", "j"]);
}

#[test]
fn the_hold_ends_with_the_aim_and_any_other_command_drops_one() {
    let mut h = examine_h(0, goblin());
    h.ctx.hostiles_in_view = 1;
    fire(&mut h);
    h.mode(MouseMode::TARGET_PATH, Some((13, 8)));
    assert!(h.r.holding_view(Mode::Targeting, h.now));
    assert!(!h.r.holding_view(Mode::More, h.now));
    assert!(h.r.holding_view(Mode::Targeting, h.now));
    h.mode(MouseMode::COMMAND, None);
    assert!(!h.r.holding_view(Mode::Command, h.now));
    assert!(!h.r.holding_view(Mode::Targeting, h.now));
    fire(&mut h);
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    assert!(!h.r.holding_view(Mode::Targeting, h.now));
    let mut e = examine_h(0, goblin());
    fire(&mut e);
    e.now += LOOK_WINDOW_MS + 1.0;
    assert!(!e.r.holding_view(Mode::Command, e.now));
    e.ctx.mode = Mode::Targeting;
    assert!(!e.r.holding_view(Mode::Targeting, e.now));
}

#[test]
fn with_nothing_for_crawl_to_pick_f_still_goes_out_alone() {
    let mut h = examine_h(3, goblin());
    h.ctx.readied_action = Some("a stone".into());
    fire(&mut h);
    assert_eq!(h.keys(), vec!["f"]);
    assert_eq!(h.cam.facing(), 3);
    h.mode(MouseMode::TARGET_PATH, Some((10, 10)));
    assert_eq!(h.keys(), vec!["f"]);
}

#[test]
fn a_typed_z_or_any_key_opens_its_aim_untouched() {
    let mut h = examine_h(2, goblin());
    h.send(cm::input("z"));
    h.send(cm::input("a"));
    h.mode(MouseMode::TARGET, Some((10, 10)));
    assert_eq!(h.keys(), vec!["z", "a"]);
    let mut d = examine_h(2, goblin());
    d.send(cm::input("C"));
    d.mode(MouseMode::TARGET_DIR, None);
    assert_eq!(d.keys(), vec!["C"]);
}

#[test]
fn fire_does_nothing_in_look_mode() {
    let mut h = examine_h(0, Target::none("floor"));
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    fire(&mut h);
    assert_eq!(h.keys(), vec!["x"]);
}

#[test]
fn is_look_mode_from_the_targeting_after_x_until_that_targeting_ends() {
    let mut h = examine_h(0, goblin());
    let now = h.now;
    assert!(!h.r.examining(Mode::Command, now));
    examine(&mut h);
    assert!(!h.r.examining(Mode::Command, now));
    assert!(h.r.examining(Mode::Targeting, now));
    assert!(h.r.examining(Mode::Targeting, now));
    assert!(!h.r.examining(Mode::Command, now));
    assert!(!h.r.examining(Mode::Targeting, now));
}

#[test]
fn look_survives_a_more_and_the_description_popup() {
    let mut h = examine_h(0, goblin());
    let now = h.now;
    examine(&mut h);
    assert!(h.r.examining(Mode::Targeting, now));
    assert!(!h.r.examining(Mode::More, now));
    assert!(h.r.examining(Mode::Targeting, now));
    assert!(!h.r.examining(Mode::Popup, now));
    assert!(h.r.examining(Mode::Targeting, now));
    assert!(!h.r.examining(Mode::Command, now));
    assert!(!h.r.examining(Mode::Targeting, now));
}

#[test]
fn a_look_walks_the_cursor_up_and_down_the_screen() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    assert!(h.ctx.examining);
    h.step(0, StepOpts::default());
    h.step(4, StepOpts::default());
    assert_eq!(last(&h, 2), vec!["u", "b"]);
    for d in [2, 6, 1, 7] {
        h.step(d, StepOpts::default());
    }
    assert_eq!(last(&h, 4), vec!["n", "y", "l", "k"]);
}

#[test]
fn left_and_right_walk_the_cursor_round_the_player() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 8)));
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["j"]);
    h.cursor(12, 8);
    h.step(6, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["h"]);
    for ((x, y), right) in [((12, 9), "j"), ((12, 10), "j"), ((12, 11), "j"), ((12, 12), "h"), ((11, 12), "h"), ((8, 12), "k"), ((8, 10), "k"), ((8, 8), "l"), ((10, 8), "l")] {
        h.cursor(x, y);
        h.step(2, StepOpts::default());
        assert_eq!(last(&h, 1), vec![right], "at {x},{y}");
    }
    // a whole lap of the ring: the cursor never leaves it, either way round
    for side in [2, 6] {
        let mut at = (12, 8);
        h.cursor(at.0, at.1);
        for _ in 0..16 {
            h.st.cursors = vec![Some(Pos { x: at.0, y: at.1 })];
            h.step(side, StepOpts::default());
            let d = delta(&last(&h, 1)[0]);
            at = (at.0 + d.0, at.1 + d.1);
            assert_eq!((at.0 - 10).abs().max((at.1 - 10).abs()), 2);
        }
        assert_eq!(at, (12, 8));
    }
}

#[test]
fn a_held_key_keeps_walking_the_ring_while_the_servers_cursor_trails_it() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 8)));
    let mut at = (12, 8);
    for _ in 0..12 {
        h.step(2, StepOpts::default());
        let d = delta(&last(&h, 1)[0]);
        at = (at.0 + d.0, at.1 + d.1);
        assert_eq!((at.0 - 10).abs().max((at.1 - 10).abs()), 2);
    }
    assert_eq!(at, (8, 8));
}

#[test]
fn takes_the_servers_cursor_as_it_catches_up() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 8)));
    for _ in 0..4 {
        h.step(2, StepOpts::default());
    }
    assert_eq!(last(&h, 4), vec!["j", "j", "j", "j"]);
    h.st.cursors = vec![Some(Pos { x: 12, y: 9 })];
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["h"]);
}

#[test]
fn gives_the_walk_up_when_crawl_never_moves_the_cursor() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 12)));
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["h"]);
    h.now += ORBIT_WINDOW_MS + 1.0;
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["h"]);
}

#[test]
fn drops_the_walk_when_anything_else_moves_the_cursor() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 8)));
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["j"]);
    h.run(|r, env, hk| r.click_cell(8, 8, 1, env, hk));
    h.st.cursors = vec![Some(Pos { x: 8, y: 8 })];
    h.step(2, StepOpts::default());
    assert_eq!(last(&h, 1), vec!["l"]);
}

#[test]
fn a_fire_or_an_attack_down_a_side_is_a_direction_not_a_walk() {
    let mut h = examine_h(1, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((12, 8)));
    h.step(2, StepOpts { run: true, ..Default::default() });
    assert_eq!(last(&h, 1), vec!["N"]);
    h.step(6, StepOpts { attack: true, ..Default::default() });
    assert_eq!(h.sent.last(), Some(&cm::key('Y' as i32 - 64)));
}

#[test]
fn a_look_on_a_compass_facing_is_unchanged() {
    let mut h = examine_h(2, goblin());
    examine(&mut h);
    h.mode(MouseMode::TARGET, Some((10, 10)));
    for d in [0, 4, 2] {
        h.step(d, StepOpts::default());
    }
    assert_eq!(last(&h, 3), vec!["l", "h", "j"]);
}

#[test]
fn a_typed_x_counts_too() {
    let mut h = examine_h(3, goblin());
    h.send(cm::input("x"));
    assert_eq!(h.cam.facing(), 3);
    assert!(h.r.examining(Mode::Targeting, h.now));
}

#[test]
fn forgets_an_x_nothing_followed_within_the_window() {
    let mut h = examine_h(0, goblin());
    examine(&mut h);
    h.now += LOOK_WINDOW_MS + 1.0;
    assert!(!h.r.examining(Mode::Command, h.now));
    assert!(!h.r.examining(Mode::Targeting, h.now));
}

#[test]
fn an_x_sent_outside_command_mode_is_not_a_look() {
    let mut h = examine_h(0, goblin());
    h.ctx.mode = Mode::Targeting;
    h.send(cm::input("x"));
    assert!(!h.r.examining(Mode::Targeting, h.now));
}

// ---------------------------------------------------------------------------
// the confirm-dangerous setting, and the explore tracking

#[test]
fn stairs_need_a_second_press_while_hostiles_are_in_view_with_the_setting_on() {
    let mut h = controls();
    h.ctx.under = Target::Feature { feature: Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }, label: "stairs".into(), items: None };
    let env_confirm = |h: &mut H| {
        let mut env = Env { state: &h.st, scene: &h.scene, ctx: &h.ctx, camera: &mut h.cam, now: h.now, watching: false, confirm_dangerous: true };
        h.r.contextual(None, &mut env, &mut h.hooks);
        h.sent.extend(h.r.take_outbox());
    };
    env_confirm(&mut h);
    assert!(h.sent.is_empty());
    assert_eq!(h.hooks.status, vec!["1 hostile in view: press again to confirm"]);
    env_confirm(&mut h);
    assert_eq!(h.sent, vec![cm::input(">")]);
}

#[test]
fn an_o_is_walking_while_the_mouse_mode_stays_normal() {
    let mut h = H::new(0);
    h.send(cm::input("o"));
    assert!(h.r.explore_open());
    assert!(h.r.exploring(MouseMode::NORMAL, h.now));
    assert!(!h.r.exploring(MouseMode::COMMAND, h.now));
    assert!(!h.r.explore_open());
}

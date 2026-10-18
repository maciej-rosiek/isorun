//! The port of `apps/isorun/test/bindings.test.ts` (less its command catalogue, which is the
//! palette's data, not the bindings').

use isorun_input::bindings::*;
use isorun_input::context::*;
use isorun_input::focus::{FocusInfo, FocusOp};
use isorun_input::gamepad::{Button, DirSource, PadEvent};
use isorun_scene::{DoorState, Feature, UpDown};
use isorun_webtiles::{initial_state, GameMessage, Keys, MenuItem, MenuState, MenuTitle, MouseMode, Phase};

fn ctx() -> Context {
    Context::default()
}

fn press(button: Button) -> PadEvent {
    PadEvent::Press { button, t: 0.0 }
}
fn dir(source: DirSource, d: u8) -> PadEvent {
    PadEvent::Dir { source, dir: Some(d) }
}
fn dir_repeat(source: DirSource, d: u8, n: u32) -> PadEvent {
    PadEvent::DirRepeat { source, dir: d, n }
}
fn step(d: u8, turns: bool, held: bool) -> Option<Action> {
    Some(Action::Step { dir: d, run: false, attack: false, turns, held })
}
fn seq_of(a: Option<&Action>) -> Vec<SeqItem> {
    a.and_then(|a| a.seq()).map(|s| s.to_vec()).unwrap_or_default()
}
fn text(t: &str) -> Vec<SeqItem> {
    vec![SeqItem::text(t)]
}
fn key(k: i32) -> Vec<SeqItem> {
    vec![SeqItem::key(k)]
}
fn tap(a: Option<&Action>) -> Option<&Action> {
    match a {
        Some(Action::Hold { tap, .. }) => Some(tap),
        _ => None,
    }
}
fn hold_of(a: Option<&Action>) -> Option<&Action> {
    match a {
        Some(Action::Hold { hold, .. }) => Some(hold),
        _ => None,
    }
}
fn ui(op: UiOp) -> Action {
    Action::ui(op)
}
fn stairs(dir: UpDown) -> Target {
    Target::Feature { feature: Feature::Stairs { dir, branch: false, exit: false }, label: "stairs".into(), items: None }
}
fn ogre() -> Target {
    Target::Monster { id: Some(1), hostile: true, label: "ogre".into() }
}
fn show(c: &Context) -> Vec<String> {
    prompt_labels(c).iter().map(|l| format!("{} {}{}", l.button.name(), l.label, l.hold.as_ref().map(|h| format!(" / {h}")).unwrap_or_default())).collect()
}
fn menu_of(tag: &str, flags: i64) -> MenuState {
    MenuState { tag: tag.into(), flags, kind: Some("menu".into()), last_hovered: -1, ..Default::default() }
}
fn menu_ctx(menu: MenuState) -> MenuContext {
    MenuContext::of(menu)
}

// ---------------------------------------------------------------------------
// modes come from the server, never from message text

#[test]
fn maps_every_input_mode_the_official_client_knows() {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    for (mode, want) in [(MouseMode::COMMAND, Mode::Command), (MouseMode::MORE, Mode::More), (MouseMode::YESNO, Mode::Yesno), (MouseMode::PROMPT, Mode::Prompt), (MouseMode::TARGET, Mode::Targeting), (MouseMode::TARGET_DIR, Mode::Targeting), (MouseMode::TARGET_PATH, Mode::Targeting), (MouseMode::MACRO, Mode::Macro)] {
        st.input_mode = mode;
        assert_eq!(derive_mode(&st), want, "input mode {mode}");
    }
}

#[test]
fn a_bare_getch_prompt_is_a_prompt_while_its_line_is_the_last_printed() {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.input_mode = MouseMode::NORMAL;
    st.messages.lines.push(GameMessage { text: "You kill the rat!".into(), turn: Some(40), channel: Some(0), ..Default::default() });
    assert_eq!(derive_mode(&st), Mode::Command);
    st.messages.lines.push(GameMessage { text: "This altar belongs to (a) Trog, (b) Okawaru or (c) Yredelemnul, but you can't tell which.\nPress the corresponding letter to learn more about a god, or press enter to convert or escape to cancel.".into(), turn: Some(40), channel: Some(2), ..Default::default() });
    assert_eq!(derive_mode(&st), Mode::Prompt);
    st.input_mode = MouseMode::COMMAND;
    assert_eq!(derive_mode(&st), Mode::Command);
    st.input_mode = MouseMode::NORMAL;
    st.messages.lines.push(GameMessage { text: "Okay, then.".into(), turn: Some(40), channel: Some(0), ..Default::default() });
    assert_eq!(derive_mode(&st), Mode::Command);
}

#[test]
fn a_yesno_is_the_yesno_card_however_crawl_asks_it() {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.input_mode = MouseMode::YESNO;
    st.menus.push(MenuState { tag: "prompt".into(), ..Default::default() });
    assert_eq!(derive_mode(&st), Mode::Yesno);
    st.menus[0].tag = "shop".into();
    assert_eq!(derive_mode(&st), Mode::Yesno);
    st.menus.clear();
    assert_eq!(derive_mode(&st), Mode::Yesno);
}

#[test]
fn in_macro_capture_the_pad_offers_nothing_that_would_send_a_rotated_key() {
    let c = Context { mode: Mode::Macro, ..ctx() };
    for (_, a) in binding_table(&c).iter() {
        assert!(!matches!(a, Action::Step { .. }));
    }
    assert_eq!(resolve(&dir(DirSource::Dpad, 0), &c), None);
}

// ---------------------------------------------------------------------------
// focus modes share one binding set

const FOCUS_MODES: [Mode; 7] = [Mode::Popup, Mode::Newgame, Mode::Crt, Mode::Yesno, Mode::Prompt, Mode::Dialog, Mode::Ended];

#[test]
fn focus_modes_a_selects_b_cancels_bumpers_page_dpad_moves() {
    for mode in FOCUS_MODES {
        let c = Context { mode, ..ctx() };
        let t = binding_table(&c);
        if mode != Mode::Yesno {
            assert_eq!(t.get(Button::A), Some(&Action::focus(FocusOp::Select)), "{mode:?}");
        }
        if mode != Mode::Yesno && mode != Mode::Newgame {
            assert_eq!(t.get(Button::B), Some(&Action::focus(FocusOp::Cancel)), "{mode:?}");
        }
        assert_eq!(t.get(Button::LB), None, "{mode:?}");
        assert_eq!(t.get(Button::Start), None, "{mode:?}");
        if mode != Mode::Dialog && mode != Mode::Yesno {
            let paged = binding_table(&Context { mode, pageable: true, ..ctx() });
            assert_eq!(paged.get(Button::LB), Some(&Action::focus(FocusOp::PagePrev)), "{mode:?}");
            assert_eq!(paged.get(Button::RB), Some(&Action::focus(FocusOp::PageNext)), "{mode:?}");
        }
        assert_eq!(resolve(&dir(DirSource::Dpad, 0), &c), Some(Action::focus(FocusOp::Prev)));
        assert_eq!(resolve(&dir(DirSource::Dpad, 4), &c), Some(Action::focus(FocusOp::Next)));
        assert_eq!(resolve(&dir(DirSource::Dpad, 6), &c), Some(Action::focus(FocusOp::Left)));
        assert_eq!(resolve(&dir(DirSource::Dpad, 2), &c), Some(Action::focus(FocusOp::Right)));
        assert_eq!(resolve(&dir_repeat(DirSource::Dpad, 4, 1), &c), Some(Action::focus(FocusOp::Next)));
        // diagonals mean nothing to a cursor
        assert_eq!(resolve(&dir(DirSource::Dpad, 1), &c), None);
    }
}

#[test]
fn never_sends_a_raw_arrow_key_from_the_tables() {
    for mode in FOCUS_MODES {
        assert!(matches!(resolve(&dir(DirSource::Dpad, 4), &Context { mode, ..ctx() }), Some(Action::Focus { .. })));
    }
}

#[test]
fn labels_a_and_b_from_the_focus_cursor_falling_back_to_close_confirm() {
    let focus = FocusInfo { label: Some("Fighting".into()), cancel_label: None, index: 0, count: 12, ..Default::default() };
    let crt = Context { mode: Mode::Crt, focus: Some(focus), ..ctx() };
    assert_eq!(action_label(&Action::focus(FocusOp::Select), &crt), "Fighting");
    assert_eq!(action_label(&Action::focus(FocusOp::Cancel), &crt), "Cancel");
    assert_eq!(action_label(&Action::focus(FocusOp::Select), &Context { mode: Mode::Popup, ..ctx() }), "Close");
    assert_eq!(action_label(&Action::focus(FocusOp::Cancel), &Context { mode: Mode::Popup, ..ctx() }), "Close");
    assert_eq!(action_label(&Action::focus(FocusOp::Select), &Context { mode: Mode::Prompt, ..ctx() }), "Confirm");
}

#[test]
fn a_yesno_puts_yes_on_a_and_no_on_b_and_always_on_x() {
    let prompt = ParsedPrompt { text: "Really?".into(), options: vec![PromptOption::new("Y", "Yes"), PromptOption::new("N", "No"), PromptOption::new("A", "Always")], yesno: true, cancel: true, default: Some("N".into()), ..Default::default() };
    let c = Context { mode: Mode::Yesno, prompt: Some(prompt), ..ctx() };
    let t = binding_table(&c);
    assert_eq!(t.get(Button::A), Some(&Action::prompt("Y")));
    assert_eq!(t.get(Button::B), Some(&Action::prompt("N")));
    assert_eq!(t.get(Button::X), Some(&Action::prompt("A")));
    assert_eq!(action_label(&t[Button::A], &c), "Yes");
    assert_eq!(action_label(&t[Button::B], &c), "No");
    assert_eq!(binding_table(&Context { mode: Mode::Yesno, ..ctx() }).get(Button::A), Some(&Action::prompt("Y")));
    assert_eq!(resolve(&dir(DirSource::Dpad, 2), &c), Some(Action::focus(FocusOp::Right)));
}

#[test]
fn character_creation_b_is_the_screens_own_step_back() {
    for (back, k) in [(None, 27), (Some(32), 32), (Some(8), 8)] {
        let t = binding_table(&Context { mode: Mode::Newgame, newgame_back: back, ..ctx() });
        assert_eq!(seq_of(t.get(Button::B)), key(k));
        assert_eq!(t.get(Button::B).and_then(|a| a.label()), Some("Back"));
    }
}

#[test]
fn the_skills_screen_is_a_crt_menu_focus_not_the_server_hover_set() {
    let crt = MenuContext::of(MenuState { tag: "skills".into(), kind: Some("crt".into()), ..Default::default() });
    let c = Context { mode: Mode::Menu, menu: Some(crt), crt_tag: Some("skills".into()), ..ctx() };
    assert_eq!(binding_table(&c).get(Button::A), Some(&Action::focus(FocusOp::Select)));
    assert_eq!(resolve(&dir(DirSource::Dpad, 4), &c), Some(Action::focus(FocusOp::Next)));
    let c2 = Context { mode: Mode::Menu, menu: Some(menu_ctx(menu_of("inventory", 0))), ..ctx() };
    assert_eq!(binding_table(&c2).get(Button::A), Some(&Action::menu(MenuOp::Select)));
    assert_eq!(resolve(&dir(DirSource::Dpad, 4), &c2), Some(Action::menu(MenuOp::Next)));
    assert_eq!(resolve(&dir(DirSource::Dpad, 2), &c2), Some(Action::menu(MenuOp::Right)));
    assert_eq!(binding_table(&c2).get(Button::LB), None);
    let c3 = Context { mode: Mode::Menu, menu: Some(MenuContext { sections: true, ..menu_ctx(menu_of("inventory", 0)) }), ..ctx() };
    assert_eq!(binding_table(&c3).get(Button::LB), Some(&Action::menu(MenuOp::SectionPrev)));
    assert_eq!(binding_table(&c3).get(Button::RB), Some(&Action::menu(MenuOp::SectionNext)));
}

#[test]
fn x_examines_the_hovered_row_of_a_menu_that_describes_its_rows() {
    let row = |letter: char| Some(MenuItem { text: format!(" {letter} - x"), hotkeys: Some(vec![letter as i64]), level: Some(2), ..Default::default() });
    let spells = |tag: &str, hovered: i64| {
        let menu = MenuState { tag: tag.into(), kind: Some("menu".into()), items: vec![Some(MenuItem { text: " Spells".into(), level: Some(1), ..Default::default() }), row('a'), row('b')], flags: 0x40002, last_hovered: hovered, ..Default::default() };
        Context { mode: Mode::Menu, menu: Some(MenuContext { hoverable: vec![1, 2], arrows_select: true, ..MenuContext::of(menu) }), ..ctx() }
    };
    let c = spells("spell", 1);
    assert_eq!(binding_table(&c).get(Button::X), Some(&Action::menu(MenuOp::Examine)));
    assert_eq!(action_label(&binding_table(&c)[Button::X], &c), "Examine");
    assert!(show(&c).contains(&"X Examine".to_string()));
    // on a header, or with no hover, the row has nothing to describe
    assert!(!prompt_labels(&spells("spell", 0)).iter().any(|l| l.button == Button::X));
    assert!(!prompt_labels(&spells("spell", -1)).iter().any(|l| l.button == Button::X));
    // the travel and prompt menus answer CMD_MENU_EXAMINE with nothing
    for tag in ["travel", "prompt", "game_menu"] {
        assert!(!prompt_labels(&spells(tag, 1)).iter().any(|l| l.button == Button::X), "{tag}");
    }
    for tag in ["inventory", "pickup", "use_item", "ability", "stash"] {
        assert!(show(&spells(tag, 1)).contains(&"X Examine".to_string()), "{tag}");
    }
}

#[test]
fn newgame_keeps_random_recommended_an_unparsed_prompt_keeps_list_and_help() {
    let ng = binding_table(&Context { mode: Mode::Newgame, ..ctx() });
    assert_eq!(ng.get(Button::X).and_then(|a| a.label()), Some("Random"));
    assert_eq!(ng.get(Button::Y).and_then(|a| a.label()), Some("Recommended"));
    let bare = binding_table(&Context { mode: Mode::Prompt, ..ctx() });
    assert_eq!(bare.get(Button::X).and_then(|a| a.label()), Some("List"));
    let parsed = binding_table(&Context { mode: Mode::Prompt, prompt: Some(ParsedPrompt { options: vec![PromptOption::new("w", "wield")], cancel: true, ..Default::default() }), ..ctx() });
    assert_eq!(parsed.get(Button::X), None);
    assert_eq!(parsed.get(Button::Y), None);
    assert_eq!(parsed.get(Button::A), Some(&Action::focus(FocusOp::Select)));
    assert_eq!(parsed.get(Button::B), Some(&Action::focus(FocusOp::Cancel)));
}

#[test]
fn more_takes_only_the_keys_readkey_more_takes() {
    let t = binding_table(&Context { mode: Mode::More, ..ctx() });
    assert_eq!(seq_of(t.get(Button::A)), key(Keys::SPACE));
    assert_eq!(seq_of(t.get(Button::B)), key(Keys::ESC));
    for b in [Button::X, Button::Y, Button::LT, Button::RT, Button::LB, Button::RB, Button::L3, Button::R3, Button::Select, Button::Start] {
        assert_eq!(t.get(b), None, "{b:?}");
    }
}

#[test]
fn the_more_chip_under_a_reads_as_the_panes_more_row() {
    let c = Context { mode: Mode::More, more_text: Some("--more--".into()), ..ctx() };
    let t = binding_table(&c);
    assert_eq!(action_label(&t[Button::A], &c), "<white>--more--</white>");
    let c2 = Context { mode: Mode::More, more_text: Some("<lightred>Press space</lightred>".into()), ..ctx() };
    assert_eq!(action_label(&t[Button::A], &c2), "<white><lightred>Press space</lightred></white>");
    assert_eq!(action_label(&t[Button::A], &Context { mode: Mode::More, ..ctx() }), "--more--");
}

#[test]
fn a_prompt_answer_read_off_the_log_wears_the_logs_colour() {
    let c = Context { mode: Mode::Prompt, prompt: Some(ParsedPrompt { options: vec![PromptOption { hotkey: "S".into(), label: "(S)trength".into(), held: None, colour: Some(3) }, PromptOption::new("y", "Yes")], cancel: true, ..Default::default() }), ..ctx() };
    assert_eq!(action_label(&Action::prompt("S"), &c), "<cyan>(S)trength</cyan>");
    assert_eq!(action_label(&Action::prompt("y"), &c), "Yes");
    let f = |colour: Option<i64>| Context { mode: Mode::Prompt, focus: Some(FocusInfo { label: Some(if colour.is_some() { "(S)trength" } else { "Dungeon" }.into()), colour, index: 0, count: 3, ..Default::default() }), ..ctx() };
    assert_eq!(action_label(&Action::focus(FocusOp::Select), &f(Some(15))), "<white>(S)trength</white>");
    assert_eq!(action_label(&Action::focus(FocusOp::Select), &f(None)), "Dungeon");
}

#[test]
fn every_non_command_mode_can_reach_its_section_of_the_palette() {
    for (mode, section) in [(Mode::Targeting, CommandCategory::Targeting), (Mode::Levelmap, CommandCategory::Levelmap), (Mode::Menu, CommandCategory::Menu), (Mode::Crt, CommandCategory::Command), (Mode::Prompt, CommandCategory::Command)] {
        let c = Context { mode, menu: (mode == Mode::Menu).then(|| menu_ctx(MenuState::default())), ..ctx() };
        let t = binding_table(&c);
        let pal = t.iter().map(|(_, a)| if let Action::Hold { hold, .. } = a { hold.as_ref() } else { a }).find(|a| matches!(a, Action::Ui { op: UiOp::Palette, .. }));
        assert!(matches!(pal, Some(Action::Ui { category: Some(c), .. }) if *c == section), "{mode:?}");
    }
}

// ---------------------------------------------------------------------------
// direct command controls

#[test]
fn a_tap_or_hold_binding_waits_for_release_then_splits_on_hold_ms() {
    let c = ctx();
    assert_eq!(resolve(&press(Button::LB), &c), None);
    assert_eq!(seq_of(resolve(&PadEvent::Release { button: Button::LB, t: 100.0, held: 100.0 }, &c).as_ref()), text("."));
    assert_eq!(resolve(&PadEvent::Release { button: Button::LB, t: 900.0, held: 900.0 }, &c), None);
    assert_eq!(seq_of(hold_action(Button::LB, &c).as_ref()), text("5"));
}

#[test]
fn a_press_that_acted_at_once_never_becomes_a_tap_when_the_server_changes_mode_under_it() {
    let more = Context { mode: Mode::More, ..ctx() };
    assert_eq!(seq_of(resolve(&press(Button::A), &more).as_ref()), key(32));
    assert!(!arms_tap_or_hold(Button::A, &more));
    assert!(!arms_tap_or_hold(Button::A, &Context { mode: Mode::Menu, ..ctx() }));
    assert!(!arms_tap_or_hold(Button::A, &Context { mode: Mode::Yesno, ..ctx() }));
    let command = Context { under: stairs(UpDown::Up), ..ctx() };
    assert_eq!(contextual_label(&command, false), "Ascend");
    assert!(!arms_tap_or_hold(Button::A, &command));
    assert!(arms_tap_or_hold(Button::LB, &command));
    assert!(!arms_tap_or_hold(Button::B, &command));
    assert!(!arms_tap_or_hold(Button::Y, &command));
    assert_eq!(resolve(&press(Button::Y), &command), Some(ui(UiOp::Equipment)));
    assert_eq!(resolve(&PadEvent::Release { button: Button::Y, t: 100.0, held: 100.0 }, &command), None);
    assert!(!arms_tap_or_hold(Button::X, &command));
}

#[test]
fn a_held_direction_is_a_typewriter_of_single_steps_never_a_run() {
    let c = ctx();
    assert_eq!(resolve(&dir(DirSource::LStick, 0), &c), step(0, false, false));
    let held = resolve(&dir_repeat(DirSource::LStick, 0, 1), &c);
    assert_eq!(held, step(0, false, true));
    assert!(matches!(held, Some(Action::Step { run: false, .. })));
    assert_eq!(resolve(&dir_repeat(DirSource::Dpad, 4, 2), &c), step(4, false, true));
    assert_eq!(resolve(&dir_repeat(DirSource::LStick, 0, 1), &Context { layer: Layer::Macro, ..ctx() }), step(0, false, true));
    // the left stick is a d-pad: its left / right strafe
    assert_eq!(resolve(&dir(DirSource::LStick, 2), &Context { layer: Layer::Info, ..ctx() }), step(2, false, false));
}

#[test]
fn triggers_are_actions_never_modifiers() {
    for layer in [Layer::Micro, Layer::Macro, Layer::Info] {
        let t = binding_table(&Context { layer, ..ctx() });
        assert_eq!(t.get(Button::A), Some(&Action::Contextual { alt: None }));
        assert_eq!(t.get(Button::RB), Some(&Action::Fire));
        assert_eq!(t.get(Button::RT), Some(&Action::Fight));
        assert_eq!(seq_of(tap(t.get(Button::LT))), text("o"));
        assert_eq!(hold_of(t.get(Button::LT)), Some(&ui(UiOp::InstantExplore)));
        assert_eq!(t.get(Button::X), Some(&ui(UiOp::Commands)));
        assert_eq!(t.get(Button::Select), Some(&ui(UiOp::Levelmap)));
        assert_eq!(t.get(Button::Start), Some(&ui(UiOp::System)));
    }
}

#[test]
fn lt_stops_an_autoexplore_that_is_walking() {
    let lt = binding_table(&Context { exploring: true, ..ctx() }).get(Button::LT).cloned();
    assert_eq!(seq_of(lt.as_ref()), key(27));
    assert_eq!(lt.as_ref().and_then(|a| a.label()), Some("Stop exploring"));
}

#[test]
fn lt_on_the_hold_setting_explores_while_held_and_a_tap_explores_at_once() {
    let lt = binding_table(&Context { explore_on_hold: true, ..ctx() }).get(Button::LT).cloned();
    assert_eq!(tap(lt.as_ref()), Some(&ui(UiOp::InstantExplore)));
    assert_eq!(hold_of(lt.as_ref()), Some(&ui(UiOp::ExploreWhileHeld)));
}

#[test]
fn the_dpad_always_moves_even_with_a_trigger_held() {
    for layer in [Layer::Micro, Layer::Macro, Layer::Info] {
        assert_eq!(resolve(&dir(DirSource::Dpad, 1), &Context { layer, ..ctx() }), step(1, false, false));
        assert_eq!(resolve(&dir_repeat(DirSource::Dpad, 4, 1), &Context { layer, ..ctx() }), step(4, false, true));
    }
}

#[test]
fn offers_wait_rest_on_lb_only_while_hurt_with_nothing_in_view() {
    let hurt = Context { injured: true, ..ctx() };
    let b = prompt_labels(&hurt).into_iter().find(|l| l.button == Button::LB).unwrap();
    assert_eq!((b.label.as_str(), b.hold.as_deref(), b.contextual), ("Wait one turn", Some("Rest"), true));
    assert!(!prompt_labels(&ctx()).iter().any(|l| l.button == Button::LB));
    assert!(!prompt_labels(&Context { injured: true, hostiles_in_view: 1, ..ctx() }).iter().any(|l| l.button == Button::LB));
    assert!(!prompt_labels(&Context { injured: true, mode: Mode::Menu, ..ctx() }).iter().any(|l| l.button == Button::LB));
}

#[test]
fn shows_the_readied_action_on_rb_when_there_is_something_to_shoot() {
    let c = Context { readied_action: Some("Stone Arrow".into()), hostiles_in_view: 1, ..ctx() };
    assert!(prompt_labels(&c).iter().any(|l| l.button == Button::RB && l.label == "Stone Arrow"));
    assert_eq!(binding_table(&c).get(Button::RB), Some(&Action::Fire));
    assert!(prompt_labels(&Context { readied_action: Some("Stone Arrow".into()), ahead: ogre(), ..ctx() }).iter().any(|l| l.button == Button::RB));
    assert!(!prompt_labels(&Context { readied_action: Some("Stone Arrow".into()), ..ctx() }).iter().any(|l| l.button == Button::RB));
    assert_eq!(action_label(&Action::Fire, &ctx()), "Fire");
    assert!(!prompt_labels(&ctx()).iter().any(|l| l.button == Button::RB));
}

#[test]
fn reads_the_readied_action_from_the_formatted_quiver_line() {
    assert_eq!(readied_action(Some("<brown>Throw: <lightgreen>23 darts (poison)")), Some("Throw: 23 darts (poison)".into()));
    assert_eq!(readied_action(Some("<darkgrey>Nothing quivered</darkgrey>")), None);
    assert_eq!(readied_action(Some("")), None);
    assert_eq!(readied_action(None), None);
    let c = Context { hostiles_in_view: 1, readied_action: readied_action(Some("<darkgrey>Nothing quivered</darkgrey>")), ..ctx() };
    assert!(!prompt_labels(&c).iter().any(|l| l.button == Button::RB));
}

#[test]
fn a_takes_the_stairs_underfoot_even_with_a_monster_ahead() {
    let c = Context { ahead: ogre(), under: stairs(UpDown::Down), ..ctx() };
    assert_eq!(contextual_label(&c, false), "Descend");
    assert_eq!(action_label(&Action::Fight, &c), "Autofight");
    assert_eq!(action_label(&Action::Fight, &ctx()), "Autofight");
}

#[test]
fn items_on_stairs_use_a_chooser_instead_of_a_hold_binding() {
    let under = Target::Feature { feature: Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }, label: "stairs".into(), items: Some("a +0 mace".into()) };
    let c = Context { under, ..ctx() };
    assert_eq!(contextual_label(&c, false), "Descend");
    assert_eq!(contextual_label(&c, true), "a +0 mace");
    assert_eq!(action_label(&Action::Contextual { alt: None }, &c), "Interact");
    assert_eq!(hold_action(Button::A, &c), None);
    assert_eq!(action_label(&Action::Contextual { alt: Some(false) }, &c), "Descend");
    assert_eq!(action_label(&Action::Contextual { alt: Some(true) }, &c), "a +0 mace");
    assert_eq!(contextual_label(&Context { under: stairs(UpDown::Down), ..ctx() }, true), NO_ACTION);
}

#[test]
fn rt_fires_on_press_and_goes_on_fighting_while_held() {
    let c = ctx();
    assert_eq!(resolve(&press(Button::RT), &c), Some(Action::Fight));
    for n in 1..=20 {
        assert_eq!(resolve(&PadEvent::Repeat { button: Button::RT, n }, &c), Some(Action::Fight));
    }
    assert_eq!(resolve(&PadEvent::Release { button: Button::RT, t: 1000.0, held: 1000.0 }, &c), None);
}

#[test]
fn held_on_the_level_map_the_triggers_keep_zooming_nothing_else_repeats() {
    let map = Context { mode: Mode::Levelmap, map_cursor_home: true, ..ctx() };
    assert_eq!(seq_of(resolve(&PadEvent::Repeat { button: Button::RT, n: 3 }, &map).as_ref()), text("}"));
    assert_eq!(seq_of(resolve(&PadEvent::Repeat { button: Button::LT, n: 3 }, &map).as_ref()), key(123));
    for b in [Button::LB, Button::RB, Button::X, Button::Y, Button::L3, Button::R3] {
        assert_eq!(resolve(&PadEvent::Repeat { button: b, n: 1 }, &map), None, "{b:?}");
    }
}

#[test]
fn a_held_rt_stops_where_the_swing_led_somewhere_else() {
    for c in [Context { mode: Mode::More, ..ctx() }, Context { mode: Mode::Targeting, ..ctx() }, Context { mode: Mode::Targeting, examining: true, ..ctx() }, Context { mode: Mode::Menu, ..ctx() }] {
        assert_eq!(resolve(&PadEvent::Repeat { button: Button::RT, n: 1 }, &c), None);
    }
}

#[test]
fn the_gear_the_level_map_actions_and_explore_have_direct_buttons() {
    let t = binding_table(&ctx());
    assert_eq!(t.get(Button::Y), Some(&ui(UiOp::Equipment)));
    assert_eq!(t.get(Button::Select), Some(&ui(UiOp::Levelmap)));
    assert_eq!(t.get(Button::R3), Some(&Action::Examine));
    assert_eq!(t.get(Button::L3), Some(&ui(UiOp::Dock)));
    assert_eq!(seq_of(tap(t.get(Button::LT))), text("o"));
    assert_eq!(t.get(Button::X), Some(&ui(UiOp::Commands)));
    assert_eq!(seq_of(tap(t.get(Button::LB))), text("."));
    assert_eq!(seq_of(hold_of(t.get(Button::LB))), text("5"));
    assert_eq!(seq_of(t.get(Button::B)), key(Keys::ESC));
}

#[test]
fn the_controls_sheet_describes_cancel_lb_wait_rest_and_r3_examine() {
    let sheet = control_sheet();
    let row = |b| sheet.iter().find(|r| r.button == b).map(|r| (r.tap.clone(), r.hold.clone()));
    assert_eq!(row(Button::B), Some(("Cancel".into(), None)));
    assert_eq!(row(Button::LB), Some(("Wait one turn".into(), Some("Rest".into()))));
    assert_eq!(row(Button::R3), Some(("Examine".into(), None)));
}

#[test]
fn r3_examines_once_on_press_without_repeating_or_acting_on_release() {
    let c = ctx();
    assert_eq!(resolve(&press(Button::R3), &c), Some(Action::Examine));
    assert!(!arms_tap_or_hold(Button::R3, &c));
    assert_eq!(resolve(&PadEvent::Repeat { button: Button::R3, n: 1 }, &c), None);
    assert_eq!(resolve(&PadEvent::Release { button: Button::R3, t: 100.0, held: 100.0 }, &c), None);
}

#[test]
fn a_is_only_ever_contextual_no_bare_step_no_attack() {
    assert_eq!(contextual_label(&ctx(), false), NO_ACTION);
    assert_eq!(contextual_label(&Context { ahead: ogre(), ..ctx() }, false), NO_ACTION);
    assert_eq!(contextual_label(&Context { ahead: Target::Wall { label: "wall".into() }, ..ctx() }, false), NO_ACTION);
    assert_eq!(contextual_label(&Context { ahead: stairs(UpDown::Down), ..ctx() }, false), NO_ACTION);
    assert_eq!(contextual_label(&Context { ahead: Target::DoorClosed { label: "door".into() }, ..ctx() }, false), "Open door");
    let open = Target::Feature { feature: Feature::Door { state: DoorState::Open }, label: "open door".into(), items: None };
    assert_eq!(contextual_label(&Context { ahead: open, ..ctx() }, false), "Close door");
}

#[test]
fn items_are_picked_up_by_a_underfoot_only() {
    assert_eq!(contextual_label(&Context { ahead: Target::Item { label: "dagger".into() }, ..ctx() }, false), NO_ACTION);
    assert_eq!(contextual_label(&Context { ahead: Target::Item { label: "items".into() }, ..ctx() }, false), NO_ACTION);
    assert_eq!(contextual_label(&Context { under: Target::Item { label: "a +0 halberd".into() }, ..ctx() }, false), "a +0 halberd");
}

#[test]
fn start_is_enter_only_where_enter_is_not_a() {
    for mode in [Mode::Menu, Mode::Popup, Mode::Prompt, Mode::Yesno, Mode::Crt, Mode::Newgame, Mode::Dialog, Mode::Targeting, Mode::Levelmap, Mode::More] {
        assert_eq!(binding_table(&Context { mode, ..ctx() }).get(Button::Start), None, "{mode:?}");
    }
    let menu = |any_marked| MenuContext { arrows_select: true, multiselect: true, any_marked, ..menu_ctx(menu_of("pickup", 0x40004)) };
    let start = binding_table(&Context { mode: Mode::Menu, menu: Some(menu(true)), ..ctx() }).get(Button::Start).cloned();
    assert_eq!(seq_of(start.as_ref()), key(Keys::ENTER));
    assert_eq!(start.as_ref().and_then(|a| a.label()), Some("accept"));
    // with nothing marked it is still there (the touch bar keeps its rows), but not the situation's
    let bare = binding_table(&Context { mode: Mode::Menu, menu: Some(menu(false)), ..ctx() }).get(Button::Start).cloned();
    assert_eq!(seq_of(bare.as_ref()), key(Keys::ENTER));
    assert!(matches!(bare, Some(Action::Keys { contextual: false, .. })));
    assert_eq!(seq_of(binding_table(&Context { mode: Mode::Macro, ..ctx() }).get(Button::Start)), key(Keys::ENTER));
    assert_eq!(binding_table(&Context { mode: Mode::Menu, ..ctx() }).get(Button::A), Some(&Action::menu(MenuOp::Select)));
    assert_eq!(binding_table(&Context { mode: Mode::Menu, ..ctx() }).get(Button::RT), None);
}

// ---------------------------------------------------------------------------
// look mode (x): A describes, named for what the cursor rests on

fn look(over: Context) -> Context {
    Context { mode: Mode::Targeting, examining: true, ..over }
}
fn goblin() -> Target {
    Target::Monster { id: None, hostile: true, label: "goblin".into() }
}

#[test]
fn look_a_sends_v_x_travels_y_helps_b_cancels_bumpers_cycle_monsters_triggers_objects() {
    let t = binding_table(&look(Context { monsters_in_view: 1, cursor: Some(goblin()), ..ctx() }));
    assert_eq!(t.get(Button::A), Some(&Action::Examine));
    assert_eq!(seq_of(t.get(Button::X)), text("."));
    assert_eq!(t.get(Button::X).and_then(|a| a.label()), Some("Travel here"));
    assert_eq!(seq_of(t.get(Button::Y)), text("?"));
    assert_eq!(seq_of(t.get(Button::B)), key(Keys::ESC));
    assert_eq!(seq_of(t.get(Button::LB)), text("-"));
    assert_eq!(seq_of(t.get(Button::RB)), text("+"));
    // not `/` and `*`: crawl's Shift and Ctrl prefixes in targeting (use_modifier_prefix_keys)
    assert_eq!(seq_of(t.get(Button::LT)), text(";"));
    assert_eq!(seq_of(t.get(Button::RT)), text("'"));
    assert_eq!(binding_table(&look(ctx())).get(Button::LB), None);
}

#[test]
fn look_the_corner_shows_the_three_keys_the_server_prompts() {
    let feature = Target::Feature { feature: Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }, label: "stone staircase".into(), items: None };
    for (cursor, name) in [(Some(goblin()), "goblin"), (Some(Target::Item { label: "a +0 halberd".into() }), "a +0 halberd"), (Some(feature), "stone staircase"), (Some(Target::none("here")), "here"), (None, "here")] {
        assert_eq!(show(&look(Context { cursor, ..ctx() })), vec![format!("A Examine {name}"), "X Travel here".into(), "Y Help".into()]);
    }
}

#[test]
fn an_aim_is_shaped_like_the_look_a_fires_x_describes() {
    let aim = Context { mode: Mode::Targeting, cursor: Some(goblin()), ..ctx() };
    let t = binding_table(&aim);
    assert_eq!(t.get(Button::A), Some(&Action::Fire));
    assert_eq!(seq_of(t.get(Button::X)), text("v"));
    assert_eq!(show(&aim), vec!["A Fire at goblin", "RB Fire at goblin"]);
    assert_eq!(prompt_labels(&Context { mode: Mode::Targeting, ..ctx() }).iter().map(|l| l.label.clone()).collect::<Vec<_>>(), vec!["Fire", "Fire"]);
}

#[test]
fn y_cycles_the_quiver_inside_the_aim() {
    let aim = Context { mode: Mode::Targeting, readied_action: Some("Throw: 23 darts".into()), aim_quiver: true, ..ctx() };
    let y = binding_table(&aim).get(Button::Y).cloned();
    assert_eq!(seq_of(tap(y.as_ref())), text(")"));
    assert_eq!(seq_of(hold_of(y.as_ref())), text("("));
    assert_eq!(binding_table(&Context { mode: Mode::Targeting, readied_action: Some("Throw: 23 darts".into()), ..ctx() }).get(Button::Y), None);
    assert!(show(&aim).contains(&"Y Next quiver / Previous quiver".to_string()) || show(&aim).iter().any(|s| s.starts_with("Y Next quiver")));
    assert!(!prompt_labels(&Context { mode: Mode::Targeting, ..ctx() }).iter().any(|l| l.button == Button::Y));
    assert_eq!(seq_of(binding_table(&look(Context { readied_action: Some("Throw: 23 darts".into()), ..ctx() })).get(Button::Y)), text("?"));
}

#[test]
fn rb_is_the_same_action_before_and_inside_the_aim() {
    assert_eq!(resolve(&press(Button::RB), &ctx()), Some(Action::Fire));
    assert_eq!(resolve(&press(Button::RB), &Context { mode: Mode::Targeting, ..ctx() }), Some(Action::Fire));
    assert_eq!(resolve(&PadEvent::Repeat { button: Button::RB, n: 1 }, &Context { mode: Mode::Targeting, ..ctx() }), None);
    assert_eq!(seq_of(binding_table(&Context { mode: Mode::Targeting, hostiles_in_view: 2, ..ctx() }).get(Button::LB)), text("+"));
    assert_eq!(binding_table(&Context { mode: Mode::Targeting, hostiles_in_view: 1, ..ctx() }).get(Button::LB), None);
    assert_ne!(binding_table(&look(ctx())).get(Button::RB), Some(&Action::Fire));
}

#[test]
fn the_dpad_still_moves_the_cursor_facing_relative() {
    assert_eq!(resolve(&dir(DirSource::Dpad, 0), &look(ctx())), Some(Action::Cursor { dir: 0 }));
}

// ---------------------------------------------------------------------------
// the shop menu

fn shop_row(letter: char, sign: char, name: &str) -> Option<MenuItem> {
    Some(MenuItem { text: format!("<lightgreen>{letter} {sign} </lightgreen><lightgrey>  30 gold   {name}</lightgrey>"), hotkeys: Some(vec![letter as i64]), ..Default::default() })
}

fn shop_menu(mode: &str, rows: Vec<Option<MenuItem>>, hovered: i64) -> MenuState {
    let mode_text = match mode {
        "buy" => "[<w>!</w>] <w>buy</w>|examine items",
        "examine" => "[<w>!</w>] buy|<w>examine</w> items",
        _ => "",
    };
    MenuState { tag: "shop".into(), flags: 0x0004, title: MenuTitle { text: "Welcome".into(), ..Default::default() }, more: format!("<yellow>You have 100 gold pieces.</yellow>\n[<w>Esc</w>] exit          {mode_text}"), total_items: rows.len() as i64, items: rows, last_hovered: hovered, ..Default::default() }
}

fn shop_ctx(mode: &str, rows: Vec<Option<MenuItem>>, hovered: i64) -> Context {
    let menu = shop_menu(mode, rows, hovered);
    let shop = shop_context(&menu);
    let n = menu.items.len();
    Context { mode: Mode::Menu, menu: Some(MenuContext { hoverable: (0..n).collect(), multiselect: true, shop: Some(shop), ..MenuContext::of(menu) }), ..ctx() }
}

#[test]
fn the_shop_reads_the_mode_and_each_rows_mark_from_what_the_server_printed() {
    let s = shop_context(&shop_menu("buy", vec![shop_row('a', '-', "a potion"), shop_row('b', '+', "a scroll"), shop_row('c', '$', "a wand")], 1));
    assert_eq!(s, ShopContext { can_buy: true, mode: ShopMode::Buy, hovered_marked: true, hovered_listed: false, any_marked: true, any_listed: true, sort_order: None });
    let e = shop_context(&shop_menu("examine", vec![shop_row('a', '-', "a potion")], 0));
    assert_eq!(e.mode, ShopMode::Examine);
    assert!(e.can_buy);
    let v = shop_context(&shop_menu("none", vec![shop_row('a', '$', "a potion")], 0));
    assert!(!v.can_buy && v.mode == ShopMode::Examine && v.hovered_listed);
}

#[test]
fn the_shop_buttons_wear_the_footers_own_words() {
    let c = shop_ctx("buy", vec![shop_row('a', '-', "a potion"), shop_row('b', '+', "a scroll")], 0);
    let t = binding_table(&c);
    assert_eq!(t.get(Button::A), Some(&Action::menu(MenuOp::Select)));
    assert_eq!(action_label(&t[Button::A], &c), "mark item for purchase");
    assert_eq!(t.get(Button::Y), Some(&Action::menu(MenuOp::AltSelect)));
    assert_eq!(action_label(&t[Button::Y], &c), "put item on shopping list");
    assert_eq!(action_label(&t[Button::B], &c), "exit");
    assert_eq!((seq_of(t.get(Button::X)), t.get(Button::X).and_then(|a| a.label())), (text("!"), Some("buy|examine items")));
    assert_eq!((seq_of(t.get(Button::Start)), t.get(Button::Start).and_then(|a| a.label())), (key(Keys::ENTER), Some("buy marked items")));
    assert_eq!((seq_of(t.get(Button::LT)), t.get(Button::LT).and_then(|a| a.label())), (text("$"), Some("List marked")));
    assert_eq!(seq_of(t.get(Button::R3)), text("/"));
    let c2 = shop_ctx("buy", vec![shop_row('a', '-', "a potion"), shop_row('b', '+', "a scroll")], 1);
    assert_eq!(action_label(&binding_table(&c2)[Button::A], &c2), "mark item for purchase");
    assert_eq!(action_label(&binding_table(&c2)[Button::Y], &c2), "put item on shopping list");
    assert_eq!(resolve(&dir(DirSource::Dpad, 4), &c), Some(Action::menu(MenuOp::Next)));
}

#[test]
fn with_nothing_marked_enter_buys_the_shopping_list_or_nothing() {
    let listed = shop_ctx("buy", vec![shop_row('a', '$', "a potion")], 0);
    assert_eq!(binding_table(&listed).get(Button::Start).and_then(|a| a.label()), Some("buy shopping list"));
    assert_eq!(binding_table(&listed).get(Button::LT).and_then(|a| a.label()), Some("Mark listed"));
    let bare = shop_ctx("buy", vec![shop_row('a', '-', "a potion")], 0);
    assert_eq!(binding_table(&bare).get(Button::Start), None);
    assert_eq!(binding_table(&bare).get(Button::LT), None);
}

#[test]
fn r3_names_the_order_the_footer_is_sorting_by() {
    let mut menu = shop_menu("buy", vec![shop_row('a', '-', "a potion")], 0);
    menu.more = "[<w>/</w>] sort (price)   [<w>Enter</w>] buy marked items".into();
    let shop = shop_context(&menu);
    let c = Context { mode: Mode::Menu, menu: Some(MenuContext { hoverable: vec![0], multiselect: true, shop: Some(shop), ..MenuContext::of(menu) }), ..ctx() };
    assert_eq!(binding_table(&c).get(Button::R3).and_then(|a| a.label()), Some("sort (price)"));
    assert_eq!(binding_table(&shop_ctx("buy", vec![shop_row('a', '-', "a potion")], 0)).get(Button::R3).and_then(|a| a.label()), Some("Sort"));
}

#[test]
fn in_examine_mode_a_describes_a_view_only_shop_offers_no_buy() {
    let ex = shop_ctx("examine", vec![shop_row('a', '-', "a potion")], 0);
    assert_eq!(action_label(&binding_table(&ex)[Button::A], &ex), "examine item");
    assert_eq!(binding_table(&ex).get(Button::X).and_then(|a| a.label()), Some("buy|examine items"));
    assert_eq!(binding_table(&ex).get(Button::Start).and_then(|a| a.label()), Some("describe"));
    let view = shop_ctx("none", vec![shop_row('a', '-', "a potion")], 0);
    assert_eq!(binding_table(&view).get(Button::X), None);
    assert_eq!(action_label(&binding_table(&view)[Button::A], &view), "examine item");
}

// ---------------------------------------------------------------------------
// the action bar shows only what the situation created

#[test]
fn an_empty_corridor_prompts_nothing() {
    assert!(show(&ctx()).is_empty());
    assert!(bar_labels(&ctx()).len() > 5);
}

#[test]
fn a_hostile_ahead_shows_the_forward_attack() {
    let c = Context { ahead: goblin(), hostiles_in_view: 1, ..ctx() };
    assert_eq!(show(&c), vec!["LSTICK_UP Attack goblin"]);
    let attack = prompt_labels(&c).into_iter().find(|l| l.button == Button::LStickUp).unwrap();
    assert_eq!(attack.action, Action::step(0));
    assert!(matches!(resolve(&dir(DirSource::LStick, 0), &c), Some(Action::Step { dir: 0, .. })));
}

#[test]
fn never_offers_the_forward_attack_for_friendlies_or_outside_command_mode() {
    assert!(show(&Context { ahead: Target::Monster { id: None, hostile: false, label: "goblin".into() }, ..ctx() }).is_empty());
    for mode in [Mode::Targeting, Mode::Levelmap, Mode::Menu, Mode::Prompt, Mode::More, Mode::Spectating] {
        assert!(!prompt_labels(&Context { mode, ahead: goblin(), hostiles_in_view: 1, ..ctx() }).iter().any(|l| l.button == Button::LStickUp));
    }
}

#[test]
fn a_hostile_in_view_but_not_ahead_leaves_the_corner_to_the_lessons() {
    assert!(show(&Context { hostiles_in_view: 2, ..ctx() }).is_empty());
}

#[test]
fn the_d1_exit_prompts_leave_dungeon() {
    let exit = Target::Feature { feature: Feature::Stairs { dir: UpDown::Up, branch: false, exit: true }, label: "exit from the dungeon".into(), items: None };
    assert_eq!(show(&Context { under: exit, ..ctx() }), vec!["A Leave dungeon"]);
}

#[test]
fn stairs_underfoot_prompt_descend_or_interact_with_items() {
    assert_eq!(show(&Context { under: stairs(UpDown::Down), ..ctx() }), vec!["A Descend"]);
    let with_items = Target::Feature { feature: Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }, label: "stairs".into(), items: Some("a +0 mace".into()) };
    assert_eq!(show(&Context { under: with_items, ..ctx() }), vec!["A Interact"]);
}

#[test]
fn a_shop_is_named_by_its_wares() {
    let shop = Target::Feature { feature: Feature::Shop, label: "weapon shop".into(), items: None };
    assert_eq!(show(&Context { under: shop, ..ctx() }), vec!["A Weapon shop"]);
}

#[test]
fn items_underfoot_prompt_a_items_ahead_do_not() {
    assert!(show(&Context { ahead: Target::Item { label: "scroll of identify".into() }, ..ctx() }).is_empty());
    assert_eq!(show(&Context { under: Target::Item { label: "a +0 halberd".into() }, ..ctx() }), vec!["A a +0 halberd"]);
}

#[test]
fn old_layer_values_do_not_reveal_modifier_controls() {
    assert!(show(&Context { layer: Layer::Macro, ..ctx() }).is_empty());
    assert!(show(&Context { layer: Layer::Info, hostiles_in_view: 1, ..ctx() }).is_empty());
}

#[test]
fn a_menu_of_marks_shows_start_once_something_is_marked_a_shop_its_marks() {
    let menu = MenuContext::of(MenuState { tag: "inv".into(), kind: Some("menu".into()), ..Default::default() });
    assert!(show(&Context { mode: Mode::Menu, menu: Some(menu.clone()), ..ctx() }).is_empty());
    assert_eq!(show(&Context { mode: Mode::Menu, menu: Some(MenuContext { multiselect: true, any_marked: true, ..menu.clone() }), ..ctx() }), vec!["START accept"]);
    let shop = ShopContext { can_buy: true, mode: ShopMode::Buy, hovered_marked: true, hovered_listed: false, any_marked: true, any_listed: false, sort_order: None };
    let with = |shop: ShopContext| Context { mode: Mode::Menu, menu: Some(MenuContext { shop: Some(shop), ..menu.clone() }), ..ctx() };
    assert_eq!(show(&with(shop.clone())), vec!["A mark item for purchase", "X buy|examine items", "Y put item on shopping list", "LT List marked", "START buy marked items"]);
    let bare = ShopContext { hovered_marked: false, any_marked: false, ..shop.clone() };
    assert_eq!(show(&with(bare.clone())), vec!["A mark item for purchase", "X buy|examine items", "Y put item on shopping list"]);
    assert_eq!(show(&with(ShopContext { mode: ShopMode::Examine, ..bare.clone() })), vec!["A examine item", "X buy|examine items", "Y put item on shopping list"]);
    assert_eq!(show(&with(ShopContext { can_buy: false, mode: ShopMode::Examine, ..bare })), vec!["A examine item", "Y put item on shopping list"]);
}

#[test]
fn a_prompt_shows_its_answers_without_a_start_hint() {
    assert_eq!(show(&Context { mode: Mode::Yesno, ..ctx() }), vec!["A Yes"]);
    assert!(show(&Context { mode: Mode::Popup, ..ctx() }).is_empty());
    let focus = FocusInfo { label: Some("Wield".into()), index: 0, count: 3, ..Default::default() };
    assert_eq!(show(&Context { mode: Mode::Popup, focus: Some(focus), ..ctx() }), vec!["A Wield"]);
}

#[test]
fn a_popup_puts_its_first_action_on_x() {
    let c = Context { mode: Mode::Popup, popup_actions: Some(vec![PopupAction { key: "!".into(), label: "Status".into() }, PopupAction { key: "x".into(), label: "Xamine".into() }]), ..ctx() };
    assert_eq!(show(&c), vec!["X Status"]);
    assert_eq!(binding_table(&c).get(Button::X), Some(&Action::ui_arg(UiOp::PopupAction, 0)));
    assert_eq!(binding_table(&Context { mode: Mode::Popup, popup_actions: Some(vec![]), ..ctx() }).get(Button::X), None);
    assert_eq!(binding_table(&Context { mode: Mode::Dialog, ..ctx() }).get(Button::X), None);
}

#[test]
fn a_more_names_itself_on_a() {
    assert_eq!(show(&Context { mode: Mode::More, ..ctx() }), vec!["A --more--"]);
}

// ---------------------------------------------------------------------------
// on a screen the keyboard is a pad (screen_key)

fn pickup_menu(multiselect: bool) -> MenuContext {
    MenuContext { arrows_select: true, multiselect, ..menu_ctx(menu_of("pickup", 0)) }
}

#[test]
fn enter_is_a_escape_is_b_the_arrows_are_the_dpad_on_every_screen_with_a_cursor() {
    for mode in [Mode::Popup, Mode::Newgame, Mode::Crt, Mode::Prompt, Mode::Dialog, Mode::Ended] {
        let c = Context { mode, ..ctx() };
        assert_eq!(screen_key("Enter", &c, false), Some(ScreenKey::Button(Button::A)), "{mode:?}");
        assert_eq!(screen_key("Escape", &c, false), Some(ScreenKey::Button(Button::B)));
        assert_eq!(screen_key(" ", &c, false), Some(ScreenKey::Button(Button::A)));
        assert_eq!(screen_key("ArrowUp", &c, false), Some(ScreenKey::Dir(0)));
        assert_eq!(screen_key("ArrowRight", &c, false), Some(ScreenKey::Dir(2)));
        assert_eq!(screen_key("ArrowDown", &c, false), Some(ScreenKey::Dir(4)));
        assert_eq!(screen_key("ArrowLeft", &c, false), Some(ScreenKey::Dir(6)));
        assert_eq!(screen_key("PageDown", &c, false), Some(ScreenKey::Button(Button::RB)));
        assert_eq!(screen_key("a", &c, false), None);
    }
    let m = Context { mode: Mode::Menu, menu: Some(pickup_menu(false)), ..ctx() };
    assert_eq!(screen_key("Enter", &m, false), Some(ScreenKey::Button(Button::A)));
    assert_eq!(screen_key("ArrowDown", &m, false), Some(ScreenKey::Dir(4)));
    assert_eq!(screen_key("PageDown", &m, false), None);
}

#[test]
fn text_to_read_keeps_its_space_and_its_paging_keys() {
    let c = Context { mode: Mode::Popup, ..ctx() };
    assert_eq!(screen_key(" ", &c, true), None);
    assert_eq!(screen_key("PageDown", &c, true), None);
    assert_eq!(screen_key("Enter", &c, true), Some(ScreenKey::Button(Button::A)));
}

#[test]
fn a_menu_of_marks_enter_takes_what_is_marked_space_marks() {
    let m = Context { mode: Mode::Menu, menu: Some(pickup_menu(true)), ..ctx() };
    assert_eq!(screen_key("Enter", &m, false), Some(ScreenKey::Button(Button::Start)));
    assert_eq!(screen_key(" ", &m, false), Some(ScreenKey::Button(Button::A)));
}

#[test]
fn a_yesno_enter_and_space_are_crawls_own_escape_is_b_no() {
    let c = Context { mode: Mode::Yesno, ..ctx() };
    assert_eq!(screen_key("Enter", &c, false), None);
    assert_eq!(screen_key(" ", &c, false), None);
    assert_eq!(screen_key("Escape", &c, false), Some(ScreenKey::Button(Button::B)));
    assert_eq!(button_action(Button::B, &c), Some(Action::prompt("N")));
}

#[test]
fn an_aim_a_look_the_level_map_and_a_more_keep_their_arrows() {
    for mode in [Mode::Targeting, Mode::Levelmap, Mode::More] {
        let c = Context { mode, ..ctx() };
        assert_eq!(screen_key("Enter", &c, false), Some(ScreenKey::Button(Button::A)), "{mode:?}");
        assert_eq!(screen_key("Escape", &c, false), Some(ScreenKey::Button(Button::B)));
        assert_eq!(screen_key("ArrowUp", &c, false), None);
        assert_eq!(screen_key(" ", &c, false), None);
    }
    assert_eq!(button_action(Button::A, &look(ctx())), Some(Action::Examine));
}

#[test]
fn on_the_map_in_a_text_field_and_in_macro_capture_every_key_is_its_own() {
    for mode in [Mode::Command, Mode::Text, Mode::Macro, Mode::Spectating, Mode::Lobby] {
        for k in ["Enter", "Escape", " ", "ArrowUp"] {
            assert_eq!(screen_key(k, &Context { mode, ..ctx() }, false), None, "{mode:?} {k}");
        }
    }
}

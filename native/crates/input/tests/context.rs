//! The ports of `context-threat.test.ts`, `context-underfoot.test.ts`, `action-tabs.test.ts`
//! (its bindings half), `command-menu.test.ts` (its recorded pickup and the Start menu's
//! command list) and the context's prompt parsing.

use isorun_input::action_tabs::*;
use isorun_input::bindings::{bar_labels, binding_table, contextual_label, prompt_labels, resolve, Action, SeqItem, UiOp, NO_ACTION};
use isorun_input::command_menu::{is_gamepad_command_key, CHARACTER_COMMANDS, REPEAT_COMMAND};
use isorun_input::context::*;
use isorun_input::gamepad::{Button, PadEvent};
use isorun_scene::{cell_key, Attitude, Billboard, BillboardKind, CellKind, Feature, Scene, SceneCell, UpDown, Visibility};
use isorun_webtiles::{initial_state, reduce, GameState, MenuState, MenuTitle, MouseMode, Msgch, Phase};
use serde_json::json;

fn floor(x: i32, y: i32) -> SceneCell {
    SceneCell { x, y, kind: CellKind::Floor, visibility: Visibility::Visible, floor_tile: 1, label: Some("floor".into()), ..Default::default() }
}

fn room() -> Scene {
    let mut scene = Scene { player: (4, 4), player_on_level: true, ..Default::default() };
    for y in 0..9 {
        for x in 0..9 {
            scene.cells.insert(cell_key(x, y), floor(x, y));
        }
    }
    scene
}

fn monster(scene: &mut Scene, x: i32, y: i32, name: &str, scenery: bool) {
    let mut b = Billboard::new(x, y, 0, BillboardKind::Monster, 1.0);
    b.attitude = Some(Attitude::Hostile);
    b.scenery = scenery;
    b.name = Some(name.into());
    b.ref_id = Some(scene.billboards.len() as i64 + 1);
    scene.billboards.push(b);
}

fn playing() -> GameState {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.input_mode = MouseMode::COMMAND;
    reduce(&mut st, &json!({ "msg": "player", "pos": { "x": 4, "y": 4 }, "turn": 100 }));
    st
}

// facing north: the cell ahead is (4, 3)
const HEADING: u8 = 0;

// ---------------------------------------------------------------------------
// context-threat.test.ts: plants are not hostiles worth fighting

#[test]
fn a_plant_ahead_gets_no_attack_prompt_and_counts_for_nothing() {
    let st = playing();
    let mut scene = room();
    monster(&mut scene, 4, 3, "plant", true);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(matches!(&ctx.ahead, Target::Monster { hostile: false, label, .. } if label == "plant"));
    assert_eq!(ctx.hostiles_in_view, 0);
    assert!(!prompt_labels(&ctx).iter().any(|l| l.button == Button::RT || l.button == Button::LStickUp));
}

#[test]
fn a_plant_ahead_with_a_rat_in_view_no_attack_prompt() {
    let st = playing();
    let mut scene = room();
    monster(&mut scene, 4, 3, "plant", true);
    monster(&mut scene, 7, 7, "rat", false);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert_eq!(ctx.hostiles_in_view, 1);
    assert!(!prompt_labels(&ctx).iter().any(|l| l.button == Button::LStickUp));
}

#[test]
fn lost_hp_with_the_room_clear_is_the_rest_prompt() {
    let mut st = playing();
    reduce(&mut st, &json!({ "msg": "player", "hp": 5, "hp_max": 20, "mp": 3, "mp_max": 3 }));
    let mut scene = room();
    let hurt = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(hurt.injured);
    let lb = prompt_labels(&hurt).into_iter().find(|l| l.button == Button::LB).unwrap();
    assert_eq!((lb.label.as_str(), lb.hold.as_deref()), ("Wait one turn", Some("Rest")));
    monster(&mut scene, 7, 7, "rat", false);
    let threatened = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(threatened.injured);
    assert!(!prompt_labels(&threatened).iter().any(|l| l.button == Button::LB));
    reduce(&mut st, &json!({ "msg": "player", "hp": 20 }));
    assert!(!derive_context(&st, &room(), HEADING, Layer::Micro).injured);
}

#[test]
fn a_rat_ahead_is_still_the_attack() {
    let st = playing();
    let mut scene = room();
    monster(&mut scene, 4, 3, "rat", false);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(matches!(&ctx.ahead, Target::Monster { hostile: true, label, .. } if label == "rat"));
    assert_eq!(prompt_labels(&ctx).iter().find(|l| l.button == Button::LStickUp).map(|l| l.label.clone()), Some("Attack rat".into()));
    assert!(!prompt_labels(&ctx).iter().any(|l| l.button == Button::RT));
}

// ---------------------------------------------------------------------------
// context-underfoot.test.ts: standing on items

fn scene_with(feature: Option<Feature>) -> Scene {
    let mut scene = Scene { player: (4, 4), player_on_level: true, ..Default::default() };
    let label = if feature.is_some() { "stairs down" } else { "floor" };
    scene.cells.insert(cell_key(4, 4), SceneCell { feature, label: Some(label.into()), ..floor(4, 4) });
    scene.cells.insert(cell_key(4, 3), floor(4, 3));
    scene
}

fn msgs(st: &mut GameState, lines: &[(&str, i64, i64)]) {
    let messages: Vec<_> = lines.iter().map(|(t, turn, ch)| json!({ "text": t, "turn": turn, "channel": ch })).collect();
    reduce(st, &json!({ "msg": "msgs", "messages": messages }));
}

fn under(st: &GameState, scene: &Scene) -> String {
    contextual_label(&derive_context(st, scene, HEADING, Layer::Micro), false)
}

#[test]
fn shows_nothing_until_the_server_says_items_are_here() {
    assert!(derive_context(&playing(), &scene_with(None), HEADING, Layer::Micro).under.is_none());
}

#[test]
fn offers_the_pile_by_name_once_the_floor_items_line_arrives() {
    let mut st = playing();
    let mut scene = scene_with(None);
    msgs(&mut st, &[("<lightgrey>You see here a +0 halberd.", 100, Msgch::FLOOR_ITEMS)]);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert_eq!(ctx.under, Target::Item { label: "<lightgrey>a +0 halberd".into() });
    assert_eq!(contextual_label(&ctx, false), "<lightgrey>a +0 halberd");
    assert_eq!(prompt_labels(&ctx).iter().find(|l| l.button == Button::A).map(|l| l.label.clone()), Some("<lightgrey>a +0 halberd".into()));
    reduce(&mut st, &json!({ "msg": "player", "pos": { "x": 4, "y": 3 }, "turn": 101 }));
    scene.player = (4, 3);
    assert!(derive_context(&st, &scene, HEADING, Layer::Micro).under.is_none());
}

#[test]
fn withdraws_the_prompt_once_the_item_is_taken() {
    let mut st = playing();
    let scene = scene_with(None);
    msgs(&mut st, &[("<lightgrey>You see here a +0 halberd.", 100, Msgch::FLOOR_ITEMS)]);
    assert_eq!(under(&st, &scene), "<lightgrey>a +0 halberd");
    reduce(&mut st, &json!({ "msg": "player", "turn": 101, "inv": { "3": { "base_type": 0, "quantity": 1, "name": "+0 halberd" } } }));
    msgs(&mut st, &[("<lightgrey>d - a +0 halberd", 100, 0)]);
    assert!(derive_context(&st, &scene, HEADING, Layer::Micro).under.is_none());
    msgs(&mut st, &[("Items here: <darkgrey>) [ ?<lightgrey>.", 101, Msgch::FLOOR_ITEMS)]);
    assert_eq!(under(&st, &scene), "<darkgrey>) [ ?");
    reduce(&mut st, &json!({ "msg": "player", "turn": 102, "inv": { "4": { "base_type": 2, "quantity": 1 } } }));
    assert_eq!(under(&st, &scene), "2 items");
}

#[test]
fn on_stairs_a_offers_a_chooser() {
    let mut st = playing();
    let scene = scene_with(Some(Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }));
    msgs(&mut st, &[("Items here: <darkgrey>) [ ?<lightgrey>.", 100, Msgch::FLOOR_ITEMS)]);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(matches!(&ctx.under, Target::Feature { items: Some(i), .. } if i == "<darkgrey>) [ ?"));
    assert_eq!(contextual_label(&ctx, false), "Descend");
    assert_eq!(contextual_label(&ctx, true), "<darkgrey>) [ ?");
    let a = prompt_labels(&ctx).into_iter().find(|l| l.button == Button::A).unwrap();
    assert_eq!(a.label, "Interact");
    assert_eq!(a.hold, None);
}

#[test]
fn names_a_listed_pile_as_the_server_did_and_takes_items_ahead_without_a_name() {
    let mut st = playing();
    let mut scene = scene_with(None);
    msgs(&mut st, &[("<lightgrey>Things that are here:", 100, Msgch::FLOOR_ITEMS), ("<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>", 100, 0)]);
    assert_eq!(under(&st, &scene), "<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>");
    msgs(&mut st, &[("There are many items here.", 100, Msgch::FLOOR_ITEMS)]);
    assert_eq!(under(&st, &scene), "many items");
    let mut b = Billboard::new(4, 3, 1, BillboardKind::Item, 0.4);
    b.name = Some("dagger".into());
    scene.billboards.push(b);
    reduce(&mut st, &json!({ "msg": "player", "pos": { "x": 4, "y": 4 }, "turn": 101 }));
    msgs(&mut st, &[("There are no items here.", 101, 0)]);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(ctx.under.is_none());
    assert!(matches!(ctx.ahead, Target::Item { .. }));
    assert_eq!(contextual_label(&ctx, false), NO_ACTION);
}

#[test]
fn offers_nothing_for_a_corpse_underfoot_or_ahead() {
    let mut st = playing();
    let mut scene = scene_with(None);
    msgs(&mut st, &[("<lightgrey>You see here <darkgrey>a bat corpse</darkgrey>.", 100, Msgch::FLOOR_ITEMS)]);
    assert!(derive_context(&st, &scene, HEADING, Layer::Micro).under.is_none());
    let mut b = Billboard::new(4, 3, 1, BillboardKind::Item, 0.4);
    b.name = Some("bat corpse".into());
    scene.billboards.push(b);
    let ctx = derive_context(&st, &scene, HEADING, Layer::Micro);
    assert!(ctx.ahead.is_none());
    assert!(!contextual_label(&ctx, false).starts_with("Take"));
    msgs(&mut st, &[("<lightgrey>Things that are here:", 100, Msgch::FLOOR_ITEMS), ("<lightgrey>a +0 mace; <darkgrey>a bat corpse</darkgrey>", 100, 0)]);
    assert_eq!(under(&st, &scene), "<lightgrey>a +0 mace");
}

#[test]
fn is_silent_for_a_stale_line() {
    let mut st = playing();
    let scene = scene_with(None);
    msgs(&mut st, &[("You see here a dagger.", 90, Msgch::FLOOR_ITEMS)]);
    assert!(derive_context(&st, &scene, HEADING, Layer::Micro).under.is_none());
}

// ---------------------------------------------------------------------------
// action-tabs.test.ts

fn menu(tag: &str, title: &str) -> MenuState {
    MenuState { tag: tag.into(), title: MenuTitle { text: title.into(), ..Default::default() }, last_hovered: -1, ..Default::default() }
}

fn cast() -> MenuState {
    menu("spell", " <lightgrey>Your spells (cast)                  Type                      Failure  Level")
}

fn up(m: MenuState) -> Context {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.menus.push(m);
    derive_context(&st, &Scene::default(), 0, Layer::Micro)
}

#[test]
fn xs_actions_are_crawls_cast_list_abilities_evoke_and_quiver_menus() {
    let menus = [cast(), menu("ability", "  Ability - do what?                  Cost                            Failure"), menu("use_item", "Evoke which item?"), menu("actions", "Quiver which action? ([-] to clear)")];
    let tabs: Vec<_> = menus.iter().map(action_tab_of).collect();
    assert_eq!(tabs, vec![Some(ActionTabId::Spells), Some(ActionTabId::Abilities), Some(ActionTabId::Evocables), Some(ActionTabId::Quiver)]);
    assert_eq!(action_tab_of(&menu("spell", "Your spells (describe)")), None);
    assert_eq!(action_tab_of(&menu("use_item", "Drink which item?")), None);
    assert_eq!(action_tab_of(&menu("inventory", "Gear: 3/52 gear slots")), None);
}

#[test]
fn the_tabs_turn_round_both_ways_and_pass_none_by() {
    assert_eq!(ACTION_TABS.iter().map(|t| t.label).collect::<Vec<_>>(), vec!["Spells", "Abilities", "Evocables", "Quiver", "Memorise"]);
    assert_eq!(action_neighbour(ActionTabId::Spells, 1), ActionTabId::Abilities);
    assert_eq!(action_neighbour(ActionTabId::Memorise, 1), ActionTabId::Spells);
    assert_eq!(action_neighbour(ActionTabId::Spells, -1), ActionTabId::Memorise);
}

#[test]
fn the_tabs_are_on_the_bumpers_and_the_two_that_are_no_list_on_y_and_lt() {
    for m in [cast(), menu("ability", "  Ability - do what?"), menu("use_item", "Evoke which item?"), menu("actions", "Quiver which action?")] {
        let t = binding_table(&up(m));
        assert_eq!(t.get(Button::LB), Some(&Action::ui_arg(UiOp::ActionTab, -1)));
        assert_eq!(t.get(Button::RB), Some(&Action::ui_arg(UiOp::ActionTab, 1)));
        assert_eq!(t.get(Button::Y).and_then(|a| a.seq()), Some(&[SeqItem::key(27), SeqItem::text("'")][..]));
        assert_eq!(t.get(Button::LT).and_then(|a| a.seq()), Some(&[SeqItem::key(27), SeqItem::text("t")][..]));
    }
    let bar: Vec<(Button, String)> = bar_labels(&up(cast())).into_iter().map(|l| (l.button, l.label)).collect();
    for (b, l) in [(Button::LB, "Memorise"), (Button::RB, "Abilities"), (Button::X, "Examine"), (Button::Y, "Swap weapons"), (Button::LT, "Shout")] {
        assert!(bar.contains(&(b, l.to_string())), "{b:?} {l}");
    }
    let prompts: Vec<String> = prompt_labels(&up(cast())).iter().map(|l| format!("{} {}", l.button.name(), l.label)).collect();
    assert!(prompts.contains(&"Y Swap weapons".to_string()) && prompts.contains(&"LT Shout".to_string()));
}

// ---------------------------------------------------------------------------
// command-menu.test.ts (the parts that are not the overlays')

#[test]
fn gives_the_start_menu_its_own_commands_with_none_of_the_pads_own() {
    let keys: std::collections::HashSet<_> = CHARACTER_COMMANDS.iter().map(|c| c.key).collect();
    assert_eq!(keys.len(), CHARACTER_COMMANDS.len());
    for c in &CHARACTER_COMMANDS {
        assert!(!is_gamepad_command_key(c.key), "{}", c.key);
    }
    assert_eq!(REPEAT_COMMAND.key, "`");
    assert!(!CHARACTER_COMMANDS.iter().any(|c| c.key == "`" || c.key == "S"));
}

#[test]
fn recorded_pickup_a_marks_and_once_one_is_marked_start_sends_enter() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/menus/menu-pickup-1.json")).unwrap();
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.input_mode = MouseMode::COMMAND;
    for m in fixture["msgs"].as_array().unwrap() {
        let kind = m["msg"].as_str().unwrap_or("");
        if kind == "close_menu" || kind == "close_all_menus" {
            break;
        }
        reduce(&mut st, m);
    }
    let ctx = derive_context(&st, &Scene::default(), 0, Layer::Micro);
    assert_eq!(resolve(&PadEvent::Press { button: Button::A, t: 0.0 }, &ctx), Some(Action::menu(isorun_input::bindings::MenuOp::Select)));
    // nothing marked yet: Enter would take nothing, and Start is not offered
    assert_eq!(resolve(&PadEvent::Press { button: Button::Start, t: 1.0 }, &ctx), None);
    // the server marks the row (menu.cc MenuEntry::get_text: `a + ...`), and Start takes it
    let menu = st.menus.last_mut().unwrap();
    let row = menu.items.iter().position(|it| it.as_ref().is_some_and(|it| it.hotkeys.as_ref().is_some_and(|h| !h.is_empty()))).unwrap();
    let item = menu.items[row].as_mut().unwrap();
    item.text = regex::Regex::new(r"^(\s*\S) - ").unwrap().replace(&item.text, "$1 + ").to_string();
    let marked = derive_context(&st, &Scene::default(), 0, Layer::Micro);
    let start = resolve(&PadEvent::Press { button: Button::Start, t: 2.0 }, &marked);
    assert_eq!(start.as_ref().and_then(|a| a.seq()), Some(&[SeqItem::key(13)][..]));
}

// ---------------------------------------------------------------------------
// prompts parsed off the log

fn prompt_state(lines: &[(&str, i64)], mode: i64) -> GameState {
    let mut st = playing();
    st.input_mode = mode;
    msgs(&mut st, &lines.iter().map(|(t, c)| (*t, 100, *c)).collect::<Vec<_>>());
    st
}

fn hotkeys(p: &ParsedPrompt) -> Vec<(String, String)> {
    p.options.iter().map(|o| (o.hotkey.clone(), o.label.clone())).collect()
}

#[test]
fn a_choice_prompt_reads_its_hotkey_words_and_the_stat_gain_has_no_cancel() {
    let st = prompt_state(&[("Increase (S)trength, (I)ntelligence, or (D)exterity? ", Msgch::PROMPT)], MouseMode::PROMPT);
    let ctx = derive_context(&st, &Scene::default(), 0, Layer::Micro);
    let p = ctx.prompt.unwrap();
    assert_eq!(hotkeys(&p), vec![("S".into(), "(S)trength".into()), ("I".into(), "(I)ntelligence".into()), ("D".into(), "(D)exterity".into())]);
    assert!(p.stat_gain && !p.cancel);
    assert_eq!(prompt_lead(&p.text), Some("Increase".into()));
    // no way back, and no answer until the cursor lights one
    let t = binding_table(&Context { mode: Mode::Prompt, prompt: Some(p), ..Default::default() });
    assert_eq!(t.get(Button::B), None);
    assert_eq!(t.get(Button::A), None);
}

#[test]
fn a_yesno_takes_y_and_n_and_its_default_and_pickup_takes_lowercase() {
    let st = prompt_state(&[("Really attack? (y/N)", Msgch::PROMPT)], MouseMode::YESNO);
    let p = derive_context(&st, &Scene::default(), 0, Layer::Micro).prompt.unwrap();
    assert_eq!(hotkeys(&p), vec![("Y".into(), "Yes".into()), ("N".into(), "No".into())]);
    assert_eq!(p.default.as_deref(), Some("N"));
    let st = prompt_state(&[("Pick up a dagger? ((y)es/(n)o/(a)ll/(m)enu/*?g,/q)", Msgch::PROMPT)], MouseMode::YESNO);
    let p = derive_context(&st, &Scene::default(), 0, Layer::Micro).prompt.unwrap();
    assert_eq!(hotkeys(&p), vec![("y".into(), "Yes".into()), ("n".into(), "No".into())]);
}

#[test]
fn a_listed_prompt_reads_k_dash_label_choices_and_the_travel_prompts_stairs() {
    let st = prompt_state(&[("Where to? (Tab/Enter - D:3, ? - help) ", Msgch::PROMPT)], MouseMode::PROMPT);
    let p = derive_context(&st, &Scene::default(), 0, Layer::Micro).prompt.unwrap();
    let keys: Vec<String> = p.options.iter().map(|o| o.hotkey.clone()).collect();
    assert_eq!(keys, vec!["<", ">", "\t", "?"]);
    assert_eq!(p.options[2].label, "D:3");
    assert_eq!(p.options[3].label, "Help");
    let st = prompt_state(&[("Issue what order? ", Msgch::PROMPT), (" t - Shout!", 0), ("r - Retreat!             s - Stop attacking.", 0)], MouseMode::PROMPT);
    let p = derive_context(&st, &Scene::default(), 0, Layer::Micro).prompt.unwrap();
    assert_eq!(hotkeys(&p), vec![("t".into(), "Shout!".into()), ("r".into(), "Retreat!".into()), ("s".into(), "Stop attacking".into())]);
}

#[test]
fn the_letter_picker_names_what_each_letter_holds() {
    let mut st = prompt_state(&[], MouseMode::PROMPT);
    reduce(&mut st, &json!({ "msg": "player", "inv": { "0": { "base_type": 0, "quantity": 1, "name": "a +0 dagger" } } }));
    msgs(&mut st, &[("a - a +0 dagger (weapon)", 100, 0), ("Adjust to which letter? (? for menu, Esc to quit)", 100, Msgch::PROMPT)]);
    let p = derive_context(&st, &Scene::default(), 0, Layer::Micro).prompt.unwrap();
    assert_eq!(p.letters, Some(Letters::Item));
    assert_eq!(p.from.as_deref(), Some("a"));
    assert_eq!(p.options.len(), 52);
    assert_eq!(p.options[0].held.as_deref(), Some("a +0 dagger"));
}

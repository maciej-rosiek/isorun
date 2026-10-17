//! The bytes the official client puts on the wire. `comm.js send_message` assigns
//! `data["msg"] = msg` after the caller's fields, so `msg` is last everywhere it is used;
//! `client.js` hand-builds `key` and byte `input` as strings with `msg` first. No parser cares,
//! but the server is sent this exact text, so a builder that drifts should fail here rather than
//! on a capture nobody reads.

use isorun_webtiles::cm;
use serde_json::{json, Value};

fn wire(v: Value) -> String {
    serde_json::to_string(&v).unwrap()
}

#[test]
fn puts_msg_last_on_everything_send_message_builds() {
    assert_eq!(wire(cm::input("x")), r#"{"text":"x","msg":"input"}"#);
    assert_eq!(wire(cm::text_input("ab\r")), r#"{"text":"ab\r","msg":"text_input"}"#);
    assert_eq!(wire(cm::play("dcss-web-trunk")), r#"{"game_id":"dcss-web-trunk","msg":"play"}"#);
    assert_eq!(wire(cm::watch("orbrun")), r#"{"username":"orbrun","msg":"watch"}"#);
    assert_eq!(wire(cm::chat("hi")), r#"{"text":"hi","msg":"chat_msg"}"#);
    assert_eq!(wire(cm::click_cell(3, 4, 1)), r#"{"x":3,"y":4,"button":1,"msg":"click_cell"}"#);
    assert_eq!(wire(cm::target_cursor(-2, 7)), r#"{"x":-2,"y":7,"msg":"target_cursor"}"#);
    assert_eq!(wire(cm::menu_hover(2, true)), r#"{"hover":2,"mouse":true,"msg":"menu_hover"}"#);
    assert_eq!(wire(cm::menu_scroll(0, 9, 3)), r#"{"first":0,"last":9,"hover":3,"msg":"menu_scroll"}"#);
    assert_eq!(wire(cm::formatted_scroller_scroll(5)), r#"{"scroll":5,"msg":"formatted_scroller_scroll"}"#);
    assert_eq!(wire(cm::outer_menu_focus(97, "main")), r#"{"hotkey":97,"menu_id":"main","msg":"outer_menu_focus"}"#);
    assert_eq!(wire(cm::set_option("action_panel_show", &json!(true))), r#"{"line":"action_panel_show = true","msg":"set_option"}"#);
    assert_eq!(wire(cm::inv_item_action(4)), r#"{"slot":4,"msg":"inv_item_action"}"#);
    let state = json!({ "generation_id": 2 });
    assert_eq!(wire(cm::ui_state_sync(state.as_object().unwrap())), r#"{"generation_id":2,"msg":"ui_state_sync"}"#);
    assert_eq!(wire(cm::login("a", "b")), r#"{"username":"a","password":"b","msg":"login"}"#);
    assert_eq!(wire(cm::register("a", "b", "c")), r#"{"username":"a","password":"b","email":"c","msg":"register"}"#);
    assert_eq!(wire(cm::set_rc("g", "x")), r#"{"game_id":"g","contents":"x","msg":"set_rc"}"#);
}

#[test]
fn leads_with_msg_on_the_two_client_js_hand_builds() {
    // client.js send_keycode: '{"msg":"key","keycode":' + code + '}'
    assert_eq!(wire(cm::key(27)), r#"{"msg":"key","keycode":27}"#);
    // client.js send_bytes: '{"msg":"input","data":[' + codes + ']}'
    assert_eq!(wire(cm::input_bytes(&[123])), r#"{"msg":"input","data":[123]}"#);
}

#[test]
fn writes_a_bare_object_for_the_messages_that_carry_no_fields() {
    assert_eq!(wire(cm::go_lobby()), r#"{"msg":"go_lobby"}"#);
    assert_eq!(wire(cm::pong()), r#"{"msg":"pong"}"#);
    assert_eq!(wire(cm::main_menu_action()), r#"{"msg":"main_menu_action"}"#);
}

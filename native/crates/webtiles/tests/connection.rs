//! Unpacking server frames, as connection.ts `dispatchText` does.

use isorun_webtiles::{is_ping, unpack_frame, FrameError};
use serde_json::json;

#[test]
fn unpacks_a_batch_in_order() {
    let msgs = unpack_frame(r#"{"msgs":[{"msg":"ping"},{"msg":"go_lobby"},{"msg":"player","hp":3}]}"#).unwrap();
    assert_eq!(msgs, vec![json!({ "msg": "ping" }), json!({ "msg": "go_lobby" }), json!({ "msg": "player", "hp": 3 })]);
    assert!(is_ping(&msgs[0]));
    assert!(!is_ping(&msgs[1]));
}

#[test]
fn takes_a_single_message_as_it_is() {
    assert_eq!(unpack_frame(r#"{"msg":"ping"}"#).unwrap(), vec![json!({ "msg": "ping" })]);
    // `msgs` that is not a list is not a batch
    assert_eq!(unpack_frame(r#"{"msg":"x","msgs":3}"#).unwrap(), vec![json!({ "msg": "x", "msgs": 3 })]);
}

#[test]
fn reports_what_is_not_json() {
    assert_eq!(unpack_frame("hello"), Err(FrameError::NotJson("hello".into())));
    assert!(matches!(unpack_frame("{nope"), Err(FrameError::Parse(t)) if t == "{nope"));
    let long = format!("x{}", "y".repeat(500));
    match unpack_frame(&long) {
        Err(FrameError::NotJson(t)) => assert_eq!(t.chars().count(), 200),
        other => panic!("{other:?}"),
    }
    assert!(FrameError::Parse("{".into()).to_string().starts_with("JSON parse error"));
}

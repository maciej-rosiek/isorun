//! The reducer, replaying recorded sessions from a public server
//! (`native/fixtures`) and pinned cases; the port of reducer.test.ts.

use std::path::PathBuf;

use isorun_webtiles::*;
use serde_json::{json, Value};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn load_fixture(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path(name)).unwrap();
    text.trim().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["m"].clone()).collect()
}

fn load_json(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path(name)).unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["msgs"].as_array().unwrap().clone()
}

fn r(st: &mut GameState, m: Value) {
    reduce(st, &m);
}

fn replay(name: &str) -> GameState {
    let mut st = initial_state();
    for m in load_fixture(name) {
        reduce(&mut st, &m);
    }
    st
}

fn link(id: &str, label: &str, rc: &str) -> GameLink {
    GameLink { id: id.into(), label: label.into(), rc: Some(rc.into()), ..GameLink::default() }
}

fn texts(st: &GameState) -> Vec<&str> {
    st.messages.lines.iter().map(|l| l.text.as_str()).collect()
}

// ---- reducer replay

#[test]
fn replays_a_recorded_spectating_session_without_panicking() {
    let st = replay("cdi-0.34-watch.ndjson");
    assert_eq!(st.phase, Phase::Watching);
    assert_eq!(st.watching.as_deref(), Some("Dwarfsong"));
    let v = st.version.gamedata_version.as_deref().unwrap();
    assert!(v.len() == 40 && v.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{v}");
    assert_eq!(st.player.name, "Dwarfsong");
    assert!(st.map.cells.len() > 500);
    assert!(st.map.player_on_level);
    assert!(!st.messages.lines.is_empty());
    // no unknown message types in a modern session
    let unknown: Vec<_> = st.diagnostics.iter().filter(|d| d.text == "unknown message type").map(|d| d.msg.clone()).collect();
    assert_eq!(unknown, Vec::<Option<String>>::new());
    // and nothing in it is malformed
    assert_eq!(st.diagnostics, vec![]);
}

#[test]
fn keeps_the_player_located_at_a_known_cell() {
    let st = replay("cdi-0.34-watch.ndjson");
    let cell = st.map.cell(st.player.pos.x, st.player.pos.y).expect("the player's cell");
    assert!(cell.t.is_some());
}

#[test]
fn replays_the_autopickup_session_cleanly() {
    let st = replay("cdi-0.34-autopickup.ndjson");
    assert_eq!(st.diagnostics, vec![]);
    assert_eq!(st.player.turn, 29816);
}

#[test]
fn shares_monsters_by_id_and_keeps_the_dirty_cells() {
    let mut st = replay("cdi-0.34-watch.ndjson");
    // every shared monster is held by as many cells as its refs say, and every cell's one is there
    for (id, m) in &st.map.monsters {
        let held = st.map.cells.values().filter(|c| c.mon == Some(CellMonster::Shared(*id))).count() as i64;
        assert!(m.refs >= held && held > 0, "monster {id}: refs {} held {held}", m.refs);
    }
    for c in st.map.cells.values() {
        if c.mon.is_some() {
            assert!(st.map.monster(c).is_some());
        }
    }
    assert!(!st.dirty_cells.is_empty());
    st.clear_dirty();
    assert!(st.dirty_cells.is_empty() && !st.map_cleared);
}

#[test]
fn merges_map_cells_with_implicit_x_y() {
    let mut st = initial_state();
    r(
        &mut st,
        json!({ "msg": "map", "clear": true, "player_on_level": true, "cells": [{ "x": 5, "y": 7, "mf": 1 }, { "mf": 2 }, { "mf": 3 }, { "x": 1, "y": 9, "mf": 4 }] }),
    );
    assert_eq!(st.map.cell(6, 7).and_then(|c| c.mf), Some(2));
    assert_eq!(st.map.cell(7, 7).and_then(|c| c.mf), Some(3));
    assert_eq!(st.map.cell(1, 9).and_then(|c| c.mf), Some(4));
    assert_eq!(st.map.bounds, Some(Bounds { left: 1, top: 7, right: 7, bottom: 9 }));
    assert!(st.map_cleared);
    assert_eq!(st.dirty_cells.len(), 4);
    assert!(st.dirty_cells.contains(&map_key(6, 7)));
}

#[test]
fn tracks_monsters_by_id_across_cells() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "map", "clear": true, "cells": [{ "x": 0, "y": 0, "mon": { "id": 7, "name": "rat", "att": 0 } }] }));
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 0, "y": 0, "mon": null }, { "x": 1, "y": 0, "mon": { "id": 7 } }] }));
    assert!(st.map.cell(0, 0).unwrap().mon.is_none());
    assert_eq!(st.map.monster_at(1, 0).map(|m| m.name.as_str()), Some("rat"));
    assert_eq!(st.map.monsters[&7].refs, 1);
    // an update through one cell is seen from every cell that holds the monster
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 2, "y": 0, "mon": { "id": 7, "threat": 2 } }] }));
    assert_eq!(st.map.monster_at(1, 0).and_then(|m| m.threat), Some(2));
    assert_eq!(st.map.monsters[&7].refs, 2);
    // the last cell to let go drops it
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 1, "y": 0, "mon": null }, { "x": 2, "y": 0, "mon": null }] }));
    assert!(st.map.monsters.is_empty());
    // a monster without an id is the cell's own, att defaulting to 0
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 3, "y": 0, "mon": { "name": "plant" } }] }));
    let m = st.map.monster_at(3, 0).unwrap();
    assert_eq!((m.name.as_str(), m.att), ("plant", 0));
    // a new id over an old monster starts from it
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 3, "y": 0, "mon": { "id": 9, "att": 1 } }] }));
    let m = st.map.monster_at(3, 0).unwrap();
    assert_eq!((m.name.as_str(), m.att, m.id), ("plant", 1, Some(9)));
}

#[test]
fn merges_tiles_key_by_key() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 0, "y": 0, "t": { "fg": 5, "bg": [1, 2], "doll": [[3, 32]] } }] }));
    let t = st.map.cell(0, 0).unwrap().t.clone().unwrap();
    assert_eq!(t.fg, Some(TileWord::new(5, 0)));
    assert_eq!(t.bg.map(TileWord::value), Some((2 << 32) | 1));
    assert_eq!(t.doll, Some(vec![[3, 32]]));
    // a doll without trans is not translucent
    assert_eq!(t.trans, Some(false));
    r(&mut st, json!({ "msg": "map", "cells": [{ "x": 0, "y": 0, "t": { "fg": 6, "doll": null, "flv": { "f": 2 } } }] }));
    let t = st.map.cell(0, 0).unwrap().t.clone().unwrap();
    assert_eq!(t.fg, Some(TileWord::new(6, 0)));
    assert_eq!(t.bg.map(|w| w.hi), Some(2));
    assert_eq!(t.doll, None);
    assert_eq!(t.flv, Some(Flv { f: Some(2), s: None }));
}

#[test]
fn notes_malformed_cells_and_moves_on() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "map", "cells": [3, { "x": "a", "y": 1 }, { "x": 1, "y": 1, "t": 5 }, { "x": 2, "y": 1, "mon": 4, "mf": "x" }] }));
    let texts: Vec<_> = st.diagnostics.iter().map(|d| d.text.as_str()).collect();
    assert!(texts.contains(&"map cell is not an object; skipped"));
    assert!(texts.contains(&"map cell has a bad position; skipped"));
    assert!(texts.contains(&"map cell t is not an object; skipped"));
    assert!(texts.contains(&"map cell mon is not an object; skipped"));
    assert!(st.map.cell(2, 1).is_some());
    r(&mut st, json!(5));
    r(&mut st, json!({ "nomsg": 1 }));
    assert_eq!(st.diagnostics.last().unwrap().text, "message is not an object with a msg field; ignored");
    r(&mut st, json!({ "msg": "what" }));
    assert_eq!(st.diagnostics.last().unwrap().msg.as_deref(), Some("what"));
}

#[test]
fn player_updates_are_partial_and_record_changed_keys() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "player", "hp": 10, "hp_max": 20, "str": 12, "pos": { "x": 1, "y": 1 } }));
    r(&mut st, json!({ "msg": "player", "str": 11, "time": 20 }));
    assert_eq!(st.player.hp, 10);
    assert_eq!(st.player.str, 11);
    assert!(st.player.changed.contains("str"));
    assert_eq!(st.player.previous.get("str"), Some(&json!(12)));
    assert!(!st.player.changed.contains("hp"));
    // keys the sheet does not name are kept as sent
    r(&mut st, json!({ "msg": "player", "zigs": 3 }));
    assert_eq!(st.player.extra.get("zigs"), Some(&json!(3)));
    assert_eq!(st.player.previous.get("zigs"), Some(&Value::Null));
    assert_eq!(st.player.get("hp"), Some(json!(10)));
}

#[test]
fn unwraps_the_experience_progress_crawl_sends_as_an_int8_past_a_level() {
    // given
    let mut st = initial_state();

    // when
    r(&mut st, json!({ "msg": "player", "xl": 2, "progress": -125 }));

    // then
    assert_eq!(st.player.progress, 100);
}

#[test]
fn menus_keep_item_chunks_and_hover() {
    let mut st = initial_state();
    r(
        &mut st,
        json!({ "msg": "menu", "tag": "inventory", "flags": 0, "title": { "text": "Inventory" }, "total_items": 3, "chunk_start": 0, "items": [{ "text": "a - sword", "hotkeys": [97], "level": 2 }, "header"], "last_hovered": -1 }),
    );
    r(&mut st, json!({ "msg": "update_menu_items", "chunk_start": 2, "items": [{ "text": "b - shield", "hotkeys": [98], "level": 2 }] }));
    assert_eq!(st.menus.len(), 1);
    assert_eq!(st.menus[0].items[1].as_ref().map(|i| i.text.as_str()), Some("header"));
    assert_eq!(st.menus[0].items[2].as_ref().and_then(|i| i.hotkeys.clone()), Some(vec![98]));
    assert_eq!(st.menus[0].title.text, "Inventory");
    // an update that leaves out the hotkeys clears them; the level stays 2
    r(&mut st, json!({ "msg": "update_menu_items", "chunk_start": 0, "items": [{ "text": "a - sword!" }] }));
    let item = st.menus[0].items[0].clone().unwrap();
    assert_eq!((item.text.as_str(), item.hotkeys, item.level), ("a - sword!", None, Some(2)));
    r(&mut st, json!({ "msg": "update_menu", "total_items": 5, "more": "next" }));
    assert_eq!(st.menus[0].items.len(), 5);
    assert_eq!(st.menus[0].more, "next");
    r(&mut st, json!({ "msg": "title_prompt", "prompt": "Find:" }));
    assert_eq!(st.menus[0].title_prompt, Some(TitlePrompt { prompt: "Find:".into(), raw: false }));
    r(&mut st, json!({ "msg": "close_menu" }));
    assert_eq!(st.menus.len(), 0);
}

#[test]
fn handles_more_and_rollback() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "a" }, { "text": "b" }], "more": true }));
    assert!(st.messages.more);
    assert_eq!(st.messages.more_text, "--more--");
    r(&mut st, json!({ "msg": "msgs", "rollback": 1, "messages": [{ "text": "c" }], "more": false }));
    assert_eq!(texts(&st), vec!["a", "c"]);
    assert!(!st.messages.more);
}

#[test]
fn keeps_the_last_500_lines_and_where_the_player_read_to() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "input_mode", "mode": 5 }));
    let lines: Vec<Value> = (0..600).map(|i| json!({ "text": i.to_string() })).collect();
    r(&mut st, json!({ "msg": "msgs", "messages": lines[..300] }));
    // leaving the more: the key was read with 300 lines on screen
    r(&mut st, json!({ "msg": "input_mode", "mode": 1 }));
    assert_eq!(st.messages.read_to, 300);
    assert_eq!(st.messages.lines.last().unwrap().marker, Some(Marker::Command));
    r(&mut st, json!({ "msg": "msgs", "messages": lines[300..] }));
    assert_eq!(st.messages.lines.len(), 500);
    assert_eq!(st.messages.lines[0].text, "100");
    assert_eq!(st.messages.read_to, 200);
}

// ---- helpers

#[test]
fn marks_the_player_sheet_as_received_only_once_a_player_message_has_arrived() {
    let mut st = initial_state();
    assert!(!st.player.received);
    r(&mut st, json!({ "msg": "ui_state", "state": 1 }));
    r(&mut st, json!({ "msg": "options", "options": { "tile_display_mode": "tiles" } }));
    assert!(!st.player.received);
    r(&mut st, json!({ "msg": "player", "name": "Orb", "hp": 12, "hp_max": 12 }));
    assert!(st.player.received);
    r(&mut st, json!({ "msg": "go_lobby" }));
    assert!(!st.player.received);
}

#[test]
fn formats_colour_tags_like_the_official_client() {
    assert_eq!(formatted_string_to_html("<red>hi<lightgrey> there"), "<span class='fg4'>hi</span><span class='fg7'> there</span>");
    assert_eq!(formatted_string_to_html("a << b"), "a &lt; b");
    assert_eq!(formatted_string_to_html("x > y"), "x &gt; y");
}

#[test]
fn routes_printable_text_as_input_and_control_keys_as_key() {
    assert_eq!(key_message('a' as i32), json!({ "msg": "input", "text": "a" }));
    assert_eq!(key_message(27), json!({ "msg": "key", "keycode": 27 }));
    assert_eq!(key_message(-254), json!({ "msg": "key", "keycode": -254 }));
    assert_eq!(key_message(123), json!({ "msg": "input", "data": [123] }));
    assert_eq!(key_message(0xe9), json!({ "msg": "input", "text": "é" }));
}

#[test]
fn action_panel_items_sort_like_action_panel_js() {
    let mut st = initial_state();
    r(
        &mut st,
        json!({ "msg": "player", "inv": {
        "0": { "quantity": 1, "action_panel_order": 2, "sub_type": 1 },
        "1": { "quantity": 2, "action_panel_order": 1, "sub_type": 5 },
        "2": { "quantity": 1, "action_panel_order": 1, "sub_type": 3 },
        "3": { "quantity": 0, "action_panel_order": 0 },
        "27": { "quantity": 1, "action_panel_order": -1 },
    } }),
    );
    let items: Vec<_> = action_panel_items(&st).into_iter().map(|i| i.letter).collect();
    assert_eq!(items, vec!["c", "b", "a"]);
}

#[test]
fn has_status_matches_the_text() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "player", "status": [{ "text": "flying", "light": "Fly" }, null, { "light": "x" }] }));
    assert_eq!(st.player.status.len(), 2);
    assert!(has_status(&st.player, &regex::Regex::new("^fly").unwrap()));
    assert!(!has_status(&st.player, &regex::Regex::new("berserk").unwrap()));
}

// ---- latestGameLinks

fn games(list: &[(&str, &str)]) -> Vec<GameLink> {
    list.iter().map(|(id, label)| GameLink::new(id, label)).collect()
}

fn ids(list: &[GameLink]) -> Vec<&str> {
    list.iter().map(|g| g.id.as_str()).collect()
}

#[test]
fn keeps_trunk_and_the_newest_stable_release_all_variants() {
    let g = games(&[
        ("dcss-web-trunk", "DCSS trunk"),
        ("dcss-web-0.34", "DCSS 0.34"),
        ("dcss-web-0.33", "DCSS 0.33"),
        ("dcss-web-0.9", "DCSS 0.9"),
        ("sprint-web-0.34", "Sprint 0.34"),
        ("sprint-web-0.33", "Sprint 0.33"),
        ("tut-web-trunk", "Tutorial trunk"),
        ("spr-git", "Sprint trunk"),
    ]);
    assert_eq!(ids(&latest_game_links(&g)), vec!["dcss-web-trunk", "dcss-web-0.34", "sprint-web-0.34", "tut-web-trunk", "spr-git"]);
}

#[test]
fn compares_versions_numerically_and_keeps_unversioned_links() {
    let g = games(&[("a", "DCSS 0.10"), ("b", "DCSS 0.9"), ("c", "Something odd")]);
    assert_eq!(ids(&latest_game_links(&g)), vec!["a", "c"]);
}

#[test]
fn splits_the_kept_links_into_the_latest_version_and_trunk_rows() {
    let g = games(&[
        ("dcss-web-trunk", "DCSS trunk"),
        ("dcss-web-0.34", "DCSS 0.34"),
        ("dcss-web-0.33", "DCSS 0.33"),
        ("seeded-web-0.34", "Custom seed 0.34"),
        ("sprint-web-0.34", "Sprint 0.34"),
        ("tut-web-trunk", "Tutorial trunk"),
        ("odd", "Something odd"),
        ("odd-sprint", "Odd sprint"),
    ]);
    let rows = game_link_rows(&g);
    assert_eq!(rows.latest_version.as_deref(), Some("0.34"));
    assert_eq!(ids(&rows.latest), vec!["dcss-web-0.34"]);
    assert_eq!(ids(&rows.trunk), vec!["dcss-web-trunk"]);
    assert_eq!(ids(&rows.other), vec!["odd"]);
}

// ---- session messages the official client handles outside the game

#[test]
fn keeps_the_rc_file_stale_process_notices_and_account_hold_on_the_lobby() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "rcfile_contents", "game_id": "dcss-web-trunk", "contents": "autopickup = $?!" }));
    assert_eq!(st.lobby.rcfile, Some(RcFile { game_id: Some("dcss-web-trunk".into()), contents: "autopickup = $?!".into() }));
    r(&mut st, json!({ "msg": "stale_processes", "game": "dcss-web-trunk", "timeout": 30 }));
    assert_eq!(st.lobby.stale_processes, Some(StaleProcesses { game: "dcss-web-trunk".into(), timeout: 30 }));
    r(&mut st, json!({ "msg": "force_terminate?" }));
    assert!(st.lobby.force_terminate);
    // process_handler.py sends hide_dialog once the stale game is stopped: both notices go
    r(&mut st, json!({ "msg": "hide_dialog" }));
    assert_eq!(st.lobby.stale_processes, None);
    assert!(!st.lobby.force_terminate);
    r(&mut st, json!({ "msg": "stale_processes", "game": "dcss-web-trunk", "timeout": 10 }));
    r(&mut st, json!({ "msg": "game_started" }));
    assert_eq!(st.lobby.stale_processes, None);
    r(&mut st, json!({ "msg": "set_account_hold" }));
    assert!(st.lobby.account_hold);
    r(&mut st, json!({ "msg": "clear_account_hold" }));
    assert!(!st.lobby.account_hold);
    r(&mut st, json!({ "msg": "reload_url" }));
    assert!(st.lobby.reload_url);
    r(&mut st, json!({ "msg": "go_admin" }));
    assert!(st.lobby.admin);
    r(&mut st, json!({ "msg": "admin_log", "text": "x" }));
    assert_eq!(st.lobby.admin_log, vec!["x"]);
    assert_eq!(st.diagnostics, vec![]);
}

#[test]
fn collects_chat_spectators_announcements_and_dump_urls_as_chat_js_does() {
    let mut st = initial_state();
    let rev = st.rev.chat;
    r(&mut st, json!({ "msg": "update_spectators", "count": 2, "names": "<span>a</span>, <span>b</span>" }));
    assert_eq!(st.spectators.as_ref().map(|s| s.count), Some(2));
    r(&mut st, json!({ "msg": "chat", "sender": "a", "content": "<span class=\"chat_sender\">a</span>: <span class=\"chat_msg\">hi</span>" }));
    r(&mut st, json!({ "msg": "server_announcement", "text": "restart soon" }));
    r(&mut st, json!({ "msg": "dump", "url": "https://x/morgue-a" }));
    assert_eq!(st.chat.iter().map(|c| c.sender.as_str()).collect::<Vec<_>>(), vec!["a", "server", ""]);
    assert!(st.chat[1].meta);
    assert!(st.chat[1].html.contains("Serverwide announcement"));
    assert!(st.chat[2].html.contains("morgue-a.txt"));
    assert!(st.rev.chat > rev);
    r(&mut st, json!({ "msg": "toggle_chat" }));
    assert!(!st.chat_visible);
    r(&mut st, json!({ "msg": "super_hide_chat" }));
    assert!(st.chat_super_hidden);
    // a new game starts with an empty chat (client.js game_cleanup)
    r(&mut st, json!({ "msg": "game_client", "version": "abc", "content": "" }));
    assert_eq!(st.chat, vec![]);
}

#[test]
fn keeps_the_lobby_roster_in_order() {
    let mut st = initial_state();
    r(&mut st, json!({ "id": 2, "username": "b", "game_id": "g", "xl": "3", "msg": "lobby_entry" }));
    r(&mut st, json!({ "id": 1, "username": "a", "game_id": "g", "msg": "lobby_entry" }));
    r(&mut st, json!({ "id": 2, "username": "b", "game_id": "g", "xl": "4", "msg": "lobby_entry" }));
    assert_eq!(st.lobby.entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2, 1]);
    assert_eq!(st.lobby.entry(2).and_then(|e| e.xl.as_deref()), Some("4"));
    r(&mut st, json!({ "msg": "lobby_remove", "id": 2 }));
    assert_eq!(st.lobby.entries.len(), 1);
    r(&mut st, json!({ "msg": "lobby_clear" }));
    assert!(st.lobby.entries.is_empty());
}

// ---- game phase

#[test]
fn stays_playing_when_game_client_follows_game_started() {
    // as crawl.dcss.io sends them (recorded playing dcss-git 2026-09-09)
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "game_started" }));
    assert_eq!(st.phase, Phase::Playing);
    r(&mut st, json!({ "msg": "game_client", "version": "abc", "content": "" }));
    assert_eq!(st.phase, Phase::Playing);
    r(&mut st, json!({ "msg": "game_ended", "reason": "saved" }));
    assert_eq!(st.phase, Phase::Ended);
    assert_eq!(st.exit, Some(GameExit { reason: Some("saved".into()), ..GameExit::default() }));
    // a watched game's end remembers whose it was, for the lobby's wording
    let mut w = initial_state();
    r(&mut w, json!({ "msg": "watching_started", "username": "alice" }));
    r(&mut w, json!({ "msg": "game_ended", "reason": "dead", "dump": "/morgue/alice/morgue-alice-1" }));
    assert_eq!(
        w.exit,
        Some(GameExit {
            reason: Some("dead".into()),
            message: None,
            dump: Some("/morgue/alice/morgue-alice-1".into()),
            watched: Some("alice".into())
        })
    );
    // a game_client before any game_started is a game still on its way
    let mut fresh = initial_state();
    r(&mut fresh, json!({ "msg": "game_client", "version": "abc", "content": "" }));
    assert_eq!(fresh.phase, Phase::Loading);
}

// ---- popup scroll lines from the server

#[test]
fn counts_each_scroll_arrival_so_the_client_can_apply_a_repeat() {
    // ui-layouts.js recv_ui_scroll, formatted_scroller_update
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "ui-push", "type": "describe-item", "title": "t", "body": "b" }));
    assert_eq!(st.ui[0].scroll_seq, 0);
    r(&mut st, json!({ "msg": "ui-scroller-scroll", "scroll": 0, "from_webtiles": false }));
    assert_eq!((st.ui[0].scroll, st.ui[0].scroll_from_webtiles, st.ui[0].scroll_seq), (Some(0), false, 1));
    // an echo of the client's own scrolling is marked as such
    r(&mut st, json!({ "msg": "ui-scroller-scroll", "scroll": 0, "from_webtiles": true }));
    assert_eq!((st.ui[0].scroll, st.ui[0].scroll_from_webtiles, st.ui[0].scroll_seq), (Some(0), true, 2));
    // a formatted scroller gets its line on its ui-state, not on ui-scroller-scroll
    r(&mut st, json!({ "msg": "ui-push", "type": "formatted-scroller", "title": "help", "text": "x" }));
    r(&mut st, json!({ "msg": "ui-scroller-scroll", "scroll": 5, "from_webtiles": false }));
    assert_eq!(st.ui[1].scroll_seq, 0);
    r(&mut st, json!({ "msg": "ui-state", "type": "formatted-scroller", "scroll": 7, "from_webtiles": false }));
    assert_eq!((st.ui[1].scroll, st.ui[1].scroll_from_webtiles, st.ui[1].scroll_seq), (Some(7), false, 1));
    r(&mut st, json!({ "msg": "ui-state", "type": "formatted-scroller", "text": "page two" }));
    assert_eq!(st.ui[1].scroll_seq, 1);
    assert_eq!(st.ui[1].data.get("text"), Some(&json!("page two")));
    assert_eq!(st.ui_scroll.map(|s| s.seq), Some(3));
}

#[test]
fn follows_ui_state_sync_for_the_popup_it_is_about() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "ui-push", "type": "describe-item", "generation_id": 4 }));
    r(&mut st, json!({ "msg": "ui-state-sync", "generation_id": 3, "widget_id": "x", "has_focus": 1 }));
    assert_eq!(st.ui[0].sync, None);
    r(&mut st, json!({ "msg": "ui-state-sync", "generation_id": 4, "widget_id": "x", "has_focus": 1 }));
    let sync = st.ui[0].sync.clone().unwrap();
    assert_eq!((sync.widget_id.as_deref(), sync.has_focus, sync.seq), (Some("x"), Some(true), 2));
    // the player's own client ignores the echo of its own newgame focus
    r(&mut st, json!({ "msg": "ui-push", "type": "newgame-choice" }));
    let before = st.rev.ui;
    r(&mut st, json!({ "msg": "ui-state", "from_client": true, "x": 1 }));
    assert_eq!(st.rev.ui, before);
    assert_eq!(st.ui[1].state.get("x"), Some(&json!(1)));
}

/// Items under the player are not on the map: `tileidx_player` (tilepick-p.cc) sets no S_UNDER
/// flag on the player's tile, so the only word the server gives is `item_check`'s line on
/// MSGCH_FLOOR_ITEMS (items.cc). Pinned to the recorded session: the `player` update with the
/// new `pos` precedes the `msgs` line, both on the arrival turn; one stale line arrives ten
/// turns after its pile was left.
mod items_underfoot_cases {
    use super::*;

    fn text_of(m: &Value) -> String {
        m["messages"].as_array().map(|a| a.iter().map(|l| l["text"].as_str().unwrap_or("")).collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }

    #[test]
    fn is_announced_on_arrival_and_gone_on_the_next_step() {
        let msgs = load_fixture("cdi-0.34-watch.ndjson");
        let mut st = initial_state();
        let at = msgs.iter().position(|m| m["msg"] == "msgs" && text_of(m).contains("You see here a +0 halberd.")).unwrap();
        assert!(at > 0);
        for m in &msgs[..at] {
            reduce(&mut st, m);
        }
        // the player update with the new pos came first, at the same turn as the line
        assert_eq!(msgs[at - 1]["msg"], "player");
        assert_eq!(st.player.arrived_turn, 12258);
        assert!(!items_underfoot(&st));
        reduce(&mut st, &msgs[at]);
        assert!(items_underfoot(&st));
        // the name in the log's own words and colours, the line's own <lightgrey> and all
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 halberd"));
        // the next player update moves on
        let mut j = at + 1;
        while msgs[j]["msg"] != "player" {
            reduce(&mut st, &msgs[j]);
            j += 1;
        }
        assert!(items_underfoot(&st));
        reduce(&mut st, &msgs[j]);
        assert_eq!(st.player.pos, Pos { x: 39, y: 5 });
        assert!(!items_underfoot(&st));
    }

    #[test]
    fn ignores_a_line_about_a_pile_already_left() {
        // recorded: turn 12185 delivered at turn 12195
        let msgs = load_fixture("cdi-0.34-watch.ndjson");
        let mut st = initial_state();
        let at = msgs
            .iter()
            .position(|m| {
                m["msg"] == "msgs"
                    && m["messages"].as_array().is_some_and(|a| a.iter().any(|l| l["turn"] == 12185 && l["channel"] == Msgch::FLOOR_ITEMS))
            })
            .unwrap();
        assert!(at > 0);
        for m in &msgs[..=at] {
            reduce(&mut st, m);
        }
        assert_eq!(st.player.turn, 12195);
        assert_eq!(st.player.floor_items.as_ref().map(|f| f.turn), Some(12185));
        assert!(!items_underfoot(&st));
        assert_eq!(floor_items_label(&st), None);
    }

    #[test]
    fn names_the_pile_in_the_words_of_each_item_check_form() {
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 12163 }));
        // the glyph summary (this rc's item_stack_summary_minimum is 4): one glyph per item, classes spaced;
        // the two corpses (††) are not offered, `pickup` would not either (items.cc `count_movable_items`)
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You feel sick.", "turn": 12162, "channel": 0 }, { "text": "<lightgrey>Items here: <darkgrey>) [ ††<lightgrey>.", "turn": 12163, "channel": 21 }] }),
        );
        // the glyphs as the log printed them, corpses and all (the count is not)
        assert_eq!(floor_items_label(&st).as_deref(), Some("<darkgrey>) [ ††"));
        assert_eq!(floor_items_count(&st.player.floor_items.as_ref().unwrap().lines), Some(2));
        // the two-line listing: the names follow on the plain channel
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>Things that are here:", "turn": 12163, "channel": 21 }, { "text": "<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>", "turn": 12163, "channel": 0 }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>"));
        // a later plain line is not part of the listing
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "You feel sick.", "turn": 12164, "channel": 0 }] }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>"));
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>There are many items here.", "turn": 12164, "channel": 21 }] }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("many items"));
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "Items here: <darkgrey>)<lightgrey>.", "turn": 12164, "channel": 21 }] }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("<darkgrey>)"));
    }

    #[test]
    fn keeps_the_colours_the_log_printed() {
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 10 }));
        // the item in its menu colour, the sentence around it in the log's default (crawl.dcss.io 0.34)
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You see here <lightgreen>a staff of alchemy<lightgrey>.", "turn": 10, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgreen>a staff of alchemy"));
        assert_eq!(formatted_string_to_html(&floor_items_label(&st).unwrap()), "<span class='fg10'>a staff of alchemy</span>");
        // a name that closes its colour rather than reopening the one before it
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "You see here <yellow>27 gold pieces</yellow>.", "turn": 10, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<yellow>27 gold pieces"));
    }

    #[test]
    fn what_the_player_drops_lies_underfoot() {
        // given
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 10, "place": "D:1", "depth": 1 }));

        // when
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You drop 2 potions of might.", "turn": 11, "channel": 0 }] }));

        // then
        assert_eq!(floor_items_label(&st).as_deref(), Some("2 potions of might"));
    }

    #[test]
    fn holds_while_standing_still_and_clears_on_no_items_a_new_level_or_a_move() {
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 10, "place": "D:1", "depth": 1 }));
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>Things that are here:", "turn": 10, "channel": Msgch::FLOOR_ITEMS }, { "text": "a +0 mace; a robe", "turn": 10, "channel": 0 }] }),
        );
        assert!(items_underfoot(&st));
        // resting: turns pass, the pos does not
        r(&mut st, json!({ "msg": "player", "turn": 25 }));
        assert!(items_underfoot(&st));
        // the pick-up that found nothing
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>There are no items here.", "turn": 26, "channel": 0 }] }));
        assert!(!items_underfoot(&st));
        // the msgs line may precede the player update in a batch: same turn still counts
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You see here a dagger.", "turn": 30, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        r(&mut st, json!({ "msg": "player", "pos": { "x": 2, "y": 1 }, "turn": 30 }));
        assert!(items_underfoot(&st));
        // stairs to a level where the same coordinates are a different cell
        r(&mut st, json!({ "msg": "player", "turn": 31, "place": "D:2", "depth": 2 }));
        assert!(!items_underfoot(&st));
        // a line without a turn takes the player's
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "Items here: ) [.", "channel": Msgch::FLOOR_ITEMS }] }));
        assert!(items_underfoot(&st));
        r(&mut st, json!({ "msg": "player", "pos": { "x": 3, "y": 1 }, "turn": 32 }));
        assert!(!items_underfoot(&st));
    }

    /// A manual pick-up prints no new floor-items line (items.cc `pickup` does not call
    /// `item_check`); what it does send is the `player` update with the slot the item went to
    /// (tileweb.cc `_send_item`). The pile is counted down by it.
    #[test]
    fn counts_the_pile_down_as_inventory_or_gold_grows_while_standing_on_it() {
        let mut st = initial_state();
        r(
            &mut st,
            json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 100, "gold": 20, "inv": { "0": { "base_type": 5, "quantity": 1, "name": "scroll of identify" } } }),
        );
        // the single item: one gain empties the pile
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You see here a +0 halberd.", "turn": 100, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 halberd"));
        r(&mut st, json!({ "msg": "player", "turn": 101, "inv": { "1": { "base_type": 0, "quantity": 1, "name": "+0 halberd" } } }));
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>b - a +0 halberd", "turn": 100, "channel": 0 }] }));
        assert!(!items_underfoot(&st));
        assert_eq!(floor_items_label(&st), None);
        // a stack merging into a slot counts as one item ("i - 9 scrolls of identify (gained 1)")
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "Items here: <darkgrey>) [ ?<lightgrey>.", "turn": 101, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<darkgrey>) [ ?"));
        r(&mut st, json!({ "msg": "player", "turn": 102, "inv": { "0": { "quantity": 2 } } }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("2 items"));
        // gold is a floor item too (items.cc `get_gold`)
        r(&mut st, json!({ "msg": "player", "turn": 103, "gold": 34 }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("1 item"));
        // using up an item is not a pick-up; neither is a slot the server empties
        r(&mut st, json!({ "msg": "player", "turn": 104, "inv": { "0": { "quantity": 1 }, "1": { "base_type": 100, "quantity": 0 } } }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("1 item"));
        r(&mut st, json!({ "msg": "player", "turn": 105, "inv": { "2": { "base_type": 2, "quantity": 1, "name": "+0 robe" } } }));
        assert!(!items_underfoot(&st));
        // the listing by name loses its names once part of it is gone: the server did not say which
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>Things that are here:", "turn": 105, "channel": Msgch::FLOOR_ITEMS }, { "text": "<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>", "turn": 105, "channel": 0 }] }),
        );
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 mace; <green>a +2 robe of positive energy<lightgrey>"));
        r(&mut st, json!({ "msg": "player", "turn": 106, "inv": { "3": { "base_type": 0, "quantity": 1, "name": "+0 mace" } } }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("1 item"));
        r(&mut st, json!({ "msg": "player", "turn": 107, "inv": { "4": { "base_type": 2, "quantity": 1 } } }));
        assert!(!items_underfoot(&st));
        // a pile of unstated size cannot be counted down: it stays until the step away
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "There are many items here.", "turn": 107, "channel": Msgch::FLOOR_ITEMS }] }));
        r(&mut st, json!({ "msg": "player", "turn": 108, "inv": { "5": { "base_type": 2, "quantity": 1 } } }));
        assert_eq!(floor_items_label(&st).as_deref(), Some("many items"));
        r(&mut st, json!({ "msg": "player", "pos": { "x": 2, "y": 1 }, "turn": 109 }));
        assert!(!items_underfoot(&st));
    }

    #[test]
    fn leaves_corpses_out() {
        // they are stationary and `g` refuses them (items.cc pickup_single_item, item-prop.cc item_is_stationary)
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "player", "pos": { "x": 1, "y": 1 }, "turn": 100, "gold": 20, "inv": {} }));
        // a corpse alone: nothing to pick up
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You see here <darkgrey>a bat corpse</darkgrey>.", "turn": 100, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert!(!items_underfoot(&st));
        assert_eq!(floor_items_label(&st), None);
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>You see here <darkgrey>a jackal skeleton</darkgrey>.", "turn": 100, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert!(!items_underfoot(&st));
        // the listing: the corpse is dropped from the name and from the count
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>Things that are here:", "turn": 100, "channel": Msgch::FLOOR_ITEMS }, { "text": "<lightgrey>a +0 mace; <darkgrey>a bat corpse</darkgrey>; <darkgrey>a rat corpse</darkgrey>", "turn": 100, "channel": 0 }] }),
        );
        assert_eq!(floor_items_count(&st.player.floor_items.as_ref().unwrap().lines), Some(1));
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 mace"));
        r(&mut st, json!({ "msg": "player", "turn": 101, "inv": { "0": { "base_type": 0, "quantity": 1, "name": "+0 mace" } } }));
        assert!(!items_underfoot(&st));
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>Things that are here:", "turn": 101, "channel": Msgch::FLOOR_ITEMS }, { "text": "<darkgrey>a bat corpse</darkgrey>; <darkgrey>a rat corpse</darkgrey>", "turn": 101, "channel": 0 }] }),
        );
        assert!(!items_underfoot(&st));
        // the glyph summary: † is a corpse, ÷ a skeleton (viewchar.cc dchar_table)
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "Items here: <darkgrey>) [ ††<lightgrey>.", "turn": 101, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert_eq!(floor_items_count(&st.player.floor_items.as_ref().unwrap().lines), Some(2));
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "Items here: <darkgrey>†† ÷<lightgrey>.", "turn": 101, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        assert!(!items_underfoot(&st));
    }

    #[test]
    fn does_not_count_autopickup_on_arrival_against_the_pile_it_leaves_behind() {
        // cdi-0.34-autopickup.ndjson: a step onto a pile during explore, gold and a scroll autopicked up, a morningstar left
        let rec = load_fixture("cdi-0.34-autopickup.ndjson");
        let mut st = initial_state();
        reduce(&mut st, &rec[0]);
        assert_eq!(st.player.turn, 29814);
        // as if the last cell had been a pile too
        r(
            &mut st,
            json!({ "msg": "msgs", "messages": [{ "text": "Items here: <darkgrey>) [<lightgrey>.", "turn": 29814, "channel": Msgch::FLOOR_ITEMS }] }),
        );
        r(&mut st, json!({ "msg": "player", "pos": { "x": 26, "y": 14 }, "turn": 29814 }));
        assert!(items_underfoot(&st));
        for m in &rec[1..] {
            reduce(&mut st, m);
        }
        // the arrival: one gained slot (84; slot 79 changed without a quantity) and gold, so the pile line is a turn behind
        assert_eq!(st.player.turn, 29816);
        assert_eq!(st.player.arrived_turn, 29816);
        assert!(st.player.arrived_pickup);
        let f = st.player.floor_items.clone().unwrap();
        assert_eq!((f.turn, f.picked), (29815, 0));
        assert!(items_underfoot(&st));
        assert_eq!(floor_items_label(&st).as_deref(), Some("<lightgrey>a +0 morningstar"));
        // the manual pick-up that follows: the slot arrives, no floor line does
        r(&mut st, json!({ "msg": "player", "turn": 29817, "inv": { "85": { "base_type": 0, "quantity": 1, "name": "+0 morningstar" } } }));
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "<lightgrey>d - a +0 morningstar", "turn": 29816, "channel": 0 }] }));
        assert!(!items_underfoot(&st));
        // and the next step is not owed the one-turn allowance
        r(&mut st, json!({ "msg": "msgs", "messages": [{ "text": "You see here a +0 dagger.", "turn": 29817, "channel": Msgch::FLOOR_ITEMS }] }));
        r(&mut st, json!({ "msg": "player", "pos": { "x": 28, "y": 14 }, "turn": 29818 }));
        assert!(!st.player.arrived_pickup);
        assert!(!items_underfoot(&st));
    }
}

// ---- ui_cutoff hides what is up when it arrives, not what comes after

#[test]
fn leaves_the_description_look_mode_opens_visible() {
    // directn.cc choose_direction cutoff_point, game.js handle_set_ui_cutoff
    let mut st = initial_state();
    // `x` from the main screen: the stack is empty, so the cutoff is 0 and hides nothing that exists
    r(&mut st, json!({ "msg": "ui_cutoff", "cutoff": 0 }));
    assert_eq!(st.ui_cutoff, 0);
    // `v` on a monster: the popup pushed after the cutoff is shown
    r(&mut st, json!({ "msg": "ui-push", "type": "describe-monster", "title": "a goblin", "body": "b" }));
    assert!(!st.ui[0].hidden);
    r(&mut st, json!({ "msg": "ui-pop" }));
    r(&mut st, json!({ "msg": "ui_cutoff", "cutoff": -1 }));
    assert_eq!(st.ui_cutoff, -1);
}

#[test]
fn counts_menus_and_popups_on_the_one_stack_in_push_order() {
    let mut st = initial_state();
    // an inventory menu, then a cutoff pushed over it (adjust.cc adjust_item): the menu is hidden, index 0 <= 1
    r(&mut st, json!({ "msg": "menu", "tag": "inventory", "flags": 0, "title": { "text": "Inventory" }, "items": [] }));
    r(&mut st, json!({ "msg": "ui_cutoff", "cutoff": 1 }));
    assert!(st.menus[0].hidden);
    // the popup that follows is not
    r(&mut st, json!({ "msg": "ui-push", "type": "describe-item", "title": "t", "body": "b" }));
    assert!(!st.ui[0].hidden);
    // the pop restores the menu
    r(&mut st, json!({ "msg": "ui-pop" }));
    r(&mut st, json!({ "msg": "ui_cutoff", "cutoff": -1 }));
    assert!(!st.menus[0].hidden);
}

// ---- text areas and input

#[test]
fn keeps_text_areas_and_the_text_input() {
    let mut st = initial_state();
    r(&mut st, json!({ "msg": "txt", "id": "crt", "lines": { "0": "a", "2": "c" } }));
    r(&mut st, json!({ "msg": "txt", "id": "crt", "lines": { "2": "C" }, "clear": true }));
    assert_eq!(st.crt.lines.iter().map(|(k, v)| (*k, v.as_str())).collect::<Vec<_>>(), vec![(2, "C")]);
    assert_eq!(st.crt.area("crt"), Some(&st.crt.lines));
    r(&mut st, json!({ "msg": "txt", "id": "side", "lines": { "1": "x" } }));
    assert_eq!(st.crt.area("side").and_then(|a| a.get(&1)).map(String::as_str), Some("x"));
    r(&mut st, json!({ "msg": "clear" }));
    assert!(st.crt.lines.is_empty());
    r(&mut st, json!({ "msg": "init_input", "type": "generic", "prompt": "Name?", "prefill": "  bob ", "historyId": "h" }));
    let input = st.text_input.clone().unwrap();
    assert_eq!((input.kind.as_deref(), input.text.as_str(), input.history_id.as_deref()), (Some("generic"), "bob", Some("h")));
    r(&mut st, json!({ "msg": "update_input", "input_text": "bobby" }));
    assert_eq!(st.text_input.as_ref().map(|i| i.text.as_str()), Some("bobby"));
    r(&mut st, json!({ "msg": "close_input" }));
    assert_eq!(st.text_input, None);
}

// ---- set_game_links

/// The lobby's "Play now" line (`set_game_links`) carries the save waiting in each game once the
/// server has checked the slot: game_links.html rendered with the strings ws_handler.py
/// `update_save_info` puts in `save_info` (fixtures/set-game-links.json).
mod set_game_links {
    use super::*;

    #[test]
    fn reads_every_game_as_a_plain_link_before_the_saves_are_checked() {
        let fixture = load_json("set-game-links.json");
        let mut st = initial_state();
        reduce(&mut st, &fixture[0]);
        assert_eq!(
            st.lobby.games,
            vec![
                link("dcss-web-trunk", "DCSS trunk", "dcss-web-trunk"),
                link("seeded-web-trunk", "Seeded trunk", "dcss-web-trunk"),
                link("sprint-web-trunk", "Sprint trunk", "dcss-web-trunk"),
                link("tut-web-trunk", "Tutorial trunk", "dcss-web-trunk"),
                link("dcss-web-0.34", "DCSS 0.34", "dcss-web-0.34"),
                link("seeded-web-0.34", "Seeded 0.34", "dcss-web-0.34"),
                link("sprint-web-0.34", "Sprint 0.34", "dcss-web-0.34"),
                link("dcss-web-0.33", "DCSS 0.33", "dcss-web-0.33"),
            ]
        );
    }

    #[test]
    fn reads_the_save_behind_each_game_once_the_slots_are_checked() {
        let fixture = load_json("set-game-links.json");
        let mut st = initial_state();
        reduce(&mut st, &fixture[0]);
        reduce(&mut st, &fixture[1]);
        let by = |id: &str| st.lobby.games.iter().find(|g| g.id == id).cloned();
        let saved = |id: &str, label: &str, save: &str, rc: &str| GameLink { save: Some(save.into()), ..link(id, label, rc) };
        // a save to continue: the link moves from the name to the bracket text
        assert_eq!(
            by("dcss-web-trunk"),
            Some(saved("dcss-web-trunk", "DCSS trunk", "orbruntest, a level 3 Minotaur Berserker of Trog", "dcss-web-trunk"))
        );
        // a game whose slot another game type holds: greyed, no link; its id comes from nowhere, so it is not listed
        assert_eq!(by("seeded-web-trunk"), None);
        // a save another session has open: still linked
        assert_eq!(by("dcss-web-0.34"), Some(saved("dcss-web-0.34", "DCSS 0.34", "playing", "dcss-web-0.34")));
        assert_eq!(by("seeded-web-0.34"), Some(saved("seeded-web-0.34", "Seeded 0.34", "playing", "dcss-web-0.34")));
        // nothing saved: a plain link
        assert_eq!(by("dcss-web-0.33"), Some(link("dcss-web-0.33", "DCSS 0.33", "dcss-web-0.33")));
        assert_eq!(by("sprint-web-trunk"), Some(link("sprint-web-trunk", "Sprint trunk", "dcss-web-trunk")));
        // the rows the home screen shows keep the save with the game
        let rows = game_link_rows(&st.lobby.games);
        assert_eq!(
            rows.trunk.iter().map(|g| g.save.clone()).collect::<Vec<_>>(),
            vec![Some("orbruntest, a level 3 Minotaur Berserker of Trog".to_string())]
        );
        assert_eq!(rows.latest.iter().map(|g| g.save.clone()).collect::<Vec<_>>(), vec![Some("playing".to_string())]);
    }

    #[test]
    fn reads_cdis_own_page() {
        // plain links for every game, a nested 'latest version:' span that is no save, and the older versions folded
        // in a details block; recorded logged in on 2026-09-09 with a 0.34 save waiting: CDI does not publish save info
        let cdi = load_json("set-game-links-cdi.json");
        let mut st = initial_state();
        reduce(&mut st, &cdi[0]);
        assert!(!st.lobby.games.iter().any(|g| g.save.is_some() || g.disabled));
        assert_eq!(
            st.lobby.games[..9].to_vec(),
            vec![
                link("dcss-0.34", "DCSS 0.34", "dcss-0.34"),
                link("seeded-0.34", "Custom seed 0.34", "dcss-0.34"),
                link("spr-0.34", "Sprint 0.34", "dcss-0.34"),
                link("tut-0.34", "Tutorial 0.34", "dcss-0.34"),
                link("dcss-git", "DCSS trunk", "dcss-git"),
                link("seeded-git", "Custom seed trunk", "dcss-git"),
                link("spr-git", "Sprint trunk", "dcss-git"),
                link("tut-git", "Tutorial trunk", "dcss-git"),
                link("dcss-0.33", "DCSS 0.33", "dcss-0.33"),
            ]
        );
        assert_eq!(st.lobby.games.len(), 24);
        assert_eq!(st.lobby.games.last(), Some(&link("tut-0.30", "Tutorial 0.30", "dcss-0.30")));
        let rows = game_link_rows(&st.lobby.games);
        assert_eq!(rows.latest_version.as_deref(), Some("0.34"));
        assert_eq!(ids(&rows.latest), vec!["dcss-0.34"]);
        assert_eq!(ids(&rows.trunk), vec!["dcss-git"]);
        assert_eq!(rows.other, vec![]);
    }

    #[test]
    fn groups_cdis_links_one_row_per_release_trunk_first() {
        let cdi = load_json("set-game-links-cdi.json");
        let mut st = initial_state();
        reduce(&mut st, &cdi[0]);
        let rows = game_version_rows(&st.lobby.games);
        assert_eq!(
            rows.iter().map(|r| (r.version.clone().unwrap(), r.game.id.as_str())).collect::<Vec<_>>(),
            vec![
                ("trunk".to_string(), "dcss-git"),
                ("0.34".to_string(), "dcss-0.34"),
                ("0.33".to_string(), "dcss-0.33"),
                ("0.32".to_string(), "dcss-0.32"),
                ("0.31".to_string(), "dcss-0.31"),
                ("0.30".to_string(), "dcss-0.30"),
            ]
        );
        assert_eq!(ids(&rows[0].modes), vec!["seeded-git", "spr-git", "tut-git"]);
    }

    #[test]
    fn leads_a_release_with_no_plain_game_with_its_first_mode_and_leaves_out_a_held_slot() {
        let rows = game_version_rows(&[
            GameLink::new("dcss-0.34", "DCSS 0.34"),
            GameLink::new("zd-0.17", "Zot Defence 0.17"),
            GameLink { save: Some("slot full".into()), disabled: true, ..GameLink::new("dcss-0.33", "DCSS 0.33") },
            GameLink::new("spr-0.33", "Sprint 0.33"),
        ]);
        assert_eq!(
            rows.iter().map(|r| (r.game.id.as_str(), ids(&r.modes))).collect::<Vec<_>>(),
            vec![("dcss-0.34", vec![]), ("spr-0.33", vec![]), ("zd-0.17", vec![])]
        );
    }

    #[test]
    fn reads_the_names_past_cdis_separators_entities_and_all() {
        let fixture = load_json("set-game-links.json");
        let mut st = initial_state();
        reduce(&mut st, &fixture[2]);
        assert_eq!(
            st.lobby.games,
            vec![
                link("dcss-web-0.34", "DCSS 0.34", "dcss-web-0.34"),
                link("seeded-web-0.34", "Seeded 0.34", "dcss-web-0.34"),
                link("sprint-web-0.34", "Sprint 0.34", "dcss-web-0.34"),
                GameLink {
                    save: Some("orbrun, a level 9 Gnoll Fighter of Okawaru".into()),
                    ..link("dcss-web-trunk", "DCSS trunk", "dcss-web-trunk")
                },
                link("sprint-web-trunk", "Sprint trunk", "dcss-web-trunk"),
                link("dcss-web-0.33", "DCSS 0.33", "dcss-web-0.33"),
            ]
        );
        let rows = game_link_rows(&st.lobby.games);
        assert_eq!(rows.latest_version.as_deref(), Some("0.34"));
        assert_eq!(rows.latest.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), vec!["DCSS 0.34"]);
        assert_eq!(
            rows.trunk.iter().map(|g| g.save.clone()).collect::<Vec<_>>(),
            vec![Some("orbrun, a level 9 Gnoll Fighter of Okawaru".to_string())]
        );
    }

    #[test]
    fn keeps_a_slot_full_game_whose_id_the_edit_rc_link_carries_unlinked() {
        let mut st = initial_state();
        r(
            &mut st,
            json!({
                "msg": "set_game_links",
                "content": "<span class=\"fg7\"><br>DCSS trunk<span>[slot full]</span><a href=\"javascript:\" class=\"edit_rc_link\" data-game_id=\"dcss-web-trunk\">(edit rc)</a></span>",
            }),
        );
        assert_eq!(
            st.lobby.games,
            vec![GameLink { save: Some("slot full".into()), disabled: true, ..link("dcss-web-trunk", "DCSS trunk", "dcss-web-trunk") }]
        );
    }

    #[test]
    fn notes_a_malformed_escape_and_keeps_the_old_links() {
        let mut st = initial_state();
        r(&mut st, json!({ "msg": "set_game_links", "content": "<span><a href=\"#play-a\">A 0.1</a></span>" }));
        r(&mut st, json!({ "msg": "set_game_links", "content": "<span><a href=\"#play-%zz\">B</a></span>" }));
        assert_eq!(ids(&st.lobby.games), vec!["a"]);
        assert!(st.diagnostics.last().unwrap().text.starts_with("reducer error"));
    }
}

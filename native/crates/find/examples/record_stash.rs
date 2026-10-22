//! Records crawl's stash search results (Ctrl-F answered with `..`, everything the stash tracker
//! knows) from an offline game into a menu fixture, as `tools/record/menus.mjs` records a
//! server's: logs in as a profile, picks a character if crawl asks, explores and fights its way
//! down a few levels, then searches and writes the `menu` message.
//!
//!     ISORUN_DATA_DIR=/tmp/rec cargo run -p isorun-find --example record_stash [profile] [out.json]
//!
//! `RECORD_LEVELS` (default 3): how many levels to explore before the search.

use isorun_client::engine;
use isorun_client::platform::net::ConnEvent;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let profile = args.next().unwrap_or_else(|| "FindRec".into());
    let out = args.next().unwrap_or_else(|| "menu-stash-1.json".into());
    let levels: usize = std::env::var("RECORD_LEVELS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    if !engine::available() {
        eprintln!("no offline engine");
        std::process::exit(2);
    }
    // the keys of one level: explore, fight what stops it, again; then down the stairs
    let mut script: Vec<Value> = Vec::new();
    for level in 0..levels {
        for _ in 0..30 {
            script.push(json!({ "msg": "input", "text": "o" }));
            script.push(json!({ "msg": "key", "keycode": 9 }));
            script.push(json!({ "msg": "key", "keycode": 27 }));
        }
        if level + 1 < levels {
            // travel down a level: G, then > (one deeper), then the travel itself
            script.push(json!({ "msg": "input", "text": "G" }));
            script.push(json!({ "msg": "input", "text": ">" }));
            script.push(json!({ "msg": "key", "keycode": 13 }));
            script.push(json!({ "msg": "key", "keycode": 27 }));
            script.push(json!({ "msg": "input", "text": ">" }));
        }
    }
    let mut conn = engine::connect(Arc::new(|| {}));
    let send = |conn: &mut Box<dyn isorun_client::platform::net::Transport>, m: Value| conn.send(m.to_string());
    let mut phase = 0;
    let mut at = 0;
    let mut next = Instant::now();
    let mut version = String::new();
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if Instant::now() > deadline {
            eprintln!("timed out in phase {phase}");
            std::process::exit(1);
        }
        events.clear();
        conn.poll(&mut events);
        for e in events.drain(..) {
            let m = match e {
                ConnEvent::Open => {
                    send(&mut conn, json!({ "msg": "login", "username": profile, "password": "" }));
                    continue;
                }
                ConnEvent::Closed { code, reason } => {
                    eprintln!("closed {code} {reason}");
                    std::process::exit(1);
                }
                ConnEvent::Diagnostic(_) => continue,
                ConnEvent::Message(m) => m,
            };
            let kind = m.get("msg").and_then(Value::as_str).unwrap_or("").to_string();
            match kind.as_str() {
                "set_game_links" if phase == 0 => {
                    let content = m["content"].as_str().unwrap_or("");
                    let id = content.split("#play-").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
                    send(&mut conn, json!({ "msg": "play", "game_id": id }));
                    phase = 1;
                }
                "game_client" => version = format!("Dungeon Crawl Stone Soup {}", m["version"].as_str().unwrap_or("")),
                "version" => version = m["text"].as_str().unwrap_or(&version).to_string(),
                // a fighter: the first species, then b (fighter), then the first weapon
                "ui-state" if phase == 1 && m["type"] == "newgame-choice" => {
                    let step = m["title"].as_str().unwrap_or("");
                    let k = if step.contains("background") { "b" } else { "a" };
                    send(&mut conn, json!({ "msg": "input", "text": k }));
                }
                "map" if phase == 1 => {
                    phase = 2;
                    next = Instant::now() + Duration::from_millis(500);
                }
                "init_input" if phase == 3 && m["tag"] == "stash_search" => {
                    send(&mut conn, json!({ "msg": "key", "keycode": 21 }));
                    send(&mut conn, json!({ "msg": "key", "keycode": 11 }));
                    send(&mut conn, json!({ "msg": "text_input", "text": "..\r" }));
                }
                "menu" if phase == 3 && m["tag"] == "stash" => {
                    let fixture = json!({ "server": "offline", "version": version, "player": profile, "tag": "menu-stash", "msgs": [m] });
                    std::fs::write(&out, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
                    println!("wrote {out}: {} items", m["items"].as_array().map_or(0, |a| a.len()));
                    send(&mut conn, json!({ "msg": "key", "keycode": 27 }));
                    send(&mut conn, json!({ "msg": "go_lobby" }));
                    phase = 4;
                }
                "msgs" if phase == 3 => {
                    for l in m["messages"].as_array().into_iter().flatten() {
                        println!("  {}", l["text"].as_str().unwrap_or(""));
                    }
                }
                "game_ended" => {
                    if phase == 4 {
                        return;
                    }
                    eprintln!("the game ended in phase {phase}: {m}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }
        if phase == 2 && Instant::now() >= next {
            if let Some(k) = script.get(at) {
                send(&mut conn, k.clone());
                at += 1;
                // explore and travel take their time
                let wait = if k["text"] == "o" || k["text"] == ">" || k["keycode"] == 13 { 900 } else { 250 };
                next = Instant::now() + Duration::from_millis(wait);
            } else {
                send(&mut conn, json!({ "msg": "key", "keycode": 27 }));
                send(&mut conn, json!({ "msg": "key", "keycode": 6 }));
                phase = 3;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

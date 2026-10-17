//! Plays the offline engine with no UI, as a check that it runs on this machine: logs in as a
//! profile, starts its game, picks a character if crawl asks (the first species, background and weapon), waits
//! for the first map, then leaves and checks that the game was saved.
//!
//!     ISORUN_DATA_DIR=/tmp/smoke cargo run -p isorun-client --example offline_smoke [profile]
//!
//! `ISORUN_ENGINE_DIR` picks an engine folder (engine/native.sh's), as the app does.

use isorun_client::engine;
use isorun_client::platform::net::ConnEvent;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let profile = std::env::args().nth(1).unwrap_or_else(|| "Smoke".into());
    if !engine::available() {
        eprintln!("no offline engine");
        std::process::exit(2);
    }
    let start = Instant::now();
    let mut conn = engine::connect(Arc::new(|| {}));
    let send = |conn: &mut Box<dyn isorun_client::platform::net::Transport>, m: Value| conn.send(m.to_string());
    let mut phase = 0;
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if Instant::now() > deadline {
            eprintln!("timed out in phase {phase}");
            std::process::exit(1);
        }
        events.clear();
        conn.poll(&mut events);
        for e in events.drain(..) {
            match e {
                ConnEvent::Open => send(&mut conn, json!({ "msg": "login", "username": profile, "password": "" })),
                ConnEvent::Diagnostic(d) => eprintln!("diagnostic: {d}"),
                ConnEvent::Closed { code, reason } => {
                    eprintln!("closed {code} {reason}");
                    std::process::exit(1);
                }
                ConnEvent::Message(m) => {
                    let kind = m.get("msg").and_then(Value::as_str).unwrap_or("").to_string();
                    if std::env::var_os("SMOKE_VERBOSE").is_some() {
                        let text = m.to_string();
                        println!("{:>6} ms  < {}", start.elapsed().as_millis(), text.chars().take(160).collect::<String>());
                    }
                    match kind.as_str() {
                        "set_game_links" if phase == 0 => {
                            let content = m["content"].as_str().unwrap_or("");
                            println!("{:>6} ms  game links: {content}", start.elapsed().as_millis());
                            let id = content.split("#play-").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
                            send(&mut conn, json!({ "msg": "play", "game_id": id }));
                            phase = 1;
                        }
                        "game_client" => println!("{:>6} ms  game_client {}", start.elapsed().as_millis(), m["version"]),
                        // a new profile is at crawl's character choice: the first of each list
                        "ui-state" if phase == 1 && m["type"] == "newgame-choice" => send(&mut conn, json!({ "msg": "input", "text": "a" })),
                        "map" if phase == 1 => {
                            println!("{:>6} ms  first map", start.elapsed().as_millis());
                            phase = 2;
                            send(&mut conn, json!({ "msg": "go_lobby" }));
                        }
                        "game_ended" => {
                            println!("{:>6} ms  game_ended {m}", start.elapsed().as_millis());
                            let reason = m["reason"].as_str().unwrap_or("");
                            if phase == 2 && (reason == "saved" || reason == "disconnect") {
                                println!("ok");
                                return;
                            }
                            eprintln!("the game ended in phase {phase}: {m}");
                            std::process::exit(1);
                        }
                        _ => {}
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

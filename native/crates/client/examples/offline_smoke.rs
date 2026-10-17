//! Plays the offline engine with no UI, as a check that it runs on this machine: logs in as a
//! profile, starts its game, picks a character if crawl asks (the first species, background and weapon), waits
//! for the first map, plays a few turns (steps, explore, the inventory and character screens, a
//! rest), then leaves and checks that the game was saved. A crash prints the engine's report
//! (its signal, its error output, crawl's crash file), as the app logs it.
//!
//!     ISORUN_DATA_DIR=/tmp/smoke cargo run -p isorun-client --example offline_smoke [profile]
//!
//! `ISORUN_ENGINE_DIR` picks an engine folder (engine/native.sh's), as the app does. The Android
//! CI runs it on an emulator, beside crawl as `libcrawl.so` (isorun-native-android.yml).

use isorun_client::engine;
use isorun_client::platform::net::ConnEvent;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The keys played after the first map, one every [`STEP`]: steps, explore, the inventory and the
/// character screen, a search for a rest, Escape after each screen.
const PLAY: &[&str] = &["h", "l", "j", "k", "o", "\x1b", "o", "\x1b", "i", "\x1b", "@", "\x1b", "s", "s", "5", "\x1b", "o", "\x1b"];
const STEP: Duration = Duration::from_millis(400);

/// The engine's log lines (its crash reports among them) on stderr.
struct Stderr;
impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}: {}", r.level(), r.target(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Info));
    let profile = std::env::args().nth(1).unwrap_or_else(|| "Smoke".into());
    if !engine::available() {
        eprintln!("no offline engine");
        std::process::exit(2);
    }
    let start = Instant::now();
    let mut conn = engine::connect(Arc::new(|| {}));
    let send = |conn: &mut Box<dyn isorun_client::platform::net::Transport>, m: Value| conn.send(m.to_string());
    let mut phase = 0;
    let mut played = 0;
    let mut next_key = Instant::now();
    let mut events = Vec::new();
    // SMOKE_SECONDS: more time for a slow machine (an emulator's first start builds crawl's caches)
    let seconds = std::env::var("SMOKE_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    let deadline = Instant::now() + Duration::from_secs(seconds);
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
                            next_key = Instant::now() + STEP;
                        }
                        "game_ended" => {
                            println!("{:>6} ms  game_ended {m}", start.elapsed().as_millis());
                            let reason = m["reason"].as_str().unwrap_or("");
                            if phase == 3 && (reason == "saved" || reason == "disconnect") {
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
        if phase == 2 && Instant::now() >= next_key {
            if let Some(k) = PLAY.get(played) {
                send(&mut conn, json!({ "msg": "input", "text": k }));
                played += 1;
                next_key = Instant::now() + STEP;
            } else {
                println!("{:>6} ms  played {played} keys; leaving", start.elapsed().as_millis());
                phase = 3;
                send(&mut conn, json!({ "msg": "go_lobby" }));
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

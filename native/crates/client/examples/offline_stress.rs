//! Plays the offline engine hard, with no UI, and times what the app's frame does with it: a
//! fighter that explores and fights (crawl's `o` and Tab) for a while, through a `Session` as
//! the game screen holds one (the messages reduced, the scene built once a "frame"). Every
//! window of [`WINDOW`] prints the turns played, the messages and bytes that came, and the
//! longest and the 99th percentile "frame" (the session's poll and its scene) and the longest
//! send: a frame or a send that grows over the minutes, or one that stalls for seconds, is what an
//! Android phone shows as "app is not responding".
//!
//!     ISORUN_DATA_DIR=/tmp/stress cargo run --release -p isorun-client --example offline_stress [seconds] [key gap ms]
//!
//! A key gap under crawl's pace (`20`) sends faster than crawl reads, as a held key or a quick
//! thumb does. It exits non-zero when a send or a frame took longer than [`STALL`].

use isorun_client::servers::offline_server;
use isorun_client::session::{Session, SessionEvent};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(20);
/// A frame or a send this long fails the run: a phone's ANR comes at five seconds.
const STALL: Duration = Duration::from_millis(500);
/// The keys played in turn: fight, explore, a downstairs when the level is done, Escape for
/// whatever asked something.
const PLAY: &[&str] = &["\t", "\t", "o", "\t", "o", "\x1b", "G", ">", "\r", ">", "\x1b"];

/// The engine's log lines (a failed send, a crash report) on stderr.
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(180);
    let gap = Duration::from_millis(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(60));
    if !isorun_client::engine::available() {
        eprintln!("no offline engine (engine/native.sh stable)");
        std::process::exit(2);
    }
    let profile = format!("Stress{}", std::process::id() % 10000);
    let mut s = Session::new(offline_server(), Some(profile), Arc::new(|| {}));
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    let mut playing = false;
    let mut next_key = Instant::now();
    let mut played = 0usize;
    let mut window = Window::new();
    let mut worst = Duration::ZERO;
    let mut games = 0;
    while Instant::now() < end {
        let t0 = Instant::now();
        let events = s.poll();
        s.flush_scene();
        let frame = t0.elapsed();
        window.frames.push(frame);
        for e in events {
            match e {
                SessionEvent::State(m) => {
                    window.messages += 1;
                    window.bytes += m.to_string().len();
                    if std::env::var_os("STRESS_VERBOSE").is_some() {
                        let text = m.to_string();
                        println!("{:>7} ms < {}", start.elapsed().as_millis(), text.chars().take(200).collect::<String>());
                    }
                    let kind = m.get("msg").and_then(Value::as_str).unwrap_or("");
                    match kind {
                        "set_game_links" if !playing => {
                            let content = m["content"].as_str().unwrap_or("");
                            let id = content.split("#play-").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
                            timed_send(&mut s, json!({ "msg": "play", "game_id": id }), &mut window);
                            games += 1;
                        }
                        // a level's attribute choice takes no other key
                        "msgs" if m.to_string().contains("Increase (S)trength") => timed_send(&mut s, json!({ "msg": "input", "text": "S" }), &mut window),
                        "ui-state" if m["type"] == "newgame-choice" => timed_send(&mut s, json!({ "msg": "input", "text": "a" }), &mut window),
                        "map" if !playing => {
                            playing = true;
                            println!("{:>5}s  game {games} drew its first map", start.elapsed().as_secs());
                        }
                        "game_ended" | "go_lobby" => playing = false,
                        _ => {}
                    }
                }
                SessionEvent::Closed(why) => {
                    eprintln!("closed: {why}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }
        if playing && Instant::now() >= next_key {
            let k = PLAY[played % PLAY.len()];
            played += 1;
            timed_send(&mut s, json!({ "msg": "input", "text": k }), &mut window);
            next_key = Instant::now() + gap;
        }
        if window.since.elapsed() >= WINDOW {
            worst = worst.max(window.report(start, s.state.player.turn, &s.state.player.place, s.state.player.depth));
            window = Window::new();
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    worst = worst.max(window.report(start, s.state.player.turn, &s.state.player.place, s.state.player.depth));
    s.close();
    for d in s.diagnostics.iter().take(20) {
        println!("diagnostic: {d}");
    }
    if worst > STALL {
        eprintln!("a frame or a send took {worst:?}");
        std::process::exit(1);
    }
    println!("ok: no frame or send over {STALL:?}");
}

fn timed_send(s: &mut Session, m: Value, w: &mut Window) {
    let t = Instant::now();
    s.send(m);
    w.sends.push(t.elapsed());
}

struct Window {
    since: Instant,
    frames: Vec<Duration>,
    sends: Vec<Duration>,
    messages: usize,
    bytes: usize,
}

impl Window {
    fn new() -> Self {
        Window { since: Instant::now(), frames: Vec::new(), sends: Vec::new(), messages: 0, bytes: 0 }
    }

    /// Print the window; returns its longest frame or send.
    fn report(&mut self, start: Instant, turn: i64, place: &str, depth: i64) -> Duration {
        self.frames.sort();
        self.sends.sort();
        let p99 = |v: &[Duration]| v.get(v.len().saturating_sub(1) * 99 / 100).copied().unwrap_or_default();
        let max = |v: &[Duration]| v.last().copied().unwrap_or_default();
        println!(
            "{:>5}s  turn {turn:>5} {place}:{depth}  {:>5} msgs {:>8} B  frame p99 {:>7.2?} max {:>8.2?}  send p99 {:>7.2?} max {:>8.2?} ({} sends)",
            start.elapsed().as_secs(),
            self.messages,
            self.bytes,
            p99(&self.frames),
            max(&self.frames),
            p99(&self.sends),
            max(&self.sends),
            self.sends.len()
        );
        max(&self.frames).max(max(&self.sends))
    }
}

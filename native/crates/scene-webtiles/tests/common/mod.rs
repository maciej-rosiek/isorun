#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use isorun_gamedata::Gamedata;
use isorun_webtiles::{initial_state, reduce, GameState};
use once_cell::sync::Lazy;
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The checked-in copy of one gamedata version's scripts (native/fixtures).
pub fn fixture_dir() -> PathBuf {
    root().join("native/fixtures")
}

static GD: Lazy<Gamedata> = Lazy::new(|| {
    let gd_root = fixture_dir().join("gamedata");
    let version = std::fs::read_dir(&gd_root).unwrap().next().unwrap().unwrap().file_name().into_string().unwrap();
    let files: HashMap<String, String> = isorun_gamedata::script_files()
        .into_iter()
        .filter_map(|f| std::fs::read_to_string(gd_root.join(&version).join(f)).ok().map(|t| (f.to_string(), t)))
        .collect();
    Gamedata::from_files(&files, &version).unwrap()
});

pub fn gd() -> &'static Gamedata {
    &GD
}

static MESSAGES: Lazy<Vec<Value>> = Lazy::new(|| {
    let text = std::fs::read_to_string(root().join("native/fixtures/cdi-0.34-watch.ndjson")).unwrap();
    text.trim().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["m"].clone()).collect()
});

/// The recorded session's messages (a CDI 0.34 game watched from the start).
pub fn messages() -> &'static [Value] {
    &MESSAGES
}

pub fn msg_type(m: &Value) -> &str {
    m["msg"].as_str().unwrap_or("")
}

/// The whole recording reduced.
pub fn replayed() -> GameState {
    let mut st = initial_state();
    for m in messages() {
        reduce(&mut st, m);
    }
    st
}

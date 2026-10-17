#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

/// The recorded scripts of one server version, as scene-webtiles' fixtures hold them.
pub const FIXTURE_SHA: &str = "acd3d60e20f899c1c8a546953d6ffa0f6c7fe0c8";

/// The gamedata the offline engine build publishes (`engine/dist`, not checked in).
pub const ENGINE_SHA: &str = "0e95e087e2d704bb8836c00c6eb7a3bd9a11c6f0";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn fixture_dir() -> PathBuf {
    repo()
        .join("packages/scene-webtiles/test/fixtures/gamedata")
        .join(FIXTURE_SHA)
}

pub fn engine_dir() -> PathBuf {
    repo().join("engine/dist/gamedata").join(ENGINE_SHA)
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_dir().join(name))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// Every script of a gamedata directory, by name.
pub fn read_dir(dir: &std::path::Path) -> HashMap<String, String> {
    isorun_gamedata::script_files()
        .into_iter()
        .filter_map(|f| {
            std::fs::read_to_string(dir.join(f))
                .ok()
                .map(|t| (f.to_string(), t))
        })
        .collect()
}

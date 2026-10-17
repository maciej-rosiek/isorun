//! The offline engine: crawl in this process's reach, with no server. Natively crawl runs as a
//! child process and speaks its own WebTiles socket protocol; on the web it is the wasm build.
//! Until a build carries it, Offline is not offered.

use crate::platform::net::{ConnEvent, Transport};
use std::sync::Arc;

/// Whether this build can play offline.
pub fn available() -> bool {
    false
}

/// Where the offline engine's gamedata is read from.
pub fn gamedata_base() -> String {
    String::new()
}

/// One of the offline engine's gamedata files.
pub fn gamedata_file(version: &str, file: &str) -> Result<Vec<u8>, String> {
    Err(format!("no offline engine for {version}/{file}"))
}

/// A connection to the offline engine.
pub fn connect(_wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn Transport> {
    Box::new(Missing { told: false })
}

struct Missing {
    told: bool,
}

impl Transport for Missing {
    fn send(&mut self, _text: String) {}
    fn close(&mut self) {}
    fn poll(&mut self, out: &mut Vec<ConnEvent>) {
        if !self.told {
            self.told = true;
            out.push(ConnEvent::Closed { code: 1011, reason: "this build has no offline engine".into() });
        }
    }
    fn reopen(&mut self) {
        self.told = false;
    }
}

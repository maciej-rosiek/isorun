//! A server's round trip for the server list (the port of `ping.ts`). WebTiles has no ping a
//! client can ask for, so this times a request to the server's web root: one to open the
//! connection, then the best of two over it, so DNS and TLS are not counted. Kept a minute, so
//! redraws of the list reuse it; a server that has not answered within four seconds gets no figure.

use isorun_client::servers::ServerInfo;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const FRESH_SECONDS: f64 = 60.0;
const TIMEOUT_SECONDS: f64 = 4.0;

type Slot = Arc<Mutex<Option<Option<u32>>>>;

#[derive(Default)]
pub struct Pings {
    readings: HashMap<String, (f64, Slot)>,
}

impl Pings {
    /// The server's round trip in milliseconds once it is known; asks for it when it is not.
    pub fn get(&mut self, sv: &ServerInfo, wake: Arc<dyn Fn() + Send + Sync>) -> Option<u32> {
        if sv.offline || sv.http.is_empty() {
            return None;
        }
        let now = isorun_client::platform::seconds();
        if let Some((at, slot)) = self.readings.get(&sv.id) {
            let v = *slot.lock().unwrap();
            // a failed reading is asked again the next time the list opens, not held for a minute
            let failed = v == Some(None);
            if now - at < FRESH_SECONDS && !failed {
                return v.flatten();
            }
            if failed && now - at < 5.0 {
                return None;
            }
        }
        let slot: Slot = Arc::new(Mutex::new(None));
        self.readings.insert(sv.id.clone(), (now, slot.clone()));
        let url = format!("{}/", sv.http.trim_end_matches('/'));
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::thread::Builder::new().name("ping".into()).spawn(move || {
            let start = std::time::Instant::now();
            let once = || {
                let t = std::time::Instant::now();
                isorun_client::platform::http::fetch_blocking(&url).ok().map(|_| t.elapsed().as_secs_f64() * 1000.0)
            };
            let mut best = None;
            if once().is_some() {
                for _ in 0..2 {
                    if let Some(ms) = once() {
                        best = Some(best.map_or(ms, |b: f64| b.min(ms)));
                    }
                }
            }
            let ms = best.filter(|_| start.elapsed().as_secs_f64() < TIMEOUT_SECONDS * 3.0).map(|m| m.round() as u32);
            *slot.lock().unwrap() = Some(ms);
            wake();
        });
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (url, wake);
            *slot.lock().unwrap() = Some(None);
        }
        None
    }
}

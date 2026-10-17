//! What differs between the desktop, Android and the web: where settings are kept, how a socket
//! and an HTTP request are made, the clock.

pub mod http;
pub mod net;
pub mod storage;

/// Milliseconds since the Unix epoch, as `Date.now()`.
pub fn now_ms() -> f64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// Seconds on a monotonic clock.
pub fn seconds() -> f64 {
    isorun_render::now_seconds()
}

/// Whether this build is the Android app.
pub fn android() -> bool {
    cfg!(target_os = "android")
}

/// Whether this build runs in a browser.
pub fn web() -> bool {
    cfg!(target_arch = "wasm32")
}

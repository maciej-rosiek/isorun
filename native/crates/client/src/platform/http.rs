//! HTTP GETs that answer later: on worker threads natively, with `fetch` on the web. The caller
//! polls (`Request::take`) from its frame, as the rest of the app runs.

use std::sync::{Arc, Mutex};

pub type Body = Result<Vec<u8>, String>;

/// A request on its way: `take` hands its body over once it has come.
#[derive(Clone)]
pub struct Request {
    slot: Arc<Mutex<Option<Body>>>,
}

impl Request {
    pub fn take(&self) -> Option<Body> {
        self.slot.lock().ok()?.take()
    }

    pub fn done(&self) -> bool {
        self.slot.lock().map(|s| s.is_some()).unwrap_or(true)
    }
}

/// Start a GET of `url`; `wake` is called when it lands (to ask for a frame).
pub fn get(url: &str, wake: impl Fn() + Send + Sync + 'static) -> Request {
    let slot = Arc::new(Mutex::new(None));
    let request = Request { slot: slot.clone() };
    let url = url.to_string();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = std::thread::Builder::new().name("http".into()).spawn(move || {
            let body = fetch_blocking(&url);
            if let Ok(mut s) = slot.lock() {
                *s = Some(body);
            }
            wake();
        });
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            let body = fetch_web(&url).await;
            if let Ok(mut s) = slot.lock() {
                *s = Some(body);
            }
            wake();
        });
    }
    request
}

/// Send a beacon: an empty POST to `url` whose answer nobody reads (the visit counts,
/// `visits.rs`). `navigator.sendBeacon` on the web, which the browser sends even while the page
/// is closing; a worker thread elsewhere.
pub fn beacon(url: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let url = url.to_string();
        let _ = std::thread::Builder::new().name("beacon".into()).spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(20))).build().into();
            let _ = agent.post(&url).send_empty();
        });
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().send_beacon(url);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_blocking(url: &str) -> Body {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(60))).build().into();
    let mut response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    response.body_mut().with_config().limit(64 * 1024 * 1024).read_to_vec().map_err(|e| format!("{url}: {e}"))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_web(url: &str) -> Body {
    use wasm_bindgen::JsCast;
    let window = web_sys::window().ok_or("no window")?;
    let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await.map_err(|e| format!("{url}: {e:?}"))?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| "not a response".to_string())?;
    if !response.ok() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

//! The web's side (wasm32): the files are fetched from the page's `audio/` folder, as the
//! TypeScript does, and the long ones (the beds, the music) play from `<audio>` elements, read
//! as they play, since kira cannot stream without threads.

use std::sync::mpsc::Sender;

use kira::sound::static_sound::StaticSoundData;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::mixer::Voice;

/// Fetch and decode each file (`sfx/step1`) from `base`, and send it to the engine.
pub(crate) fn fetch_all(base: String, paths: Vec<String>, tx: Sender<(String, StaticSoundData)>) {
    wasm_bindgen_futures::spawn_local(async move {
        for path in paths {
            match fetch(&format!("{base}{path}.ogg")).await {
                Some(bytes) => match StaticSoundData::from_cursor(std::io::Cursor::new(bytes)) {
                    Ok(data) => {
                        if tx.send((path, data)).is_err() {
                            return;
                        }
                    }
                    Err(e) => log::warn!("cannot decode {path}: {e}"),
                },
                None => log::warn!("cannot fetch {path}"),
            }
        }
    });
}

async fn fetch(url: &str) -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url)).await.ok()?.dyn_into().ok()?;
    if !response.ok() {
        return None;
    }
    let buffer = JsFuture::from(response.array_buffer().ok()?).await.ok()?;
    Some(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// A bed or a track in an `<audio>` element: its volume is the fade times the master and its bus.
pub(crate) struct MediaVoice {
    element: web_sys::HtmlAudioElement,
    last: f32,
}

impl MediaVoice {
    pub fn start(url: &str, looped: bool) -> Option<MediaVoice> {
        let element = web_sys::HtmlAudioElement::new_with_src(url).ok()?;
        element.set_preload("auto");
        element.set_loop(looped);
        element.set_volume(0.0);
        let _ = element.play();
        Some(MediaVoice { element, last: 0.0 })
    }
}

impl Voice for MediaVoice {
    fn set_gain(&mut self, gain: f32, bus: f32) {
        let v = (gain * bus).clamp(0.0, 1.0);
        if (v - self.last).abs() > 0.0005 {
            self.last = v;
            self.element.set_volume(v as f64);
        }
    }

    fn finished(&self) -> bool {
        self.element.ended()
    }

    fn stop(&mut self) {
        let _ = self.element.pause();
        self.element.set_src("");
    }

    fn set_paused(&mut self, paused: bool) {
        if paused {
            let _ = self.element.pause();
        } else {
            let _ = self.element.play();
        }
    }

    fn duration(&self) -> Option<f64> {
        // The element says when it has ended.
        None
    }
}

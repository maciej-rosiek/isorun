//! A small key-value store, as the web build's `localStorage`: the browser's own on the web, a
//! JSON file in the app's data folder elsewhere. Values are JSON text, as the web build keeps them,
//! so the keys and their shapes stay the same (`isorun.settings`, `isorun.accounts`, ...).

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

struct Store {
    values: BTreeMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    path: Option<std::path::PathBuf>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::open()))
}

/// Where the app keeps its files on this device (the store, the offline engine's saves).
#[cfg(not(target_arch = "wasm32"))]
pub fn data_dir() -> Option<std::path::PathBuf> {
    static DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("ISORUN_DATA_DIR") {
            return Some(d.into());
        }
        #[cfg(target_os = "android")]
        {
            return ANDROID_DIR.get().cloned();
        }
        #[cfg(not(target_os = "android"))]
        native_data_dir()
    })
    .clone()
}

/// The platform's folder for an app's data: `$XDG_DATA_HOME/isorun` (or `~/.local/share/isorun`) on
/// Linux, `%APPDATA%\\Isorun` on Windows, `~/Library/Application Support/Isorun` on macOS.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn native_data_dir() -> Option<std::path::PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    if cfg!(windows) {
        return env("APPDATA").map(|d| d.join("Isorun"));
    }
    if cfg!(target_os = "macos") {
        return env("HOME").map(|h| h.join("Library/Application Support/Isorun"));
    }
    env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share"))).map(|d| d.join("isorun"))
}

#[cfg(target_os = "android")]
pub static ANDROID_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

impl Store {
    #[cfg(not(target_arch = "wasm32"))]
    fn open() -> Store {
        let path = data_dir().map(|d| d.join("storage.json"));
        let values = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Store { values, path }
    }

    #[cfg(target_arch = "wasm32")]
    fn open() -> Store {
        Store { values: BTreeMap::new() }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn flush(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // written beside and renamed over, so a crash never leaves half a file
        let tmp = path.with_extension("json.tmp");
        if let Ok(text) = serde_json::to_string_pretty(&self.values) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// The raw text under `key`, or none.
pub fn get_raw(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        return local_storage().and_then(|s| s.get_item(key).ok().flatten());
    }
    #[cfg(not(target_arch = "wasm32"))]
    store().lock().ok()?.values.get(key).cloned()
}

/// Keep `value` under `key`, or forget the key (`None`).
pub fn set_raw(key: &str, value: Option<&str>) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(s) = local_storage() {
            let _ = match value {
                Some(v) => s.set_item(key, v),
                None => s.remove_item(key),
            };
        }
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(mut s) = store().lock() {
        match value {
            Some(v) => s.values.insert(key.to_string(), v.to_string()),
            None => s.values.remove(key),
        };
        s.flush();
    }
}

/// The value under `key` read as JSON, or `fallback` when missing or unreadable.
pub fn load<T: DeserializeOwned>(key: &str, fallback: T) -> T {
    get_raw(key).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(fallback)
}

pub fn save<T: Serialize>(key: &str, value: &T) {
    if let Ok(text) = serde_json::to_string(value) {
        set_raw(key, Some(&text));
    }
}

pub fn has(key: &str) -> bool {
    get_raw(key).is_some()
}

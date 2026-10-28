//! The app's own log, for a report of a problem: every line the app logs is kept, the last
//! [`KEEP_LINES`] in memory, and natively each one is appended at once to `isorun-log.txt` in the
//! data folder (on a thread of its own), so the lines before a hang are there after the system
//! closed the app. The file holds the runs one after another, each from a `--- Isorun` line; past
//! [`FILE_LIMIT`] it becomes `isorun-log.old.txt` and a new one starts. [`text`] is what Share the
//! log sends: the newest of both files (the memory on the web).
//!
//! And a watch on the app's thread (Android, `android_lifecycle`): the thread says what it is
//! doing ([`stage`]) and when it waits for the system again ([`idle`]); a thread of the watch's
//! ([`watch`]) logs a stage that holds it longer than [`HELD`], and how long it held once it
//! goes on. Android's "isn't responding" is that thread held five seconds; the log then says where.

use std::collections::VecDeque;
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

/// The lines kept in memory.
const KEEP_LINES: usize = 3000;
/// How big the log file grows before it is put aside for a new one.
#[cfg(not(target_arch = "wasm32"))]
const FILE_LIMIT: u64 = 1 << 20;
/// The most of the log Share the log sends, in characters (Android's share sheet takes the text
/// through a binder call, which holds a megabyte).
const SHARE_CHARS: usize = 200_000;
/// How long the app's thread may be in one stage before the watch logs it.
#[cfg(not(target_arch = "wasm32"))]
const HELD: f64 = 1.0;

static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
#[cfg(not(target_arch = "wasm32"))]
static FILE: OnceLock<std::sync::mpsc::Sender<String>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static FILE_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

/// The platform's logger behind the app's log: a line the app's level lets through is kept, and
/// handed on to the platform's logger when that one wants it.
struct Tee {
    inner: Box<dyn log::Log>,
    level: log::LevelFilter,
}

impl Tee {
    fn keeps(&self, m: &log::Metadata) -> bool {
        // the activity's comings and goings (android-activity's debug lines: Pause, Resume,
        // NativeWindowCreated, ...) say what the system waited for
        m.level() <= self.level || (m.level() <= log::Level::Debug && m.target().starts_with("android_activity"))
    }
}

impl log::Log for Tee {
    fn enabled(&self, m: &log::Metadata) -> bool {
        self.keeps(m) || self.inner.enabled(m)
    }

    fn log(&self, r: &log::Record) {
        if self.inner.enabled(r.metadata()) {
            self.inner.log(r);
        }
        if self.keeps(r.metadata()) {
            let thread = std::thread::current();
            push(format!("{} {:<5} [{}] {}: {}", clock(), r.level(), thread.name().unwrap_or("?"), r.target(), r.args()));
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Log through `inner` (the platform's logger) and keep the lines at `level` and above.
pub fn init(inner: Box<dyn log::Log>, level: log::LevelFilter) {
    if log::set_boxed_logger(Box::new(Tee { inner, level })).is_ok() {
        log::set_max_level(log::LevelFilter::Debug.max(level));
    }
}

/// Keep a line (the log's, or one of the app's own).
pub fn push(line: String) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(tx) = FILE.get() {
        let _ = tx.send(line.clone());
    }
    let Ok(mut ring) = RING.lock() else { return };
    if ring.len() >= KEEP_LINES {
        ring.pop_front();
    }
    ring.push_back(line);
}

/// The time of day (UTC) to the millisecond.
fn clock() -> String {
    let ms = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600 % 24, s / 60 % 60, s % 60, ms % 1000)
}

/// Start the log file in `dir` (once the data folder is known): this run's lines so far first.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_file(dir: &std::path::Path) {
    use std::io::Write;
    let path = dir.join("isorun-log.txt");
    let old = dir.join("isorun-log.old.txt");
    if FILE_PATH.set(path.clone()).is_err() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let first: Vec<String> = RING.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default();
    let started = std::thread::Builder::new().name("log file".into()).spawn(move || {
        let open = |path: &std::path::Path| std::fs::OpenOptions::new().create(true).append(true).open(path);
        let mut size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > FILE_LIMIT {
            let _ = std::fs::rename(&path, &old);
            size = 0;
        }
        let Ok(mut file) = open(&path) else { return };
        let date = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let head =
            format!("--- Isorun {} on {} {}, started {} (Unix seconds)\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, date);
        let _ = file.write_all(head.as_bytes());
        size += head.len() as u64;
        for line in first.into_iter().chain(rx) {
            if size > FILE_LIMIT {
                let _ = std::fs::rename(&path, &old);
                match open(&path) {
                    Ok(f) => file = f,
                    Err(_) => return,
                }
                size = 0;
            }
            let _ = file.write_all(line.as_bytes());
            let _ = file.write_all(b"\n");
            size += line.len() as u64 + 1;
        }
    });
    if started.is_ok() {
        let _ = FILE.set(tx);
    }
}

/// Where the log file is, once it is open.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_path() -> Option<std::path::PathBuf> {
    FILE_PATH.get().cloned()
}

/// The log to send with a report: the newest [`SHARE_CHARS`] of the log files (the runs before
/// this one too: the one that hung), or of the lines in memory.
pub fn text() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = FILE_PATH.get() {
        let old = path.with_file_name("isorun-log.old.txt");
        let mut all = std::fs::read_to_string(old).unwrap_or_default();
        all.push_str(&std::fs::read_to_string(path).unwrap_or_default());
        if !all.is_empty() {
            return newest(&all, SHARE_CHARS).to_string();
        }
    }
    let lines: Vec<String> = RING.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default();
    newest(&lines.join("\n"), SHARE_CHARS).to_string()
}

/// The last `chars` characters of `s`, from the start of a line.
fn newest(s: &str, chars: usize) -> &str {
    let count = s.chars().count();
    if count <= chars {
        return s;
    }
    let (at, _) = s.char_indices().nth(count - chars).unwrap_or((0, ' '));
    let tail = &s[at..];
    tail.find('\n').map(|i| &tail[i + 1..]).unwrap_or(tail)
}

// ------------------------------------------------------------------ the app's thread

static STAGE: Mutex<Option<(&'static str, f64)>> = Mutex::new(None);

/// The app's thread is at `name` now (an event of the system's, a part of the frame).
pub fn stage(name: &'static str) {
    let now = super::seconds();
    if let Ok(mut s) = STAGE.lock() {
        *s = Some((name, now));
    }
}

/// The app's thread waits for the system again.
pub fn idle() {
    if let Ok(mut s) = STAGE.lock() {
        *s = None;
    }
}

/// Watch the app's thread: log a stage it is held in past [`HELD`], again every five seconds it
/// stays, and how long it was held once it goes on.
#[cfg(not(target_arch = "wasm32"))]
pub fn watch() {
    let spawned = std::thread::Builder::new().name("watch".into()).spawn(|| {
        let mut held: Option<(&'static str, f64, f64)> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let now = super::seconds();
            let current = STAGE.lock().ok().and_then(|s| *s);
            if let Some((name, since, _)) = held {
                if current.is_none_or(|(n, s)| n != name || s != since) {
                    log::warn!("the app's thread went on after {:.1} s in {name}", now - since);
                    held = None;
                }
            }
            let Some((name, since)) = current else { continue };
            let for_s = now - since;
            if for_s < HELD {
                continue;
            }
            let next = held.map_or(HELD, |(_, _, next)| next);
            if for_s >= next {
                log::warn!("the app's thread is held in {name} for {for_s:.1} s");
                held = Some((name, since, if next < 5.0 { 5.0 } else { next + 5.0 }));
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("no thread to watch the app's: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_of_a_log_from_a_whole_line() {
        assert_eq!(newest("one\ntwo\nthree", 100), "one\ntwo\nthree");
        assert_eq!(newest("one\ntwo\nthree", 7), "three");
        assert_eq!(newest("ąęś\nżół", 4), "żół");
    }
}

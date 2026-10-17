//! What's new, in the app (the port of `news.ts`): CHANGELOG.md read on a screen of its own (a row
//! a release, its number and date set apart, the latest lit), and whether this device has read it
//! since the release it runs came out. The title screen's What's new row wears a dot until it has.
//!
//! What was read is the release the app was when What's new was last opened here; a device that
//! never opened it has read nothing.

use crate::markdown::{self, Block, Inline};
use isorun_client::platform::storage::{get_raw, set_raw};

/// The repository's changelog: a few kilobytes, so it comes with the app and reads offline.
pub const CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

/// The release this build is (the title screen's corner names it).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SEEN_KEY: &str = "isorun.newsSeen";

/// The release this device last read What's new under, if ever. The web build keeps it as bare
/// text, not JSON.
pub fn news_seen() -> Option<String> {
    get_raw(SEEN_KEY).map(|v| serde_json::from_str::<String>(&v).unwrap_or(v))
}

/// Whether there is something in What's new this device has not read: the dot on its row.
pub fn news_unread() -> bool {
    news_seen().as_deref() != Some(VERSION)
}

/// What's new was opened: what it says now is read.
pub fn mark_news_seen() {
    set_raw(SEEN_KEY, Some(VERSION));
}

/// `a` is a later release than `b` (`0.2.10` after `0.2.9`).
pub fn newer(a: &str, b: &str) -> bool {
    let pa: Vec<f64> = a.split('.').map(|x| x.parse().unwrap_or(f64::NAN)).collect();
    let pb: Vec<f64> = b.split('.').map(|x| x.parse().unwrap_or(f64::NAN)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let d = pa.get(i).copied().unwrap_or(0.0) - pb.get(i).copied().unwrap_or(0.0);
        if d != 0.0 && !d.is_nan() {
            return d > 0.0;
        }
    }
    false
}

/// The changelog's newest release as the screen names it under its title: "Version 0.2.6 · 2026-09-29".
pub fn latest_line(md: &str) -> String {
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some((ver, date)) = rest.split_once(" — ") {
                let date = date.trim();
                if date.len() >= 10 && date[..10].chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() }) {
                    return format!("Version {} · {}", ver.trim(), &date[..10]);
                }
            }
        }
    }
    "Newest first".into()
}

/// One release of the changelog, as What's new shows it: a row the cursor stands on.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub date: String,
    /// The newest: marked Latest.
    pub latest: bool,
    /// Released since the release this device last read under: marked New.
    pub fresh: bool,
    /// Its changes.
    pub body: Vec<Block>,
}

/// The changelog as What's new's rows, the newest first; its opening lines are the screen's to say.
pub fn releases(md: &str, seen: Option<&str>) -> Vec<Release> {
    let blocks = markdown::parse(md, true);
    let mut out: Vec<Release> = Vec::new();
    for b in blocks {
        match b {
            Block::Heading(3, spans) => {
                let text = markdown::plain(&spans);
                let (version, date) = match text.split_once(" — ") {
                    Some((v, d)) => (v.to_string(), d.to_string()),
                    None => (text.clone(), String::new()),
                };
                let first = out.is_empty();
                let fresh = !date.is_empty() && seen.is_some_and(|s| newer(&version, s));
                out.push(Release { version, date, latest: first, fresh, body: Vec::new() });
            }
            other => {
                if let Some(r) = out.last_mut() {
                    r.body.push(other);
                }
            }
        }
    }
    out
}

/// A release's changes as lines of inlines (its bullets, or its paragraphs).
pub fn release_lines(r: &Release) -> Vec<Vec<Inline>> {
    let mut out = Vec::new();
    for b in &r.body {
        match b {
            Block::List(items) => out.extend(items.iter().cloned()),
            Block::Para(p) => out.push(p.clone()),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(newer("0.2.10", "0.2.9"));
        assert!(!newer("0.2.9", "0.2.10"));
        assert!(!newer("0.2.9", "0.2.9"));
        assert!(newer("1.0", "0.9.9"));
    }

    #[test]
    fn the_changelog() {
        let md = "# What's new\n\nNotable changes.\n\n## 0.2.0 — 2026-10-01\n\n- B\n\n## 0.1.9 — 2026-09-01\n\n- A\n- A2\n\n## 0.1.8 — 2026-08-01\n\n- old\n";
        assert_eq!(latest_line(md), "Version 0.2.0 · 2026-10-01");
        let r = releases(md, Some("0.1.8"));
        assert_eq!(r.len(), 3);
        assert!(r[0].latest && r[0].fresh);
        assert!(!r[1].latest && r[1].fresh);
        assert!(!r[2].fresh);
        assert_eq!(release_lines(&r[1]).len(), 2);
        assert!(!releases(CHANGELOG, None).is_empty());
    }
}

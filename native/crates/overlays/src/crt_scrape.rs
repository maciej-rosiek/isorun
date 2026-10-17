//! CRT screens (`txt` messages: the skills screen and its kin) arrive as lines of server-rendered
//! HTML with no structure a cursor could use (crt-scrape.ts). This module scrapes the hotkeys each
//! known screen prints so the focus layer can put a cursor over its rows. One scraper per screen,
//! keyed by the crt menu's tag, each pinned by a fixture recorded from a real server: a DCSS
//! update that changes the layout breaks a test here, not play. A screen with no scraper of its
//! own still yields the switches it prints in brackets, so no CRT screen is left with nothing the
//! cursor can reach.
//!
//! Activation always sends exactly the key the screen printed for the row.

use crate::menu_nav::char_at;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CrtKind {
    Row,
    Footer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrtHotkey {
    /// the character the screen printed; activation sends it as typed input
    pub key: String,
    pub label: String,
    /// 0-based line of the screen
    pub line: usize,
    /// 0-based column of the hotkey in the plain text of that line
    pub col: usize,
    /// width of the region the row covers, for the cursor
    pub len: usize,
    /// column group for left / right: skills print two columns of rows
    pub group: String,
    pub kind: CrtKind,
}

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());

/// The plain text of one CRT line: tags dropped, the entities the server escapes restored.
pub fn crt_plain_text(html: &str) -> String {
    TAG_RE.replace_all(html, "").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ").replace("&amp;", "&")
}

/// A selectable skill row: hotkey, training sign (`+` training, `-` not, `*` focused), then the
/// name, which always starts with a capital. Anchoring on `<sign> <Capital>` keeps the digits of
/// the level and aptitude columns from matching.
static SKILL_HOTKEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([a-z0-9]) [+\-*] [A-Z]").unwrap());

/// A footer switch: `[!] training`, `[?] help`, `[=] set target`.
static CRT_BRACKET_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[(\S)\]").unwrap());

/// What follows a switch up to `limit`: the label (to a run of two spaces) and the region's width.
fn label_after(chars: &[char], from: usize, limit: usize) -> (String, usize) {
    let rest: String = chars[from.min(limit)..limit].iter().collect();
    let stop = rest.find("  ").map(|b| char_at(&rest, b));
    let label: String = match stop {
        Some(s) => rest.chars().take(s).collect(),
        None => rest.clone(),
    };
    let len = match stop {
        Some(s) => s,
        None => rest.trim_end().chars().count(),
    };
    (label.trim().to_string(), len)
}

/// The switches one line prints (`[!] training  [?] help`), as a menu's more line prints its own.
fn bracket_switches(text: &str, line: usize) -> Vec<CrtHotkey> {
    let chars: Vec<char> = text.chars().collect();
    let foots: Vec<(String, usize, usize)> = CRT_BRACKET_RE.captures_iter(text).map(|c| (c[1].to_string(), char_at(text, c.get(0).unwrap().start()), char_at(text, c.get(0).unwrap().end()))).collect();
    foots
        .iter()
        .enumerate()
        .map(|(i, (key, col, end))| {
            let limit = foots.get(i + 1).map_or(chars.len(), |f| f.1);
            let (label, span) = label_after(&chars, *end, limit);
            CrtHotkey { key: key.clone(), label: if label.is_empty() { key.clone() } else { label }, line, col: *col, len: end - col + span, group: format!("foot{i}"), kind: CrtKind::Footer }
        })
        .collect()
}

fn scrape_switches(plain: &[String]) -> Vec<CrtHotkey> {
    plain.iter().enumerate().flat_map(|(line, t)| bracket_switches(t, line)).collect()
}

/// Scrape the skills screen (menu tag `skills`).
pub fn scrape_skills(plain: &[String]) -> Vec<CrtHotkey> {
    let mut out = Vec::new();
    for (line, text) in plain.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        let rows: Vec<(String, usize)> = SKILL_HOTKEY_RE.captures_iter(text).map(|c| (c[1].to_string(), char_at(text, c.get(0).unwrap().start()))).collect();
        for (i, (key, col)) in rows.iter().enumerate() {
            let end = rows.get(i + 1).map_or(chars.len(), |r| r.1);
            // the name runs from after "x + " to the first double space (the level column)
            let (label, _) = label_after(&chars, col + 4, end);
            out.push(CrtHotkey { key: key.clone(), label: if label.is_empty() { key.clone() } else { label }, line, col: *col, len: (end - col).max(1), group: String::new(), kind: CrtKind::Row });
        }
        if rows.is_empty() {
            out.extend(bracket_switches(text, line));
        }
    }
    // the column a row belongs to is where its hotkey sits on the screen, so a line with only a
    // right-hand entry still lands the cursor in the right column
    let mut cols: Vec<usize> = out.iter().filter(|h| h.kind == CrtKind::Row).map(|h| h.col).collect();
    cols.sort();
    cols.dedup();
    for h in out.iter_mut().filter(|h| h.kind == CrtKind::Row) {
        h.group = format!("col{}", cols.iter().position(|&c| c == h.col).unwrap());
    }
    out
}

/// Screens the scraper knows.
pub fn known_crt_screen(tag: &str) -> bool {
    tag == "skills"
}

/// Hotkeys of the CRT screen `tag`, from its HTML lines in order. A tag with no scraper falls
/// back to the switches the screen prints in brackets.
pub fn scrape_crt(tag: &str, html_lines: &[String]) -> Vec<CrtHotkey> {
    let plain: Vec<String> = html_lines.iter().map(|l| crt_plain_text(l)).collect();
    if tag == "skills" {
        scrape_skills(&plain)
    } else {
        scrape_switches(&plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_webtiles::{reduce, GameState};

    fn msgs() -> Vec<serde_json::Value> {
        serde_json::from_str(include_str!("../../../../apps/isorun/test/fixtures/skills-crt.json")).unwrap()
    }

    fn lines_after(n: usize) -> Vec<String> {
        let mut st = GameState::default();
        for m in msgs().iter().take(n) {
            reduce(&mut st, m);
        }
        let area = st.crt.area("menu_txt").unwrap();
        let max = area.keys().max().copied().unwrap_or(-1);
        (0..=max).map(|i| area.get(&i).cloned().unwrap_or_default()).collect()
    }

    #[test]
    fn plain_text() {
        assert_eq!(crt_plain_text("  <span class=\"fg8 bg0\">b - Maces &amp; Flails   0.0   </span><span class=\"fg3 bg0\">0.8   </span>"), "  b - Maces & Flails   0.0   0.8   ");
        assert_eq!(crt_plain_text("&lt;w&gt;x&lt;/w&gt;"), "<w>x</w>");
    }

    #[test]
    fn skills_rows() {
        assert!(known_crt_screen("skills") && !known_crt_screen("crt"));
        let hot = scrape_crt("skills", &lines_after(2));
        let rows: Vec<&CrtHotkey> = hot.iter().filter(|h| h.kind == CrtKind::Row).collect();
        assert_eq!(
            rows.iter().map(|r| format!("{}:{}", r.key, r.label)).collect::<Vec<_>>(),
            ["a:Fighting", "i:Spellcasting", "b:Maces & Flails", "j:Conjurations", "c:Unarmed Combat", "k:Translocations", "d:Throwing", "l:Evocations", "e:Ranged Weapons", "f:Armour", "g:Dodging", "h:Stealth"]
        );
        let by = |k: &str| rows.iter().find(|r| r.key == k).unwrap();
        assert_eq!(by("a").group, "col0");
        assert_eq!(by("i").group, "col1");
        assert_eq!(by("i").line, by("a").line);
        assert_eq!(by("l").group, "col1");
        assert_eq!(by("l").line, 6);
        assert_eq!(by("a").col, 2);
        assert_eq!(by("i").col, 41);
        assert_eq!(by("a").len, 39);
        for r in &rows {
            assert!(r.key.len() == 1 && ('a'..='l').contains(&r.key.chars().next().unwrap()));
        }
    }

    #[test]
    fn skills_footer_and_updates() {
        let foot: Vec<String> = scrape_crt("skills", &lines_after(2)).into_iter().filter(|h| h.kind == CrtKind::Footer).map(|f| format!("{}:{}", f.key, f.label)).collect();
        assert_eq!(foot, ["?:Help", "=:set a skill target", "/:auto|manual mode", "*:useful|all skills", "!:training|cost|targets"]);
        let later: Vec<String> = scrape_crt("skills", &lines_after(4)).into_iter().filter(|h| h.kind == CrtKind::Footer).map(|f| f.key).collect();
        assert_eq!(later, ["?", "-", "/", "*", "!"]);
        let keys = |n| scrape_crt("skills", &lines_after(n)).into_iter().filter(|h| h.kind == CrtKind::Row).map(|h| h.key).collect::<Vec<_>>();
        assert_eq!(keys(3), keys(2));
        assert_eq!(keys(4), keys(2));
    }

    #[test]
    fn unknown_screens_and_distributed_training() {
        assert!(scrape_crt("crt", &lines_after(2)).iter().all(|h| h.kind != CrtKind::Row));
        assert_eq!(scrape_crt("macro", &["<span>[?] help</span>".to_string()]), vec![CrtHotkey { key: "?".into(), label: "help".into(), line: 0, col: 0, len: 8, group: "foot0".into(), kind: CrtKind::Footer }]);
        assert!(scrape_skills(&["      Skill           Level Cost   Apt".to_string(), "    + Fighting         2.6   3.0    0".to_string()]).is_empty());
    }
}

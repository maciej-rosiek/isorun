//! The lobby's game links (`set_game_links`): [`GameLink`], and the rows the home screen and
//! the Other versions screen show ([`game_link_rows`], [`game_version_rows`]).

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GameLink {
    pub id: String,
    pub label: String,
    /// The save waiting in this game, in the lobby's own words (game_links.html `save_info`,
    /// ws_handler.py `update_save_info`): the bracket text after the game's name. "isoruntest, a
    /// level 3 Minotaur Berserker of Trog" is a save to continue (player.cc
    /// `player_save_info::short_desc`); "playing" is a save another session has open; "slot full"
    /// is a save of another game type in the same slot, which the lobby greys out and does not
    /// link. `None` when the lobby shows a plain link: no save, or a server that does not publish
    /// save info (`show_save_info` off), which look the same.
    pub save: Option<String>,
    /// The lobby offers no link for this game: its save slot holds another game type ("slot full").
    pub disabled: bool,
    /// The game whose `(edit rc)` opens the rc file this one reads: the first game of its rc
    /// directory (game_links.html puts the link on that one only, and the games after it share the
    /// file until the next link). The id to `get_rc` and `set_rc` with. `None` when the lobby
    /// offers no link.
    pub rc: Option<String>,
}

impl GameLink {
    pub fn new(id: &str, label: &str) -> Self {
        GameLink { id: id.into(), label: label.into(), ..GameLink::default() }
    }
}

static PLAY_HREF_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)href=["']#play-([^"']+)["']"#).unwrap());
static RC_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)data-game_id=["']([^"']+)["']"#).unwrap());
static INNER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<span[^>]*>\s*(?:<a[^>]*>)?\s*\[([^\]]*)\]\s*(?:</a>)?\s*</span>").unwrap());
static NAME_LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)<a[^>]*href=["']#play-[^"']+["'][^>]*>([\s\S]*?)</a>"#).unwrap());
static EDIT_RC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)<a[^>]*class=["']edit_rc_link["'][\s\S]*?</a>"#).unwrap());
static SPAN_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<span\b[^>]*>|</span>").unwrap());
static ANCHOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)<a[^>]*href=["']#play-([^"']+)["'][^>]*>([\s\S]*?)</a>"#).unwrap());
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());
static ENTITY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)&(nbsp|amp|lt|gt|quot|#39|apos|#x[0-9a-f]+|#[0-9]+);").unwrap());
static SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\s\u{a0}]+").unwrap());
static TRUNK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(trunk|git)\b").unwrap());
static VERSION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([0-9]+\.[0-9]+)\b").unwrap());
static HIDDEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(seed|seeded|sprint|tut|tutorial)\b").unwrap());
static HIDDEN_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(seeded|sprint|tut)-").unwrap());
static MODE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(zd|zotdef|zot def(ence|ense))\b|^zd-").unwrap());

/// JavaScript `decodeURIComponent`: `%XX` escapes as UTF-8; a malformed one is an error.
fn decode_uri_component(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok());
            match hex {
                Some(v) => {
                    out.push(v);
                    i += 3;
                }
                None => return Err("URI malformed".into()),
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "URI malformed".into())
}

/// The text of a piece of the lobby's html: tags gone, entities decoded, whitespace (and
/// `&nbsp;`) collapsed. A numeric entity past the last code point is an error, as
/// `String.fromCodePoint` makes it one.
fn html_text(html: &str) -> Result<String, String> {
    let untagged = TAG_RE.replace_all(html, " ");
    let mut err = None;
    let decoded = ENTITY_RE.replace_all(&untagged, |c: &Captures| {
        let k = c[1].to_ascii_lowercase();
        match k.as_str() {
            "nbsp" => " ".into(),
            "amp" => "&".into(),
            "lt" => "<".into(),
            "gt" => ">".into(),
            "quot" => "\"".into(),
            "#39" | "apos" => "'".into(),
            _ => {
                let n = match k.strip_prefix("#x") {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => k[1..].parse::<u32>().ok(),
                };
                match n.and_then(char::from_u32) {
                    Some(ch) => ch.to_string(),
                    None => {
                        err = Some(format!("Invalid code point {}", &c[1]));
                        String::new()
                    }
                }
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(SPACE_RE.replace_all(&decoded, " ").trim().to_owned())
}

/// The bodies of the outermost `<span>` elements of `html`, in order; a lobby without the spans
/// (an older template) gives one per play link.
fn top_level_spans(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start: Option<usize> = None;
    for m in SPAN_TAG_RE.find_iter(html) {
        if m.as_str().as_bytes().get(1) != Some(&b'/') {
            if depth == 0 {
                start = Some(m.end());
            }
            depth += 1;
        } else if depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    out.push(&html[s..m.start()]);
                }
            }
        }
    }
    if out.is_empty() {
        out.extend(ANCHOR_RE.find_iter(html).map(|m| m.as_str()));
    }
    out
}

/// The lobby's "Play now" line (`set_game_links`, game_links.html): one top-level `<span>` per
/// game, holding a separator, the game's name, and, when the server has checked the save slot, a
/// nested `<span>` with the save in brackets. The `#play-<id>` link is on the name when there is
/// no save and on the bracket text when there is one; a game whose slot another game type holds
/// ("[slot full]") has no link at all, and its id is only known from the `(edit rc)` link's
/// `data-game_id`, which the first game of an rc directory carries. That link says which rc file
/// each game reads ([`GameLink::rc`]). An error for a malformed escape, which the TS throws.
pub fn parse_game_links(html: &str) -> Result<Vec<GameLink>, String> {
    let mut out = Vec::new();
    // the rc file of the games since the last `(edit rc)` link
    let mut rc_of: Option<String> = None;
    for block in top_level_spans(html) {
        let ids = PLAY_HREF_RE.captures_iter(block).map(|c| decode_uri_component(&c[1])).collect::<Result<Vec<_>, _>>()?;
        let rc = RC_ID_RE.captures(block).map(|c| decode_uri_component(&c[1])).transpose()?;
        let id = ids.first().cloned().or_else(|| rc.clone());
        if rc.is_some() {
            rc_of = rc;
        }
        let Some(id) = id else { continue };
        // the nested span: "[save]", linked or not
        let inner = INNER_RE.captures(block);
        // the name: the link's text when the name is the link; otherwise what stands between the
        // separator and the save. The separator is the server's own markup (CDI: "<br>&nbsp;...
        // latest version: &nbsp;"), so the name is what follows its last colon or bar
        let head = match &inner {
            Some(c) => &block[..c.get(0).map_or(0, |m| m.start())],
            None => block,
        };
        let name_link = NAME_LINK_RE.captures(head);
        let mut text = match &name_link {
            Some(c) => html_text(&c[1])?,
            None => html_text(&EDIT_RC_RE.replace_all(head, ""))?,
        };
        if name_link.is_none() {
            let cut = match (text.rfind(':'), text.rfind('|')) {
                (Some(a), Some(b)) => a.max(b) + 1,
                (Some(a), None) | (None, Some(a)) => a + 1,
                (None, None) => 0,
            };
            text = text[cut..].trim().to_owned();
        }
        if text.is_empty() {
            continue;
        }
        let mut link = GameLink { id, label: text, rc: rc_of.clone(), ..GameLink::default() };
        if let Some(c) = &inner {
            link.save = Some(html_text(&c[1])?);
            link.disabled = ids.is_empty();
        }
        out.push(link);
    }
    Ok(out)
}

/// The version a lobby game link belongs to, from its label or id: "trunk" for git / trunk
/// builds, a numeric "0.34"-style string for stable releases, or `None` when nothing
/// recognisable is present.
pub fn game_link_version(g: &GameLink) -> Option<String> {
    let text = format!("{} {}", g.label, g.id);
    if TRUNK_RE.is_match(&text) {
        return Some("trunk".into());
    }
    VERSION_RE.captures(&text).map(|c| c[1].to_owned())
}

/// The game variants the lobby does not offer: custom seed, Dungeon Sprint and the tutorial.
/// Matched on the label or the id ("seeded-web-trunk", "sprint-0.34", "tut-web-trunk").
fn is_hidden_game_link(g: &GameLink) -> bool {
    HIDDEN_RE.is_match(&format!("{} {}", g.label, g.id)) || HIDDEN_ID_RE.is_match(&g.id)
}

/// A mode of play rather than a release's plain game: custom seed, Sprint, the tutorial, Zot
/// Defence.
fn is_mode_link(g: &GameLink) -> bool {
    is_hidden_game_link(g) || MODE_RE.is_match(&format!("{} {}", g.label, g.id))
}

/// "0.34" as (0, 34).
fn version_parts(v: &str) -> (i64, i64) {
    let mut it = v.split('.').map(|p| p.parse::<i64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn latest_indices(games: &[GameLink]) -> Vec<usize> {
    let mut latest: Option<String> = None;
    for g in games {
        if let Some(v) = game_link_version(g).filter(|v| v != "trunk") {
            if latest.as_deref().is_none_or(|l| version_parts(&v) > version_parts(l)) {
                latest = Some(v);
            }
        }
    }
    (0..games.len())
        .filter(|&i| match game_link_version(&games[i]) {
            None => true,
            Some(v) => v == "trunk" || Some(&v) == latest.as_ref(),
        })
        .collect()
}

/// Keep only the links for trunk and for the newest stable release. Links with no recognisable
/// version are kept, so unusual servers still show something.
pub fn latest_game_links(games: &[GameLink]) -> Vec<GameLink> {
    latest_indices(games).into_iter().map(|i| games[i].clone()).collect()
}

/// The lobby's "Play now" rows: the newest stable release, trunk, and anything unversioned.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GameLinkRows {
    /// The newest stable version, e.g. "0.34", or `None` if the server has none.
    pub latest_version: Option<String>,
    pub latest: Vec<GameLink>,
    pub trunk: Vec<GameLink>,
    pub other: Vec<GameLink>,
}

/// The rows as indices into `games`, so a caller can tell the links apart by identity.
fn game_link_row_indices(games: &[GameLink]) -> (Option<String>, Vec<usize>, Vec<usize>, Vec<usize>) {
    let (mut version, mut latest, mut trunk, mut other) = (None, Vec::new(), Vec::new(), Vec::new());
    for i in latest_indices(games) {
        let g = &games[i];
        if is_hidden_game_link(g) {
            continue;
        }
        match game_link_version(g) {
            Some(v) if v == "trunk" => trunk.push(i),
            None => other.push(i),
            Some(v) => {
                version = Some(v);
                latest.push(i);
            }
        }
    }
    (version, latest, trunk, other)
}

/// Split the links [`latest_game_links`] keeps into the rows the official lobby shows: "latest
/// version:" and "trunk:". Older releases are dropped; nobody plays them and they only push the
/// player table down the screen. The custom seed, Sprint and tutorial variants are dropped too.
pub fn game_link_rows(games: &[GameLink]) -> GameLinkRows {
    let (latest_version, latest, trunk, other) = game_link_row_indices(games);
    let pick = |ix: Vec<usize>| ix.into_iter().map(|i| games[i].clone()).collect();
    GameLinkRows { latest_version, latest: pick(latest), trunk: pick(trunk), other: pick(other) }
}

/// One release on the account's Other versions screen: its plain game, and the modes beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameVersionRow {
    /// "trunk", "0.34", or `None` for links with no version.
    pub version: Option<String>,
    /// The plain game; a release with none (Zot Defence 0.17 alone) leads with its first mode.
    pub game: GameLink,
    /// Custom seed, Sprint, the tutorial, Zot Defence..., in the server's order.
    pub modes: Vec<GameLink>,
}

/// Every enabled link, one row per release: trunk, then the releases newest first, then anything
/// unversioned the home screen does not already show. A slot another game holds is left out, as
/// it is from the home screen.
pub fn game_version_rows(games: &[GameLink]) -> Vec<GameVersionRow> {
    let rank = |v: &Option<String>| match v.as_deref() {
        Some("trunk") => 1_000_000_000,
        None => -1,
        Some(v) => {
            let (a, b) = version_parts(v);
            a * 1000 + b
        }
    };
    let shown_on_home = game_link_row_indices(games).3;
    // by version, in the order each was first seen: (version, plain, modes)
    let mut by: Vec<(Option<String>, Vec<usize>, Vec<usize>)> = Vec::new();
    for (i, g) in games.iter().enumerate() {
        if g.disabled {
            continue;
        }
        let v = game_link_version(g);
        let mode = is_mode_link(g);
        if v.is_none() && !mode && shown_on_home.contains(&i) {
            continue;
        }
        let slot = match by.iter().position(|e| e.0 == v) {
            Some(p) => p,
            None => {
                by.push((v, Vec::new(), Vec::new()));
                by.len() - 1
            }
        };
        if mode {
            by[slot].2.push(i);
        } else {
            by[slot].1.push(i);
        }
    }
    let mut rows = Vec::new();
    for (version, plain, modes) in by {
        let all: Vec<GameLink> = plain.into_iter().chain(modes).map(|i| games[i].clone()).collect();
        if version.is_none() {
            // unversioned links have nothing in common: each is a row of its own
            rows.extend(all.into_iter().map(|g| GameVersionRow { version: None, game: g, modes: Vec::new() }));
        } else {
            let mut it = all.into_iter();
            if let Some(game) = it.next() {
                rows.push(GameVersionRow { version, game, modes: it.collect() });
            }
        }
    }
    // stable: unversioned rows keep the server's order, at the end
    rows.sort_by_key(|r| std::cmp::Reverse(rank(&r.version)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_and_escapes() {
        assert_eq!(html_text("<b>a</b>&nbsp;&amp;&#x41;&#66;  c").unwrap(), "a &AB c");
        assert!(html_text("&#99999999;").is_err());
        assert_eq!(decode_uri_component("a%20b%C3%A9").unwrap(), "a bé");
        assert!(decode_uri_component("%zz").is_err());
    }
}

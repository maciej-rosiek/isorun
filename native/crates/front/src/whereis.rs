//! `<player>.where`: what the server itself says is waiting in a player's save (the port of
//! `whereis.ts`).
//!
//! Crawl writes one of these per player into the morgue directory on every save and every exit,
//! and dgamelaunch servers publish the morgue directory. It is the only account-wide word on a save
//! that a server keeps its `set_game_links` save info from (CDI): the WebTiles socket cannot be
//! asked, and this device's own memory is not evidence, since the same account can be played out
//! and killed from any other browser.
//!
//! One file per player, not per version — it is rewritten by whichever game ran last — so it
//! answers for the version named in `v` and says nothing about any other.

use isorun_client::servers::{LastCharacter, ServerInfo};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Whereis {
    /// The row this line answers for: "0.34" for a release, "trunk" for an alpha build.
    pub version: Option<String>,
    /// The `status` field as crawl wrote it: "saved", "dead", "won", "quit", "bailed out", "crash", …
    pub status: String,
    pub character: LastCharacter,
}

/// One xlog line into its fields: `key=value` pairs separated by `:`, with a literal colon in a
/// value written `::` — which is how a place reaches us as `place=D::2`.
pub fn parse_xlog(line: &str) -> BTreeMap<String, String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != ':' {
            cur.push(chars[i]);
        } else if chars.get(i + 1) == Some(&':') {
            cur.push(':');
            i += 1;
        } else {
            parts.push(std::mem::take(&mut cur));
        }
        i += 1;
    }
    parts.push(cur);
    let mut out = BTreeMap::new();
    for p in parts {
        if let Some(eq) = p.find('=') {
            if eq > 0 {
                out.insert(p[..eq].to_string(), p[eq + 1..].to_string());
            }
        }
    }
    out
}

/// Which home-screen row a `v=` field belongs to: a trunk build carries an alpha marker
/// ("0.35-a0-1015-g…"), a release is its major and minor ("0.34.1-4-g…" is the "0.34" row).
pub fn whereis_version(v: &str) -> Option<String> {
    let b = v.as_bytes();
    if (0..b.len().saturating_sub(2)).any(|i| b[i] == b'-' && b[i + 1] == b'a' && b[i + 2].is_ascii_digit()) {
        return Some("trunk".into());
    }
    let mut it = v.splitn(3, |c: char| !c.is_ascii_digit());
    let major = it.next()?;
    if major.is_empty() || v.as_bytes().get(major.len()) != Some(&b'.') {
        return None;
    }
    let minor: String = v[major.len() + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
    (!minor.is_empty()).then(|| format!("{major}.{minor}"))
}

/// A roster line or a save's fields as a character (servers.ts `characterOf`).
pub fn character_of(name: &str, species: &str, xl: &str, place: &str, title: &str, god: &str) -> LastCharacter {
    let (place, depth) = split_place(place);
    let title = if title.is_empty() { String::new() } else { format!("the {title}") };
    LastCharacter { name: name.into(), title, species: species.into(), god: god.into(), xl: xl.trim().parse().unwrap_or(0), place, depth }
}

/// "D:2" into the branch and the depth; a place with no depth is all branch.
pub fn split_place(place: &str) -> (String, i64) {
    match place.rsplit_once(':') {
        Some((b, d)) if !b.is_empty() && d.trim().parse::<i64>().is_ok_and(|n| n != 0) => (b.to_string(), d.trim().parse().unwrap_or(0)),
        _ => (place.to_string(), 0),
    }
}

/// Read a `.where` file. None when it is not one (an error page, an empty file, a line without a character).
pub fn parse_whereis(text: &str) -> Option<Whereis> {
    let line = text.split('\n').find(|l| l.contains("status=") && l.contains("name="))?;
    let f = parse_xlog(line.trim());
    let get = |k: &str| f.get(k).map(String::as_str).unwrap_or("");
    if get("name").is_empty() || get("status").is_empty() {
        return None;
    }
    let species = [get("race"), get("cls")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    Some(Whereis {
        version: if get("v").is_empty() { None } else { whereis_version(get("v")) },
        status: get("status").into(),
        character: character_of(get("name"), &species, get("xl"), get("place"), get("title"), get("god")),
    })
}

/// Whether the line says a save is sitting there: crawl wrote it as it saved. Every other status
/// is not a yes.
pub fn save_waiting(w: &Whereis) -> bool {
    w.status == "saved" || w.status == "save"
}

/// Split an address into its scheme, host (with the port) and path.
fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let cut = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let path = &rest[cut..];
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    Some((scheme, &rest[..cut], path))
}

/// Where a server keeps a player's morgue directory, as `game_ended`'s `dump` spells it: the path
/// less the file, e.g. "/crawl/morgue/orbrun/". None when the dump is not a path on the server.
pub fn morgue_dir_of(server: &ServerInfo, dump: &str) -> Option<String> {
    let (_, host, _) = split_url(&server.http)?;
    let path = if dump.contains("://") {
        let (_, h, p) = split_url(dump)?;
        if !h.eq_ignore_ascii_case(host) {
            return None;
        }
        p.to_string()
    } else if dump.starts_with('/') {
        dump[..dump.find(['?', '#']).unwrap_or(dump.len())].to_string()
    } else {
        format!("/{dump}")
    };
    let cut = path.rfind('/')?;
    (cut > 0).then(|| path[..=cut].to_string())
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The directories to try for a player's morgue before a `game_ended` has named the real one:
/// the layouts dgamelaunch-config ships with (CDI keeps them under /crawl/, most servers at the
/// root, CAO under /rawdata/).
pub fn morgue_dir_guesses(username: &str) -> Vec<String> {
    let u = encode(username);
    vec![format!("/crawl/morgue/{u}/"), format!("/morgue/{u}/"), format!("/rawdata/{u}/")]
}

/// The address of the `.where` file in a morgue directory: the server's own natively, the hosted
/// page's proxy in a browser (no CORS to get round here).
pub fn whereis_url(server: &ServerInfo, dir: &str) -> Option<String> {
    let who = dir.split('/').filter(|s| !s.is_empty()).last()?;
    let (scheme, host, _) = split_url(&server.http)?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let path = format!("{}{who}.where", if dir.starts_with('/') { dir.to_string() } else { format!("/{dir}") });
    if cfg!(target_arch = "wasm32") {
        Some(format!("{}/morgue-proxy/{host}{path}", isorun_client::servers::web_origin()))
    } else {
        Some(format!("{scheme}://{host}{path}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> ServerInfo {
        ServerInfo {
            id: "cdi".into(),
            name: "CDI".into(),
            region: None,
            ws: "wss://crawl.dcss.io/socket".into(),
            http: "https://crawl.dcss.io".into(),
            host: "crawl.dcss.io".into(),
            custom: false,
            offline: false,
        }
    }

    #[test]
    fn xlog_fields() {
        let f = parse_xlog("v=0.34.1:name=bob:place=D::2:status=saved");
        assert_eq!(f["place"], "D:2");
        assert_eq!(f["status"], "saved");
        assert_eq!(f["v"], "0.34.1");
    }

    #[test]
    fn versions() {
        assert_eq!(whereis_version("0.35-a0-1015-gabc").as_deref(), Some("trunk"));
        assert_eq!(whereis_version("0.34.1-4-g0e95").as_deref(), Some("0.34"));
        assert_eq!(whereis_version("nonsense"), None);
    }

    #[test]
    fn a_where_file() {
        let w = parse_whereis("v=0.34.1:name=Kai:race=Minotaur:cls=Berserker:xl=3:place=D::4:god=Trog:status=saved\n").unwrap();
        assert_eq!(w.version.as_deref(), Some("0.34"));
        assert!(save_waiting(&w));
        assert_eq!(w.character.species, "Minotaur Berserker");
        assert_eq!(w.character.place, "D");
        assert_eq!(w.character.depth, 4);
        assert_eq!(w.character.xl, 3);
        assert!(parse_whereis("<html>404</html>").is_none());
        assert!(!save_waiting(&parse_whereis("name=K:status=dead").unwrap()));
    }

    #[test]
    fn morgue_dirs() {
        let s = server();
        assert_eq!(morgue_dir_of(&s, "/crawl/morgue/orbrun/morgue-orbrun-1").as_deref(), Some("/crawl/morgue/orbrun/"));
        assert_eq!(morgue_dir_of(&s, "https://crawl.dcss.io/morgue/x/m").as_deref(), Some("/morgue/x/"));
        assert_eq!(morgue_dir_of(&s, "https://elsewhere.org/morgue/x/m"), None);
        assert_eq!(morgue_dir_guesses("a b")[0], "/crawl/morgue/a%20b/");
        assert_eq!(whereis_url(&s, "/crawl/morgue/orbrun/").as_deref(), Some("https://crawl.dcss.io/crawl/morgue/orbrun/orbrun.where"));
    }
}

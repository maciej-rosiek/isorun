//! What the front end's screens say, as pure functions (from `menu.ts`): the roster's columns,
//! the exit report's words, a mode's name beside its release, the rc file's state, an offline
//! game's build.

use isorun_webtiles::{GameLink, LobbyEntry};

/// Where a key moves the cursor, or none when it is not a movement key (menu.ts `navDir`). The
/// arrows work everywhere except that left and right are left to a text field, so the caret can
/// be moved; the vim keys (k up, j down, h left, l right, as in the game) are letters, so a text
/// field takes them all. Ctrl / Alt / Meta combos are never movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDir {
    Up,
    Down,
    Left,
    Right,
}

pub fn nav_dir(key: egui::Key, modifiers: egui::Modifiers, in_input: bool) -> Option<NavDir> {
    if modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd {
        return None;
    }
    let arrow = match key {
        egui::Key::ArrowUp => Some(NavDir::Up),
        egui::Key::ArrowDown => Some(NavDir::Down),
        egui::Key::ArrowLeft => Some(NavDir::Left),
        egui::Key::ArrowRight => Some(NavDir::Right),
        _ => None,
    };
    if let Some(a) = arrow {
        return if in_input && matches!(a, NavDir::Left | NavDir::Right) { None } else { Some(a) };
    }
    if in_input {
        return None;
    }
    match key {
        egui::Key::K => Some(NavDir::Up),
        egui::Key::J => Some(NavDir::Down),
        egui::Key::H => Some(NavDir::Left),
        egui::Key::L => Some(NavDir::Right),
        _ => None,
    }
}

// ------------------------------------------------------------------ the roster

/// The roster's columns (client.html #player_list, in its order). The four that always stand say
/// who and where; the rest are `extra`: shown on a wide screen, and on a narrow one as the details
/// line a row opens (right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RosterCol {
    pub key: &'static str,
    pub head: &'static str,
    pub cls: &'static str,
    pub extra: bool,
}

pub const ROSTER_COLS: [RosterCol; 11] = [
    RosterCol { key: "username", head: "Player", cls: "user", extra: false },
    RosterCol { key: "char", head: "Char", cls: "char", extra: false },
    RosterCol { key: "xl", head: "XL", cls: "xl", extra: false },
    RosterCol { key: "place", head: "Place", cls: "place", extra: false },
    RosterCol { key: "spectator_count", head: "Watching", cls: "specs", extra: true },
    RosterCol { key: "game_id", head: "Game", cls: "game", extra: true },
    RosterCol { key: "god", head: "God", cls: "god", extra: true },
    RosterCol { key: "turn", head: "Turn", cls: "turn", extra: true },
    RosterCol { key: "dur", head: "Time", cls: "time", extra: true },
    RosterCol { key: "idle_time", head: "Idle", cls: "idle", extra: true },
    RosterCol { key: "milestone", head: "Milestone", cls: "milestone", extra: true },
];

/// The extra columns in the order the width folds them into the details row: the first three go
/// at a middling width, the rest on a narrow one.
pub const DETAILS_ORDER: [&str; 7] = ["specs", "time", "idle", "game", "god", "turn", "milestone"];

/// Which columns stand at a width (css px): all of them on a wide screen; who watches, the time
/// played and the idle time fold into the details under 1200; every extra under 900.
pub fn roster_col_shown(col: &RosterCol, width: f32) -> bool {
    if width <= 900.0 {
        return !col.extra;
    }
    if width <= 1200.0 {
        return !matches!(col.cls, "specs" | "time" | "idle");
    }
    true
}

fn field<'a>(e: &'a LobbyEntry, key: &str) -> Option<String> {
    match key {
        "username" => Some(e.username.clone()),
        "char" => e.char.clone(),
        "xl" => e.xl.clone(),
        "place" => e.place.clone(),
        "spectator_count" => e.spectator_count.map(|c| c.to_string()),
        "game_id" => Some(e.game_id.clone()),
        "god" => e.god.clone(),
        "turn" => e.turn.clone(),
        "dur" => e.dur.clone(),
        "idle_time" => e.idle_time.map(|c| c.to_string()),
        "milestone" => e.milestone.clone(),
        _ => None,
    }
}

/// One column of a roster row as it reads: the value formatted as client.js formats it, nothing for none.
pub fn roster_cell(e: &LobbyEntry, key: &str) -> String {
    match key {
        "dur" => e.dur.as_deref().map(format_duration).unwrap_or_default(),
        "idle_time" => format_idle(e.idle_time.unwrap_or(0)),
        "spectator_count" => e.spectator_count.filter(|c| *c != 0).map(|c| c.to_string()).unwrap_or_default(),
        _ => field(e, key).unwrap_or_default(),
    }
}

/// The details line of a row: the columns the width hides, in the order they fold.
pub fn roster_details(e: &LobbyEntry, width: f32) -> Vec<String> {
    DETAILS_ORDER
        .iter()
        .filter_map(|cls| ROSTER_COLS.iter().find(|c| c.cls == *cls))
        .filter(|c| !roster_col_shown(c, width))
        .filter_map(|c| {
            let v = roster_cell(e, c.key);
            (!v.is_empty()).then(|| format!("{} {v}", c.head))
        })
        .collect()
}

/// The whole row, spoken, every column whether or not the width shows it.
pub fn roster_here(e: &LobbyEntry) -> String {
    let xl = e.xl.as_deref().filter(|x| !x.is_empty()).map(|x| format!("XL{x}")).unwrap_or_default();
    let who: Vec<String> = [e.char.clone().unwrap_or_default(), xl, e.place.clone().unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect();
    let mut facts = Vec::new();
    if let Some(g) = e.god.as_deref().filter(|g| !g.is_empty()) {
        facts.push(format!("a worshipper of {g}"));
    }
    if let Some(t) = e.turn.as_deref().filter(|t| !t.is_empty()) {
        facts.push(format!("turn {t}"));
    }
    if let Some(d) = e.dur.as_deref().filter(|d| !d.is_empty()) {
        facts.push(format!("{} played", format_duration(d)));
    }
    if e.idle_time.unwrap_or(0) > 0 {
        facts.push(format!("idle {}", format_idle(e.idle_time.unwrap_or(0))));
    }
    if e.spectator_count.unwrap_or(0) > 0 {
        facts.push(format!("{} watching", e.spectator_count.unwrap_or(0)));
    }
    let who = if who.is_empty() { String::new() } else { format!(" ({})", who.join(", ")) };
    let facts = if facts.is_empty() { String::new() } else { format!(", {}", facts.join(", ")) };
    let end = match e.milestone.as_deref().filter(|m| !m.is_empty()) {
        Some(m) => format!(": {m}"),
        None => ".".into(),
    };
    format!("You see here {}{who}{facts}{end}", e.username)
}

/// client.js: idle seconds as "42s", "7m" or "2684h"; nothing when not idle.
pub fn format_idle(seconds: i64) -> String {
    if seconds <= 0 {
        String::new()
    } else if seconds < 120 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

/// client.js format_duration: play time in seconds as "27s", "18m" or "2h 8m".
pub fn format_duration(dur: &str) -> String {
    let Ok(seconds) = dur.trim().parse::<f64>() else { return dur.to_string() };
    if !seconds.is_finite() || seconds < 0.0 {
        return dur.to_string();
    }
    if seconds < 60.0 {
        return format!("{}s", seconds.floor());
    }
    let s = seconds as i64;
    let (hh, m) = (s / 3600, (s % 3600) / 60);
    if hh > 0 {
        format!("{hh}h {m}m")
    } else {
        format!("{m}m")
    }
}

// ------------------------------------------------------------------ the exit report

/// client.js exit_reason_message: the line above a game's parting words when the way it ended
/// calls for one. A normal end (death, win, quit, save) says nothing of its own; a watched game's
/// end always says whose it was.
pub fn exit_reason_message(reason: &str, watched: Option<&str>) -> Option<String> {
    let tail = if reason != "unknown" { format!(" ({reason})") } else { String::new() };
    if let Some(w) = watched {
        let whose = format!("{w}'{}", if w.ends_with('s') { "" } else { "s" });
        return match reason {
            "quit" | "won" | "bailed out" | "dead" => None,
            "cancel" => Some(format!("{w} quit before creating a character.")),
            "saved" => Some(format!("{w} stopped playing (saved).")),
            "crash" => Some(format!("{whose} game crashed.")),
            "error" => Some(format!("{whose} game was terminated due to an error.")),
            "disconnect" => Some(format!("{w} has been disconnected.")),
            _ => Some(format!("{whose} game ended unexpectedly.{tail}")),
        };
    }
    match reason {
        "quit" | "won" | "bailed out" | "dead" | "saved" | "cancel" => None,
        "crash" => Some("Unfortunately your game crashed.".into()),
        "error" => Some("Unfortunately your game terminated due to an error.".into()),
        "disconnect" => Some("You have been disconnected.".into()),
        _ => Some(format!("Unfortunately your game ended unexpectedly.{tail}")),
    }
}

/// The exit screen's name: whose game, and how it went.
pub fn exit_title(reason: &str, watched: Option<&str>) -> String {
    let whose = match watched {
        Some(w) => format!("{w}’s game"),
        None => "Your game".into(),
    };
    match (reason, watched) {
        ("dead", Some(w)) => format!("{w} died"),
        ("dead", None) => "You died".into(),
        ("won", Some(w)) => format!("{w} won"),
        ("won", None) => "You won".into(),
        ("quit", _) => format!("{whose} was quit"),
        ("bailed out", _) => format!("{whose} was abandoned"),
        ("saved", _) => format!("{whose} was saved"),
        ("cancel", _) => "No character made".into(),
        ("crash", _) => format!("{whose} crashed"),
        ("error", _) => format!("{whose} stopped with an error"),
        ("disconnect", Some(w)) => format!("{w} was disconnected"),
        ("disconnect", None) => "Disconnected".into(),
        _ => format!("{whose} ended"),
    }
}

// ------------------------------------------------------------------ servers, versions, rc

/// Where a server is, under its name on the server list: its region and host.
pub fn server_where(region: Option<&str>, host: &str) -> String {
    [region.unwrap_or(""), host].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// A mode's name beside its release: "Custom seed 0.34" is (seed), "Tutorial trunk" (tutorial).
pub fn mode_name(g: &GameLink) -> String {
    let text = format!("{} {}", g.label, g.id).to_lowercase();
    let words: Vec<&str> = text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let id = g.id.to_lowercase();
    if text.contains("seed") {
        return "seed".into();
    }
    if words.iter().any(|w| *w == "spr" || *w == "sprint") || id.starts_with("spr-") {
        return "sprint".into();
    }
    if words.iter().any(|w| *w == "tut" || *w == "tutorial") || id.starts_with("tut-") {
        return "tutorial".into();
    }
    if words.contains(&"zd") || text.contains("zotdef") || text.contains("zot def") {
        return "zot defence".into();
    }
    let rest: Vec<&str> = g
        .label
        .split_whitespace()
        .filter(|w| {
            let l = w.to_lowercase();
            !(l == "dcss" || l == "trunk" || l == "git" || is_version_word(&l))
        })
        .collect();
    let name = rest.join(" ").trim().to_lowercase();
    if name.is_empty() {
        g.label.clone()
    } else {
        name
    }
}

fn is_version_word(w: &str) -> bool {
    let mut parts = w.split('.');
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if !a.is_empty() && !b.is_empty() && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()))
}

/// The rc file editor's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcState {
    Reading,
    Failed,
    Ready,
    Saving,
}

/// What the Save row says of the file: where it stands against the one on the server.
pub fn rc_word(state: RcState, text: &str, saved: Option<&str>) -> &'static str {
    match state {
        RcState::Reading => "reading…",
        RcState::Saving => "saving…",
        RcState::Failed => "not read",
        RcState::Ready => {
            if Some(text) == saved {
                "no changes"
            } else {
                "unsaved changes"
            }
        }
    }
}

/// The other games that read the rc file `rc` names, as words after `lead`; nothing when there are none.
pub fn shared_rc(games: &[GameLink], rc: &str, lead: &str) -> String {
    let also: Vec<&str> = games.iter().filter(|g| g.rc.as_deref() == Some(rc) && g.id != rc && !g.disabled).map(|g| g.label.as_str()).collect();
    match also.len() {
        0 => String::new(),
        1 => format!("{lead}{} reads it too", also[0]),
        n => format!("{lead}{} and {} read it too", also[..n - 1].join(", "), also[n - 1]),
    }
}

/// Which build an offline game plays, in the message line's small print: crawl's version less
/// its commit hash (`0.34.1-4`).
pub fn engine_build(version: &str) -> Option<String> {
    let v = match version.rfind("-g") {
        Some(i) if i + 2 < version.len() && version[i + 2..].chars().all(|c| c.is_ascii_hexdigit()) => &version[..i],
        _ => version,
    };
    (!v.is_empty()).then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> LobbyEntry {
        LobbyEntry {
            id: 1,
            username: "bob".into(),
            game_id: "dcss-0.34".into(),
            xl: Some("12".into()),
            char: Some("MiBe".into()),
            place: Some("D:7".into()),
            god: Some("Trog".into()),
            dur: Some("7684".into()),
            idle_time: Some(150),
            spectator_count: Some(2),
            milestone: Some("killed Sigmund.".into()),
            ..Default::default()
        }
    }

    #[test]
    fn roster_words() {
        let e = entry();
        assert_eq!(roster_here(&e), "You see here bob (MiBe, XL12, D:7), a worshipper of Trog, 2h 8m played, idle 2m, 2 watching: killed Sigmund.");
        assert_eq!(roster_cell(&e, "dur"), "2h 8m");
        assert_eq!(roster_cell(&e, "idle_time"), "2m");
        assert_eq!(format_idle(42), "42s");
        assert_eq!(format_idle(9_662_400), "2684h");
        assert_eq!(format_duration("27"), "27s");
        assert_eq!(format_duration("1080"), "18m");
        assert_eq!(roster_details(&e, 1600.0), Vec::<String>::new());
        assert_eq!(roster_details(&e, 1000.0), vec!["Watching 2", "Time 2h 8m", "Idle 2m"]);
        assert_eq!(roster_details(&e, 800.0).len(), 6);
    }

    #[test]
    fn exit_words() {
        assert_eq!(exit_reason_message("dead", None), None);
        assert_eq!(exit_reason_message("crash", Some("Iris")).as_deref(), Some("Iris' game crashed."));
        assert_eq!(exit_reason_message("weird", None).as_deref(), Some("Unfortunately your game ended unexpectedly. (weird)"));
        assert_eq!(exit_reason_message("unknown", None).as_deref(), Some("Unfortunately your game ended unexpectedly."));
        assert_eq!(exit_title("dead", None), "You died");
        assert_eq!(exit_title("saved", Some("bob")), "bob’s game was saved");
    }

    #[test]
    fn modes() {
        let g = |id: &str, label: &str| GameLink { id: id.into(), label: label.into(), ..Default::default() };
        assert_eq!(mode_name(&g("seeded-web-0.34", "Custom seed 0.34")), "seed");
        assert_eq!(mode_name(&g("spr-web-trunk", "DCSS trunk Sprint")), "sprint");
        assert_eq!(mode_name(&g("tut-web-trunk", "Tutorial trunk")), "tutorial");
        assert_eq!(mode_name(&g("zd-0.17", "Zot Defence 0.17")), "zot defence");
        assert_eq!(mode_name(&g("x", "DCSS 0.34 Hellcrawl")), "hellcrawl");
    }

    #[test]
    fn rc_words() {
        assert_eq!(rc_word(RcState::Ready, "a", Some("a")), "no changes");
        assert_eq!(rc_word(RcState::Ready, "ab", Some("a")), "unsaved changes");
        let g = |id: &str, label: &str, rc: &str| GameLink { id: id.into(), label: label.into(), rc: Some(rc.into()), ..Default::default() };
        let games = [g("dcss-0.34", "DCSS 0.34", "dcss-0.34"), g("spr-0.34", "Sprint 0.34", "dcss-0.34"), g("tut-0.34", "Tutorial 0.34", "dcss-0.34")];
        assert_eq!(shared_rc(&games, "dcss-0.34", " — "), " — Sprint 0.34 and Tutorial 0.34 read it too");
        assert_eq!(shared_rc(&games[..2], "dcss-0.34", " · "), " · Sprint 0.34 reads it too");
        assert_eq!(engine_build("0.35-a0-1079-g0e95e087e2").as_deref(), Some("0.35-a0-1079"));
        assert_eq!(server_where(Some("Europe"), "cbro.berotato.org"), "Europe · cbro.berotato.org");
    }

    #[test]
    fn keys_to_moves() {
        let none = egui::Modifiers::NONE;
        assert_eq!(nav_dir(egui::Key::ArrowLeft, none, true), None);
        assert_eq!(nav_dir(egui::Key::ArrowUp, none, true), Some(NavDir::Up));
        assert_eq!(nav_dir(egui::Key::J, none, false), Some(NavDir::Down));
        assert_eq!(nav_dir(egui::Key::J, none, true), None);
        assert_eq!(nav_dir(egui::Key::ArrowUp, egui::Modifiers::CTRL, false), None);
    }
}

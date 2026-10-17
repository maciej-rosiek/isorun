//! What each of the front end's screens holds (from `menu.ts`): its head, its rows with what
//! stands beside them, its notices and what stands under them. Built afresh every frame from the
//! state; the layout draws it.

use crate::front::*;
use crate::settings;
use crate::words::{self, RcState};
use isorun_client::servers::{self, Account, LoginState, ServerInfo};
use isorun_webtiles::{game_link_rows, game_link_version, game_version_rows, GameLink};

/// The margin glyph for a player on this device, where a server account has `\`: a wall, a place of its own.
const DEVICE_MARKER: &str = "#";

/// Who waits in a game, when the server is the one saying so.
struct Save {
    sub: String,
    hint: String,
}

impl FrontEnd {
    /// The screen on show, as a list to draw.
    pub fn screen(&mut self) -> Screen {
        let mut s = match self.view.clone() {
            View::Home => self.home_screen(),
            View::Exit => self.home_screen(),
            View::Watch => self.watch_screen(),
            View::Settings => self.settings_screen(),
            View::SettingsGroup => self.settings_group_screen(),
            View::Controls => list(Cls::Settings, "Gamepad controls", Some("What every button does, on each layer."), vec![Row::back("Back to the controls.")]).below(Below::Controls),
            View::Accounts => self.accounts_screen(),
            View::Versions => self.versions_screen(),
            View::Rc => self.rc_screen(),
            View::Servers => self.servers_screen(),
            View::AddServer => self.add_server_screen(),
            View::Login => self.login_screen(),
            View::Register => self.register_screen(),
            View::Doc => self.doc_screen(),
            View::News => self.news_screen(),
            View::Boot => list(Cls::List, "", None, Vec::new()),
        };
        if let Some(e) = &self.error {
            s.notices.insert(0, Notice { text: e.clone(), error: true, hint: false, ask: false });
        }
        s
    }

    // ------------------------------------------------------------------ home

    /// The home screen: every offered DCSS version, then Watch and Settings. The account and the
    /// information live in the footer. Without an account, Play leads to adding one; Watch asks
    /// for a server. A login still to come puts Log in in Play's place.
    fn home_screen(&mut self) -> Screen {
        let chosen = servers::get_chosen_account().and_then(|a| servers::find_server(&a.server_id).map(|s| (a, s)));
        let mut rows: Vec<Row> = Vec::new();
        let mut notices: Vec<Notice> = Vec::new();
        let mut account_row: Option<Row> = None;
        if let Some((account, server)) = &chosen {
            let s = self.session_on(server, Some(&account.username));
            let open = s.is_some_and(|s| s.conn.open);
            let lobby = s.map(|s| s.state.lobby.clone());
            let logged_in = if open { lobby.as_ref().and_then(|l| l.username.clone()) } else { None };
            let closed = self.session.as_ref().is_some_and(|x| x.closed && x.server.id == server.id);
            let login = self.login_state_of(account);
            let problem = if closed {
                Some("Disconnected")
            } else if open && login == LoginState::Out {
                Some("Not logged in")
            } else {
                None
            };
            let mut r = Row::new("account", if server.offline { account.username.clone() } else { format!("{} · {}", logged_in.clone().unwrap_or(account.username.clone()), server.name) }, if server.offline { DEVICE_MARKER } else { "\\" }, "Switch accounts, add one, or remove one.", Act::Accounts);
            r.sub = problem.map(str::to_string);
            r.conn = Some(self.conn_of(account, true));
            account_row = Some(r);
            let games = self.games(server).unwrap_or_default();
            let links = game_link_rows(&games);
            let mut offered: Vec<(GameLink, Option<String>)> = Vec::new();
            offered.extend(links.latest.iter().map(|g| (g.clone(), links.latest_version.clone())));
            offered.extend(links.trunk.iter().map(|g| (g.clone(), Some("trunk".to_string()))));
            offered.extend(links.other.iter().map(|g| (g.clone(), None)));
            offered.retain(|(g, _)| !g.disabled);
            if login == LoginState::Out && !server.offline {
                let mut r = Row::new("login", "Log in", "\\", format!("{} asks who you are before it offers a game.", server.host), Act::Login).sub(format!("as {}", account.username)).main().gap();
                r.gap = true;
                rows.push(r);
            } else {
                let who = logged_in.clone().unwrap_or(account.username.clone());
                let where_ = self.where_for(server, Some(&who));
                // one row per version, in the lobby's own order: the latest release leads, always, then
                // trunk. The row under the cursor is the same one every time the screen comes up
                let first = offered.first().map(|(g, _)| g.id.clone());
                for (g, version) in &offered {
                    let save = self.save_of(g, version.as_deref(), Some(&who), where_.as_ref());
                    let build = if server.offline { isorun_client::engine::install().and_then(|i| words::engine_build(&i.build.crawl_version)) } else { None };
                    let mut r = Row::new(
                        format!("play:{}", g.id),
                        format!("{} {}", if save.is_some() { "Continue" } else { "Play" }, g.label),
                        ">",
                        match (&save, server.offline) {
                            (Some(s), _) => s.hint.clone(),
                            (None, true) => format!("A new character in {}, on this device.", g.label),
                            (None, false) => format!("A new game of {} on {}.", g.label, server.host),
                        },
                        Act::Play(account.clone(), g.id.clone()),
                    );
                    r.sub = save.as_ref().map(|s| s.sub.clone()).or(g.save.as_ref().map(|s| format!("[{s}]")));
                    r.main = Some(&g.id) == first.as_ref();
                    r.fine = build;
                    rows.push(r);
                }
                if offered.is_empty() {
                    let complete = lobby.as_ref().is_some_and(|l| l.complete);
                    notices.push(hint(if !open { "Connecting…" } else if complete { "No games offered by this server." } else { "Loading game versions…" }));
                }
            }
            // nobody else plays on this device
            if !server.offline {
                let n = lobby.as_ref().map(|l| l.entries.len()).unwrap_or(0);
                let complete = lobby.as_ref().is_some_and(|l| l.complete);
                rows.push(Row::new("watch", "Watch", "{", format!("Look into the pool: who is playing on {} right now, and watch them.", server.host), Act::Watch).sub(if open {
                    if complete {
                        format!("{n} playing")
                    } else {
                        "loading…".into()
                    }
                } else {
                    "connecting…".into()
                }));
            }
            // the server's notices
            if let Some(l) = &lobby {
                if l.account_hold {
                    notices.push(notice("This account is being held for administrator approval. Until approved, community features and some game modes may be restricted, and games will not be visible to other players."));
                }
                if let Some(st) = &l.stale_processes {
                    notices.push(notice(&format!("Closing your last session on {}. Your game starts in about {} seconds.", server.host, if st.timeout > 0 { st.timeout } else { 10 })));
                } else if self.lost {
                    notices.push(notice("Connection lost. Your game is saved: Continue picks it up."));
                }
                if l.force_terminate {
                    let game = l.stale_processes.as_ref().map(|s| s.game.clone()).filter(|g| !g.is_empty()).unwrap_or("game".into());
                    notices.push(Notice { text: format!("Couldn't stop one of your stale {game} processes gracefully. Force its termination? [yn]"), error: false, hint: false, ask: true });
                }
                if open && l.username.is_none() {
                    if let Some(f) = &l.login_failed {
                        notices.push(Notice { text: f.clone(), error: true, hint: false, ask: false });
                    }
                }
            } else if self.lost {
                notices.push(notice("Connection lost. Your game is saved: Continue picks it up."));
            }
        } else {
            // with an account to pick, Play picks one; with none, it adds one
            let any = !servers::list_accounts().is_empty();
            let offline = servers::offline_offered();
            let none = if offline { "Add an offline account on this device, or an account on a server." } else { "Choose a public server, then log in or create an account." };
            rows.push(Row::new("account", "Play", ">", if any { "Pick an account, or add one." } else { none }, if any || offline { Act::Accounts } else { Act::ServersAdd }).main());
            rows.push(Row::new("watch", "Watch", "{", "Pick a server and watch a live game. No login needed.", Act::ServersWatch).sub("No account needed"));
        }
        let mut utilities: Vec<Row> = account_row.into_iter().collect();
        let unread = crate::news::news_unread();
        let mut news = Row::new("news", "What’s new", "!", if unread { format!("What changed in Isorun {}, and before.", crate::news::VERSION) } else { "What changed in Isorun, newest first.".into() }, Act::News);
        news.dot = unread;
        utilities.push(news);
        if self.options.app_downloads {
            utilities.push(Row::new("app", "Get the app", "+", "Isorun as an app for Linux, SteamOS and Windows: offline games, no browser.", Act::App));
            utilities.push(Row::new("steam", "Add to Steam", "+", "Five steps that put the Linux app in a Steam Deck’s library.", Act::Steam));
        }
        utilities.push(Row::new("about", "About & credits", "?", "About Isorun, and the people behind the game.", Act::About));
        if self.options.testbed {
            utilities.push(Row::new("testbed", "Test bed", "?", "The renderer's test level: every kind of cell, a monster of each status.", Act::Testbed));
        }
        rows.push(Row::new("settings", "Settings", "?", format!("Graphics, controls, the HUD: kept on this device.{}", if self.options.pad { " (X)" } else { "" }), Act::Settings));
        if self.options.can_fullscreen {
            let full = self.options.fullscreen;
            rows.push(Row::new("fullscreen", if full { "Leave full screen" } else { "Full screen" }, "#", if full { "Back to a window. F11 or Escape does the same." } else { "Fill the screen with Isorun. F11 or Escape is the way back." }, Act::Fullscreen));
        }
        if self.options.can_quit {
            rows.push(Row::new("quit", "Quit", "<", "Close Isorun. A game in progress is kept on the server.", Act::Quit));
        }
        let mut s = list(Cls::Home, "Isorun", Some("An unofficial isometric client for Dungeon\u{a0}Crawl\u{a0}Stone\u{a0}Soup."), rows);
        s.splash = Some(self.splash);
        s.utilities = utilities;
        s.notices = notices;
        // how the last game ended: a dialog over this screen until closed; a saved game says
        // nothing unless the server had words
        if let Some(exit) = self.exit_of() {
            let reason = exit.reason.clone().unwrap_or_default();
            let why = words::exit_reason_message(&reason, exit.watched.as_deref());
            let words = exit.message.clone().unwrap_or_default().trim_end().to_string();
            if exit.watched.is_some() || why.is_some() || !words.is_empty() {
                if self.view != View::Exit {
                    self.set_view(View::Exit);
                }
                return self.exit_screen(&exit, why, words);
            } else if let Some(sess) = &mut self.session {
                sess.state.exit = None;
            }
        } else if self.view == View::Exit {
            self.set_view(View::Home);
        }
        s
    }

    /// How the last game ended, as the official lobby's exit dialog: the reason, the game's
    /// parting words, and its morgue file or character dump, over the home screen.
    fn exit_screen(&self, exit: &isorun_webtiles::GameExit, why: Option<String>, words: String) -> Screen {
        let reason = exit.reason.clone().unwrap_or_default();
        let mut rows = vec![Row::new("exit:close", "Close", "<", "Back to the front.", Act::ExitClose)];
        if let (Some(dump), Some(s)) = (&exit.dump, &self.session) {
            let text = match reason.as_str() {
                "saved" => "Character dump",
                "crash" => "Crash log",
                _ => "Morgue file",
            };
            let url = servers::morgue_url_for(&s.server, dump);
            let href = if dump.starts_with("http") { format!("{dump}.txt") } else { format!("{}{}{dump}.txt", s.server.http, if dump.starts_with('/') { "" } else { "/" }) };
            rows.push(Row::new("exit:dump", text, "?", format!("Read the {} here.", text.to_lowercase()), Act::ExitDump { title: text.into(), url, href }));
        }
        let mut s = list(Cls::Exit, &words::exit_title(&reason, exit.watched.as_deref()), None, rows);
        s.over = true;
        if let Some(w) = why {
            s.notices.push(Notice { text: w, error: false, hint: true, ask: false });
        }
        if !words.is_empty() {
            s.notices.push(Notice { text: words, error: false, hint: false, ask: false });
        }
        s
    }

    /// Who waits in a game, when the server is the one saying so: the lobby's own word, the
    /// roster (the account's game still open on the server), and `<player>.where`.
    fn save_of(&self, g: &GameLink, version: Option<&str>, who: Option<&str>, where_: Option<&crate::whereis::Whereis>) -> Option<Save> {
        let stairs = |c: &servers::LastCharacter| {
            let place = servers::describe_place(c);
            format!("Down the stairs to your game.{}", if place.is_empty() { String::new() } else { format!(" {place} lies below.") })
        };
        if let Some(save) = g.save.as_deref().filter(|s| *s != "playing" && *s != "slot full") {
            let sub = match who {
                Some(w) if save.starts_with(&format!("{w}, ")) => save[w.len() + 2..].to_string(),
                _ => save.to_string(),
            };
            return Some(Save { sub, hint: "Down the stairs to your game.".into() });
        }
        if let (Some(w), Some(s)) = (who, &self.session) {
            if s.conn.open {
                if let Some(e) = s.state.lobby.entries.iter().find(|e| e.game_id == g.id && e.username.eq_ignore_ascii_case(w)) {
                    let live = crate::whereis::character_of(&e.username, e.char.as_deref().unwrap_or(""), e.xl.as_deref().unwrap_or(""), e.place.as_deref().unwrap_or(""), e.title.as_deref().unwrap_or(""), e.god.as_deref().unwrap_or(""));
                    let place = servers::describe_place(&live);
                    return Some(Save { sub: servers::describe_character(&live), hint: format!("Down the stairs to your game, still open on the server.{}", if place.is_empty() { String::new() } else { format!(" {place} lies below.") }) });
                }
            }
        }
        // one `.where` per player: it answers for its own version and says nothing about any other
        if let (Some(w), Some(v)) = (where_, version) {
            if w.version.as_deref() == Some(v) && crate::whereis::save_waiting(w) {
                return Some(Save { sub: servers::describe_character(&w.character), hint: stairs(&w.character) });
            }
        }
        None
    }

    // ------------------------------------------------------------------ watch

    /// Watch: the roster of games running, a table by player name.
    fn watch_screen(&mut self) -> Screen {
        self.update_roster();
        let chosen = if self.watch_on.is_some() { None } else { servers::get_chosen_account() };
        let server = self.watch_on.clone().or_else(|| chosen.as_ref().and_then(|a| servers::find_server(&a.server_id)));
        let Some(server) = server else { return list(Cls::Watch, "Watch", None, vec![Row::back("Back to the front.")]) };
        let s = self.session.as_ref();
        let open = s.is_some_and(|s| s.conn.open);
        let complete = s.is_some_and(|s| s.state.lobby.complete);
        let all = s.map(|s| s.state.lobby.entries.len().min(200)).unwrap_or(0);
        let lede = format!(
            "{}{} · {}",
            server.host,
            chosen.as_ref().map(|a| format!(" · {}", a.username)).unwrap_or_default(),
            if !open {
                "connecting…".to_string()
            } else if complete {
                format!("{all} playing")
            } else {
                "loading…".into()
            }
        );
        let mut sc = list(Cls::Watch, &format!("Watch · {}", server.name), Some(&lede), vec![Row::back("Back to the front.")]);
        sc.below = Below::Roster;
        let empty = if !open {
            format!("Connecting to {}…", server.host)
        } else if !complete {
            "Loading games…".into()
        } else if all == 0 {
            "Nobody is playing right now.".into()
        } else {
            String::new()
        };
        if !empty.is_empty() {
            sc.notices.push(hint(&empty));
        }
        sc.focus = self.marks.get(&View::Watch).cloned().or_else(|| self.roster_entries().first().map(|e| format!("watch:{}", e.id)));
        sc
    }

    // ------------------------------------------------------------------ settings

    fn settings_screen(&self) -> Screen {
        let mut rows = vec![Row::back("Back to where you were.")];
        for (g, h) in settings::SETTING_GROUPS {
            rows.push(Row::new(format!("settings:{g}"), g, ">", h, Act::SettingsGroup(g.into())).sub(h));
        }
        list(Cls::Settings, "Settings", Some("They are kept on this device."), rows)
    }

    fn settings_group_screen(&self) -> Screen {
        let group = self.settings_group.clone().unwrap_or_else(|| "Graphics".into());
        let mut s = list(Cls::Settings, &group, Some(settings::group_hint(&group)), vec![Row::back("Back to the settings.")]);
        s.below = Below::Settings(group);
        s
    }

    /// The focusable rows of a settings page: each setting, then the page's own actions.
    pub fn settings_items(&self, group: &str) -> Vec<(String, Act)> {
        let mut out: Vec<(String, Act)> = settings::group_rows(group).into_iter().map(|r| (format!("setting:{}", r.key), Act::Setting(r.key.into()))).collect();
        if group == "Controls" {
            out.push(("replay-gamepad-tips".into(), Act::ReplayTips));
            out.push(("gamepad-controls".into(), Act::Controls));
        }
        out.push(("settings-reset".into(), Act::ResetSettings(group.into())));
        out
    }

    /// What the message line says on a settings page's row.
    pub fn settings_hint(&self, group: &str, id: &str) -> Option<String> {
        if let Some(key) = id.strip_prefix("setting:") {
            return settings::rows().into_iter().find(|r| r.key == key).map(|r| r.hint.to_string());
        }
        Some(match id {
            "replay-gamepad-tips" => "Forget which gamepad controls you have used and teach them again: switches Hints to Adaptive.".into(),
            "gamepad-controls" => "What every button on the pad does, on each layer.".into(),
            "settings-reset" => format!("Put every {} setting back to its default.", group.to_lowercase()),
            _ => return None,
        })
    }

    // ------------------------------------------------------------------ accounts

    /// The accounts on this device, in two groups: on this device (Offline), then on servers, each
    /// with its own Add at its foot. The chosen one reads "in use" under it.
    fn accounts_screen(&mut self) -> Screen {
        let accounts = servers::list_accounts();
        let chosen = servers::get_chosen_account();
        let offline = servers::offline_offered();
        let mut rows = vec![Row::back("Back to the front.")];
        let account_row = |f: &mut FrontEnd, rows: &mut Vec<Row>, a: &Account| {
            let Some(server) = servers::find_server(&a.server_id) else { return };
            let is_chosen = servers::same_account(Some(a), chosen.as_ref());
            let mut r = Row::new(format!("account:{}/{}", a.server_id, a.username), if server.offline { a.username.clone() } else { format!("{} · {}", a.username, server.name) }, if server.offline { DEVICE_MARKER } else { "\\" }, format!("Play as {} on {}.", a.username, server.host), Act::ChooseAccount(a.clone()));
            if is_chosen {
                r.sub = Some("in use".into());
                r.conn = Some(f.conn_of(a, true));
                r.main = true;
            }
            let id = format!("{}/{}", a.server_id, a.username);
            if server.offline {
                r.also.push(Beside { id: format!("rc:{id}"), label: "(edit rc)".into(), title: format!("{}'s rc file: the options every game on this device starts with.", a.username), act: Act::Rc(a.clone(), None) });
                let asking = f.deleting.as_ref().is_some_and(|d| servers::same_account(Some(d), Some(a)));
                r.also.push(if asking {
                    Beside { id: format!("delete:{id}"), label: "(delete, and its saves?)".into(), title: format!("Delete {} and every character saved under it. This cannot be undone.", a.username), act: Act::Delete(a.clone()) }
                } else {
                    Beside { id: format!("delete:{id}"), label: "(delete)".into(), title: format!("Delete {} from this device, and its saved characters.", a.username), act: Act::AskDelete(a.clone()) }
                });
            } else {
                r.also.push(Beside { id: format!("versions:{id}"), label: "(other versions)".into(), title: format!("Every version {} offers, with its Sprint, tutorial and seeded games.", server.name), act: Act::Versions(a.clone()) });
                if f.login_state_of(a) == LoginState::Out {
                    r.also.push(Beside { id: format!("login:{id}"), label: "(log in)".into(), title: format!("{} has forgotten {}'s login: log in again.", server.host, a.username), act: Act::LoginAs(a.clone()) });
                }
                r.also.push(Beside { id: format!("remove:{id}"), label: "(remove)".into(), title: format!("Take {} off this device, and forget its login. The account stays on {}.", a.username, server.host), act: Act::Remove(a.clone()) });
            }
            rows.push(r);
        };
        if offline {
            rows.push(Row::heading("offline", "Offline"));
            for a in accounts.iter().filter(|a| a.server_id == "offline") {
                account_row(self, &mut rows, a);
            }
            rows.push(Row::new("add-player", "Add an offline account", "+", "An account on this device: a name, no password, and saved characters of its own.", Act::AddPlayer).sub("on this device"));
            rows.push(Row::heading("servers", "Servers"));
        }
        for a in accounts.iter().filter(|a| a.server_id != "offline") {
            account_row(self, &mut rows, a);
        }
        // asked ahead, so the home screen an account is switched to comes up with its Continue already there
        for a in &accounts {
            if let Some(server) = servers::find_server(&a.server_id) {
                self.where_for(&server, Some(&a.username));
            }
        }
        let on_servers = accounts.iter().any(|a| a.server_id != "offline");
        let mut add = Row::new("add", if offline { "Add a server account" } else { "Add an account" }, "+", "Choose a server, then log in or register.", Act::ServersAdd).sub(if on_servers { "on any server" } else { "to play online" });
        add.gap = !offline;
        add.main = accounts.is_empty();
        rows.push(add);
        list(Cls::List, "Accounts", Some("The accounts on this device."), rows)
    }

    /// Everything an account's server offers, one row per release, the modes beside it.
    fn versions_screen(&mut self) -> Screen {
        let Some(a) = self.versions_of.clone() else { return list(Cls::List, "Other versions", None, vec![Row::back("Back to the accounts.")]) };
        let Some(server) = servers::find_server(&a.server_id) else { return list(Cls::List, "Other versions", None, vec![Row::back("Back to the accounts.")]) };
        let s = self.session_on(&server, Some(&a.username));
        let open = s.is_some_and(|s| s.conn.open);
        let complete = s.is_some_and(|s| s.state.lobby.complete);
        let who = s.filter(|s| s.conn.open).and_then(|s| s.state.lobby.username.clone()).unwrap_or(a.username.clone());
        let where_ = self.where_for(&server, Some(&who));
        let games = self.games(&server).unwrap_or_default();
        let versions = game_version_rows(&games);
        let mut rows = vec![Row::back("Back to the accounts.")];
        for v in &versions {
            let save = self.save_of(&v.game, v.version.as_deref(), Some(&who), where_.as_ref());
            let mut r = Row::new(format!("play:{}", v.game.id), v.game.label.clone(), ">", save.as_ref().map(|s| s.hint.clone()).unwrap_or(format!("A new game of {} on {}.", v.game.label, server.host)), Act::Play(a.clone(), v.game.id.clone()));
            r.sub = save.map(|s| s.sub);
            if let Some(rc) = &v.game.rc {
                r.also.push(Beside { id: format!("rc:{rc}"), label: "(edit rc)".into(), title: format!("Your rc file for {}: the options crawl starts with{}.", v.game.label, words::shared_rc(&games, rc, " — ")), act: Act::Rc(a.clone(), Some(rc.clone())) });
            }
            for m in &v.modes {
                let saved = m.save.as_deref().is_some_and(|s| s != "playing" && s != "slot full");
                r.also.push(Beside {
                    id: format!("play:{}", m.id),
                    label: format!("({}{})", if saved { "continue " } else { "" }, words::mode_name(m)),
                    title: if saved { format!("Continue {}: {}.", m.label, m.save.clone().unwrap_or_default()) } else { format!("A new game of {} on {}.", m.label, server.host) },
                    act: Act::Play(a.clone(), m.id.clone()),
                });
            }
            rows.push(r);
        }
        let mut sc = list(Cls::List, &format!("Other versions · {}", server.name), Some("Every version the server offers, and its other modes."), rows);
        if versions.is_empty() {
            sc.notices.push(hint(if !open { "Connecting…" } else if complete { "No games offered by this server." } else { "Loading game versions…" }));
        }
        sc
    }

    /// An account's rc file: the options crawl starts with, as text to edit and save back.
    fn rc_screen(&self) -> Screen {
        let Some(rc) = &self.rc else { return list(Cls::Rc, "rc file", None, vec![Row::back("Back.")]) };
        let server = servers::find_server(&rc.account.server_id);
        let offline = server.as_ref().is_some_and(|s| s.offline);
        let host = server.as_ref().map(|s| s.host.clone()).unwrap_or_default();
        let place = if offline { "this device".to_string() } else { host.clone() };
        let s = server.as_ref().and_then(|sv| self.session_on(sv, Some(&rc.account.username)));
        let logged_in = s.is_some_and(|s| s.conn.open && s.state.lobby.username.is_some());
        let games = match &server {
            Some(sv) if !sv.offline => self.games(sv).unwrap_or_default(),
            _ => Vec::new(),
        };
        let game = games.iter().find(|g| Some(&g.id) == rc.game_id.as_ref());
        let mut notices = Vec::new();
        if let Some((text, error)) = &rc.note {
            notices.push(Notice { text: text.clone(), error: *error, hint: !*error, ask: false });
        } else if rc.state == RcState::Reading {
            let open = s.is_some_and(|s| s.conn.open);
            notices.push(hint(&if !open {
                "Connecting…".to_string()
            } else if logged_in || s.is_some_and(|s| s.logging_in) {
                format!("Reading your rc file from {place}…")
            } else {
                format!("Log in to {host} to read your rc file.")
            }));
        }
        let version = if offline { None } else { game.and_then(game_link_version) };
        let guide = format!("https://github.com/crawl/crawl/blob/{}/crawl-ref/docs/options_guide.txt", match version.as_deref() {
            Some(v) if v != "trunk" => format!("stone_soup-{v}"),
            _ => "master".into(),
        });
        let mut text_row = Row::new("rc:text", "rc file", "", format!("Crawl’s options, one to a line.{}", if self.options.pad { " A to type, Y when done; the right stick scrolls." } else { " Enter to edit, Esc when done, Ctrl+S to save." }), Act::None);
        text_row.text = true;
        let mut back = Row::back(if offline { "Back to the accounts." } else { "Back to the versions." });
        if rc.leaving {
            back.label = "Back, without saving?".into();
        }
        let mut save = Row::new("rc:save", "Save", "", if offline { "Keep it on this device. It counts from the next game you start or continue.".to_string() } else { format!("Save it on {host}. It counts from the next game you start or continue.") }, Act::RcSave).sub(words::rc_word(rc.state, &rc.text, rc.saved.as_deref()));
        save.also.push(Beside { id: "rc:revert".into(), label: if rc.state == RcState::Failed { "(try again)".into() } else { "(revert)".into() }, title: if rc.state == RcState::Failed { format!("Ask {place} for the file again.") } else { format!("Put back the file as {place} has it, and lose what was not saved.") }, act: Act::RcRevert });
        save.also.push(Beside { id: "rc:guide".into(), label: "(options guide)".into(), title: "Every option crawl reads, in its own guide. Opens in a new window.".into(), act: Act::Open(guide) });
        let lede = if offline {
            format!("{} · every game on this device", rc.account.username)
        } else {
            format!("{} on {}{}{}", rc.account.username, server.as_ref().map(|s| s.name.clone()).unwrap_or_default(), game.map(|g| format!(" · {}", g.label)).unwrap_or_default(), rc.game_id.as_ref().map(|g| words::shared_rc(&games, g, " · ")).unwrap_or_default())
        };
        let mut sc = list(Cls::Rc, "rc file", Some(&lede), vec![back, text_row, save]);
        sc.notices = notices;
        sc
    }

    // ------------------------------------------------------------------ servers

    /// The server list: one row per server with its region and host (and the accounts kept there).
    fn servers_screen(&mut self) -> Screen {
        let watch = self.servers_mode_watch;
        let accounts = servers::list_accounts();
        let mut rows = vec![Row::back("Back.")];
        for sv in servers::list_servers() {
            let mine: Vec<String> = accounts.iter().filter(|a| a.server_id == sv.id).map(|a| a.username.clone()).collect();
            let mut also = Vec::new();
            if !watch {
                also.push(Beside { id: format!("site:{}", sv.id), label: "(site ↗)".into(), title: format!("Open {} in a new window: the server's own page, with its news, rules and how to support it.", sv.host), act: Act::Open(format!("{}/", sv.http)) });
            }
            // one the player added comes off again, unless an account still stands on it
            if sv.custom && mine.is_empty() {
                also.push(Beside { id: format!("remove:{}", sv.id), label: "(remove)".into(), title: format!("Take {} off the list.", sv.host), act: Act::RemoveServer(sv.id.clone()) });
            }
            let ping = self.ping_of(&sv).map(|ms| format!(" · {ms} ms")).unwrap_or_default();
            let sub = format!("{}{}{}", if sv.custom { "added on this device".to_string() } else { words::server_where(sv.region.as_deref(), &sv.host) }, if mine.is_empty() { String::new() } else { format!(" · {}", mine.join(", ")) }, ping);
            let hint = if watch {
                format!("Watch who is playing on {}.", sv.host)
            } else {
                format!("An account on {}.{}", sv.host, if mine.is_empty() { String::new() } else { format!(" {} {} already here.", mine.join(" and "), if mine.len() == 1 { "is" } else { "are" }) })
            };
            let mut r = Row::new(format!("server:{}", sv.id), sv.name.clone(), "\\", hint, Act::PickServer(sv.clone())).sub(sub);
            r.also = also;
            rows.push(r);
        }
        rows.push(Row::new("add-server", "Add a server", "+", "A WebTiles server not listed here, by its address.", Act::AddServer).sub("one not on this list").gap());
        let title = if watch {
            "Watch"
        } else if servers::offline_offered() {
            "Add a server account"
        } else {
            "Add an account"
        };
        list(Cls::List, title, Some("Choose a server."), rows)
    }

    fn add_server_screen(&self) -> Screen {
        let typing = if self.options.pad { " Press X to type." } else { "" };
        let rows = vec![
            Row::back("Back to the servers."),
            Row::field("address", "Address", Field::Address, format!("Its host, or the address of its page: crawl.example.org, or http://crawl.example.org:8080.{typing}")),
            Row::new("add", "Add", "+", "Add it to the list, to log in or watch there.", Act::SubmitAddServer).sub("to the server list").main(),
        ];
        let mut s = list(Cls::Form, "Add a server", Some(if self.servers_mode_watch { "To watch" } else { "For an account" }), rows);
        s.focus = Some("address".into());
        s
    }

    /// The lobby page's login form; a new player on this device has a name and nothing else.
    fn login_screen(&self) -> Screen {
        let typing = if self.options.pad { " Press X to type." } else { "" };
        if self.adding.as_ref().is_some_and(|a| a.offline) {
            let rows = vec![
                Row::back("Back to the accounts."),
                Row::field("username", "Name", Field::Name, format!("Letters, digits, - and _.{typing}")),
                Row::new("login", "Add", "+", "Add this player and go to its games.", Act::SubmitNewProfile).sub("this device").main(),
            ];
            return list(Cls::Form, "A new player", Some("On this device"), rows);
        }
        let chosen = if self.adding.is_some() { None } else { servers::get_chosen_account().and_then(|a| servers::find_server(&a.server_id)) };
        let server = self.adding.clone().or(chosen).unwrap_or_else(servers::offline_server);
        let mut rows = vec![
            Row::back("Back without logging in."),
            Row::field("username", "Name", Field::Name, format!("Your name on {}.{typing}", server.host)),
            Row::field("password", "Password", Field::Password, format!("It is sent to the server once; only a login token is kept on this device.{typing}")),
            Row::new("login", "Log in", "\\", format!("Log in to {}.", server.host), Act::SubmitLogin).sub(server.name.clone()).main(),
        ];
        if self.adding.is_some() {
            rows.push(Row::new("register", "Register", "+", "A new account on this server.", Act::Register).sub("a new account").gap());
        }
        let mut s = list(Cls::Form, "Log in", Some(&server.name), rows);
        if let Some(f) = self.session.as_ref().filter(|s| s.server.id == server.id).and_then(|s| s.state.lobby.login_failed.clone()) {
            s.notices.push(Notice { text: f, error: true, hint: false, ask: false });
        }
        s
    }

    fn register_screen(&self) -> Screen {
        let typing = if self.options.pad { " Press X to type." } else { "" };
        let server = self.adding.clone().or(servers::get_chosen_account().and_then(|a| servers::find_server(&a.server_id))).unwrap_or_else(servers::offline_server);
        let rows = vec![
            Row::back("Back to the login."),
            Row::field("username", "Name", Field::Name, format!("What your name will be on {}.{typing}", server.host)),
            Row::field("password", "Password", Field::Password, format!("Choose a password. It is sent to the server once; only a login token is kept on this device.{typing}")),
            Row::field("password2", "Repeat", Field::Password2, format!("Enter it again.{typing}")),
            Row::field("email", "Email", Field::Email, format!("Optional; the server uses it to reset a lost password.{typing}")),
            Row::new("register", "Register", "\\", format!("Register on {}.", server.host), Act::SubmitRegister).sub(server.name.clone()).main(),
        ];
        let mut s = list(Cls::Form, "Register", Some(&format!("A new account on {}", server.name)), rows);
        if let Some(f) = self.session.as_ref().filter(|s| s.server.id == server.id).and_then(|s| s.state.lobby.register_failed.clone()) {
            s.notices.push(Notice { text: f, error: true, hint: false, ask: false });
        }
        s
    }

    // ------------------------------------------------------------------ documents

    fn doc_screen(&self) -> Screen {
        let Some((title, lede, body, _)) = &self.doc else {
            // Get the app: the desktop app's packages from the latest release
            let mut rows = vec![Row::back("Back.")];
            for (file, label, sub, hint) in APP_DOWNLOADS {
                rows.push(Row::new(format!("app:{file}"), label, "+", hint, Act::Open(format!("{LATEST_RELEASE_DOWNLOAD}{file}"))).sub(sub));
            }
            rows.push(Row::new("steam", "Add to Steam", "?", "Five steps that put the Linux app in a Steam Deck’s library.", Act::Steam));
            return list(Cls::List, "Get the app", Some("Isorun as an app: offline games on this device, with no browser. The downloads come from the latest release on GitHub."), rows);
        };
        let pad = if self.options.pad { "Up and down scroll it; so does the right stick." } else { "Up and down scroll it." };
        let mut s = list(Cls::Doc, title, Some(lede.as_deref().unwrap_or(pad)), vec![Row::back("Back.")]);
        s.below = Below::Doc(body.clone());
        s.focus = Some(BACK.into());
        // the way out, only once reading here has failed
        if matches!(self.doc_text, Some(Err(_))) {
            s.links = self.doc_links.clone();
        }
        s
    }

    pub fn releases(&self) -> Vec<crate::news::Release> {
        let since = self.news_since.clone().flatten();
        crate::news::releases(crate::news::CHANGELOG, since.as_deref())
    }

    fn news_screen(&self) -> Screen {
        let mut s = list(Cls::News, "What’s new", Some(&crate::news::latest_line(crate::news::CHANGELOG)), vec![Row::back("Back.")]);
        s.below = Below::News;
        s.focus = self.releases().first().map(|r| format!("release:{}", r.version));
        s
    }
}

/// A list screen with no extras.
pub fn list(cls: Cls, title: &str, lede: Option<&str>, rows: Vec<Row>) -> Screen {
    Screen { cls, title: title.to_string(), lede: lede.map(str::to_string), splash: None, rows, utilities: Vec::new(), notices: Vec::new(), below: Below::None, focus: None, over: false, links: Vec::new() }
}

impl Screen {
    pub fn below(mut self, b: Below) -> Self {
        self.below = b;
        self
    }
}

fn notice(text: &str) -> Notice {
    Notice { text: text.into(), error: false, hint: false, ask: false }
}

fn hint(text: &str) -> Notice {
    Notice { text: text.into(), error: false, hint: true, ask: false }
}

#[allow(dead_code)]
fn _server(_: &ServerInfo) {}

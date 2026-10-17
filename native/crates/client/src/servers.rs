//! The servers, the accounts on them and the settings kept on this device (the port of
//! `servers.ts`). The keys and the shapes of what is stored are the web build's, so the two read
//! the same data.

use crate::platform::storage::{has, load, save};
use crate::platform::now_ms;
use isorun_webtiles::GameLink;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const APP_NAME: &str = "Isorun";
pub const HOME_TITLE: &str = "Isorun - Dungeon Crawl Stone Soup";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub ws: String,
    pub http: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom: bool,
    /// Games on this device: no socket, the engine in the app.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub offline: bool,
}

/// The hosted page, whose Worker serves the proxies a shell without its own uses.
pub const HOSTED_ORIGIN: &str = "https://isorun.maciej-rosiek.workers.dev";

/// The server that is this device: the offline engine. Its accounts are profiles.
pub fn offline_server() -> ServerInfo {
    ServerInfo { id: "offline".into(), name: "Offline".into(), region: None, ws: String::new(), http: String::new(), host: "this device".into(), custom: false, offline: true }
}

pub const DEFAULT_PROFILE: &str = "Player";

/// Whether Offline is offered: wherever the engine is in the build.
pub fn offline_offered() -> bool {
    crate::engine::available()
}

const KEY: &str = "isorun.servers";
const TOKEN_KEY: &str = "isorun.tokens";
const LAST_KEY: &str = "isorun.last";
const GAMES_KEY: &str = "isorun.games";
const ACCOUNTS_KEY: &str = "isorun.accounts";
const ACCOUNT_KEY: &str = "isorun.account";
const USED_KEY: &str = "isorun.accountUsed";
const MORGUE_KEY: &str = "isorun.morgue";
const SETTINGS_KEY: &str = "isorun.settings";
const CAMERA_KEY: &str = "isorun.camera";

pub fn bundled() -> Vec<ServerInfo> {
    serde_json::from_str(include_str!("../data/servers.json")).unwrap_or_default()
}

pub fn list_servers() -> Vec<ServerInfo> {
    let mut all = bundled();
    all.extend(load::<Vec<ServerInfo>>(KEY, Vec::new()));
    all
}

/// Add a server by its host or the address of its page; a host already listed gives back that server.
pub fn add_server(url: &str) -> Result<ServerInfo, String> {
    let text = url.trim();
    if text.is_empty() {
        return Err("Enter a host or an address.".into());
    }
    let with_scheme = if text.contains("://") { text.to_string() } else { format!("https://{text}") };
    let (scheme, rest) = with_scheme.split_once("://").ok_or(format!("{text} is not an address."))?;
    let scheme = scheme.to_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("A server is reached over http or https.".into());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").to_lowercase();
    if host.is_empty() || text.chars().any(char::is_whitespace) || !host.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("{text} is not an address."));
    }
    if let Some(had) = list_servers().into_iter().find(|s| !s.offline && s.host.to_lowercase() == host) {
        return Ok(had);
    }
    let ws = format!("{}{host}/socket", if scheme == "http" { "ws://" } else { "wss://" });
    let info = ServerInfo { id: host.clone(), name: host.clone(), region: None, ws, http: format!("{scheme}://{host}"), host, custom: true, offline: false };
    let mut custom: Vec<ServerInfo> = load(KEY, Vec::new());
    custom.retain(|s| s.id != info.id);
    custom.push(info.clone());
    save(KEY, &custom);
    Ok(info)
}

pub fn remove_server(id: &str) {
    let mut custom: Vec<ServerInfo> = load(KEY, Vec::new());
    custom.retain(|s| s.id != id);
    save(KEY, &custom);
}

/// A server by id or host, from the bundled list or the custom ones, or this device's.
pub fn find_server(id_or_host: &str) -> Option<ServerInfo> {
    if offline_offered() && id_or_host == "offline" {
        return Some(offline_server());
    }
    list_servers().into_iter().find(|s| s.id == id_or_host || s.host == id_or_host)
}

// ------------------------------------------------------------------ accounts

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub username: String,
}

fn token_key(server_id: &str, username: &str) -> String {
    format!("{server_id}/{}", username.to_lowercase())
}

pub fn same_account(a: Option<&Account>, b: Option<&Account>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.server_id == b.server_id && a.username.to_lowercase() == b.username.to_lowercase())
}

fn stored_accounts() -> Vec<Account> {
    migrate_accounts();
    load::<Vec<Account>>(ACCOUNTS_KEY, Vec::new()).into_iter().filter(|a| !a.server_id.is_empty() && !a.username.is_empty()).collect()
}

/// Older builds kept one token per server and the chosen server on its own: those make one account.
fn migrate_accounts() {
    if has(ACCOUNTS_KEY) {
        return;
    }
    let mut tokens = read_tokens();
    let last: Option<LastPlayed> = load(LAST_KEY, None);
    let mut accounts = Vec::new();
    for (server_id, token) in tokens.clone() {
        if server_id.contains('/') || find_server(&server_id).is_none() {
            continue;
        }
        tokens.remove(&server_id);
        let username = last.as_ref().filter(|l| l.server_id == server_id).and_then(|l| l.username.clone());
        let Some(username) = username else { continue };
        tokens.insert(token_key(&server_id, &username), token);
        accounts.push(Account { server_id, username });
    }
    save(TOKEN_KEY, &tokens);
    save(ACCOUNTS_KEY, &accounts);
    let chosen_server: Option<String> = load("isorun.server", None);
    let chosen = accounts.iter().find(|a| Some(&a.server_id) == chosen_server.as_ref()).or(accounts.first());
    if let Some(c) = chosen {
        save(ACCOUNT_KEY, c);
    }
}

/// The accounts on this device, the one picked last first; offline profiles only where Offline is offered.
pub fn list_accounts() -> Vec<Account> {
    let mut all: Vec<Account> = stored_accounts().into_iter().filter(|a| offline_offered() || a.server_id != "offline").collect();
    let used: BTreeMap<String, f64> = load(USED_KEY, BTreeMap::new());
    let at = |a: &Account| used.get(&token_key(&a.server_id, &a.username)).copied().unwrap_or(0.0);
    all.sort_by(|a, b| at(b).partial_cmp(&at(a)).unwrap_or(std::cmp::Ordering::Equal));
    all
}

pub fn add_account(a: &Account) {
    let mut all: Vec<Account> = stored_accounts().into_iter().filter(|x| !same_account(Some(x), Some(a))).collect();
    all.push(a.clone());
    save(ACCOUNTS_KEY, &all);
}

/// A device with no account yet gets a player of its own, chosen.
pub fn ensure_device_account() {
    if !offline_offered() || !stored_accounts().is_empty() {
        return;
    }
    let player = Account { server_id: "offline".into(), username: DEFAULT_PROFILE.into() };
    add_account(&player);
    set_chosen_account(Some(&player));
}

pub fn remove_account(a: &Account) {
    let all: Vec<Account> = stored_accounts().into_iter().filter(|x| !same_account(Some(x), Some(a))).collect();
    save(ACCOUNTS_KEY, &all);
    set_token(&a.server_id, &a.username, None, None);
    let mut used: BTreeMap<String, f64> = load(USED_KEY, BTreeMap::new());
    used.remove(&token_key(&a.server_id, &a.username));
    save(USED_KEY, &used);
    if same_account(get_chosen_account().as_ref(), Some(a)) {
        save(ACCOUNT_KEY, &list_accounts().first().cloned());
    }
}

pub fn get_chosen_account() -> Option<Account> {
    migrate_accounts();
    load::<Option<Account>>(ACCOUNT_KEY, None).filter(|a| !a.server_id.is_empty() && !a.username.is_empty() && find_server(&a.server_id).is_some())
}

pub fn set_chosen_account(a: Option<&Account>) {
    save(ACCOUNT_KEY, &a);
    let Some(a) = a else { return };
    let mut used: BTreeMap<String, f64> = load(USED_KEY, BTreeMap::new());
    used.insert(token_key(&a.server_id, &a.username), now_ms());
    save(USED_KEY, &used);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum StoredToken {
    Bare(String),
    Timed { token: String, #[serde(default, skip_serializing_if = "Option::is_none")] until: Option<f64> },
}

fn read_tokens() -> BTreeMap<String, StoredToken> {
    load(TOKEN_KEY, BTreeMap::new())
}

const DAY_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// The account's token, or none when there is none or the server will have forgotten it.
pub fn get_token(server_id: &str, username: &str) -> Option<String> {
    let mut all = read_tokens();
    let key = token_key(server_id, username);
    match all.get(&key)? {
        StoredToken::Bare(t) => Some(t.clone()).filter(|t| !t.is_empty()),
        StoredToken::Timed { token, until } => {
            if until.is_some_and(|u| now_ms() >= u) {
                all.remove(&key);
                save(TOKEN_KEY, &all);
                return None;
            }
            Some(token.clone()).filter(|t| !t.is_empty())
        }
    }
}

/// File `token` under the account, or forget it; `expires_days` is `login_cookie`'s `expires`.
pub fn set_token(server_id: &str, username: &str, token: Option<&str>, expires_days: Option<f64>) {
    let mut all = read_tokens();
    let key = token_key(server_id, username);
    match token {
        Some(t) => {
            let until = expires_days.filter(|d| d.is_finite() && *d > 0.0).map(|d| now_ms() + d * DAY_MS);
            all.insert(key, StoredToken::Timed { token: t.to_string(), until });
        }
        None => {
            all.remove(&key);
        }
    }
    save(TOKEN_KEY, &all);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginState {
    In,
    Pending,
    Out,
}

pub fn login_state(account: Option<&Account>, logged_in: Option<&str>, logging_in: bool) -> LoginState {
    if logged_in.is_some_and(|s| !s.is_empty()) {
        return LoginState::In;
    }
    if logging_in {
        return LoginState::Pending;
    }
    if let Some(a) = account {
        if a.server_id == "offline" || get_token(&a.server_id, &a.username).is_some() {
            return LoginState::Pending;
        }
    }
    LoginState::Out
}

/// A character as a roster line or a save describes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LastCharacter {
    pub name: String,
    pub title: String,
    pub species: String,
    pub god: String,
    pub xl: i64,
    pub place: String,
    pub depth: i64,
}

pub fn describe_character(c: &LastCharacter) -> String {
    let who = if c.title.is_empty() { c.name.clone() } else { format!("{}{}{}", c.name, if c.title.starts_with(',') { "" } else { " " }, c.title) };
    let what = [c.species.clone(), if c.xl > 0 { format!("XL{}", c.xl) } else { String::new() }].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    [who, what].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
}

pub fn describe_place(c: &LastCharacter) -> String {
    if c.place.is_empty() {
        String::new()
    } else if c.depth > 0 {
        format!("{}:{}", c.place, c.depth)
    } else {
        c.place.clone()
    }
}

/// "isorun the Chiller | Vine Stalker of Vehumet - Isorun": a game's window title.
pub fn game_title(c: &LastCharacter, base: &str) -> String {
    let who = if c.title.is_empty() { c.name.clone() } else { format!("{}{}{}", c.name, if c.title.starts_with(',') { "" } else { " " }, c.title) };
    let what = format!("{}{}", c.species, if c.god.is_empty() { String::new() } else { format!("{}{}", if c.species.is_empty() { "of " } else { " of " }, c.god) });
    let game = [who, what].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" | ");
    [game, base.to_string()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" - ")
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LastPlayed {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "gameId", default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(rename = "gamedataVersion", default, skip_serializing_if = "Option::is_none")]
    pub gamedata_version: Option<String>,
}

pub fn get_last() -> Option<LastPlayed> {
    load(LAST_KEY, None)
}

pub fn set_last(l: &LastPlayed) {
    save(LAST_KEY, l);
}

pub fn get_morgue_dir(server_id: &str, username: &str) -> Option<String> {
    load::<BTreeMap<String, String>>(MORGUE_KEY, BTreeMap::new()).get(&token_key(server_id, username)).cloned()
}

pub fn set_morgue_dir(server_id: &str, username: &str, dir: &str) {
    let mut all: BTreeMap<String, String> = load(MORGUE_KEY, BTreeMap::new());
    all.insert(token_key(server_id, username), dir.to_string());
    save(MORGUE_KEY, &all);
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedGame {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rc: Option<String>,
}

pub fn get_games(server_id: &str) -> Option<Vec<CachedGame>> {
    load::<BTreeMap<String, Vec<CachedGame>>>(GAMES_KEY, BTreeMap::new()).remove(server_id)
}

pub fn set_games(server_id: &str, games: &[GameLink]) {
    let mut all: BTreeMap<String, Vec<CachedGame>> = load(GAMES_KEY, BTreeMap::new());
    all.insert(server_id.to_string(), games.iter().map(|g| CachedGame { id: g.id.clone(), label: g.label.clone(), save: g.save.clone(), disabled: g.disabled, rc: g.rc.clone() }).collect());
    save(GAMES_KEY, &all);
}

/// Where gamedata is fetched from for a server: the server itself natively (no CORS to get
/// round), its proxy on the hosted page in a browser.
pub fn gamedata_base_for(server: &ServerInfo) -> String {
    if server.offline {
        return crate::engine::gamedata_base();
    }
    if cfg!(target_arch = "wasm32") {
        format!("{}/gamedata-proxy/{}", web_origin(), server.host)
    } else {
        server.http.clone()
    }
}

/// Where a game's morgue file is read from: the server's own address natively, the proxy in a browser.
pub fn morgue_url_for(server: &ServerInfo, dump: &str) -> Option<String> {
    if server.offline {
        return None;
    }
    let full = if dump.starts_with("http://") || dump.starts_with("https://") { format!("{dump}.txt") } else { format!("{}{}{}.txt", server.http, if dump.starts_with('/') { "" } else { "/" }, dump) };
    if cfg!(target_arch = "wasm32") {
        let (_, rest) = full.split_once("://")?;
        Some(format!("{}/morgue-proxy/{rest}", web_origin()))
    } else {
        Some(full)
    }
}

/// The page's own origin on the web (the same-origin proxies), the hosted page elsewhere.
pub fn web_origin() -> String {
    #[cfg(target_arch = "wasm32")]
    if let Some(origin) = web_sys::window().and_then(|w| w.location().origin().ok()) {
        if !origin.starts_with("file:") {
            return origin;
        }
    }
    HOSTED_ORIGIN.to_string()
}

// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "uiScale")]
    pub ui_scale: f32,
    #[serde(rename = "confirmStairs")]
    pub confirm_stairs: bool,
    #[serde(rename = "minimapTiles")]
    pub minimap_tiles: u32,
    #[serde(rename = "minimapCell")]
    pub minimap_cell: u32,
    pub hints: String,
    pub graphics: String,
    #[serde(with = "text_or_number")]
    pub resolution: String,
    #[serde(rename = "ambientOcclusion")]
    pub ambient_occlusion: String,
    pub antialias: String,
    #[serde(rename = "surfaceDetail")]
    pub surface_detail: String,
    #[serde(rename = "torchShadows", with = "text_or_number")]
    pub torch_shadows: String,
    pub look: String,
    #[serde(rename = "animationSpeed")]
    pub animation_speed: f64,
    #[serde(rename = "arrowKeys")]
    pub arrow_keys: String,
    #[serde(rename = "leftStick")]
    pub left_stick: String,
    #[serde(rename = "exploreTrigger")]
    pub explore_trigger: String,
    pub travel: String,
    #[serde(rename = "masterVolume")]
    pub master_volume: f32,
    #[serde(rename = "effectsVolume")]
    pub effects_volume: f32,
    #[serde(rename = "ambientVolume")]
    pub ambient_volume: f32,
    #[serde(rename = "musicVolume")]
    pub music_volume: f32,
    pub muted: bool,
    pub hud: String,
    #[serde(rename = "damageNumbers")]
    pub damage_numbers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ui_scale: 1.0,
            confirm_stairs: false,
            minimap_tiles: 19,
            minimap_cell: 20,
            hints: "adaptive".into(),
            graphics: "medium".into(),
            resolution: "auto".into(),
            ambient_occlusion: "auto".into(),
            antialias: "auto".into(),
            surface_detail: default_surface_detail().into(),
            torch_shadows: "auto".into(),
            look: "diorama".into(),
            animation_speed: 1.5,
            arrow_keys: "aim".into(),
            left_stick: "aim".into(),
            explore_trigger: "hold".into(),
            travel: "paced".into(),
            master_volume: 70.0,
            effects_volume: 100.0,
            ambient_volume: 70.0,
            music_volume: 50.0,
            muted: false,
            hud: "dock".into(),
            damage_numbers: true,
        }
    }
}

/// A phone starts with the surface work off (graphics.ts `defaultSurfaceDetail`).
pub fn default_surface_detail() -> &'static str {
    if cfg!(target_os = "android") {
        "off"
    } else {
        "medium"
    }
}

/// A setting the web build keeps as a word (`"auto"`) or a number (`0.75`, `2`): one string here,
/// written back as the number it holds, so the two builds read each other's settings.
mod text_or_number {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &str, s: S) -> Result<S::Ok, S::Error> {
        match v.parse::<f64>() {
            Ok(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 => s.serialize_i64(n as i64),
            Ok(n) if n.is_finite() => s.serialize_f64(n),
            _ => s.serialize_str(v),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        Ok(match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        })
    }
}

pub fn get_settings() -> Settings {
    let saved: serde_json::Value = load(SETTINGS_KEY, serde_json::Value::Object(Default::default()));
    let mut s: Settings = serde_json::from_value(saved.clone()).unwrap_or_default();
    // the hints were two fields before: `gamepadHints` and `keyHints`
    if saved.get("hints").is_none() {
        s.hints = match saved.get("gamepadHints").and_then(|v| v.as_str()) {
            Some(v @ ("adaptive" | "contextual" | "off")) => v.to_string(),
            _ if saved.get("keyHints") == Some(&serde_json::Value::Bool(false)) => "off".into(),
            _ => "adaptive".into(),
        };
    }
    let d = Settings::default();
    if !["low", "medium", "high"].contains(&s.graphics.as_str()) {
        s.graphics = d.graphics.clone();
    }
    if s.look != "custom" && isorun_render::lights(&s.look).is_none() {
        s.look = d.look.clone();
    }
    if s.hud != "dock" && s.hud != "classic" {
        s.hud = d.hud.clone();
    }
    s
}

/// Only what the player changed is written down, so a default moved later moves for everyone.
pub fn save_settings(s: &Settings) {
    let all = serde_json::to_value(s).unwrap_or_default();
    let defaults = serde_json::to_value(Settings::default()).unwrap_or_default();
    let mut out = serde_json::Map::new();
    if let (Some(a), Some(d)) = (all.as_object(), defaults.as_object()) {
        for (k, v) in a {
            if d.get(k) != Some(v) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    save(SETTINGS_KEY, &out);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedView {
    pub angles: isorun_render::CameraAngles,
    pub zoom: Option<f32>,
}

pub fn get_saved_view() -> Option<SavedView> {
    let v: serde_json::Value = load(CAMERA_KEY, serde_json::Value::Null);
    let rotation = v.get("rotation")?.as_f64()? as f32;
    let tilt = v.get("tilt")?.as_f64()? as f32;
    if !rotation.is_finite() || !tilt.is_finite() {
        return None;
    }
    let perspective = v.get("perspective").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
    let zoom = v.get("zoom").and_then(|z| z.as_f64()).map(|z| z as f32).filter(|z| z.is_finite() && *z > 0.0);
    Some(SavedView { angles: isorun_render::CameraAngles { rotation, tilt, perspective }, zoom })
}

pub fn save_view(view: &SavedView) {
    let mut v = serde_json::json!({ "rotation": view.angles.rotation, "tilt": view.angles.tilt, "perspective": view.angles.perspective });
    if let Some(z) = view.zoom {
        v["zoom"] = z.into();
    }
    save(CAMERA_KEY, &v);
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn numbers_and_words_both_read() {
        let s: Settings = serde_json::from_value(serde_json::json!({ "resolution": 0.75, "torchShadows": 2, "look": "dusk" })).unwrap();
        assert_eq!((s.resolution.as_str(), s.torch_shadows.as_str(), s.look.as_str()), ("0.75", "2", "dusk"));
        let back = serde_json::to_value(&s).unwrap();
        assert_eq!(back["resolution"], serde_json::json!(0.75));
        assert_eq!(back["torchShadows"], serde_json::json!(2));
        let auto: Settings = serde_json::from_value(serde_json::json!({ "resolution": "auto" })).unwrap();
        assert_eq!(serde_json::to_value(&auto).unwrap()["resolution"], "auto");
    }
}

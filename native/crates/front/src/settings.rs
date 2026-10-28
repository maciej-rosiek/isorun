//! Isorun's own settings, one row each (the port of `settings-rows.ts` and `graphics.ts`): what a
//! row is called, which key it sets, the values it cycles through (in the order left and right
//! walk them) and how a value reads. The same list draws the settings pages of the front end and
//! the settings over a game, so the two never disagree.
//!
//! The settings are kept as the web build keeps them (`isorun.settings`, only what differs from
//! the defaults), as JSON values: Resolution and Torch shadows are `"auto"` or a number there, which
//! [`isorun_client::servers::Settings`] reads as text, so [`client_settings`] hands it the numbers
//! as text.

use isorun_client::platform::storage::{load, save};
use isorun_client::servers;
use isorun_render::{CellLights, IsoQuality, SurfaceDetail};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const SETTINGS_KEY: &str = "isorun.settings";

/// The settings as a map of the web build's keys to their values.
pub type Settings = Map<String, Value>;

/// The groups the settings are laid out in, in order, each with the line under its name: the
/// panel draws one group at a time, the way a game's options screen has a tab per kind of
/// setting, so no group is ever a scroll long.
pub const SETTING_GROUPS: [(&str, &str); 4] = [
    ("Graphics", "How the dungeon looks, and how much work each frame does."),
    ("Controls", "What the keys, the d-pad and the sticks do."),
    ("Interface", "What the HUD shows around the view, and how big it is."),
    ("Audio", "How loud the sounds, the ambience of the dungeon and the music are."),
];

static PHONE: AtomicBool = AtomicBool::new(false);

/// Say whether the screen is a phone's: a touch screen in the compact window (graphics.ts
/// `onPhone`). A phone starts with the surface work off and stops at Medium.
pub fn set_phone(phone: bool) {
    PHONE.store(phone, Ordering::Relaxed);
}

pub fn on_phone() -> bool {
    PHONE.load(Ordering::Relaxed) || cfg!(target_os = "android")
}

/// The Surface detail a player who never chose one gets: Off on a phone, Medium elsewhere.
pub fn default_surface_detail() -> &'static str {
    if on_phone() {
        "off"
    } else {
        "medium"
    }
}

/// Every setting's default, as `servers.ts` `defaultSettings` has them.
pub fn defaults() -> Settings {
    let v = json!({
        "uiScale": 1,
        "confirmStairs": false,
        "minimap": "auto",
        "minimapTiles": 19,
        "minimapCell": 20,
        "hints": "adaptive",
        "graphics": "medium",
        "resolution": "auto",
        "ambientOcclusion": "auto",
        "antialias": "auto",
        "surfaceDetail": default_surface_detail(),
        "voxels": false,
        "torchShadows": "auto",
        "globalIllumination": false,
        "sightLight": false,
        "look": "diorama",
        "animationSpeed": 1.5,
        "frameRate": 60,
        "arrowKeys": "aim",
        "leftStick": "aim",
        "exploreTrigger": "hold",
        "travel": "paced",
        "masterVolume": 70,
        "effectsVolume": 100,
        "ambientVolume": 70,
        "musicVolume": 50,
        "muted": false,
        "hud": "dock",
        "damageNumbers": true,
    });
    v.as_object().cloned().unwrap_or_default()
}

/// The settings kept on this device over the defaults; a value no build offers any more goes
/// back to its default.
pub fn get() -> Settings {
    from_stored(&load(SETTINGS_KEY, Value::Object(Map::new())))
}

/// [`get`] from what was stored (pure, for the tests).
pub fn from_stored(saved: &Value) -> Settings {
    let mut s = defaults();
    if let Some(o) = saved.as_object() {
        for (k, v) in o {
            if s.contains_key(k) && !v.is_null() {
                s.insert(k.clone(), v.clone());
            }
        }
        // the hints were two fields before: `gamepadHints` and `keyHints`
        if !o.contains_key("hints") {
            let hints = match o.get("gamepadHints").and_then(Value::as_str) {
                Some(v @ ("adaptive" | "contextual" | "off")) => v,
                _ if o.get("keyHints") == Some(&Value::Bool(false)) => "off",
                _ => "adaptive",
            };
            s.insert("hints".into(), hints.into());
        }
    }
    let d = defaults();
    let keep = |s: &mut Settings, key: &str, ok: &dyn Fn(&Value) -> bool| {
        if !s.get(key).is_some_and(ok) {
            s.insert(key.into(), d[key].clone());
        }
    };
    keep(&mut s, "graphics", &|v| matches!(v.as_str(), Some("low" | "medium" | "high")));
    keep(&mut s, "look", &|v| v.as_str().is_some_and(|l| l == "custom" || isorun_render::lights(l).is_some()));
    keep(&mut s, "hud", &|v| matches!(v.as_str(), Some("dock" | "classic")));
    keep(&mut s, "minimap", &|v| matches!(v.as_str(), Some("auto" | "on" | "off")));
    keep(&mut s, "frameRate", &|v| v.as_str() == Some("off") || v.as_f64().is_some_and(|x| x.is_finite() && x >= 10.0));
    keep(&mut s, "uiScale", &|v| v.as_f64().is_some_and(|x| x.is_finite() && x > 0.2 && x < 4.0));
    s
}

/// Keep `s`: only what differs from the defaults is written down, so a default moved later
/// moves for everyone.
pub fn save_settings(s: &Settings) {
    set_frame_cap(s);
    save(SETTINGS_KEY, &Value::Object(changed(s)));
}

/// The Frame rate setting's cap in frames a second, 0 for none (Unlimited, so the screen's own).
static FRAME_CAP: AtomicU32 = AtomicU32::new(60);

/// The cap the app paces its frames to: the Frame rate kept last (with [`save_settings`] or this).
pub fn frame_cap() -> u32 {
    FRAME_CAP.load(Ordering::Relaxed)
}

pub fn set_frame_cap(s: &Settings) {
    FRAME_CAP.store(frame_rate(s), Ordering::Relaxed);
}

/// The Frame rate setting in frames a second, 0 for Unlimited.
pub fn frame_rate(s: &Settings) -> u32 {
    s.get("frameRate").and_then(Value::as_f64).map(|x| x.round() as u32).unwrap_or(0)
}

/// The Frame rate setting's stops: a phone's battery and heat want 30 or 60, a fast screen more.
fn frame_rates() -> Vec<Value> {
    vec![json!(30), json!(60), json!(90), json!(120), json!(144), json!("off")]
}

/// What of `s` differs from the defaults.
pub fn changed(s: &Settings) -> Settings {
    let d = defaults();
    s.iter().filter(|(k, v)| d.get(*k).is_none_or(|dv| !same(dv, v))).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Two setting values alike: numbers by value, so 1 and 1.0 are one.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

/// The settings as the client's struct (what the game screen reads): Resolution and Torch
/// shadows as text, the rest as they are.
pub fn client_settings(s: &Settings) -> servers::Settings {
    let mut v = s.clone();
    for key in ["resolution", "torchShadows"] {
        if let Some(n) = v.get(key).and_then(Value::as_f64) {
            v.insert(key.into(), Value::String(fmt_num(n)));
        }
    }
    serde_json::from_value(Value::Object(v)).unwrap_or_default()
}

pub fn str_of<'a>(s: &'a Settings, key: &str) -> &'a str {
    s.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn num_of(s: &Settings, key: &str) -> f64 {
    s.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

pub fn bool_of(s: &Settings, key: &str) -> bool {
    s.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The UI scale: the size of the HUD, menus and messages.
pub fn ui_scale(s: &Settings) -> f32 {
    let v = num_of(s, "uiScale") as f32;
    if v > 0.0 {
        v
    } else {
        1.0
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// ------------------------------------------------------------------ graphics.ts

/// The Resolution setting: the share of the screen's pixels the dungeon is drawn with, or the
/// Quality preset's.
pub fn resolutions() -> Vec<Value> {
    vec![json!("auto"), json!(100), json!(85), json!(75), json!(66), json!(50)]
}

/// The Ambient occlusion setting: off, or the soft shadows in corners at a size and a number of
/// samples.
fn ambient_occlusion(name: &str) -> Option<(f32, u32)> {
    match name {
        "off" => Some((0.0, 8)),
        "low" => Some((0.5, 8)),
        "medium" => Some((0.5, 16)),
        "high" => Some((1.0, 16)),
        _ => None,
    }
}

/// What the renderer does a frame under these settings: the Quality preset, with each graphics
/// setting that is not Auto in place of the preset's.
pub fn quality_for(s: &Settings, device_pixel_ratio: f32) -> IsoQuality {
    let preset = isorun_render::quality(str_of(s, "graphics")).unwrap_or(isorun_render::lights::QUALITY_MEDIUM);
    let mut q = preset;
    match s.get("resolution").and_then(Value::as_f64) {
        Some(r) => {
            q.pixel_ratio = device_pixel_ratio * r as f32 / 100.0;
            q.adaptive = false;
        }
        None => q.adaptive = true,
    }
    if let Some((res, samples)) = ambient_occlusion(str_of(s, "ambientOcclusion")) {
        q.ao_resolution = res;
        q.ao_samples = samples;
    }
    match str_of(s, "antialias") {
        "on" => q.antialias = true,
        "off" => q.antialias = false,
        _ => {}
    }
    q
}

/// The Surface detail levels offered here: a phone stops at Medium.
pub fn surface_details() -> Vec<Value> {
    let all = ["off", "low", "medium", "high"];
    all.iter().filter(|l| !(on_phone() && **l == "high")).map(|l| json!(l)).collect()
}

pub fn surface_detail(s: &Settings) -> SurfaceDetail {
    match str_of(s, "surfaceDetail") {
        "off" => SurfaceDetail::Off,
        "low" => SurfaceDetail::Low,
        "high" => SurfaceDetail::High,
        _ => SurfaceDetail::Medium,
    }
}

pub fn cell_lights(s: &Settings) -> CellLights {
    CellLights { gi: bool_of(s, "globalIllumination"), sight: bool_of(s, "sightLight") }
}

/// The graphics settings on a renderer: the quality, the surface detail, the look and the
/// animation speed, as the game draws them.
pub fn apply_graphics(r: &mut isorun_render::IsoRenderer, s: &Settings, device_pixel_ratio: f32) {
    r.set_quality(quality_for(s, device_pixel_ratio));
    r.set_surface_detail(surface_detail(s));
    r.set_voxels(bool_of(s, "voxels"));
    r.set_cell_lights(cell_lights(s));
    r.set_lights(crate::lights_panel::lights_for(str_of(s, "look")));
    r.set_animation_speed(num_of(s, "animationSpeed").max(0.1));
}

// ------------------------------------------------------------------ settings-rows.ts

/// One setting: its name, the key it sets, the values left and right walk, how a value reads, and
/// what the message line says on it.
pub struct SettingRow {
    pub label: &'static str,
    pub group: &'static str,
    pub key: &'static str,
    pub values: Vec<Value>,
    pub fmt: fn(&Value) -> String,
    pub hint: &'static str,
    /// When the row does nothing under the current settings: left and right leave it alone and it
    /// is drawn dim, the way the game's menus draw an item that cannot be taken.
    pub off: Option<fn(&Settings) -> bool>,
}

impl SettingRow {
    pub fn value(&self, s: &Settings) -> String {
        (self.fmt)(s.get(self.key).unwrap_or(&Value::Null))
    }

    pub fn is_off(&self, s: &Settings) -> bool {
        self.off.is_some_and(|f| f(s))
    }
}

fn cap(v: &str) -> String {
    let mut c = v.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => fmt_num(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn on_off(v: &Value) -> String {
    if v.as_bool() == Some(true) {
        "On".into()
    } else {
        "Off".into()
    }
}

fn auto_or(v: &Value, f: impl Fn(&Value) -> String) -> String {
    if v.as_str() == Some("auto") {
        "Auto".into()
    } else {
        f(v)
    }
}

fn percent(v: &Value) -> String {
    format!("{}%", plain(v))
}

fn nums(values: &[f64]) -> Vec<Value> {
    values.iter().map(|v| json!(v)).collect()
}

fn strs(values: &[&str]) -> Vec<Value> {
    values.iter().map(|v| json!(v)).collect()
}

/// The minimap's sizes in tiles across: every odd count from 9 to 61, so the player always stands
/// on the centre tile.
pub fn minimap_tiles() -> Vec<Value> {
    (0..=26).map(|i| json!(9 + 2 * i)).collect()
}

/// The minimap's tile sizes in px.
pub const MINIMAP_CELLS: [f64; 9] = [8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 28.0, 32.0];

fn volumes() -> Vec<Value> {
    (0..=10).map(|i| json!(i * 10)).collect()
}

/// Every row, in group order.
pub fn rows() -> Vec<SettingRow> {
    let row = |group, label, key, values, hint, fmt: fn(&Value) -> String| SettingRow { label, group, key, values, fmt, hint, off: None };
    let mut looks: Vec<Value> = isorun_render::LIGHTS_NAMES.iter().filter(|n| isorun_render::lights(n).is_some()).map(|n| json!(n)).collect();
    looks.push(json!("custom"));
    vec![
        row("Graphics", "Quality", "graphics", strs(&["low", "medium", "high"]), "How much work each frame does: the pixels drawn, the soft shadows in the corners, and the edges. Lower is faster on a weak GPU. The rows under it can change one of them.", |v| isorun_render::quality(v.as_str().unwrap_or("")).map(|q| q.label.to_string()).unwrap_or_else(|| plain(v))),
        row("Graphics", "Surface detail", "surfaceDetail", surface_details(), "What the tiles show of their surface. Low: rim light, dirt and wear, and how each cell differs. Medium: the depth of the bricks and stones, gloss, wetness and glowing tiles too. High: sparkle and deep liquids too. A phone stops at Medium.", |v| cap(&plain(v))),
        row("Graphics", "Voxels", "voxels", vec![json!(false), json!(true)], "Builds the dungeon of little blocks: the bricks of the walls stand out of their mortar, the floor takes a light relief, and you, the monsters and the items are small figures that break into pieces when they fall, as do the walls. High Quality cuts the tiles finer. It costs more on a weak GPU.", |v| if v.as_bool() == Some(true) { "On".into() } else { "Off".into() }),
        row("Graphics", "Global illumination", "globalIllumination", vec![json!(false), json!(true)], "Real light on the level: lava, fire, glowing things and spells light the cells they reach, and the light bounces off the walls and floors in their colours.", on_off),
        row("Graphics", "Sight light", "sightLight", vec![json!(false), json!(true)], "You carry a light whose shadows are exactly what you can see: the walls cast them on the floor, and what you only remember is drawn as a cool map. Halos, umbras and darkness change the light.", on_off),
        row("Graphics", "Resolution", "resolution", resolutions(), "How many of the screen's pixels the dungeon is drawn with; the rest are filled in when the picture is scaled up. Fewer is much faster on a big screen. Auto takes the Quality's, and draws fewer for a while when the GPU cannot keep up with the screen.", |v| auto_or(v, percent)),
        row("Graphics", "Ambient occlusion", "ambientOcclusion", strs(&["auto", "off", "low", "medium", "high"]), "The soft shadows in the corners and at the foot of the walls. Low draws them with fewer samples, High at the full size of the screen. Auto takes the Quality's.", |v| auto_or(v, |v| cap(&plain(v)))),
        row("Graphics", "Antialiasing", "antialias", strs(&["auto", "off", "on"]), "Smooths the edges of the walls by drawing the scene four times over; it costs about as much as the rest of the scene. Auto takes the Quality's.", |v| auto_or(v, |v| if v.as_str() == Some("on") { "On".into() } else { "Off".into() })),
        row("Graphics", "Frame rate", "frameRate", frame_rates(), "The most frames a second the app draws. Fewer save battery and keep a phone cool; more are smoother on a fast screen. Unlimited draws as many as the screen shows. On the web a change takes effect when the page next loads.", |v| if v.as_str() == Some("off") { "Unlimited".into() } else { format!("{} fps", plain(v)) }),
        row("Graphics", "Animation speed", "animationSpeed", nums(&[1.0, 1.25, 1.5, 2.0, 3.0]), "How fast the player and the monsters walk and fight on the screen. The game itself does not wait for them.", |v| format!("{}x", plain(v))),
        row("Graphics", "Travel", "travel", strs(&["paced", "fast"]), "Paced: travel and explore wait for every hop, so the walk shows cell by cell. Fast: they go as fast as the server does, and a long way is a jump.", |v| if v.as_str() == Some("paced") { "Paced".into() } else { "Fast".into() }),
        row("Graphics", "Look", "look", looks, "The lights, the colours and the screen effects of the dungeon. Custom is what Tune the look set last.", |v| match v.as_str() {
            Some("custom") => "Custom".into(),
            Some(n) => isorun_render::lights(n).map(|l| l.label.to_string()).unwrap_or_else(|| n.to_string()),
            None => String::new(),
        }),
        row("Controls", "Arrow keys", "arrowKeys", strs(&["aim", "step"]), "Aim: the arrows point an arrow on the ground (two of them a diagonal) and Space or Enter steps that way. Step: every arrow steps at once.", |v| if v.as_str() == Some("aim") { "Aim, then Space".into() } else { "Step at once".into() }),
        row("Controls", "Left stick", "leftStick", strs(&["step", "aim"]), "Step: the left stick steps at once. Aim: the stick points an arrow on the ground and A steps that way, again and again while held.", |v| if v.as_str() == Some("aim") { "Aim, then A".into() } else { "Step at once".into() }),
        row("Controls", "LT explore", "exploreTrigger", strs(&["tap", "hold"]), "Tap: a tap of LT explores with every step shown, a hold explores at once. Hold: LT held explores with every step shown and letting go stops; a tap explores at once. While an explore walks, LT stops it.", |v| if v.as_str() == Some("tap") { "Tap to walk, hold for at once".into() } else { "Hold to walk, tap for at once".into() }),
        row("Controls", "Hints", "hints", strs(&["adaptive", "contextual", "off"]), "Gamepad prompts in the corner of the view for what is in front of you. Adaptive also teaches the controls until used, across all runs. Contextual only skips the teaching. Off hides gameplay hints; menu and targeting controls remain.", |v| match v.as_str() {
            Some("adaptive") => "Adaptive".into(),
            Some("contextual") => "Contextual only".into(),
            _ => "Off".into(),
        }),
        row("Interface", "HUD", "hud", strs(&["dock", "classic"]), "Dock: health and magic orbs, quick slots and a button for every screen along the foot of the screen, all usable with the mouse and the pad. Classic: the WebTiles panes, as the official client lays them out.", |v| if v.as_str() == Some("dock") { "Dock".into() } else { "Classic".into() }),
        row("Interface", "UI scale", "uiScale", nums(&[1.0, 1.15, 1.3, 1.5, 0.85]), "The size of the HUD, menus and messages.", |v| format!("{}%", (v.as_f64().unwrap_or(1.0) * 100.0).round())),
        row("Interface", "Minimap", "minimap", strs(&["auto", "on", "off"]), "The small map of the level around you in the corner. Auto hides it on a small screen (a phone's), where it covers the view, and shows it on a bigger one. The level map (X) is there either way.", |v| match v.as_str() {
            Some("on") => "On".into(),
            Some("off") => "Off".into(),
            _ => "Auto".into(),
        }),
        row("Interface", "Minimap size", "minimapTiles", minimap_tiles(), "How many tiles across the minimap shows: it grows out of its corner over the view, at the same cell size.", |v| format!("{} tiles", plain(v))),
        row("Interface", "Minimap tile size", "minimapCell", nums(&MINIMAP_CELLS), "How big each minimap tile is drawn: the same tiles, larger, until the map runs out of room over the view.", |v| format!("{}px", plain(v))),
        row("Interface", "Damage numbers", "damageNumbers", vec![json!(true), json!(false)], "Numbers over the fighters as blows land: the damage dealt, and the damage taken in red, bigger the more of your life it took.", |v| if v.as_bool() == Some(true) { "On".into() } else { "Off".into() }),
        row("Audio", "Master volume", "masterVolume", volumes(), "How loud everything is.", percent),
        row("Audio", "Effects volume", "effectsVolume", volumes(), "How loud the steps, the fights, the doors, the spells and the menus are.", percent),
        row("Audio", "Ambient volume", "ambientVolume", volumes(), "How loud the dungeon itself is: drips and wind, water and lava near you.", percent),
        row("Audio", "Music volume", "musicVolume", volumes(), "How loud the music is.", percent),
        row("Audio", "Mute", "muted", vec![json!(false), json!(true)], "Silences everything and keeps the volumes.", |v| if v.as_bool() == Some(true) { "Muted".into() } else { "Off".into() }),
    ]
}

/// The rows of one group.
pub fn group_rows(group: &str) -> Vec<SettingRow> {
    rows().into_iter().filter(|r| r.group == group).collect()
}

/// The line under a group's name.
pub fn group_hint(group: &str) -> &'static str {
    SETTING_GROUPS.iter().find(|(g, _)| *g == group).map(|(_, h)| *h).unwrap_or("")
}

/// Move `r` on by `d` steps in `s` (right is next, left is previous, wrapping). A row that is
/// off is left as it is. A value that is not one of the stops (a default between two, an old
/// saved value) steps to the next stop in that direction (`off_stop`). Pure: the caller saves.
pub fn adjust(r: &SettingRow, s: &mut Settings, d: i32) {
    if r.is_off(s) || r.values.is_empty() {
        return;
    }
    let cur = s.get(r.key).cloned().unwrap_or(Value::Null);
    let n = r.values.len() as i32;
    let i = r.values.iter().position(|v| same(v, &cur));
    let next = match (i, cur.as_f64()) {
        (None, Some(c)) if r.values.iter().all(|v| v.as_f64().is_some()) => {
            let stops: Vec<f64> = r.values.iter().filter_map(Value::as_f64).collect();
            json!(off_stop(&stops, c, d))
        }
        _ => r.values[((i.unwrap_or(0) as i32 + d) % n + n) as usize % r.values.len()].clone(),
    };
    s.insert(r.key.into(), next);
}

/// The stop `d` steps from `cur`, which is not itself a stop. On an ascending scale one step
/// right is the first stop above and one step left the first stop below, so no stop is skipped;
/// on any other scale it is the nearest stop, then the step.
pub fn off_stop(values: &[f64], cur: f64, d: i32) -> f64 {
    let n = values.len() as i32;
    let ascending = values.windows(2).all(|w| w[1] > w[0]);
    if ascending {
        let above = values.iter().position(|v| *v > cur).map(|i| i as i32).unwrap_or(n);
        let i = if d > 0 { above + d - 1 } else { above + d };
        return values[i.clamp(0, n - 1) as usize];
    }
    let mut i = 0;
    for (k, v) in values.iter().enumerate() {
        if (v - cur).abs() < (values[i] - cur).abs() {
            i = k;
        }
    }
    values[((i as i32 + d) % n + n) as usize % values.len()]
}

/// Whether every row of `group` is at its default.
pub fn group_at_defaults(group: &str, s: &Settings) -> bool {
    let d = defaults();
    rows().iter().filter(|r| r.group == group).all(|r| same(s.get(r.key).unwrap_or(&Value::Null), d.get(r.key).unwrap_or(&Value::Null)))
}

/// Put every row of `group` back to its default.
pub fn reset_group(group: &str, s: &mut Settings) {
    let d = defaults();
    for r in rows().iter().filter(|r| r.group == group) {
        if let Some(v) = d.get(r.key) {
            s.insert(r.key.into(), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_over_defaults() {
        let s = from_stored(&json!({ "graphics": "high", "resolution": 85, "look": "nonsense", "bogus": 1 }));
        assert_eq!(str_of(&s, "graphics"), "high");
        assert_eq!(num_of(&s, "resolution"), 85.0);
        assert_eq!(str_of(&s, "look"), "diorama");
        assert!(!s.contains_key("bogus"));
        assert_eq!(str_of(&s, "minimap"), "auto");
        assert_eq!(client_settings(&from_stored(&json!({ "minimap": "off" }))).minimap, "off");
        assert_eq!(str_of(&from_stored(&json!({ "minimap": "sometimes" })), "minimap"), "auto");
    }

    #[test]
    fn frame_rate_reads() {
        assert_eq!(frame_rate(&from_stored(&json!({}))), 60);
        assert_eq!(frame_rate(&from_stored(&json!({ "frameRate": 120 }))), 120);
        assert_eq!(frame_rate(&from_stored(&json!({ "frameRate": "off" }))), 0);
        assert_eq!(frame_rate(&from_stored(&json!({ "frameRate": "fast" }))), 60);
    }

    #[test]
    fn old_hints_migrate() {
        assert_eq!(str_of(&from_stored(&json!({ "gamepadHints": "contextual" })), "hints"), "contextual");
        assert_eq!(str_of(&from_stored(&json!({ "keyHints": false })), "hints"), "off");
        assert_eq!(str_of(&from_stored(&json!({})), "hints"), "adaptive");
    }

    #[test]
    fn only_changes_are_kept() {
        let mut s = defaults();
        s.insert("graphics".into(), json!("low"));
        s.insert("uiScale".into(), json!(1.0));
        let c = changed(&s);
        assert_eq!(c.len(), 1);
        assert_eq!(c["graphics"], json!("low"));
    }

    #[test]
    fn adjust_wraps_and_steps_off_stops() {
        let rows = rows();
        let quality = rows.iter().find(|r| r.key == "graphics").unwrap();
        let mut s = defaults();
        adjust(quality, &mut s, 1);
        assert_eq!(str_of(&s, "graphics"), "high");
        adjust(quality, &mut s, 1);
        assert_eq!(str_of(&s, "graphics"), "low");
        adjust(quality, &mut s, -1);
        assert_eq!(str_of(&s, "graphics"), "high");
        assert_eq!(off_stop(&[1.0, 1.25, 1.5, 2.0, 3.0], 1.4, 1), 1.5);
        assert_eq!(off_stop(&[1.0, 1.25, 1.5, 2.0, 3.0], 1.4, -1), 1.25);
        assert_eq!(off_stop(&[1.0, 1.15, 1.3, 1.5, 0.85], 1.2, 1), 1.3);
        let speed = rows.iter().find(|r| r.key == "animationSpeed").unwrap();
        s.insert("animationSpeed".into(), json!(1.4));
        adjust(speed, &mut s, 1);
        assert_eq!(num_of(&s, "animationSpeed"), 1.5);
    }

    #[test]
    fn values_read() {
        let rows = rows();
        let s = defaults();
        let read = |key: &str| rows.iter().find(|r| r.key == key).unwrap().value(&s);
        assert_eq!(read("graphics"), "Medium");
        assert_eq!(read("resolution"), "Auto");
        assert_eq!(read("uiScale"), "100%");
        assert_eq!(read("animationSpeed"), "1.5x");
        assert_eq!(read("look"), "Diorama");
        assert_eq!(read("masterVolume"), "70%");
        assert_eq!(read("minimapTiles"), "19 tiles");
        assert_eq!(read("minimap"), "Auto");
    }

    #[test]
    fn quality_overrides() {
        let mut s = defaults();
        s.insert("resolution".into(), json!(50));
        s.insert("antialias".into(), json!("on"));
        s.insert("ambientOcclusion".into(), json!("off"));
        let q = quality_for(&s, 2.0);
        assert_eq!(q.pixel_ratio, 1.0);
        assert!(!q.adaptive && q.antialias);
        assert_eq!(q.ao_resolution, 0.0);
        let c = client_settings(&s);
        assert_eq!(c.resolution, "50");
    }

    #[test]
    fn cell_lights_are_off_until_turned_on() {
        // given
        let mut s = defaults();

        // when
        let before = cell_lights(&s);
        s.insert("globalIllumination".into(), json!(true));
        s.insert("sightLight".into(), json!(true));

        // then
        assert_eq!(before, CellLights { gi: false, sight: false });
        assert_eq!(cell_lights(&s), CellLights { gi: true, sight: true });
    }

    #[test]
    fn reset_a_group() {
        let mut s = defaults();
        s.insert("masterVolume".into(), json!(10));
        assert!(!group_at_defaults("Audio", &s));
        assert!(group_at_defaults("Graphics", &s));
        reset_group("Audio", &mut s);
        assert!(group_at_defaults("Audio", &s));
    }
}

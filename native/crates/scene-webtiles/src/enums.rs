//! The enum values the builder needs, from the server's own `enums.js`. Values the server
//! does not publish fall back to the protocol tables, and every difference between the two is
//! reported, so a hardcoded constant can never drift silently.

use isorun_gamedata::{verify_enums, Gamedata};
use isorun_webtiles::{MapFeature, MenuFlag, MouseMode, UiState};

macro_rules! mf_table {
    ($($field:ident: $name:ident),* $(,)?) => {
        /// Map-feature categories (map-feature.h). The server publishes them as `MF_*` in
        /// enums.js; [`MapFeature`] is only the fallback for a server that does not.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct MfTable {
            $(pub $field: i64,)*
        }

        impl Default for MfTable {
            fn default() -> Self {
                MfTable { $($field: MapFeature::$name,)* }
            }
        }

        impl MfTable {
            /// The server's `MF_*` where it publishes one, the protocol table's otherwise.
            pub fn for_gamedata(gd: &Gamedata) -> MfTable {
                let d = MfTable::default();
                MfTable { $($field: gd.enums.get("", concat!("MF_", stringify!($name))).unwrap_or(d.$field),)* }
            }

            /// Every name with the protocol's value, in map-feature.h order.
            pub const PROTOCOL: &'static [(&'static str, i64)] = &[$((stringify!($name), MapFeature::$name),)*];
        }
    };
}

mf_table! {
    unseen: UNSEEN,
    floor: FLOOR,
    wall: WALL,
    map_floor: MAP_FLOOR,
    map_wall: MAP_WALL,
    door: DOOR,
    item: ITEM,
    mons_friendly: MONS_FRIENDLY,
    mons_peaceful: MONS_PEACEFUL,
    mons_neutral: MONS_NEUTRAL,
    mons_hostile: MONS_HOSTILE,
    mons_no_exp: MONS_NO_EXP,
    stair_up: STAIR_UP,
    stair_down: STAIR_DOWN,
    stair_branch: STAIR_BRANCH,
    feature: FEATURE,
    water: WATER,
    lava: LAVA,
    trap: TRAP,
    excl_root: EXCL_ROOT,
    excl: EXCL,
    player: PLAYER,
    deep_water: DEEP_WATER,
    portal: PORTAL,
    transporter: TRANSPORTER,
    transporter_landing: TRANSPORTER_LANDING,
    explore_horizon: EXPLORE_HORIZON,
}

/// Halo values on the wire (enums.js `HALO_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HaloTable {
    pub range: i64,
    pub umbra_first: i64,
    pub umbra_last: i64,
}

pub const HALO_DEFAULT: HaloTable = HaloTable { range: 1, umbra_first: 2, umbra_last: 5 };

/// The enum values in use for one gamedata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEnums {
    pub mf: MfTable,
    pub halo: HaloTable,
    /// One line per constant that differs from what the client hardcodes.
    pub mismatches: Vec<String>,
}

/// Enum values from the server's own enums.js. The TS client logs the mismatches once per
/// gamedata; here the caller does, from [`ServerEnums::mismatches`].
pub fn server_enums(gd: &Gamedata) -> ServerEnums {
    ServerEnums { mf: MfTable::for_gamedata(gd), halo: halo_table(gd), mismatches: enum_mismatches(gd) }
}

pub(crate) fn halo_table(gd: &Gamedata) -> HaloTable {
    let get = |n: &str, d: i64| gd.enums.get("", n).unwrap_or(d);
    HaloTable {
        range: get("HALO_RANGE", HALO_DEFAULT.range),
        umbra_first: get("HALO_UMBRA_FIRST", HALO_DEFAULT.umbra_first),
        umbra_last: get("HALO_UMBRA_LAST", HALO_DEFAULT.umbra_last),
    }
}

const MOUSE_MODE: &[(&str, i64)] = &[
    ("NORMAL", MouseMode::NORMAL),
    ("COMMAND", MouseMode::COMMAND),
    ("TARGET", MouseMode::TARGET),
    ("TARGET_DIR", MouseMode::TARGET_DIR),
    ("TARGET_PATH", MouseMode::TARGET_PATH),
    ("MORE", MouseMode::MORE),
    ("MACRO", MouseMode::MACRO),
    ("PROMPT", MouseMode::PROMPT),
    ("YESNO", MouseMode::YESNO),
];

const MENU_FLAG: &[(&str, i64)] = &[
    ("NOSELECT", MenuFlag::NOSELECT),
    ("SINGLESELECT", MenuFlag::SINGLESELECT),
    ("MULTISELECT", MenuFlag::MULTISELECT),
    ("SELECT_QTY", MenuFlag::SELECT_QTY),
    ("ANYPRINTABLE", MenuFlag::ANYPRINTABLE),
    ("SELECT_BY_PAGE", MenuFlag::SELECT_BY_PAGE),
    ("INIT_HOVER", MenuFlag::INIT_HOVER),
    ("WRAP", MenuFlag::WRAP),
    ("ALLOW_FILTER", MenuFlag::ALLOW_FILTER),
    ("ALLOW_FORMATTING", MenuFlag::ALLOW_FORMATTING),
    ("SHOW_PAGENUMBERS", MenuFlag::SHOW_PAGENUMBERS),
    ("START_AT_END", MenuFlag::START_AT_END),
    ("PRESELECTED", MenuFlag::PRESELECTED),
    ("ARROWS_SELECT", MenuFlag::ARROWS_SELECT),
    ("PAGED_INVENTORY", MenuFlag::PAGED_INVENTORY),
];

const UI: &[(&str, i64)] = &[("NORMAL", UiState::NORMAL), ("CRT", UiState::CRT), ("VIEW_MAP", UiState::VIEW_MAP)];

const TEXTURE: &[(&str, i64)] = &[("FLOOR", 0), ("WALL", 1), ("FEAT", 2), ("PLAYER", 3), ("DEFAULT", 4), ("GUI", 5), ("ICONS", 6)];

/// Every constant the client hardcodes (the protocol's MouseMode, MenuFlag, UiState and
/// MapFeature; the HALO values; the texture order) checked against enums.js. Map-feature
/// names past what the server publishes (transporters, explore horizon) are only reported
/// when the server publishes a different value.
pub fn enum_mismatches(gd: &Gamedata) -> Vec<String> {
    let halo: &[(&str, i64)] =
        &[("HALO_RANGE", HALO_DEFAULT.range), ("HALO_UMBRA_FIRST", HALO_DEFAULT.umbra_first), ("HALO_UMBRA_LAST", HALO_DEFAULT.umbra_last)];
    let mut out = verify_enums(gd, &[("mouse_mode", MOUSE_MODE), ("menu_flag", MENU_FLAG), ("ui", UI), ("texture", TEXTURE), ("", halo)]);
    for (k, v) in MfTable::PROTOCOL {
        if let Some(got) = gd.enums.value("", &format!("MF_{k}")) {
            if got.as_f64() != Some(*v as f64) {
                let shown = match got {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push(format!("enums.js MF_{k} = {shown}, client assumes {v}"));
            }
        }
    }
    out
}

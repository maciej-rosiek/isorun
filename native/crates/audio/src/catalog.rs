//! What the game can sound: the effects, the ambient loops and the music (`catalog.ts`).

/// One effect: the files it picks from at random, how loud and how varied it plays, and how many
/// may sound at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundSpec {
    pub files: &'static [&'static str],
    pub gain: f32,
    /// Playback rate of the file as it is, below 1 is lower and slower.
    pub rate: f64,
    /// How far the pitch may stray from `rate`, as a fraction.
    pub pitch_spread: f64,
    /// How far the volume may fall below `gain`, as a fraction.
    pub gain_spread: f32,
    pub voices: usize,
    /// Seconds the sound waits before it may sound again.
    pub cooldown: f64,
}

const fn spec(files: &'static [&'static str], gain: f32, voices: usize) -> SoundSpec {
    SoundSpec { files, gain, rate: 1.0, pitch_spread: 0.08, gain_spread: 0.15, voices, cooldown: 0.03 }
}

const fn cooldown(s: SoundSpec, cooldown: f64) -> SoundSpec {
    SoundSpec { cooldown, ..s }
}

const fn pitch(s: SoundSpec, pitch_spread: f64) -> SoundSpec {
    SoundSpec { pitch_spread, ..s }
}

const fn rate(s: SoundSpec, rate: f64) -> SoundSpec {
    SoundSpec { rate, ..s }
}

/// A sound with no spread at all: the same every time.
const fn steady(s: SoundSpec) -> SoundSpec {
    SoundSpec { pitch_spread: 0.0, gain_spread: 0.0, ..s }
}

macro_rules! sounds {
    ($($variant:ident = $name:literal => $spec:expr,)*) => {
        /// Every effect by name; the files are in `public/audio/sfx`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum SoundName {
            $($variant,)*
        }

        impl SoundName {
            pub const ALL: &'static [SoundName] = &[$(SoundName::$variant,)*];

            /// The name the TypeScript and the trace use: `hit-heavy`, `ui-click`.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(SoundName::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<SoundName> {
                match name {
                    $($name => Some(SoundName::$variant),)*
                    _ => None,
                }
            }

            pub fn spec(self) -> &'static SoundSpec {
                match self {
                    $(SoundName::$variant => {
                        const S: SoundSpec = $spec;
                        &S
                    })*
                }
            }
        }
    };
}

sounds! {
    Step = "step" => pitch(cooldown(spec(&["step1", "step2", "step3", "step4", "step5"], 0.35, 3), 0.07), 0.12),
    Swing = "swing" => spec(&["swing1", "swing2"], 0.6, 3),
    Miss = "miss" => spec(&["miss1", "miss2", "miss3"], 0.7, 3),
    Hit = "hit" => spec(&["hit1", "hit2", "hit3", "hit4"], 0.8, 4),
    HitHeavy = "hit-heavy" => spec(&["hit-heavy1", "hit-heavy2"], 0.9, 2),
    Hurt = "hurt" => cooldown(spec(&["hurt1", "hurt2"], 1.0, 2), 0.1),
    Death = "death" => pitch(rate(spec(&["death1", "death2"], 0.9, 3), 0.75), 0.15),
    Heartbeat = "heartbeat" => cooldown(steady(spec(&["heartbeat"], 0.9, 1)), 0.4),
    DoorOpen = "door-open" => cooldown(spec(&["door-open1", "door-open2"], 0.7, 2), 0.15),
    DoorClose = "door-close" => cooldown(spec(&["door-close1", "door-close2"], 0.7, 2), 0.15),
    Stairs = "stairs" => cooldown(spec(&["stairs"], 0.7, 1), 0.5),
    Pickup = "pickup" => cooldown(spec(&["pickup1", "pickup2"], 0.9, 2), 0.1),
    Gold = "gold" => cooldown(spec(&["gold1", "gold2"], 1.0, 2), 0.1),
    Levelup = "levelup" => steady(spec(&["levelup"], 2.0, 1)),
    Cast = "cast" => spec(&["cast-generic"], 1.4, 2),
    CastFire = "cast-fire" => spec(&["cast-fire"], 0.7, 2),
    CastIce = "cast-ice" => rate(spec(&["cast-ice"], 0.8, 2), 0.8),
    CastDark = "cast-dark" => spec(&["cast-dark"], 0.9, 2),
    Potion = "potion" => spec(&["potion"], 1.4, 2),
    Scroll = "scroll" => spec(&["scroll1", "scroll2"], 1.1, 2),
    UiClick = "ui-click" => cooldown(pitch(spec(&["ui-click1", "ui-click2"], 0.5, 3), 0.04), 0.04),
    UiOpen = "ui-open" => cooldown(spec(&["ui-open"], 0.5, 2), 0.1),
    UiClose = "ui-close" => cooldown(spec(&["ui-close"], 0.5, 2), 0.1),
    UiBack = "ui-back" => cooldown(spec(&["ui-back"], 0.5, 2), 0.1),
    More = "more" => cooldown(pitch(spec(&["more"], 0.25, 1), 0.0), 0.3),
    Gameover = "gameover" => pitch(rate(spec(&["gameover"], 0.9, 1), 0.55), 0.0),
}

impl std::fmt::Display for SoundName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The loops under the game: a bed of the dungeon, and the water or lava near the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AmbientName {
    Dungeon,
    Cavern,
    Water,
    Lava,
}

/// One loop: its file in `public/audio/ambient`, how loud it is at its full level, and whether it
/// is read as it plays (a long bed) or decoded once (a short loop).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientSpec {
    pub file: &'static str,
    pub gain: f32,
    pub stream: bool,
}

impl AmbientName {
    pub const ALL: [AmbientName; 4] = [AmbientName::Dungeon, AmbientName::Cavern, AmbientName::Water, AmbientName::Lava];

    pub fn spec(self) -> AmbientSpec {
        match self {
            AmbientName::Dungeon => AmbientSpec { file: "dungeon", gain: 0.9, stream: true },
            AmbientName::Cavern => AmbientSpec { file: "cavern", gain: 0.8, stream: true },
            AmbientName::Water => AmbientSpec { file: "water", gain: 0.9, stream: false },
            AmbientName::Lava => AmbientSpec { file: "lava", gain: 0.8, stream: false },
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// How loud each loop should be, 0 to 1; one at 0 fades out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AmbientLevels {
    pub dungeon: f32,
    pub cavern: f32,
    pub water: f32,
    pub lava: f32,
}

impl AmbientLevels {
    pub fn get(&self, name: AmbientName) -> f32 {
        match name {
            AmbientName::Dungeon => self.dungeon,
            AmbientName::Cavern => self.cavern,
            AmbientName::Water => self.water,
            AmbientName::Lava => self.lava,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mood {
    Calm,
    Dark,
}

/// The tracks by mood; the files are in `public/audio/music`.
pub fn music(mood: Mood) -> &'static [&'static str] {
    match mood {
        Mood::Calm => &["ruined-temple", "persistence", "cave", "abandoned-castle"],
        Mood::Dark => &["evil-temple", "eternal-sleep"],
    }
}

/// Every file the catalog names, as a path under `public/audio` without the `.ogg`:
/// `sfx/step1`, `ambient/water`, `music/cave`.
pub fn all_files() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in SoundName::ALL {
        for file in name.spec().files {
            let path = format!("sfx/{file}");
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    for name in AmbientName::ALL {
        out.push(format!("ambient/{}", name.spec().file));
    }
    for mood in [Mood::Calm, Mood::Dark] {
        for track in music(mood) {
            out.push(format!("music/{track}"));
        }
    }
    out
}

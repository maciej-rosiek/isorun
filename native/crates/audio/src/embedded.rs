//! The sound files, embedded in the binary (desktop and Android); the web fetches them instead.
macro_rules! files {
    ($($path:literal,)*) => {
        /// Every file under `apps/isorun/public/audio`, by its path without the `.ogg`.
        pub(crate) static FILES: &[(&str, &[u8])] = &[
            $(($path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../apps/isorun/public/audio/", $path, ".ogg"))),)*
        ];
    };
}

files! {
    "sfx/cast-dark",
    "sfx/cast-fire",
    "sfx/cast-generic",
    "sfx/cast-ice",
    "sfx/death1",
    "sfx/death2",
    "sfx/door-close1",
    "sfx/door-close2",
    "sfx/door-open1",
    "sfx/door-open2",
    "sfx/gameover",
    "sfx/gold1",
    "sfx/gold2",
    "sfx/heartbeat",
    "sfx/hit-heavy1",
    "sfx/hit-heavy2",
    "sfx/hit1",
    "sfx/hit2",
    "sfx/hit3",
    "sfx/hit4",
    "sfx/hurt1",
    "sfx/hurt2",
    "sfx/levelup",
    "sfx/miss1",
    "sfx/miss2",
    "sfx/miss3",
    "sfx/more",
    "sfx/pickup1",
    "sfx/pickup2",
    "sfx/potion",
    "sfx/scroll1",
    "sfx/scroll2",
    "sfx/stairs",
    "sfx/step1",
    "sfx/step2",
    "sfx/step3",
    "sfx/step4",
    "sfx/step5",
    "sfx/swing1",
    "sfx/swing2",
    "sfx/ui-back",
    "sfx/ui-click1",
    "sfx/ui-click2",
    "sfx/ui-close",
    "sfx/ui-open",
    "ambient/cavern",
    "ambient/dungeon",
    "ambient/lava",
    "ambient/water",
    "music/abandoned-castle",
    "music/cave",
    "music/eternal-sleep",
    "music/evil-temple",
    "music/persistence",
    "music/ruined-temple",
}

/// The bytes of a file, by its path under `public/audio` without the `.ogg` (`sfx/step1`).
pub fn bytes(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

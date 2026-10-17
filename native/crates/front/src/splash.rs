//! The line under the title (the port of `splash.ts`): what a god says about the Orb, picked once
//! per visit the way Minecraft picks a splash. Every line is verbatim from crawl's
//! dat/database/godspeak.txt, and only lines that name the Orb qualify: Xom's "orb gift" speech and
//! nothing else; the weights are crawl's own.

pub struct Splash {
    pub text: &'static str,
    /// How many draws of the hat this line gets (crawl's `w:`).
    pub w: u32,
}

pub const XOM_SPLASHES: [Splash; 3] = [
    Splash { text: "Ponder my incredible Orb!", w: 3 },
    Splash { text: "Is this the Orb you wanted?", w: 1 },
    Splash { text: "Many adventurers have sought this Orb.", w: 1 },
];

pub fn splash_weight(pool: &[Splash]) -> u32 {
    pool.iter().map(|s| s.w).sum()
}

/// One line from the pool by weight, `random` being a draw in [0, 1).
pub fn pick_splash(random: f64, pool: &[Splash]) -> &'static str {
    let mut at = random.clamp(0.0, 1.0 - f64::EPSILON) * splash_weight(pool) as f64;
    for s in pool {
        at -= s.w as f64;
        if at < 0.0 {
            return s.text;
        }
    }
    pool[pool.len() - 1].text
}

/// A draw in [0, 1) from the clock: enough for a splash and a starting place.
pub fn random() -> f64 {
    let ns = isorun_client::platform::now_ms() * 1000.0 + isorun_client::platform::seconds() * 1e9;
    let mut x = ns as u64 ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights() {
        assert_eq!(splash_weight(&XOM_SPLASHES), 5);
        assert_eq!(pick_splash(0.0, &XOM_SPLASHES), "Ponder my incredible Orb!");
        assert_eq!(pick_splash(0.59, &XOM_SPLASHES), "Ponder my incredible Orb!");
        assert_eq!(pick_splash(0.61, &XOM_SPLASHES), "Is this the Orb you wanted?");
        assert_eq!(pick_splash(1.0, &XOM_SPLASHES), "Many adventurers have sought this Orb.");
        let r = random();
        assert!((0.0..1.0).contains(&r));
    }
}

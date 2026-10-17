//! The level map, kept on screen across a key that leaves it for a screen of
//! the main view's (the port of `map-hold.ts`; bindings.rs `off_map`: the stash
//! search, travel, the overview). Crawl closes the map first, then opens the next
//! screen; between the two the view would cut to the dungeon and back. Held, the
//! map stays drawn under the search prompt and its results until crawl brings its
//! own map back (a search result, picked, opens it at the find) or the player is
//! back in the dungeon: the next screen put away, or none came.

use crate::context::Mode;

/// How long the map waits on the screen a key leaves it for, as the runner's `awaitPrompt` waits on a prompt.
const PATIENCE_MS: f64 = 1500.0;

#[derive(Clone, Debug, Default)]
pub struct MapHold {
    hold: Option<(f64, bool)>,
}

impl MapHold {
    pub fn begin(&mut self, now: f64) {
        self.hold = Some((now + PATIENCE_MS, false));
    }

    /// Whether the map shows now: crawl's own, or the one held. `mode` is the context's, from the state just in.
    pub fn shown(&mut self, map_open: bool, mode: Mode, now: f64) -> bool {
        let Some((until, opened)) = self.hold else { return map_open };
        // still the map: the Escape has not landed yet, or crawl's map is back
        if mode == Mode::Levelmap {
            if opened {
                self.hold = None;
            }
            return true;
        }
        if mode != Mode::Command {
            self.hold = Some((until, true));
        } else if opened || now > until {
            self.hold = None;
            return map_open;
        }
        true
    }
}

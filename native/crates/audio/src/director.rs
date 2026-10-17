//! Turns what happens in a game into sounds (`director.ts`): the fight, the steps, the doors,
//! the messages, the level and the player's health. It owns the timers of the heartbeat and of
//! the music's pauses, and the engine is the one place that makes noise.

use isorun_render::{ActorStep, IsoRenderer, PLAYER_ACTOR};
use isorun_scene::{BillboardKind, Scene};
use isorun_webtiles::{formatted_string_to_text, GameMessage, GameState, Keys, Msgch};

use crate::catalog::{Mood, SoundName};
use crate::cues::{ambient_levels, cues_for_combat, heartbeat_delay, hearing, mood_for, sound_for_line, sound_for_use, use_kind_of_key, Cell, CombatEvent, Fighter, Heard, UseKind};
use crate::engine::{AudioSettings, PlayOptions, SoundEngine};

/// What a drawn frame showed (the fields of `IsoRenderer::take_events`' `FrameEvents`):
/// the `act` tokens whose blows showed, the hops started, the doors that moved (x, y, open).
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioFrame<'a> {
    pub shown: &'a [u64],
    pub hops: &'a [ActorStep],
    pub doors: &'a [(i32, i32, bool)],
}

/// What the director needs of the dungeon view.
pub trait AudioView {
    /// Where a cell is on the view, in logical pixels.
    fn screen_of(&self, x: i32, y: i32) -> (f32, f32);
    /// The view's width in logical pixels.
    fn width(&self) -> f32;
    /// The Animation speed setting: a hop of `seconds` on the actors' clock takes
    /// `seconds / animation_speed` on the wall.
    fn animation_speed(&self) -> f64;
}

/// The renderer as the director's view: its projection, with the width and the speed it was
/// given.
pub struct RendererView<'a> {
    pub renderer: &'a IsoRenderer,
    pub width: f32,
    pub animation_speed: f64,
}

impl AudioView for RendererView<'_> {
    fn screen_of(&self, x: i32, y: i32) -> (f32, f32) {
        self.renderer.screen_of(x as f32, y as f32)
    }
    fn width(&self) -> f32 {
        self.width
    }
    fn animation_speed(&self) -> f64 {
        self.animation_speed
    }
}

/// How long a use of an item waits for the message that tells what came of it, in seconds.
const USE_WAIT: f64 = 8.0;
/// After a new level the doors it builds are not opened by anyone, in seconds.
const LEVEL_SETTLE: f64 = 0.8;
/// A monster's footsteps fainter than this are not made.
const FOOTSTEP_MIN_GAIN: f32 = 0.3;
/// The quiet between two tracks, in seconds.
const MUSIC_PAUSE: f64 = 6.0;
const MUSIC_PAUSE_SPREAD: f64 = 14.0;
/// The start of the music after the first level is shown, in seconds.
const MUSIC_START: f64 = 2.5;
/// Cells around the player that the water and the lava are counted in.
const SURROUND: i32 = 4;
/// A blow whose token the renderer never shows is let go after this long, in seconds.
const BLOW_FORGOTTEN: f64 = 10.0;

/// One effect, ready to play.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Play {
    sound: SoundName,
    options: PlayOptions,
}

/// The sounds of one game screen; dropping it fades the music and the beds out.
pub struct GameAudio {
    engine: SoundEngine,
    pending: Option<(UseKind, f64)>,
    place: String,
    depth: i64,
    mood: Option<Mood>,
    level_changed_at: f64,
    /// When the heart beats next, on the engine's clock.
    heartbeat_at: Option<f64>,
    heartbeat_delay: Option<f64>,
    /// When the next track starts.
    music_at: Option<f64>,
    more: bool,
    ambient_key: String,
    /// Blows that wait for the renderer to show them: the `act` token, when they were made, the
    /// sounds.
    blows: Vec<(u64, f64, Vec<Play>)>,
}

impl GameAudio {
    pub fn new(engine: SoundEngine) -> GameAudio {
        GameAudio {
            engine,
            pending: None,
            place: String::new(),
            depth: -1,
            mood: None,
            level_changed_at: f64::NEG_INFINITY,
            heartbeat_at: None,
            heartbeat_delay: None,
            music_at: None,
            more: false,
            ambient_key: String::new(),
            blows: Vec::new(),
        }
    }

    pub fn engine(&self) -> &SoundEngine {
        &self.engine
    }

    /// Follow the Audio settings.
    pub fn apply_settings(&self, settings: &AudioSettings) {
        self.engine.apply_settings(settings);
    }

    /// The window is hidden or shown again.
    pub fn set_hidden(&self, hidden: bool) {
        self.engine.set_hidden(hidden);
    }

    /// The player's blow went out before the server answered.
    pub fn swing(&self) {
        self.engine.play(SoundName::Swing, PlayOptions::default());
    }

    /// A key went to the server (`key` message): Escape lets go of the use that waited.
    pub fn sent_key(&mut self, keycode: i64) {
        if keycode == Keys::ESC as i64 {
            self.pending = None;
        }
    }

    /// Text went to the server (`input` message) in the command mode or not: a key that uses an
    /// item waits for what came of it.
    pub fn sent_input(&mut self, text: &str, command_mode: bool) {
        if !command_mode {
            return;
        }
        self.pending = use_kind_of_key(text).map(|kind| (kind, self.engine.now()));
    }

    /// A new game state: `fresh` are the message lines that arrived since the last ones read
    /// (the lines `combat.ts` `linesSince` gives), `scene` the scene built from the state. Plays
    /// the lines' sounds, the --more--, the heartbeat, and follows the level and the water and
    /// lava around the player.
    pub fn on_state(&mut self, state: &GameState, scene: &Scene, fresh: &[GameMessage]) {
        self.messages(fresh);
        self.status(state.player.hp, state.player.hp_max, state.messages.more, state.watching.is_some());
        if !state.player.place.is_empty() && scene.player_on_level {
            self.level(&state.player.place, state.player.depth);
            self.surroundings(scene);
        }
        self.tick();
    }

    /// The sounds of a fight message, heard from where the fighters stand now. With a `token`
    /// (the one given to `IsoRenderer::act`) they play when the renderer shows the blow
    /// (`FrameEvents::shown`, read by `on_frame`); without one they play now.
    pub fn combat(&mut self, event: &CombatEvent, scene: &Scene, view: &dyn AudioView, token: Option<u64>) {
        let cell_of = |who: Fighter| match who {
            Fighter::Player => Some(scene.player),
            Fighter::Monster(id) => monster_cell(scene, id),
        };
        let plays: Vec<Play> = cues_for_combat(event, cell_of)
            .into_iter()
            .filter_map(|cue| {
                let heard = self.heard(cue.at, scene.player, view)?;
                Some(Play { sound: cue.sound, options: PlayOptions { pan: heard.pan, gain: heard.gain * cue.gain, delay: cue.after, rate: 1.0 } })
            })
            .collect();
        match token {
            Some(token) => self.blows.push((token, self.engine.now(), plays)),
            None => self.play_all(&plays),
        }
    }

    /// A frame was drawn: the renderer's events (`IsoRenderer::take_events`), the scene it
    /// shows and the view. Plays the hops' steps, the doors and the blows shown, and moves the
    /// timers and the engine's fades.
    pub fn on_frame(&mut self, events: AudioFrame<'_>, scene: &Scene, view: &dyn AudioView) {
        for step in events.hops {
            self.hop(step, scene, view);
        }
        for &(x, y, open) in events.doors {
            self.door((x, y), open, scene, view);
        }
        if !events.shown.is_empty() {
            let mut shown = Vec::new();
            self.blows.retain(|(token, _, plays)| {
                if events.shown.contains(token) {
                    shown.extend_from_slice(plays);
                    false
                } else {
                    true
                }
            });
            self.play_all(&shown);
        }
        let now = self.engine.now();
        self.blows.retain(|(_, at, _)| now - at < BLOW_FORGOTTEN);
        self.tick();
        self.engine.update();
    }

    /// An actor started a hop; its foot comes down near the end of it.
    pub fn hop(&self, step: &ActorStep, scene: &Scene, view: &dyn AudioView) {
        let seconds = step.seconds / view.animation_speed().max(0.01);
        if step.id == PLAYER_ACTOR {
            self.engine.play(SoundName::Step, PlayOptions { delay: seconds * 0.8, ..Default::default() });
            return;
        }
        let Some(heard) = self.heard(Some(step.to), scene.player, view) else { return };
        if heard.gain < FOOTSTEP_MIN_GAIN {
            return;
        }
        self.engine.play(SoundName::Step, PlayOptions { pan: heard.pan, gain: 0.45 * heard.gain, delay: seconds * 0.8, rate: 1.0 });
    }

    /// A door swung open or shut.
    pub fn door(&self, cell: Cell, open: bool, scene: &Scene, view: &dyn AudioView) {
        if self.engine.now() - self.level_changed_at < LEVEL_SETTLE {
            return;
        }
        if let Some(heard) = self.heard(Some(cell), scene.player, view) {
            let sound = if open { SoundName::DoorOpen } else { SoundName::DoorClose };
            self.engine.play(sound, PlayOptions { pan: heard.pan, gain: heard.gain, ..Default::default() });
        }
    }

    /// The message lines that arrived since the last ones.
    pub fn messages(&mut self, lines: &[GameMessage]) {
        let texts: Vec<String> = lines.iter().filter(|l| l.channel != Some(Msgch::PROMPT)).map(|l| formatted_string_to_text(&l.text).trim().to_string()).collect();
        if texts.is_empty() {
            return;
        }
        if let Some((kind, at)) = self.pending {
            if self.engine.now() - at < USE_WAIT {
                if let Some(sound) = sound_for_use(kind, &texts) {
                    self.engine.play(sound, PlayOptions::default());
                    self.pending = None;
                }
            }
        }
        let mut sounds: Vec<SoundName> = Vec::new();
        for text in &texts {
            if let Some(sound) = sound_for_line(text) {
                if !sounds.contains(&sound) {
                    sounds.push(sound);
                }
            }
        }
        for sound in sounds {
            self.engine.play(sound, PlayOptions::default());
        }
    }

    /// The player's hit points, and whether the game waits at a --more--: the heart beats when
    /// they are low. A watched game's heart is not the watcher's.
    pub fn status(&mut self, hp: i64, hp_max: i64, more: bool, watching: bool) {
        if more && !self.more {
            self.engine.play(SoundName::More, PlayOptions::default());
        }
        self.more = more;
        let delay = if watching || hp_max <= 0 { None } else { heartbeat_delay(hp as f64 / hp_max as f64) };
        self.heartbeat_delay = delay;
        if delay.is_some() && self.heartbeat_at.is_none() {
            self.beat();
        }
    }

    /// The player is on this level; the music follows the place, the stairs sound when the
    /// level is a new one.
    pub fn level(&mut self, place: &str, depth: i64) {
        if place == self.place && depth == self.depth {
            return;
        }
        let first = self.depth < 0;
        self.place = place.to_string();
        self.depth = depth;
        self.level_changed_at = self.engine.now();
        self.ambient_key.clear();
        if !first {
            self.engine.play(SoundName::Stairs, PlayOptions::default());
        }
        let mood = mood_for(place, depth);
        if Some(mood) == self.mood {
            return;
        }
        self.mood = Some(mood);
        self.music_at = Some(self.engine.now() + if first { MUSIC_START } else { 0.0 });
        self.tick();
    }

    /// The player stands here: water and lava around add their sound to the bed of the place.
    pub fn surroundings(&mut self, scene: &Scene) {
        let (mut water, mut lava) = (0, 0);
        let (x, y) = scene.player;
        for dy in -SURROUND..=SURROUND {
            for dx in -SURROUND..=SURROUND {
                if let Some(cell) = scene.cell(x + dx, y + dy) {
                    if cell.flags.water {
                        water += 1;
                    }
                    if cell.flags.lava {
                        lava += 1;
                    }
                }
            }
        }
        let levels = ambient_levels(&self.place, water, lava);
        let key = format!("{}|{}|{}", self.place, (levels.water * 5.0).round(), (levels.lava * 5.0).round());
        if key == self.ambient_key {
            return;
        }
        self.ambient_key = key;
        self.engine.set_ambient(levels);
    }

    /// Fire the timers that are due: the heartbeat, the next track.
    fn tick(&mut self) {
        let now = self.engine.now();
        if self.engine.take_music_ended() {
            self.music_at = Some(now + MUSIC_PAUSE + self.engine.random() * MUSIC_PAUSE_SPREAD);
        }
        if self.music_at.is_some_and(|t| now >= t) {
            self.music_at = None;
            if let Some(mood) = self.mood {
                self.engine.play_mood(mood);
            }
        }
        if self.heartbeat_at.is_some_and(|t| now >= t) {
            self.beat();
        }
    }

    fn beat(&mut self) {
        let Some(delay) = self.heartbeat_delay else {
            self.heartbeat_at = None;
            return;
        };
        self.engine.play(SoundName::Heartbeat, PlayOptions::default());
        self.heartbeat_at = Some(self.engine.now() + delay);
    }

    fn play_all(&self, plays: &[Play]) {
        for p in plays {
            self.engine.play(p.sound, p.options);
        }
    }

    fn heard(&self, at: Option<Cell>, listener: Cell, view: &dyn AudioView) -> Option<Heard> {
        let Some(at) = at else { return Some(Heard { pan: 0.0, gain: 1.0 }) };
        let half = (view.width() / 2.0).max(0.5);
        hearing(listener, at, (view.screen_of(at.0, at.1).0 - half) / half)
    }
}

impl Drop for GameAudio {
    fn drop(&mut self) {
        self.engine.dispose();
    }
}

/// Where the monster with this id stands on the scene, if it is in view (`combat.ts`
/// `monsterCell`).
pub fn monster_cell(scene: &Scene, id: i64) -> Option<Cell> {
    scene.billboards.iter().find(|b| b.kind == BillboardKind::Monster && b.ref_id == Some(id)).map(|b| (b.x, b.y))
}

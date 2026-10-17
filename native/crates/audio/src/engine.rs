//! The game's sound (`engine.ts`): effects decoded once and played through small voices, the
//! ambient beds and the music on buses of their own, all under one master gain that follows the
//! settings. It starts on `unlock` (on the web the first gesture of the page, as browsers
//! require; natively at once) and sleeps while the window is hidden. With no device every call
//! still runs its logic (and the trace) and nothing is heard.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};

use kira::sound::static_sound::StaticSoundData;

use crate::catalog::{music, AmbientLevels, AmbientName, Mood, SoundName};
use crate::mixer::{Bus, Mixer, NullMixer, Shot, Voice, Volumes};

/// The most sounds that play at once, of any kind.
const MAX_VOICES: usize = 24;
/// Seconds a track takes to fade in or out, and the time a bed or a loop takes to follow the
/// level it should have.
const MUSIC_FADE: f64 = 3.0;
const AMBIENT_FADE: f64 = 1.5;

/// Where the time comes from, in seconds: the wall clock, or a hand-moved one in tests.
pub trait Clock {
    fn now(&self) -> f64;
}

/// The wall clock (`web_time`, so the web has it too).
pub struct SystemClock(web_time::Instant);

impl SystemClock {
    pub fn new() -> SystemClock {
        SystemClock(web_time::Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

/// A clock moved by hand, shared by its clones.
#[derive(Clone, Default)]
pub struct ManualClock(Rc<Cell<f64>>);

impl ManualClock {
    pub fn new() -> ManualClock {
        ManualClock::default()
    }
    pub fn set(&self, seconds: f64) {
        self.0.set(seconds);
    }
    pub fn advance(&self, seconds: f64) {
        self.0.set(self.0.get() + seconds);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> f64 {
        self.0.get()
    }
}

/// The Audio settings: volumes 0 to 100, and the mute that keeps them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub effects_volume: f32,
    pub ambient_volume: f32,
    pub music_volume: f32,
    /// Silences everything and keeps the volumes.
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings { master_volume: 70.0, effects_volume: 100.0, ambient_volume: 70.0, music_volume: 50.0, muted: false }
    }
}

/// How an effect plays beside its spec.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayOptions {
    /// -1 is the left ear, 1 the right.
    pub pan: f32,
    /// Louder or softer than the sound is by itself.
    pub gain: f32,
    /// Seconds from now.
    pub delay: f64,
    pub rate: f64,
}

impl Default for PlayOptions {
    fn default() -> Self {
        PlayOptions { pan: 0.0, gain: 1.0, delay: 0.0, rate: 1.0 }
    }
}

/// A gain that follows its target as WebAudio's `setTargetAtTime` does: it covers about 63% of
/// the way in `tau` seconds, 95% in three times that.
#[derive(Clone, Copy, Debug)]
struct Fader {
    value: f32,
    target: f32,
    tau: f64,
}

impl Fader {
    fn step(&mut self, dt: f64) {
        let k = 1.0 - (-dt / self.tau).exp();
        self.value += (self.target - self.value) * k as f32;
    }
}

struct Layer {
    voice: Option<Box<dyn Voice>>,
    fader: Fader,
    level: f32,
    /// When a layer at 0 lets go of its voice, on the audio clock.
    stop_at: Option<f64>,
}

struct Track {
    name: &'static str,
    voice: Box<dyn Voice>,
    fader: Fader,
    /// Audio seconds played, to find its end where the mixer cannot tell (no device).
    played: f64,
    duration: Option<f64>,
}

struct Fading {
    voice: Box<dyn Voice>,
    fader: Fader,
    stop_at: f64,
}

struct Engine {
    clock: Rc<dyn Clock>,
    mixer: Option<Box<dyn Mixer>>,
    make_mixer: Option<Box<dyn FnOnce(&str) -> Box<dyn Mixer>>>,
    /// Where the files are fetched from on the web.
    base: String,
    decoded: HashMap<String, StaticSoundData>,
    loader: Option<Receiver<(String, StaticSoundData)>>,
    /// The effects that sound now, and when each ends on the audio clock.
    active: Vec<(SoundName, f64)>,
    last_start: HashMap<SoundName, f64>,
    layers: [Option<Layer>; 4],
    wanted_mood: Option<Mood>,
    current: Option<Track>,
    fading: Vec<Fading>,
    volumes: Volumes,
    ambient_levels: AmbientLevels,
    trace: Option<Vec<String>>,
    music_ended: bool,
    hidden: bool,
    /// The audio clock: it stands while the window is hidden, as a suspended `AudioContext`.
    audio_base: f64,
    resumed_at: f64,
    last_update: f64,
    rng: u64,
}

/// The one sound engine of the app; a cheap handle, its clones share it.
#[derive(Clone)]
pub struct SoundEngine {
    inner: Rc<RefCell<Engine>>,
}

impl SoundEngine {
    /// An engine on the wall clock that plays on the default device once unlocked.
    pub fn new() -> SoundEngine {
        let engine = SoundEngine::with_clock(Rc::new(SystemClock::new()), seed());
        engine.inner.borrow_mut().make_mixer = Some(Box::new(device_mixer));
        engine
    }

    /// An engine with no device, unlocked at once: the logic, the timing and the trace run as
    /// with a device. For tests, and for a machine with no sound.
    pub fn null(clock: Rc<dyn Clock>, seed: u64) -> SoundEngine {
        let engine = SoundEngine::with_clock(clock, seed);
        engine.inner.borrow_mut().make_mixer = Some(Box::new(|_| Box::new(NullMixer)));
        engine.unlock();
        engine
    }

    /// An engine mixed by kira with no device, unlocked at once; the mix is pulled with the
    /// returned output (tests, and rendering a mix to a file).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn offline(clock: Rc<dyn Clock>, seed: u64, sample_rate: u32) -> (SoundEngine, crate::mixer::OfflineOutput) {
        use crate::mixer::{KiraMixer, OfflineBackend, OfflineSettings};
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let output = crate::mixer::OfflineOutput { slot: slot.clone(), sample_rate };
        let engine = SoundEngine::with_clock(clock, seed);
        engine.inner.borrow_mut().make_mixer = Some(Box::new(move |_| match KiraMixer::<OfflineBackend>::new(OfflineSettings { sample_rate, slot }, "") {
            Ok(m) => Box::new(m) as Box<dyn Mixer>,
            Err(_) => Box::new(NullMixer),
        }));
        engine.unlock();
        (engine, output)
    }

    fn with_clock(clock: Rc<dyn Clock>, seed: u64) -> SoundEngine {
        let now = clock.now();
        SoundEngine {
            inner: Rc::new(RefCell::new(Engine {
                clock,
                mixer: None,
                make_mixer: None,
                base: "audio/".into(),
                decoded: HashMap::new(),
                loader: None,
                active: Vec::new(),
                last_start: HashMap::new(),
                layers: [None, None, None, None],
                wanted_mood: None,
                current: None,
                fading: Vec::new(),
                volumes: Volumes { master: 0.7, effects: 1.0, ambient: 0.7, music: 0.5 },
                ambient_levels: AmbientLevels::default(),
                trace: None,
                music_ended: false,
                hidden: false,
                audio_base: 0.0,
                resumed_at: now,
                last_update: now,
                rng: seed | 1,
            })),
        }
    }

    /// Where the sound files are on the web, ending in `/` (the page's `audio/` by default);
    /// natively they are in the binary. Set it before `unlock`.
    pub fn set_base(&self, base: &str) {
        self.inner.borrow_mut().base = base.to_string();
    }

    /// Start the sound (on the web from a gesture of the player); does nothing the second time.
    /// When the device cannot be opened the engine goes on with none.
    pub fn unlock(&self) {
        let mut e = self.inner.borrow_mut();
        let Some(make) = e.make_mixer.take() else { return };
        let mut mixer = make(&e.base);
        mixer.set_volumes(e.effective_volumes());
        e.mixer = Some(mixer);
        let now = e.clock.now();
        e.resumed_at = now;
        e.last_update = now;
        e.preload();
        let levels = e.ambient_levels;
        e.set_ambient(levels);
        if let Some(mood) = e.wanted_mood {
            e.play_mood(mood);
        }
    }

    /// Whether sounds are heard now: unlocked and not hidden.
    pub fn running(&self) -> bool {
        let e = self.inner.borrow();
        e.mixer.is_some() && !e.hidden
    }

    /// Whether every effect and loop is decoded.
    pub fn loaded(&self) -> bool {
        let mut e = self.inner.borrow_mut();
        e.poll_loader();
        e.loader.is_none() && e.mixer.is_some()
    }

    /// Wait until every effect is decoded (natively; tests and tools).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_loaded(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.loaded() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// Follow the volumes of the settings.
    pub fn apply_settings(&self, settings: &AudioSettings) {
        let mut e = self.inner.borrow_mut();
        e.volumes = Volumes {
            master: if settings.muted { 0.0 } else { settings.master_volume / 100.0 },
            effects: settings.effects_volume / 100.0,
            ambient: settings.ambient_volume / 100.0,
            music: settings.music_volume / 100.0,
        };
        let v = e.effective_volumes();
        if let Some(m) = e.mixer.as_mut() {
            m.set_volumes(v);
        }
    }

    /// Play an effect; a sound that is too frequent, too many at once, or not decoded yet is
    /// dropped.
    pub fn play(&self, name: SoundName, options: PlayOptions) {
        self.inner.borrow_mut().play(name, options);
    }

    /// Let the beds and loops sound at these levels (0 to 1); one at 0 fades out.
    pub fn set_ambient(&self, levels: AmbientLevels) {
        self.inner.borrow_mut().set_ambient(levels);
    }

    /// Fade to a random track of the mood, other than the one that plays.
    pub fn play_mood(&self, mood: Mood) {
        self.inner.borrow_mut().play_mood(mood);
    }

    /// Fade the music out.
    pub fn stop_music(&self) {
        self.inner.borrow_mut().stop_music();
    }

    /// Fade everything out.
    pub fn dispose(&self) {
        let mut e = self.inner.borrow_mut();
        e.stop_music();
        e.set_ambient(AmbientLevels::default());
    }

    /// The window is hidden (or shown again): the sound sleeps, as a hidden tab's.
    pub fn set_hidden(&self, hidden: bool) {
        let mut e = self.inner.borrow_mut();
        if e.hidden == hidden {
            return;
        }
        let now = e.clock.now();
        if hidden {
            e.audio_base += now - e.resumed_at;
        } else {
            e.resumed_at = now;
            e.last_update = now;
        }
        e.hidden = hidden;
        if let Some(m) = e.mixer.as_mut() {
            m.set_paused(hidden);
        }
        for voice in e.voices_mut() {
            voice.set_paused(hidden);
        }
    }

    /// Move the fades, start what was waiting for its file, end what came to its end. Call it
    /// once a frame (`GameAudio::on_frame` does).
    pub fn update(&self) {
        self.inner.borrow_mut().update();
    }

    /// Keep the names of the sounds played from now on (`?audio-trace`).
    pub fn enable_trace(&self) {
        let mut e = self.inner.borrow_mut();
        if e.trace.is_none() {
            e.trace = Some(Vec::new());
        }
    }

    /// The names of the sounds played since the trace was enabled: an effect by its name
    /// (`hit-heavy`), a track as `music:<name>`.
    pub fn trace(&self) -> Vec<String> {
        self.inner.borrow().trace.clone().unwrap_or_default()
    }

    pub fn take_trace(&self) -> Vec<String> {
        self.inner.borrow_mut().trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Whether the track that was playing came to its end since the last call.
    pub fn take_music_ended(&self) -> bool {
        std::mem::take(&mut self.inner.borrow_mut().music_ended)
    }

    /// The name of the track that plays.
    pub fn current_track(&self) -> Option<&'static str> {
        self.inner.borrow().current.as_ref().map(|t| t.name)
    }

    /// The level each loop is fading to times its gain, 0 for one that is off.
    pub fn ambient_target(&self, name: AmbientName) -> f32 {
        self.inner.borrow().layers[name.index()].as_ref().map_or(0.0, |l| l.fader.target)
    }

    /// The engine's wall clock, in seconds.
    pub fn now(&self) -> f64 {
        self.inner.borrow().clock.now()
    }

    /// A random number in 0..1 from the engine's generator.
    pub fn random(&self) -> f64 {
        self.inner.borrow_mut().random()
    }
}

impl Default for SoundEngine {
    fn default() -> Self {
        SoundEngine::new()
    }
}

fn seed() -> u64 {
    let t = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9e37_79b9_7f4a_7c15);
    t ^ 0x2545_f491_4f6c_dd1d
}

#[cfg(feature = "device")]
fn device_mixer(base: &str) -> Box<dyn Mixer> {
    use kira::backend::cpal::CpalBackend;
    match crate::mixer::KiraMixer::<CpalBackend>::new(Default::default(), base) {
        Ok(m) => Box::new(m),
        Err(e) => {
            log::warn!("no sound device ({e}); the game is silent");
            Box::new(NullMixer)
        }
    }
}

#[cfg(not(feature = "device"))]
fn device_mixer(_: &str) -> Box<dyn Mixer> {
    Box::new(NullMixer)
}

impl Engine {
    fn audio_time(&self) -> f64 {
        if self.hidden {
            self.audio_base
        } else {
            self.audio_base + (self.clock.now() - self.resumed_at)
        }
    }

    fn random(&mut self) -> f64 {
        // xorshift64*: enough for picking a file and a little spread.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn effective_volumes(&self) -> Volumes {
        self.volumes
    }

    fn play(&mut self, name: SoundName, options: PlayOptions) {
        self.poll_loader();
        if self.mixer.is_none() || self.hidden {
            return;
        }
        let spec = name.spec();
        let now = self.audio_time();
        let start_at = now + options.delay;
        self.active.retain(|(_, end)| *end > now);
        let last = self.last_start.get(&name).copied().unwrap_or(f64::NEG_INFINITY);
        let count = self.active.iter().filter(|(n, _)| *n == name).count();
        if start_at - last < spec.cooldown || count >= spec.voices || self.active.len() >= MAX_VOICES {
            return;
        }
        let file = spec.files[((self.random() * spec.files.len() as f64) as usize).min(spec.files.len() - 1)];
        let Some(data) = self.decoded.get(&format!("sfx/{file}")).cloned() else { return };
        if let Some(trace) = self.trace.as_mut() {
            trace.push(name.as_str().to_string());
        }
        self.last_start.insert(name, start_at);
        let rate = spec.rate * options.rate * (1.0 + (self.random() * 2.0 - 1.0) * spec.pitch_spread);
        let gain = spec.gain * options.gain * (1.0 - self.random() as f32 * spec.gain_spread);
        let end = start_at + data.duration().as_secs_f64() / rate.max(0.01);
        self.active.push((name, end));
        if let Some(m) = self.mixer.as_mut() {
            m.play(Shot { data: &data, rate, gain, pan: options.pan, delay: options.delay });
        }
    }

    fn set_ambient(&mut self, levels: AmbientLevels) {
        self.ambient_levels = levels;
        if self.mixer.is_none() {
            return;
        }
        for name in AmbientName::ALL {
            let level = levels.get(name);
            if self.layers[name.index()].is_none() && level <= 0.0 {
                continue;
            }
            self.layer_for(name, level);
        }
    }

    fn layer_for(&mut self, name: AmbientName, level: f32) {
        let spec = name.spec();
        let now = self.audio_time();
        let i = name.index();
        if self.layers[i].is_none() {
            self.layers[i] = Some(Layer { voice: None, fader: Fader { value: 0.0, target: 0.0, tau: AMBIENT_FADE / 3.0 }, level: 0.0, stop_at: None });
            self.start_layer(name);
        }
        let layer = self.layers[i].as_mut().expect("made above");
        layer.level = level;
        layer.fader.target = level * spec.gain;
        layer.stop_at = if level <= 0.0 { Some(now + AMBIENT_FADE * 3.0) } else { None };
    }

    fn start_layer(&mut self, name: AmbientName) {
        let spec = name.spec();
        let path = format!("ambient/{}", spec.file);
        let data = if spec.stream {
            None
        } else {
            match self.decoded.get(&path) {
                Some(d) => Some(d.clone()),
                // Not decoded yet: `update` starts it when it is.
                None => return,
            }
        };
        let hidden = self.hidden;
        let Some(mixer) = self.mixer.as_mut() else { return };
        let voice = mixer.start(&path, data.as_ref(), true, Bus::Ambient);
        if let Some(layer) = self.layers[name.index()].as_mut() {
            layer.voice = voice;
            if hidden {
                if let Some(v) = layer.voice.as_mut() {
                    v.set_paused(true);
                }
            }
        }
    }

    fn play_mood(&mut self, mood: Mood) {
        if self.mixer.is_none() {
            self.wanted_mood = Some(mood);
            return;
        }
        let playing = self.current.as_ref().map(|t| t.name);
        let tracks: Vec<&'static str> = music(mood).iter().copied().filter(|t| Some(*t) != playing).collect();
        if tracks.is_empty() {
            return;
        }
        let pick = ((self.random() * tracks.len() as f64) as usize).min(tracks.len() - 1);
        self.play_track(tracks[pick]);
    }

    fn stop_music(&mut self) {
        self.wanted_mood = None;
        if let Some(track) = self.current.take() {
            self.fade_out(track);
        }
    }

    fn play_track(&mut self, name: &'static str) {
        if let Some(track) = self.current.take() {
            self.fade_out(track);
        }
        let hidden = self.hidden;
        let Some(mixer) = self.mixer.as_mut() else { return };
        let Some(mut voice) = mixer.start(&format!("music/{name}"), None, false, Bus::Music) else { return };
        if hidden {
            voice.set_paused(true);
        }
        let duration = voice.duration();
        self.current = Some(Track { name, voice, fader: Fader { value: 0.0, target: 1.0, tau: MUSIC_FADE / 3.0 }, played: 0.0, duration });
        if let Some(trace) = self.trace.as_mut() {
            trace.push(format!("music:{name}"));
        }
    }

    fn fade_out(&mut self, track: Track) {
        let mut fader = track.fader;
        fader.target = 0.0;
        self.fading.push(Fading { voice: track.voice, fader, stop_at: self.audio_time() + MUSIC_FADE });
    }

    fn voices_mut(&mut self) -> Vec<&mut Box<dyn Voice>> {
        let mut out: Vec<&mut Box<dyn Voice>> = Vec::new();
        if let Some(t) = self.current.as_mut() {
            out.push(&mut t.voice);
        }
        for f in self.fading.iter_mut() {
            out.push(&mut f.voice);
        }
        for layer in self.layers.iter_mut().flatten() {
            if let Some(v) = layer.voice.as_mut() {
                out.push(v);
            }
        }
        out
    }

    fn update(&mut self) {
        self.poll_loader();
        let wall = self.clock.now();
        let dt = if self.hidden { 0.0 } else { (wall - self.last_update).max(0.0) };
        self.last_update = wall;
        if self.mixer.is_none() {
            return;
        }
        let now = self.audio_time();
        let music_bus = self.volumes.master * self.volumes.music;
        let ambient_bus = self.volumes.master * self.volumes.ambient;

        if let Some(track) = self.current.as_mut() {
            track.fader.step(dt);
            track.played += dt;
            track.voice.set_gain(track.fader.value, music_bus);
            let ended = track.voice.finished() || track.duration.is_some_and(|d| track.played >= d);
            if ended {
                let track = self.current.take().expect("checked");
                self.fade_out(track);
                self.music_ended = true;
            }
        }
        self.fading.retain_mut(|f| {
            f.fader.step(dt);
            f.voice.set_gain(f.fader.value, music_bus);
            if now >= f.stop_at {
                f.voice.stop();
                return false;
            }
            true
        });

        for name in AmbientName::ALL {
            let i = name.index();
            let Some(layer) = self.layers[i].as_mut() else { continue };
            if layer.stop_at.is_some_and(|t| now >= t) && layer.level <= 0.0 {
                if let Some(v) = layer.voice.as_mut() {
                    v.stop();
                }
                self.layers[i] = None;
                continue;
            }
            if layer.voice.is_none() {
                self.start_layer(name);
            }
            let layer = self.layers[i].as_mut().expect("kept");
            layer.fader.step(dt);
            if let Some(v) = layer.voice.as_mut() {
                v.set_gain(layer.fader.value, ambient_bus);
            }
        }
        self.active.retain(|(_, end)| *end > now);
    }

    // ------------------------------------------------------------------------------- loading

    fn poll_loader(&mut self) {
        let Some(rx) = self.loader.as_ref() else { return };
        loop {
            match rx.try_recv() {
                Ok((path, data)) => {
                    self.decoded.insert(path, data);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.loader = None;
                    return;
                }
            }
        }
    }

    /// The effects and the short loops, decoded once.
    fn preload_paths() -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        for name in SoundName::ALL {
            for file in name.spec().files {
                let p = format!("sfx/{file}");
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
        for name in AmbientName::ALL {
            if !name.spec().stream {
                paths.push(format!("ambient/{}", name.spec().file));
            }
        }
        paths
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn preload(&mut self) {
        let (tx, rx) = channel();
        self.loader = Some(rx);
        let paths = Engine::preload_paths();
        let spawned = std::thread::Builder::new().name("isorun-audio-decode".into()).spawn(move || {
            for path in paths {
                let Some(bytes) = crate::embedded::bytes(&path) else {
                    log::warn!("no sound file {path}");
                    continue;
                };
                match StaticSoundData::from_cursor(std::io::Cursor::new(bytes)) {
                    Ok(data) => {
                        if tx.send((path, data)).is_err() {
                            return;
                        }
                    }
                    Err(e) => log::warn!("cannot decode {path}: {e}"),
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("no thread to decode the sounds: {e}");
            self.loader = None;
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn preload(&mut self) {
        let (tx, rx) = channel();
        self.loader = Some(rx);
        crate::web::fetch_all(self.base.clone(), Engine::preload_paths(), tx);
    }
}

//! Where the sound goes: kira on a device (cpal: ALSA, WASAPI, AAudio, WebAudio), kira rendered
//! by hand (`OfflineOutput`, for tests), or nowhere (`NullMixer`, when there is no device). The
//! engine decides what plays and when; a mixer only makes it heard.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use kira::backend::{Backend, Renderer};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, Easing, Panning, StartTime, Tween};

/// The buses a loop or a track can go to (the effects have a third, under the same master).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bus {
    Ambient,
    Music,
}

/// The gains of the master and the buses, 0 to 1 each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Volumes {
    pub master: f32,
    pub effects: f32,
    pub ambient: f32,
    pub music: f32,
}

/// One effect to start: its decoded sound and how it plays.
pub(crate) struct Shot<'a> {
    pub data: &'a StaticSoundData,
    pub rate: f64,
    pub gain: f32,
    pub pan: f32,
    pub delay: f64,
}

/// A loop or a track that keeps playing: its loudness is set by the engine's fades.
pub(crate) trait Voice {
    /// `gain` is the voice's own (its fade times its level); `bus` is the master times its bus,
    /// which a mixer with real buses ignores.
    fn set_gain(&mut self, gain: f32, bus: f32);
    /// Came to its end by itself.
    fn finished(&self) -> bool;
    fn stop(&mut self);
    /// Only for voices outside the mixer's buses (the web's media elements).
    fn set_paused(&mut self, _paused: bool) {}
    /// How long one pass of it lasts, when known.
    fn duration(&self) -> Option<f64>;
}

pub(crate) trait Mixer {
    fn play(&mut self, shot: Shot<'_>);
    /// Start a loop or a track from its file (`ambient/dungeon`, `music/cave`): read as it plays
    /// when `stream`, else from `data`, decoded before.
    fn start(&mut self, path: &str, data: Option<&StaticSoundData>, looped: bool, bus: Bus) -> Option<Box<dyn Voice>>;
    fn set_volumes(&mut self, volumes: Volumes);
    fn set_paused(&mut self, paused: bool);
}

/// An amplitude as kira's decibels; kira takes -60 dB as silence.
pub(crate) fn decibels(gain: f32) -> Decibels {
    if gain <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels((20.0 * gain.log10()).max(Decibels::SILENCE.0))
    }
}

fn tween(seconds: f64) -> Tween {
    Tween { start_time: StartTime::Immediate, duration: Duration::from_secs_f64(seconds), easing: Easing::Linear }
}

/// A gain change is followed this fast: a frame or two, so the engine's fades come out smooth.
const GAIN_TWEEN: f64 = 0.04;
/// The volumes of the settings follow in about this long (`VOLUME_FADE` three times over).
const VOLUME_TWEEN: f64 = 0.15;

// ---------------------------------------------------------------------------------------- null

/// No device: nothing is heard, the engine's logic runs as it would.
pub(crate) struct NullMixer;

struct NullVoice {
    duration: Option<f64>,
}

impl Voice for NullVoice {
    fn set_gain(&mut self, _: f32, _: f32) {}
    fn finished(&self) -> bool {
        false
    }
    fn stop(&mut self) {}
    fn duration(&self) -> Option<f64> {
        self.duration
    }
}

impl Mixer for NullMixer {
    fn play(&mut self, _: Shot<'_>) {}

    fn start(&mut self, path: &str, data: Option<&StaticSoundData>, _looped: bool, _bus: Bus) -> Option<Box<dyn Voice>> {
        let duration = match data {
            Some(d) => Some(d.duration().as_secs_f64()),
            None => stream_duration(path),
        };
        Some(Box::new(NullVoice { duration }))
    }

    fn set_volumes(&mut self, _: Volumes) {}
    fn set_paused(&mut self, _: bool) {}
}

#[cfg(not(target_arch = "wasm32"))]
fn stream_duration(path: &str) -> Option<f64> {
    let bytes = crate::embedded::bytes(path)?;
    kira::sound::streaming::StreamingSoundData::from_cursor(std::io::Cursor::new(bytes)).ok().map(|d| d.duration().as_secs_f64())
}

#[cfg(target_arch = "wasm32")]
fn stream_duration(_: &str) -> Option<f64> {
    None
}

// ---------------------------------------------------------------------------------------- kira

pub(crate) struct KiraMixer<B: Backend> {
    manager: AudioManager<B>,
    effects: TrackHandle,
    ambient: TrackHandle,
    music: TrackHandle,
    #[cfg(target_arch = "wasm32")]
    base: String,
    #[cfg(target_arch = "wasm32")]
    volumes: Volumes,
}

impl<B: Backend> KiraMixer<B> {
    pub fn new(settings: B::Settings, base: &str) -> Result<Self, String>
    where
        B::Error: std::fmt::Debug,
    {
        let _ = base;
        let mut manager = AudioManager::<B>::new(AudioManagerSettings {
            capacities: Default::default(),
            main_track_builder: Default::default(),
            // The modulators and clocks update every 128 frames, about 3 ms.
            internal_buffer_size: 128,
            backend_settings: settings,
        })
        .map_err(|e| format!("{e:?}"))?;
        let mut track = || manager.add_sub_track(TrackBuilder::new().sound_capacity(64)).map_err(|e| format!("{e:?}"));
        let effects = track()?;
        let ambient = track()?;
        let music = track()?;
        Ok(KiraMixer {
            manager,
            effects,
            ambient,
            music,
            #[cfg(target_arch = "wasm32")]
            base: base.to_string(),
            #[cfg(target_arch = "wasm32")]
            volumes: Volumes { master: 1.0, effects: 1.0, ambient: 1.0, music: 1.0 },
        })
    }

    fn track(&mut self, bus: Bus) -> &mut TrackHandle {
        match bus {
            Bus::Ambient => &mut self.ambient,
            Bus::Music => &mut self.music,
        }
    }
}

impl<B: Backend> Mixer for KiraMixer<B> {
    fn play(&mut self, shot: Shot<'_>) {
        let mut settings = StaticSoundSettings::new().volume(decibels(shot.gain)).playback_rate(shot.rate);
        if shot.pan.abs() > 0.02 {
            settings = settings.panning(Panning(shot.pan.clamp(-1.0, 1.0)));
        }
        if shot.delay > 0.0 {
            settings = settings.start_time(StartTime::Delayed(Duration::from_secs_f64(shot.delay)));
        }
        if let Err(e) = self.effects.play(shot.data.with_settings(settings)) {
            log::debug!("an effect was not played: {e:?}");
        }
    }

    fn start(&mut self, path: &str, data: Option<&StaticSoundData>, looped: bool, bus: Bus) -> Option<Box<dyn Voice>> {
        if let Some(data) = data {
            let mut d = data.volume(Decibels::SILENCE);
            if looped {
                d = d.loop_region(..);
            }
            let duration = d.duration().as_secs_f64();
            let handle = self.track(bus).play(d).ok()?;
            return Some(Box::new(KiraStatic { handle, last: -1.0, duration }));
        }
        self.stream(path, looped, bus)
    }

    fn set_volumes(&mut self, v: Volumes) {
        let t = tween(VOLUME_TWEEN);
        self.manager.main_track().set_volume(decibels(v.master), t);
        self.effects.set_volume(decibels(v.effects), t);
        self.ambient.set_volume(decibels(v.ambient), t);
        self.music.set_volume(decibels(v.music), t);
        #[cfg(target_arch = "wasm32")]
        {
            self.volumes = v;
        }
    }

    fn set_paused(&mut self, paused: bool) {
        let t = tween(0.0);
        for track in [&mut self.effects, &mut self.ambient, &mut self.music] {
            if paused {
                track.pause(t);
            } else {
                track.resume(t);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<B: Backend> KiraMixer<B> {
    fn stream(&mut self, path: &str, looped: bool, bus: Bus) -> Option<Box<dyn Voice>> {
        use kira::sound::streaming::StreamingSoundData;
        let bytes = crate::embedded::bytes(path)?;
        let mut data = StreamingSoundData::from_cursor(std::io::Cursor::new(bytes)).ok()?.volume(Decibels::SILENCE);
        if looped {
            data = data.loop_region(..);
        }
        let duration = data.duration().as_secs_f64();
        let handle = self.track(bus).play(data).ok()?;
        Some(Box::new(KiraStream { handle, last: -1.0, duration }))
    }
}

#[cfg(target_arch = "wasm32")]
impl<B: Backend> KiraMixer<B> {
    /// kira cannot stream on the web (no threads), and decoding minutes of music on the page's
    /// one thread would stall it: a stream is an `<audio>` element, as in the TypeScript.
    fn stream(&mut self, path: &str, looped: bool, bus: Bus) -> Option<Box<dyn Voice>> {
        let _ = bus;
        crate::web::MediaVoice::start(&format!("{}{path}.ogg", self.base), looped).map(|v| Box::new(v) as Box<dyn Voice>)
    }
}

struct KiraStatic {
    handle: kira::sound::static_sound::StaticSoundHandle,
    last: f32,
    duration: f64,
}

impl Voice for KiraStatic {
    fn set_gain(&mut self, gain: f32, _: f32) {
        if (gain - self.last).abs() > 0.0005 {
            self.last = gain;
            self.handle.set_volume(decibels(gain), tween(GAIN_TWEEN));
        }
    }
    fn finished(&self) -> bool {
        self.handle.state() == kira::sound::PlaybackState::Stopped
    }
    fn stop(&mut self) {
        self.handle.stop(tween(0.02));
    }
    fn duration(&self) -> Option<f64> {
        Some(self.duration)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct KiraStream {
    handle: kira::sound::streaming::StreamingSoundHandle<kira::sound::FromFileError>,
    last: f32,
    duration: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Voice for KiraStream {
    fn set_gain(&mut self, gain: f32, _: f32) {
        if (gain - self.last).abs() > 0.0005 {
            self.last = gain;
            self.handle.set_volume(decibels(gain), tween(GAIN_TWEEN));
        }
    }
    fn finished(&self) -> bool {
        self.handle.state() == kira::sound::PlaybackState::Stopped
    }
    fn stop(&mut self) {
        self.handle.stop(tween(0.02));
    }
    fn duration(&self) -> Option<f64> {
        Some(self.duration)
    }
}

// ---------------------------------------------------------------------------------------- offline

/// A kira backend with no device: the samples are pulled by hand through [`OfflineOutput`].
pub struct OfflineBackend {
    slot: Arc<Mutex<Option<Renderer>>>,
}

/// The settings of an [`OfflineBackend`]: its sample rate and where its renderer goes.
pub struct OfflineSettings {
    pub sample_rate: u32,
    pub slot: Arc<Mutex<Option<Renderer>>>,
}

impl Backend for OfflineBackend {
    type Settings = OfflineSettings;
    type Error = ();

    fn setup(settings: OfflineSettings, _internal_buffer_size: usize) -> Result<(Self, u32), ()> {
        Ok((OfflineBackend { slot: settings.slot }, settings.sample_rate))
    }

    fn start(&mut self, renderer: Renderer) -> Result<(), ()> {
        *self.slot.lock().map_err(|_| ())? = Some(renderer);
        Ok(())
    }
}

/// The mix of an engine made with `SoundEngine::offline`, rendered on demand.
pub struct OfflineOutput {
    pub(crate) slot: Arc<Mutex<Option<Renderer>>>,
    pub sample_rate: u32,
}

impl OfflineOutput {
    /// The next `seconds` of the mix: interleaved stereo samples, -1 to 1.
    pub fn render(&self, seconds: f64) -> Vec<f32> {
        let frames = (seconds * self.sample_rate as f64).round() as usize;
        let mut out = vec![0.0; frames * 2];
        if let Ok(mut slot) = self.slot.lock() {
            if let Some(renderer) = slot.as_mut() {
                for chunk in out.chunks_mut(512 * 2) {
                    renderer.on_start_processing();
                    renderer.process(chunk, 2);
                }
            }
        }
        out
    }
}

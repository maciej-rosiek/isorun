# isorun-audio

The game's sound, the port of `apps/isorun/src/audio`: the effects, the ambient beds and the
music, and the cues that turn a game into sounds.

- `catalog.rs` (`catalog.ts`): `SoundName` (every effect with its `SoundSpec`: files, gain, rate,
  pitch and volume spread, voices, cooldown), `AmbientName` (the dungeon and cavern beds, the
  water and lava loops), `Mood` and `music(mood)`, and `all_files()`.
- `cues.rs` (`cues.ts`), pure: `sound_for_line` (gold, a pickup, a level, the player's death),
  `use_kind_of_key` and `sound_for_use` (a potion, a scroll, a spell by its school, never a
  refusal), `cues_for_combat` (a swing, a hit, a hurt, a heavy blow and a death, each heard from
  where it happened), `hearing` (fainter with the distance in cells, panned by the side of the
  screen), `mood_for`, `ambient_levels`, `heartbeat_delay`. `CombatEvent` and `Fighter` mirror
  `combat.ts`; whoever ports `combatEvents` maps theirs onto these (or uses them).
- `engine.rs` (`engine.ts`): `SoundEngine`, one per app, a cheap clonable handle. Effects are
  decoded once (on a thread natively, fetched on the web) and played with their spread, under
  the limits per sound (voices, cooldown) and in total (24); a sound not decoded yet is dropped.
  The beds and the music are streamed, looped beds fade to their levels, a track fades in and
  out; the buses (effects, ambient, music) sit under the master gain, which follow
  `AudioSettings` (0 to 100 and a mute). `set_hidden(true)` sleeps: nothing new plays, the
  streams pause and the audio clock (cooldowns, fades) stands.
- `director.rs` (`director.ts`): `GameAudio`, one per game screen, fed by the game.
- `mixer.rs`: where the sound goes. kira on the default device; kira rendered by hand
  (`SoundEngine::offline`, `OfflineOutput::render`); or nowhere (`NullMixer`) when there is no
  device: the logic, the timing and the trace run the same.
- `embedded.rs` (not on the web): every `.ogg` of `apps/isorun/public/audio`, in the binary.

## The stack

[kira](https://crates.io/crates/kira) 0.12 with its cpal backend and symphonia's Ogg Vorbis
decoder: cpal plays on ALSA (Linux), WASAPI (Windows), AAudio (Android) and WebAudio (wasm), with
a small buffer (kira mixes 128 frames at a time). kira does what the engine needs: a decoded
sound played many times at once, a playback rate, a pan and a volume per sound, a start delay,
looping, sub-tracks for the buses, and streaming from memory with a decoder thread. rodio would
need the voices, buses and tweens built by hand.

The device needs ALSA's development files on Linux (`libasound2-dev`, `alsa-lib` in nixpkgs)
to build. The `device` feature (on by default) can be turned off for a build with no output.

kira cannot stream on the web (no threads), so there the beds and the music play from `<audio>`
elements (`web.rs`), as the TypeScript does, with the same fades; the effects and the short
loops go through kira.

## Where the files are

Natively the files are embedded (`include_bytes!`, about 10 MB): the desktop app is one file
and the Android APK needs no asset reader (an `.so` cannot open the APK's assets with a path).
On the web they are fetched from the page's `audio/` folder (`SoundEngine::set_base`), as the
TypeScript does: 10 MB more of wasm would hold back the start of the page, and fetched files
are cached by the browser; the music is only read when it plays.

## Use

    let audio = SoundEngine::new();             // one per app
    audio.apply_settings(&AudioSettings { master_volume: 70.0, .. });
    audio.unlock();                             // natively at once; on the web on the first key or click
    audio.play(SoundName::UiClick, PlayOptions::default());   // a button; UiOpen / UiClose for menus

    let mut game = GameAudio::new(audio.clone()); // one per game screen; dropping it fades out
    game.sent_input("q", command_mode);         // each `input` message sent; sent_key(27) for Escape
    game.swing();                               // the player's own melee key, before the answer
    game.on_state(&state, &scene, &fresh_lines);// each state: lines, --more--, heartbeat, level, water
    game.combat(&event, &scene, &view, Some(token)); // a fight event; the token given to IsoRenderer::act
    let ev = renderer.take_events();
    game.on_frame(AudioFrame { shown: &ev.shown, hops: &ev.hops, doors: &ev.doors }, &scene, &view);
    game.set_hidden(window_hidden);

`view` is an `AudioView` (where a cell is on the view, its width, the Animation speed);
`RendererView { renderer, width, animation_speed }` makes one of the `IsoRenderer`.

The trace (`?audio-trace`): `audio.enable_trace()`, then `audio.trace()` lists the sounds played,
an effect by its name (`hit-heavy`) and a track as `music:<name>`.

## Tests

    cargo test -p isorun-audio

`tests/cues.rs` is `apps/isorun/test/audio-cues.test.ts` and more of the cues' rules;
`tests/director.rs` runs the engine's limits and `GameAudio`'s timing (the use of an item, the
blows shown, footsteps, doors, stairs, music, heartbeat, the beds) with no device and a clock
moved by hand; `tests/files.rs` decodes every file of the catalog and renders a mix with no
device (it is heard, and panned). `ISORUN_AUDIO_WAV=/tmp/mix.wav cargo test -p isorun-audio
--test files` writes that mix to listen to.

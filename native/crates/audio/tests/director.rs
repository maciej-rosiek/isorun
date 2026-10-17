//! The engine's limits and the director's timing, on an engine with no device and a clock moved
//! by hand. The trace is what the TypeScript's `?audio-trace` reads.

use std::rc::Rc;
use std::time::Duration;

use isorun_audio::{AmbientName, AudioFrame, AudioView, CombatEvent, Fighter, GameAudio, ManualClock, PlayOptions, SoundEngine, SoundName};
use isorun_render::{ActorStep, PLAYER_ACTOR};
use isorun_scene::{cell_key, Billboard, BillboardKind, CellKind, Scene, SceneCell};
use isorun_webtiles::{GameMessage, GameState, Msgch};

/// A view 800 pixels wide, a cell 40 pixels across from the left edge.
struct View;

impl AudioView for View {
    fn screen_of(&self, x: i32, _y: i32) -> (f32, f32) {
        (x as f32 * 40.0, 300.0)
    }
    fn width(&self) -> f32 {
        800.0
    }
    fn animation_speed(&self) -> f64 {
        1.0
    }
}

fn engine() -> (SoundEngine, ManualClock) {
    let clock = ManualClock::new();
    clock.set(100.0);
    let engine = SoundEngine::null(Rc::new(clock.clone()), 7);
    assert!(engine.wait_loaded(Duration::from_secs(60)), "the sounds decode");
    engine.enable_trace();
    (engine, clock)
}

fn line(text: &str) -> GameMessage {
    GameMessage { text: text.into(), turn: Some(1), channel: Some(0), marker: None }
}

fn scene() -> Scene {
    let mut s = Scene { player: (10, 10), player_on_level: true, ..Default::default() };
    let mut goblin = Billboard::new(12, 10, 0, BillboardKind::Monster, 1.0);
    goblin.ref_id = Some(4);
    s.billboards.push(goblin);
    s
}

fn state(place: &str, depth: i64, hp: i64, hp_max: i64) -> GameState {
    let mut st = GameState::default();
    st.player.place = place.into();
    st.player.depth = depth;
    st.player.hp = hp;
    st.player.hp_max = hp_max;
    st
}

#[test]
fn an_effect_waits_its_cooldown_and_its_voices() {
    // given
    let (engine, clock) = engine();

    // when: three steps at once, then one after the cooldown
    for _ in 0..3 {
        engine.play(SoundName::Step, PlayOptions::default());
    }
    clock.advance(0.08);
    engine.play(SoundName::Step, PlayOptions::default());

    // then: the cooldown drops the second and the third
    assert_eq!(engine.trace(), vec!["step", "step"]);
}

#[test]
fn no_more_voices_of_a_sound_than_its_spec_allows() {
    // given: the hit has four voices and a 30 ms cooldown
    let (engine, clock) = engine();

    // when
    for _ in 0..6 {
        engine.play(SoundName::Hit, PlayOptions::default());
        clock.advance(0.031);
    }

    // then
    assert_eq!(engine.trace().len(), 4);
}

#[test]
fn nothing_plays_while_hidden_and_the_audio_clock_stands() {
    // given: the gold has two voices and a 100 ms cooldown
    let (engine, clock) = engine();
    engine.play(SoundName::Gold, PlayOptions::default());

    // when: hidden for a long while, then shown
    engine.set_hidden(true);
    engine.play(SoundName::Stairs, PlayOptions::default());
    clock.advance(10.0);
    engine.set_hidden(false);
    engine.play(SoundName::Gold, PlayOptions::default());
    clock.advance(0.15);
    engine.play(SoundName::Gold, PlayOptions::default());

    // then: the cooldown counted only the time shown
    assert_eq!(engine.trace(), vec!["gold", "gold"]);
}

#[test]
fn the_lines_sound_once_each_and_a_prompt_does_not() {
    // given
    let (engine, _clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let mut prompt = line("f - a +0 dagger.");
    prompt.channel = Some(Msgch::PROMPT);

    // when
    audio.messages(&[line("You now have 59 gold pieces."), line("You now have 60 gold pieces."), prompt]);

    // then
    assert_eq!(engine.trace(), vec!["gold"]);
}

#[test]
fn a_potion_sounds_when_its_answer_comes_and_not_after_escape() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());

    // when
    audio.sent_input("q", true);
    audio.messages(&[line("You feel better.")]);
    clock.advance(1.0);
    audio.sent_input("q", true);
    audio.sent_key(27);
    audio.messages(&[line("You feel better.")]);
    clock.advance(1.0);
    audio.sent_input("q", false);
    audio.messages(&[line("You feel better.")]);

    // then
    assert_eq!(engine.trace(), vec!["potion"]);
}

#[test]
fn a_blow_sounds_when_the_renderer_shows_it_heard_from_its_side() {
    // given
    let (engine, _clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();

    // when
    audio.combat(&CombatEvent::Hit { on: Fighter::Monster(4), from: Some((10, 10)) }, &scene, &View, Some(77));
    let before = engine.trace();
    audio.on_frame(AudioFrame { shown: &[77], ..Default::default() }, &scene, &View);

    // then
    assert!(before.is_empty());
    assert_eq!(engine.trace(), vec!["hit"]);
}

#[test]
fn a_kill_plays_the_heavy_blow_and_the_death() {
    // given
    let (engine, _clock) = engine();
    let mut audio = GameAudio::new(engine.clone());

    // when
    audio.combat(&CombatEvent::Death { on: Fighter::Monster(4), from: None }, &scene(), &View, None);

    // then
    assert_eq!(engine.trace(), vec!["hit-heavy", "death"]);
}

#[test]
fn far_footsteps_are_not_made_and_the_player_s_are() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();
    let far = ActorStep { id: 4, from: (19, 10), to: (20, 10), seconds: 0.2 };
    let near = ActorStep { id: 4, from: (11, 10), to: (12, 10), seconds: 0.2 };
    let mine = ActorStep { id: PLAYER_ACTOR, from: (9, 10), to: (10, 10), seconds: 0.2 };

    // when
    audio.on_frame(AudioFrame { hops: &[far], ..Default::default() }, &scene, &View);
    audio.on_frame(AudioFrame { hops: &[near], ..Default::default() }, &scene, &View);
    clock.advance(0.1);
    audio.on_frame(AudioFrame { hops: &[mine], ..Default::default() }, &scene, &View);

    // then
    assert_eq!(engine.trace(), vec!["step", "step"]);
}

#[test]
fn a_new_level_rings_the_stairs_and_settles_its_doors() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();
    audio.on_state(&state("Dungeon", 1, 20, 20), &scene, &[]);

    // when: the doors of the first level, then a door later, then the stairs down
    audio.on_frame(AudioFrame { doors: &[(11, 10, true)], ..Default::default() }, &scene, &View);
    clock.advance(1.0);
    audio.on_frame(AudioFrame { doors: &[(11, 10, true)], ..Default::default() }, &scene, &View);
    audio.on_state(&state("Dungeon", 2, 20, 20), &scene, &[]);
    audio.on_frame(AudioFrame { doors: &[(11, 10, false)], ..Default::default() }, &scene, &View);

    // then
    assert_eq!(engine.trace(), vec!["door-open", "stairs"]);
}

#[test]
fn the_music_starts_after_the_first_level_and_follows_the_mood() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();

    // when
    audio.on_state(&state("Dungeon", 1, 20, 20), &scene, &[]);
    let at_once = engine.trace();
    clock.advance(2.6);
    audio.on_frame(AudioFrame::default(), &scene, &View);
    let calm = engine.current_track();
    audio.on_state(&state("Crypt", 1, 20, 20), &scene, &[]);
    let dark = engine.current_track();

    // then
    assert!(at_once.is_empty());
    assert!(["ruined-temple", "persistence", "cave", "abandoned-castle"].contains(&calm.unwrap()));
    assert!(["evil-temple", "eternal-sleep"].contains(&dark.unwrap()));
    assert_eq!(engine.trace().iter().filter(|t| t.starts_with("music:")).count(), 2);
}

#[test]
fn a_track_that_ends_is_followed_by_another_after_a_pause() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();
    audio.on_state(&state("Crypt", 1, 20, 20), &scene, &[]);
    clock.advance(2.6);
    audio.on_frame(AudioFrame::default(), &scene, &View);
    let first = engine.current_track().expect("a track plays");

    // when: an hour of frames
    let mut tracks = vec![first];
    for _ in 0..3600 {
        clock.advance(1.0);
        audio.on_frame(AudioFrame::default(), &scene, &View);
        if let Some(t) = engine.current_track() {
            if tracks.last() != Some(&t) {
                tracks.push(t);
            }
        }
    }

    // then: the dark tracks take turns
    assert!(tracks.len() > 3, "{tracks:?}");
    assert!(tracks.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn the_heart_beats_while_the_hit_points_are_low_and_not_for_a_watcher() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let scene = scene();

    // when: three seconds at a fifth of the hit points, then healed
    audio.on_state(&state("Dungeon", 1, 4, 20), &scene, &[]);
    for _ in 0..30 {
        clock.advance(0.1);
        audio.on_frame(AudioFrame::default(), &scene, &View);
    }
    let beats = engine.trace().iter().filter(|t| *t == "heartbeat").count();
    audio.on_state(&state("Dungeon", 1, 20, 20), &scene, &[]);
    for _ in 0..30 {
        clock.advance(0.1);
        audio.on_frame(AudioFrame::default(), &scene, &View);
    }
    let mut watched = state("Dungeon", 1, 2, 20);
    watched.watching = Some("someone".into());
    audio.on_state(&watched, &scene, &[]);
    for _ in 0..30 {
        clock.advance(0.1);
        audio.on_frame(AudioFrame::default(), &scene, &View);
    }

    // then: a beat every 1.01 s (0.5 + 0.2 / 0.35 * 0.9), and none after
    assert_eq!(beats, 3);
    assert_eq!(engine.trace().iter().filter(|t| *t == "heartbeat").count(), 3);
}

#[test]
fn the_more_prompt_sounds_once_when_it_comes() {
    // given
    let (engine, clock) = engine();
    let mut audio = GameAudio::new(engine.clone());

    // when
    audio.status(10, 10, true, false);
    clock.advance(1.0);
    audio.status(10, 10, true, false);

    // then
    assert_eq!(engine.trace(), vec!["more"]);
}

#[test]
fn water_near_the_player_adds_its_loop_and_a_cave_its_bed() {
    // given
    let (engine, _clock) = engine();
    let mut audio = GameAudio::new(engine.clone());
    let mut scene = scene();
    for x in 8..13 {
        let mut c = SceneCell { x, y: 12, kind: CellKind::Floor, ..Default::default() };
        c.flags.water = true;
        scene.cells.insert(cell_key(x, 12), c);
    }

    // when
    audio.on_state(&state("Lair", 1, 20, 20), &scene, &[]);

    // then
    assert!((engine.ambient_target(AmbientName::Water) - 0.5 * 0.9).abs() < 1e-6);
    assert!((engine.ambient_target(AmbientName::Cavern) - 0.8).abs() < 1e-6);
    assert_eq!(engine.ambient_target(AmbientName::Dungeon), 0.0);
}

#[test]
fn a_game_screen_that_goes_fades_the_music_out() {
    // given
    let (engine, clock) = engine();
    {
        let mut audio = GameAudio::new(engine.clone());
        audio.on_state(&state("Dungeon", 1, 20, 20), &scene(), &[]);
        clock.advance(3.0);
        audio.on_frame(AudioFrame::default(), &scene(), &View);
        assert!(engine.current_track().is_some());
    }

    // then
    assert_eq!(engine.current_track(), None);
    assert_eq!(engine.ambient_target(AmbientName::Dungeon), 0.0);
}

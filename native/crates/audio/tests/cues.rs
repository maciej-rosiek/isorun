//! The port of `apps/isorun/test/audio-cues.test.ts`, and the rest of the cues' rules.

use isorun_audio::cues::{ambient_levels, cues_for_combat, heartbeat_delay, hearing, mood_for, sound_for_line, sound_for_use, use_kind_of_key, CombatEvent, Fighter, UseKind};
use isorun_audio::{AmbientLevels, Mood, SoundName};

#[test]
fn names_the_sound_of_a_message_line() {
    // given
    let lines = ["You now have 59 gold pieces.", "f - a +0 dagger.", "Welcome to level 5.", "You die...", "You see here a rat."];

    // when
    let sounds: Vec<Option<SoundName>> = lines.iter().map(|l| sound_for_line(l)).collect();

    // then
    assert_eq!(sounds, vec![Some(SoundName::Gold), Some(SoundName::Pickup), Some(SoundName::Levelup), Some(SoundName::Gameover), None]);
}

#[test]
fn sounds_a_use_of_an_item_by_the_key_that_started_it_and_not_a_refusal() {
    // given
    let key = use_kind_of_key("q");

    // when
    let drunk = key.and_then(|k| sound_for_use(k, &["You feel better."]));
    let refused = key.and_then(|k| sound_for_use(k, &["You have no potions to drink."]));
    let fire = sound_for_use(UseKind::Cast, &["The fireball explodes."]);

    // then
    assert_eq!(drunk, Some(SoundName::Potion));
    assert_eq!(refused, None);
    assert_eq!(fire, Some(SoundName::CastFire));
}

#[test]
fn plays_a_swing_for_an_attack_and_a_hit_and_a_death_for_a_kill() {
    // given
    let monster = (6, 5);

    // when
    let swing = cues_for_combat(&CombatEvent::Attack { by: Fighter::Player, at: monster }, |_| Some(monster));
    let kill = cues_for_combat(&CombatEvent::Death { on: Fighter::Monster(7), from: Some((5, 5)) }, |_| Some(monster));

    // then
    assert_eq!(swing.len(), 1);
    assert_eq!(swing[0].sound, SoundName::Swing);
    assert_eq!(swing[0].at, None);
    assert_eq!(kill.iter().map(|c| c.sound).collect::<Vec<_>>(), vec![SoundName::HitHeavy, SoundName::Death]);
    assert!(kill.iter().all(|c| c.at == Some(monster)));
}

#[test]
fn pans_a_sound_by_the_screen_and_fades_it_with_the_distance() {
    // given
    let player = (10, 10);

    // when
    let near = hearing(player, (11, 10), 1.0);
    let far = hearing(player, (40, 10), 0.0);

    // then
    assert!(near.unwrap().pan > 0.0);
    assert!(near.unwrap().gain > 0.9);
    assert_eq!(far, None);
}

#[test]
fn picks_the_music_the_bed_and_the_heartbeat_by_place_and_health() {
    // given
    let place = "Crypt";

    // when
    let mood = mood_for(place, 2);
    let beds = ambient_levels("Lair", 20, 0);

    // then
    assert_eq!(mood, Mood::Dark);
    assert_eq!(mood_for("Dungeon", 3), Mood::Calm);
    assert_eq!(beds, AmbientLevels { dungeon: 0.0, cavern: 1.0, water: 1.0, lava: 0.0 });
    assert!(heartbeat_delay(0.2).unwrap() < heartbeat_delay(0.3).unwrap_or(0.0));
    assert_eq!(heartbeat_delay(0.9), None);
}

// ------------------------------------------------------------------ beyond the TypeScript's tests

#[test]
fn a_monster_s_blows_are_heard_from_where_it_stands() {
    // given
    let goblin = (3, 4);
    let cell_of = |who: Fighter| if who == Fighter::Monster(9) { Some(goblin) } else { Some((0, 0)) };

    // when
    let swing = cues_for_combat(&CombatEvent::Attack { by: Fighter::Monster(9), at: (0, 0) }, cell_of);
    let hurt = cues_for_combat(&CombatEvent::Hit { on: Fighter::Player, from: Some(goblin) }, cell_of);
    let struck = cues_for_combat(&CombatEvent::Hit { on: Fighter::Monster(9), from: None }, cell_of);
    let missed_me = cues_for_combat(&CombatEvent::Miss { on: Fighter::Player, from: Some(goblin) }, cell_of);
    let missed_it = cues_for_combat(&CombatEvent::Miss { on: Fighter::Monster(9), from: None }, cell_of);

    // then
    assert_eq!((swing[0].sound, swing[0].at, swing[0].gain), (SoundName::Swing, Some(goblin), 0.5));
    assert_eq!(hurt.iter().map(|c| (c.sound, c.at, c.gain)).collect::<Vec<_>>(), vec![(SoundName::Hurt, None, 1.0), (SoundName::Hit, None, 0.6)]);
    assert_eq!((struck[0].sound, struck[0].at), (SoundName::Hit, Some(goblin)));
    assert_eq!((missed_me[0].at, missed_me[0].gain), (None, 1.0));
    assert_eq!((missed_it[0].at, missed_it[0].gain), (Some(goblin), 0.7));
}

#[test]
fn a_death_rings_after_the_heavy_blow() {
    // when
    let kill = cues_for_combat(&CombatEvent::Death { on: Fighter::Monster(1), from: None }, |_| Some((2, 2)));

    // then
    assert_eq!(kill[0].after, 0.0);
    assert!((kill[1].after - 0.12).abs() < 1e-9);
}

#[test]
fn a_spell_sounds_by_the_school_its_lines_name() {
    assert_eq!(sound_for_use(UseKind::Cast, &["You cast Freeze.", "The rat is frozen."]), Some(SoundName::CastIce));
    assert_eq!(sound_for_use(UseKind::Cast, &["Pain shoots through the rat."]), Some(SoundName::CastDark));
    assert_eq!(sound_for_use(UseKind::Cast, &["You blink."]), Some(SoundName::Cast));
    assert_eq!(sound_for_use(UseKind::Scroll, &["As you read the scroll, it crumbles to dust."]), Some(SoundName::Scroll));
    assert_eq!(sound_for_use(UseKind::Cast, &["You cannot cast spells when silenced!"]), None);
    assert_eq!(sound_for_use(UseKind::Potion, &["Okay, then."]), None);
    assert_eq!(sound_for_use::<&str>(UseKind::Potion, &[]), None);
}

#[test]
fn only_the_use_keys_start_a_use() {
    assert_eq!(use_kind_of_key("r"), Some(UseKind::Scroll));
    assert_eq!(use_kind_of_key("Z"), Some(UseKind::Cast));
    assert_eq!(use_kind_of_key("v"), Some(UseKind::Cast));
    assert_eq!(use_kind_of_key("o"), None);
}

#[test]
fn more_lines_name_their_sounds() {
    assert_eq!(sound_for_line("You pick up 12 gold pieces."), Some(SoundName::Gold));
    assert_eq!(sound_for_line("You have reached level 3!"), Some(SoundName::Levelup));
    assert_eq!(sound_for_line("You feel more experienced."), Some(SoundName::Levelup));
    assert_eq!(sound_for_line("you die..."), Some(SoundName::Gameover));
    assert_eq!(sound_for_line("You died in the dungeon."), None);
}

#[test]
fn hearing_fades_to_the_faintest_at_its_edge_and_pans_no_further_than_the_widest() {
    // when
    let edge = hearing((0, 0), (12, -3), 4.0).unwrap();
    let same = hearing((0, 0), (0, 0), -1.0).unwrap();

    // then
    assert!((edge.gain - 0.15).abs() < 1e-6);
    assert!((edge.pan - 0.65).abs() < 1e-6);
    assert!((same.pan + 0.65).abs() < 1e-6);
    assert_eq!(same.gain, 1.0);
    assert_eq!(hearing((0, 0), (13, 0), 0.0), None);
}

#[test]
fn the_deep_dungeon_and_the_dark_branches_are_dark() {
    assert_eq!(mood_for("D", 12), Mood::Dark);
    assert_eq!(mood_for("Dungeon", 11), Mood::Calm);
    assert_eq!(mood_for("Lair", 14), Mood::Calm);
    assert_eq!(mood_for("Zot", 1), Mood::Dark);
    assert_eq!(ambient_levels("Dungeon", 5, 30), AmbientLevels { dungeon: 1.0, cavern: 0.0, water: 0.5, lava: 1.0 });
}

#[test]
fn the_heart_beats_faster_the_lower_the_hit_points() {
    assert_eq!(heartbeat_delay(0.0), None);
    assert_eq!(heartbeat_delay(0.35), None);
    assert!((heartbeat_delay(0.175).unwrap() - 0.95).abs() < 1e-9);
}

#[test]
fn every_sound_has_files_and_a_name_that_reads_back() {
    for &name in SoundName::ALL {
        assert!(!name.spec().files.is_empty(), "{name}");
        assert_eq!(SoundName::from_name(name.as_str()), Some(name));
    }
    assert_eq!(SoundName::Death.spec().rate, 0.75);
    assert_eq!(SoundName::Step.spec().cooldown, 0.07);
    assert_eq!(SoundName::Heartbeat.spec().gain_spread, 0.0);
}

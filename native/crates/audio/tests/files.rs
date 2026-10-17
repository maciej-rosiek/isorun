//! Every file the catalog names is in the binary and decodes; and a mix rendered with no device
//! is not silent (written to `$ISORUN_AUDIO_WAV` when set, to listen to).

use std::rc::Rc;
use std::time::Duration;

use isorun_audio::{all_files, AmbientLevels, AudioSettings, ManualClock, Mood, PlayOptions, SoundEngine, SoundName};
use kira::sound::static_sound::StaticSoundData;

#[test]
fn every_file_of_the_catalog_decodes() {
    let files = all_files();
    assert_eq!(files.len(), 45 + 4 + 6);
    for path in files {
        let bytes = isorun_audio::embedded::bytes(&path).unwrap_or_else(|| panic!("{path} is embedded"));
        let data = StaticSoundData::from_cursor(std::io::Cursor::new(bytes)).unwrap_or_else(|e| panic!("{path} decodes: {e}"));
        let seconds = data.duration().as_secs_f64();
        assert!(seconds > 0.005, "{path} lasts {seconds} s");
        let loud = (0..data.num_frames()).step_by(97).filter_map(|i| data.frame_at_index(i)).any(|f| f.left.abs() > 0.01 || f.right.abs() > 0.01);
        assert!(loud, "{path} is not silent");
    }
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

#[test]
fn a_mix_rendered_with_no_device_is_heard() {
    // given
    let clock = ManualClock::new();
    let (engine, output) = SoundEngine::offline(Rc::new(clock.clone()), 3, 44_100);
    assert!(engine.wait_loaded(Duration::from_secs(60)));
    engine.enable_trace();
    engine.apply_settings(&AudioSettings { master_volume: 100.0, ..Default::default() });
    let mut mix: Vec<f32> = Vec::new();
    let run = |seconds: f64, mix: &mut Vec<f32>| {
        // a frame at a time, as the game would: the engine's clock and the mix move together
        let frames = (seconds * 60.0).round() as usize;
        for _ in 0..frames {
            clock.advance(1.0 / 60.0);
            engine.update();
            mix.extend(output.render(1.0 / 60.0));
            // the music is read as it plays, by kira's decoder thread: let it keep up
            std::thread::sleep(Duration::from_micros(300));
        }
    };

    // when: a quiet start, a fight panned left and right, the water's loop, the music
    run(0.25, &mut mix);
    let silence = rms(&mix);
    engine.play(SoundName::Swing, PlayOptions { pan: -0.65, ..Default::default() });
    engine.play(SoundName::Hit, PlayOptions { pan: 0.65, delay: 0.15, ..Default::default() });
    engine.play(SoundName::Death, PlayOptions { delay: 0.3, ..Default::default() });
    let fight_from = mix.len();
    run(1.0, &mut mix);
    let fight = rms(&mix[fight_from..]);
    engine.set_ambient(AmbientLevels { water: 1.0, ..Default::default() });
    engine.play_mood(Mood::Calm);
    let loops_from = mix.len() + (2.0 * 44_100.0 * 2.0) as usize;
    run(4.0, &mut mix);
    let loops = rms(&mix[loops_from..]);

    if let Ok(path) = std::env::var("ISORUN_AUDIO_WAV") {
        let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).expect("the wav is written");
        for s in &mix {
            w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).expect("a sample");
        }
        w.finalize().expect("the wav is closed");
    }

    // then
    assert_eq!(silence, 0.0);
    assert!(fight > 0.01, "the fight is heard: {fight}");
    assert!(loops > 0.01, "the water and the music are heard: {loops}");
    assert_eq!(engine.trace()[..3], ["swing", "hit", "death"]);
    assert!(engine.trace()[3].starts_with("music:"));
}

#[test]
fn the_left_ear_hears_a_sound_panned_left() {
    // given
    let clock = ManualClock::new();
    let (engine, output) = SoundEngine::offline(Rc::new(clock.clone()), 5, 44_100);
    assert!(engine.wait_loaded(Duration::from_secs(60)));
    engine.apply_settings(&AudioSettings { master_volume: 100.0, ..Default::default() });
    output.render(0.2);

    // when
    engine.play(SoundName::Hit, PlayOptions { pan: -1.0, ..Default::default() });
    let mix = output.render(0.3);

    // then
    let left: f32 = mix.iter().step_by(2).map(|s| s.abs()).sum();
    let right: f32 = mix.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    assert!(left > 0.0);
    assert!(right < left * 0.05, "left {left}, right {right}");
}

use std::sync::Arc;

use orb_engine::frame::{Devices, Engine};
use orb_engine::studio::{Settings, Studio, convolution::Convolver};
use orb_engine::tables::Tables;

fn render(row: usize, voice: usize) -> Vec<i16> {
    render_mix(row, voice, None)
}

fn render_mix(row: usize, voice: usize, settings: Option<Settings>) -> Vec<i16> {
    let mut engine = Engine::new(
        Arc::new(Tables::new()),
        Devices::implemented(Some(rbm::standard::builtin_samples())),
    );
    engine.mixer_channel(row as u8, 1, 100);
    if let Some(settings) = settings {
        let mut studio = Studio::new(Box::new(Convolver::new(&[[0.0; 2]]).unwrap()));
        studio.settings = Box::new(settings);
        engine.frame.studio = Some(Box::new(studio));
    }
    let pattern = engine.frame.devices.channels[row].patterns_mut().unwrap();
    for step in [0, 4, 8, 12] {
        pattern[2 + step * 12 + voice] = if row == 3 { 2 } else { 1 };
    }
    engine.play();
    let mut audio = vec![0; 2 * 44100 * 4];
    for block in audio.chunks_mut(2048) {
        engine.render(block, block.len() / 2);
    }
    audio
}

#[test]
fn calibrated_808_closes_the_kick_gap_without_boosting_the_909() {
    let unity = Settings {
        drum_gain_db: [0.0; 2],
        ..Settings::default()
    };
    assert_eq!(render_mix(2, 1, Some(unity.clone())), render(2, 1));
    assert_eq!(render_mix(3, 1, Some(Settings::default())), render(3, 1));
    let rms = |audio: &[i16]| {
        (audio.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / audio.len() as f64).sqrt()
    };
    let base = render(2, 1);
    let calibrated = render_mix(2, 1, Some(Settings::default()));
    let reference = render(3, 1);
    let improvement = 20.0 * (rms(&calibrated) / rms(&base)).log10();
    assert!((improvement - 6.0).abs() < 0.1);
    assert!((20.0 * (rms(&reference) / rms(&calibrated)).log10()).abs() < 1.5);
    assert!(calibrated.iter().all(|v| v.unsigned_abs() < 32767));
}

#[test]
fn old_projects_keep_unity_and_trim_is_bounded_smoothed_and_finite() {
    let mut json = serde_json::to_value(Settings::default()).unwrap();
    json.as_object_mut().unwrap().remove("drum_gain_db");
    let old: Settings = serde_json::from_value(json).unwrap();
    assert_eq!(old.drum_gain_db, [0.0; 2]);
    let mut studio = Studio::new(Box::new(Convolver::new(&[[0.0; 2]]).unwrap()));
    let mut previous = 1.0_f32;
    for _ in 0..200 {
        let mut block = [1.0_f32; 64];
        studio.trim_drum(2, &mut block);
        for sample in block {
            assert!((sample - previous).abs() < 0.005);
            previous = sample;
        }
    }
    assert!((previous - 10.0_f32.powf(6.0 / 20.0)).abs() < 0.001);
    studio.settings.drum_gain_db = [f64::NAN, 1000.0];
    studio.settings.sanitize();
    assert_eq!(studio.settings.drum_gain_db, [0.0, 12.0]);
}

#[test]
fn standard_kit_drum_levels() {
    for (a, b, name) in [(1, 1, "kick"), (2, 2, "snare"), (11, 8, "closed hat")] {
        let stats = |row, voice| {
            let audio = render(row, voice);
            let power =
                audio.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / audio.len() as f64;
            let peak = audio.iter().map(|v| v.unsigned_abs()).max().unwrap();
            assert!(power > 0.0);
            (power.sqrt(), peak)
        };
        let (rms808, peak808) = stats(2, a);
        let (rms909, peak909) = stats(3, b);
        println!(
            "{name}: 808 RMS {rms808:.2}, peak {peak808}; \
            909 RMS {rms909:.2}, peak {peak909}; difference {:.2} dB",
            20.0 * (rms909 / rms808).log10()
        );
    }
}

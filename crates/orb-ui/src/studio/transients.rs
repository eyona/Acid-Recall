//! Optional onset proposals, analyzed off the audio/UI threads and explicitly accepted by users.

use super::{Completed, StudioUi};
use crate::app::App;
use orb_engine::studio::{
    audio_file::Wave,
    sampler::{Lane, MAX_WARP_MARKERS},
};

#[derive(Clone)]
pub(super) struct Proposal {
    epoch: u64,
    start: f64,
    end: f64,
    pub sources: Vec<f64>,
}

impl Proposal {
    pub fn matches(&self, studio: &StudioUi, lane: usize) -> bool {
        self.epoch == studio.waveform_epoch
            && studio.waves[lane].is_some()
            && self.start == studio.settings.lanes[lane].start
            && self.end == studio.settings.lanes[lane].end
    }
}

fn detect(wave: &Wave, start: f64, end: f64) -> Vec<f64> {
    const WINDOW: usize = 256;
    let frames = wave.frames.len();
    let start = ((start * frames as f64) as usize).min(frames);
    let end = ((end * frames as f64) as usize).min(frames).max(start);
    let region = &wave.frames[start..end];
    let energy: Vec<f64> = region
        .chunks(WINDOW)
        .map(|chunk| {
            chunk
                .iter()
                .flatten()
                .map(|&x| f64::from(x).powi(2))
                .sum::<f64>()
                / (chunk.len() * 2) as f64
        })
        .collect();
    let maximum = energy.iter().copied().fold(0.0_f64, f64::max);
    if maximum < 1e-12 {
        return Vec::new();
    }
    let mut baseline = 0.0;
    let novelty: Vec<f64> = energy
        .iter()
        .map(|&e| {
            let score = (e - baseline).max(0.0);
            baseline += (e - baseline) * 0.2;
            score
        })
        .collect();
    let threshold = novelty.iter().copied().fold(0.0_f64, f64::max) * 0.2;
    let mut candidates: Vec<(usize, f64)> = (1..novelty.len().saturating_sub(1))
        .filter(|&i| {
            novelty[i] > threshold
                && energy[i] > maximum * 0.005
                && novelty[i] >= novelty[i - 1]
                && novelty[i] > novelty[i + 1]
        })
        .map(|i| {
            let a = i * WINDOW;
            let b = (a + WINDOW).min(region.len());
            let peak = region[a..b]
                .iter()
                .flatten()
                .fold(0.0_f32, |m, v| m.max(v.abs()));
            let offset = region[a..b]
                .iter()
                .position(|x| x[0].abs().max(x[1].abs()) >= peak * 0.25)
                .unwrap_or(0);
            (start + a + offset, novelty[i])
        })
        .filter(|(position, _)| *position > start + 661 && *position + 661 < end)
        .collect();
    candidates.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut chosen: Vec<usize> = Vec::new();
    for (position, _) in candidates {
        if chosen.iter().all(|&other| position.abs_diff(other) >= 2205) {
            chosen.push(position);
            if chosen.len() == MAX_WARP_MARKERS {
                break;
            }
        }
    }
    chosen.sort_unstable();
    chosen
        .into_iter()
        .map(|position| position as f64 / frames.max(1) as f64)
        .collect()
}

pub(super) fn apply(lane: &mut Lane, sources: &[f64]) -> usize {
    let mut count = 0;
    for &source in sources {
        let target = lane
            .warp_markers
            .target_for_source(source, lane.start, lane.end);
        if lane
            .warp_markers
            .insert(source, target, lane.start, lane.end)
            .is_some()
        {
            count += 1;
        }
    }
    if count > 0 {
        lane.warp = true;
        lane.warp_grid = true;
    }
    count
}

impl App {
    pub(super) fn sampler_detect(&mut self, ctx: &eframe::egui::Context, lane: usize) {
        if lane >= 8 || self.studio.busy() {
            return;
        }
        let Some(wave) = self.studio.waves[lane].clone() else {
            return;
        };
        let start = self.studio.settings.lanes[lane].start;
        let end = self.studio.settings.lanes[lane].end;
        let epoch = self.studio.waveform_epoch;
        self.studio.work(ctx, move || {
            Ok(Completed::Transients(
                lane,
                Proposal {
                    epoch,
                    start,
                    end,
                    sources: detect(&wave, start, end),
                },
            ))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onsets_are_bounded_ordered_and_detected_only_in_the_selected_region() {
        let mut wave = Wave {
            frames: vec![[0.0; 2]; 88200],
        };
        for n in [0, 22050, 44100, 66150] {
            for i in 0..400 {
                wave.frames[n + i] = [(1.0 - i as f32 / 400.0) * 0.8; 2];
            }
        }
        let points = detect(&wave, 0.2, 0.9);
        assert_eq!(points.len(), 3);
        for (point, expected) in points.iter().zip([0.25, 0.5, 0.75]) {
            assert!((*point - expected).abs() < 0.006);
        }
        assert!(
            detect(
                &Wave {
                    frames: vec![[0.0; 2]; 44100]
                },
                0.0,
                1.0
            )
            .is_empty()
        );
        let mut lane = Lane::default();
        lane.warp_markers.insert(0.4, 0.6, 0.0, 1.0).unwrap();
        let previous = lane.warp_markers.map(0.3, 0.0, 1.0).0;
        assert_eq!(apply(&mut lane, &points), 3);
        assert_eq!(lane.warp_markers.points().len(), 4);
        assert!((lane.warp_markers.map(0.3, 0.0, 1.0).0 - previous).abs() < 1e-12);
    }
}

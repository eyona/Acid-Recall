//! Studio drum extensions. Native first-page data stays in the original song format.

use std::sync::Arc;

use crate::app::App;

impl App {
    pub(crate) fn drum_length(&self, row: usize) -> usize {
        let pattern = self.panel.rows[row].pattern as usize;
        self.drum_pattern_length(row, pattern)
    }

    fn drum_pattern_length(&self, row: usize, pattern: usize) -> usize {
        let native = self.panel.rows[row].len[pattern].clamp(1, 16) as usize;
        self.studio.settings.drum_patterns[row - 2]
            .patterns
            .get(pattern)
            .map_or(native, |p| match p.length {
                32 => 32,
                64 => 64,
                _ => native,
            })
    }

    pub(crate) fn drum_set_length(&mut self, row: usize, length: usize) {
        let pattern = self.panel.rows[row].pattern as usize;
        let bank = Arc::make_mut(&mut self.studio.settings.drum_patterns[row - 2]);
        bank.sanitize();
        bank.patterns[pattern].length = match length {
            32 => 32,
            64 => 64,
            _ => 0,
        };
        if length <= 16 {
            self.panel
                .set_pattern_length(row, length.clamp(1, 16) as u8);
        }
        self.drum_changed();
    }

    pub(crate) fn drum_step(&self, row: usize, instrument: usize, step: usize) -> u8 {
        if step < 16 {
            if row == 2 {
                u8::from(self.panel.steps808[instrument][step])
            } else {
                self.panel.steps909[instrument][step]
            }
        } else {
            let pattern = self.panel.rows[row].pattern as usize;
            self.studio.settings.drum_patterns[row - 2]
                .patterns
                .get(pattern)
                .and_then(|p| p.steps.get(step - 16))
                .map_or(0, |s| s[instrument])
        }
    }

    pub(crate) fn drum_set_step(&mut self, row: usize, instrument: usize, step: usize, value: u8) {
        if step >= 64 || instrument >= 12 {
            return;
        }
        let value = if row == 2 {
            u8::from(value != 0)
        } else {
            value.min(4)
        };
        if step < 16 {
            let selected = if row == 2 {
                self.panel.inst808
            } else {
                self.panel.inst909
            };
            self.panel.select_instrument(row == 2, instrument);
            if row == 2 {
                self.panel.set_step808(step, value != 0);
            } else {
                self.panel.set_step909(step, value);
            }
            self.panel.select_instrument(row == 2, selected);
        } else {
            let pattern = self.panel.rows[row].pattern as usize;
            let bank = Arc::make_mut(&mut self.studio.settings.drum_patterns[row - 2]);
            bank.sanitize();
            bank.patterns[pattern].steps[step - 16][instrument] = value;
        }
        self.drum_changed();
    }

    fn drum_changed(&mut self) {
        self.studio.changed();
        self.doc.modified = true;
    }

    /// Includes inactive tail steps so reducing the length never destroys an arrangement.
    pub(crate) fn drum_pattern_bytes(&self, row: usize, pattern: u8) -> Vec<u8> {
        let mut bytes = vec![0; 2 + 64 * 12];
        let native = if self.panel.rows[row].pattern == pattern {
            self.panel.pattern_bytes(row)
        } else if let Some(link) = &self.link {
            let at = usize::from(pattern) * 194;
            link.patterns()[row]
                .get(at..at + 194)
                .unwrap_or(&[])
                .to_vec()
        } else {
            vec![0; 194]
        };
        if native.len() == 194 {
            bytes[..194].copy_from_slice(&native);
        }
        bytes[1] = self.drum_pattern_length(row, pattern as usize) as u8;
        if let Some(p) = self.studio.settings.drum_patterns[row - 2]
            .patterns
            .get(pattern as usize)
        {
            for (s, values) in p.steps.iter().take(48).enumerate() {
                bytes[194 + s * 12..206 + s * 12].copy_from_slice(values);
            }
        }
        bytes
    }

    pub(crate) fn drum_replace_pattern(&mut self, row: usize, pattern: u8, bytes: Vec<u8>) {
        if bytes.len() < 194 || pattern >= 32 {
            return;
        }
        let mut native = bytes[..194].to_vec();
        native[1] = bytes[1].clamp(1, 16);
        self.panel.replace_pattern(row, pattern, native);
        let bank = Arc::make_mut(&mut self.studio.settings.drum_patterns[row - 2]);
        bank.sanitize();
        let p = &mut bank.patterns[pattern as usize];
        p.length = if matches!(bytes[1], 32 | 64) {
            bytes[1]
        } else {
            0
        };
        for (s, values) in bytes[194..].as_chunks::<12>().0.iter().take(48).enumerate() {
            p.steps[s].copy_from_slice(values);
        }
        self.drum_changed();
    }

    pub(crate) fn drum_page(&self, row: usize, page: usize) -> [[u8; 12]; 16] {
        std::array::from_fn(|s| std::array::from_fn(|i| self.drum_step(row, i, page * 16 + s)))
    }

    pub(crate) fn drum_paste_page(&mut self, row: usize, page: usize, values: [[u8; 12]; 16]) {
        for (s, instruments) in values.into_iter().enumerate() {
            for (i, value) in instruments.into_iter().enumerate() {
                self.drum_set_step(row, i, page * 16 + s, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortening_preserves_tail_steps_and_page_copy_keeps_all_voices() {
        let mut app = App::new(crate::model::Panel::new(), None);
        app.drum_set_length(2, 64);
        app.drum_set_step(2, 1, 20, 1);
        app.drum_set_step(2, 11, 63, 1);
        let page = app.drum_page(2, 1);
        app.drum_paste_page(2, 2, page);
        assert_eq!(app.drum_step(2, 1, 36), 1);
        app.drum_set_length(2, 16);
        assert_eq!(app.drum_length(2), 16);
        app.drum_set_length(2, 64);
        assert_eq!(app.drum_step(2, 11, 63), 1);
        assert_eq!(app.drum_step(2, 1, 20), 1);
    }

    #[test]
    fn generator_uses_the_full_phrase_and_undo_preserves_hidden_pages() {
        for row in [2, 3] {
            for length in [32, 64] {
                let mut app = App::new(crate::model::Panel::new(), None);
                app.drum_set_length(row, length);
                app.drum_set_step(row, 1, 63, 1);
                let before = app.drum_pattern_bytes(row, 0);
                app.random_pattern_with_seed(row, 731);
                let generated = app.drum_pattern_bytes(row, 0);
                assert_eq!(generated[1], length as u8);
                for page in 0..length / 16 {
                    assert!(
                        generated[2 + page * 192..2 + (page + 1) * 192]
                            .iter()
                            .any(|&v| v != 0)
                    );
                }
                assert!(app.can_undo_random());
                assert!(app.undo_random());
                assert_eq!(app.drum_pattern_bytes(row, 0), before);
                app.random_pattern_with_seed(row, 731);
                app.drum_set_step(row, 1, 20, 1 - app.drum_step(row, 1, 20).min(1));
                assert!(
                    !app.can_undo_random(),
                    "manual tail edits must block stale undo"
                );
            }
        }
    }
}

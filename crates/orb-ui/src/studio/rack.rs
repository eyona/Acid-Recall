//! Permanent sampler and effect column for the Graphite rack.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, Vec2};
use orb_engine::studio::sampler::{Lane, Mode};
use orb_player::Command;

use super::editors::{trim_at, waveform};
use crate::app::App;
use crate::midi::MidiMenu;

const ACID: Color32 = Color32::from_rgb(190, 215, 126);
const FACE: Color32 = Color32::from_rgb(29, 31, 32);

fn surface(ui: &mut Ui, rect: Rect, id: &str, draw: impl FnOnce(&mut Ui)) {
    ui.painter().rect_filled(rect, 5.0, FACE);
    ui.painter().rect_stroke(
        rect,
        5.0,
        Stroke::new(1.0, Color32::from_gray(55)),
        egui::StrokeKind::Inside,
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .id_salt(id),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(rect));
            let scale = (rect.width() / 300.0).clamp(0.8, 1.15);
            ui.style_mut().override_font_id = Some(FontId::proportional(12.0 * scale));
            ui.spacing_mut().item_spacing = Vec2::new(5.0 * scale, 3.0 * scale);
            ui.spacing_mut().interact_size.y = 18.0 * scale;
            ui.visuals_mut().selection.bg_fill = Color32::from_rgb(67, 78, 46);
            ui.visuals_mut().selection.stroke = Stroke::new(1.0, ACID);
            draw(ui);
        },
    );
}

fn float_knob(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    label: &str,
    id: &str,
) {
    let (lo, hi) = (*range.start(), *range.end());
    ui.vertical(|ui| {
        let width = (ui.available_width() / 3.0).clamp(32.0, 52.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, width + 34.0), Sense::click_and_drag());
        let response = response.midi_menu(ui, id);
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|i| -i.pointer.delta().y);
            let fine = ui.input(|i| if i.modifiers.shift { 0.1 } else { 1.0 });
            *value = (*value + f64::from(delta * fine) * (hi - lo) / 160.0).clamp(lo, hi);
        }
        let center = rect.center_top() + Vec2::new(0.0, width * 0.5);
        let radius = width * 0.34;
        ui.painter()
            .circle_filled(center, radius, Color32::from_gray(23));
        ui.painter()
            .circle_stroke(center, radius, Stroke::new(2.0, Color32::from_gray(83)));
        let angle = -std::f32::consts::PI * 1.25
            + ((*value - lo) / (hi - lo)) as f32 * std::f32::consts::PI * 1.5;
        let tip = center + Vec2::angled(angle) * radius * 0.82;
        ui.painter()
            .line_segment([center, tip], Stroke::new(2.0, ACID));
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 20.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.5),
            Color32::from_gray(200),
        );
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 2.0),
            Align2::CENTER_BOTTOM,
            format!("{:.2}", *value),
            FontId::monospace(9.5),
            Color32::GRAY,
        );
        response.on_hover_text(format!(
            "{label}: drag vertically; Shift for fine adjustment"
        ));
    });
}

pub(super) fn gater_controls(ui: &mut Ui, lane: &mut Lane, prefix: &str) {
    ui.horizontal_wrapped(|ui| {
        let mut on = lane.gater_source != 0;
        if ui
            .checkbox(&mut on, "GATER")
            .midi_menu(ui, &format!("{prefix}.gater_source"))
            .changed()
        {
            lane.gater_source = if on { 4 } else { 0 };
        }
        egui::ComboBox::from_id_salt("gater-source")
            .width(66.0)
            .selected_text(match lane.gater_source {
                1 => "TB-A",
                2 => "TB-B",
                3 => "TR-808",
                _ => "TR-909",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut lane.gater_source, 3, "TR-808")
                    .midi_menu(ui, &format!("{prefix}.gater_source"));
                ui.selectable_value(&mut lane.gater_source, 4, "TR-909")
                    .midi_menu(ui, &format!("{prefix}.gater_source"));
            });
        let instruments = if lane.gater_source == 3 {
            [
                "BD", "SD", "LT", "MT", "HT", "RS", "CP", "CB", "CY", "OH", "CH",
            ]
        } else {
            [
                "BD", "SD", "LT", "MT", "HT", "RS", "CP", "CH", "OH", "CC", "RC",
            ]
        };
        let selected = usize::from(lane.gater_instrument.clamp(1, 11) - 1);
        egui::ComboBox::from_id_salt("gater-instrument")
            .width(42.0)
            .selected_text(instruments[selected])
            .show_ui(ui, |ui| {
                for (i, label) in instruments.iter().enumerate() {
                    ui.selectable_value(&mut lane.gater_instrument, i as u8 + 1, *label)
                        .midi_menu(ui, &format!("{prefix}.gater_instrument"));
                }
            });
        let mut depth = lane.gater_depth * 100.0;
        if ui
            .add(
                crate::midi::drag_value(ui, &mut depth)
                    .range(0.0..=100.0)
                    .prefix("Depth ")
                    .suffix("%"),
            )
            .changed()
        {
            lane.gater_depth = depth / 100.0;
        }
        let holds = [
            (0.125, "1/128"),
            (0.25, "1/64"),
            (0.5, "1/32"),
            (1.0, "1/16"),
            (2.0, "1/8"),
            (4.0, "1/4"),
            (8.0, "1/2"),
            (16.0, "1 bar"),
        ];
        let hold_text = holds
            .iter()
            .find(|(v, _)| (lane.gater_hold - v).abs() < 1e-6)
            .map_or_else(
                || format!("{:.2} steps", lane.gater_hold),
                |(_, text)| (*text).into(),
            );
        egui::ComboBox::from_id_salt("gater-hold")
            .width(73.0)
            .selected_text(format!("Hold {hold_text}"))
            .show_ui(ui, |ui| {
                for (value, text) in [
                    (0.125, "1/128"),
                    (0.25, "1/64"),
                    (0.5, "1/32"),
                    (1.0, "1/16"),
                    (2.0, "1/8"),
                    (4.0, "1/4"),
                    (8.0, "1/2"),
                    (16.0, "1 bar"),
                ] {
                    ui.selectable_value(&mut lane.gater_hold, value, text)
                        .midi_menu(ui, &format!("{prefix}.gater_hold"));
                }
            });
        ui.add(
            crate::midi::drag_value(ui, &mut lane.gater_release_ms)
                .range(1.0..=500.0)
                .prefix("Smooth ")
                .suffix("ms"),
        )
        .midi_menu(ui, &format!("{prefix}.gater_release_ms"));
        ui.checkbox(&mut lane.sidechain, "SIDECHAIN")
            .midi_menu(ui, &format!("{prefix}.sidechain"))
            .on_hover_text(
                "Sampler ducking follows the selected sidechain drum voice, source and release",
            );
    });
}

pub(super) fn warp_controls(ui: &mut Ui, lane: &mut Lane, frames: Option<usize>, prefix: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut lane.warp, "TIME WARP")
            .midi_menu(ui, &format!("{prefix}.warp"))
            .on_hover_text("Follow the song tempo without changing the sample pitch");
        ui.add(
            crate::midi::drag_value(ui, &mut lane.warp_bpm)
                .range(20.0..=400.0)
                .speed(0.1)
                .prefix("Src BPM "),
        )
        .midi_menu(ui, &format!("{prefix}.warp_bpm"))
        .on_hover_text("The original tempo of this sample, not the song tempo");
        ui.add(
            crate::midi::drag_value(ui, &mut lane.warp_beats)
                .range(1..=256)
                .prefix("Beats "),
        )
        .midi_menu(ui, &format!("{prefix}.warp_beats"))
        .on_hover_text("Number of quarter-note beats in the selected IN/OUT region");
        let fitted = frames.and_then(|frames| lane.fitted_warp_bpm(frames));
        if ui
            .add_enabled(fitted.is_some(), egui::Button::new("FIT"))
            .midi_menu(ui, &format!("{prefix}.warp_fit"))
            .on_hover_text("Calculate source BPM from Beats and the selected audio; enable Warp")
            .clicked()
        {
            lane.warp_bpm = fitted.unwrap();
            lane.warp = true;
        }
        ui.small(if lane.warp {
            "Tempo sync"
        } else {
            "Free playback"
        });
    });
}

fn lane_row(
    ui: &mut Ui,
    index: usize,
    name: &str,
    active: bool,
    lane: &mut Lane,
    selected: &mut usize,
    row_width: f32,
) -> [Rect; 3] {
    let mut regions = [Rect::NOTHING; 3];
    ui.push_id(("sampler-lane", index), |ui| {
        ui.spacing_mut().interact_size.x = 20.0;
        ui.spacing_mut().button_padding.x = 3.0;
        ui.horizontal(|ui| {
            let width = (row_width - 40.0 - 2.0 * ui.spacing().item_spacing.x).max(40.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
            regions[0] = rect;
            ui.painter().rect_filled(
                rect,
                3.0,
                if active {
                    Color32::from_rgb(67, 78, 46)
                } else {
                    Color32::from_gray(43)
                },
            );
            ui.painter().with_clip_rect(rect.shrink(3.0)).text(
                rect.left_center() + Vec2::new(4.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{}  {name}", index + 1),
                FontId::proportional(11.0),
                if lane.muted {
                    Color32::GRAY
                } else {
                    Color32::WHITE
                },
            );
            if response.on_hover_text(name).clicked() {
                *selected = index;
            }
            let solo = ui.add_sized([20.0, 20.0], egui::Button::new("S").selected(lane.solo));
            regions[1] = solo.rect;
            if solo
                .midi_menu(ui, &format!("sampler{}.solo", index + 1))
                .on_hover_text("Solo this sample")
                .clicked()
            {
                lane.solo = !lane.solo;
            }
            let mute = ui.add_sized([20.0, 20.0], egui::Button::new("M").selected(lane.muted));
            regions[2] = mute.rect;
            if mute
                .midi_menu(ui, &format!("sampler{}.mute", index + 1))
                .on_hover_text("Mute this sample")
                .clicked()
            {
                lane.muted = !lane.muted;
            }
        });
    });
    regions
}

impl App {
    fn graphite_sampler_mixer(&mut self, ui: &mut Ui, rect: Rect) {
        let meter = self
            .link
            .as_mut()
            .map(|link| link.player().snapshot().sampler_meter)
            .unwrap_or([0; 2]);
        if meter.iter().any(|&v| v > 0) {
            ui.ctx().request_repaint_after(crate::app::METER_FRAME);
        }
        surface(ui, rect, "sampler-mixer", |ui| {
            let bus = &mut self.studio.settings.sampler_bus;
            ui.horizontal(|ui| {
                ui.colored_label(ACID, egui::RichText::new("SAMPLER MIX").strong());
            });
            let scale = rect.height() / 320.0;
            let x_scale = crate::graphite_rack::WIDTH / 1280.0 * scale;
            let power_rect = Rect::from_min_size(
                egui::pos2(rect.right() - 53.0 * x_scale, rect.top() + 9.0 * scale),
                Vec2::new(42.0 * x_scale, 19.0 * scale),
            );
            if crate::graphite_rack::machine_power_button(
                ui,
                power_rect,
                scale,
                "sampler.on",
                bus.enabled,
            )
            .clicked()
            {
                bus.enabled = !bus.enabled;
            }
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.small("LEVEL");
                    ui.add_sized(
                        [26.0, 102.0],
                        crate::midi::slider(ui, &mut bus.gain, 0.0..=2.0)
                            .vertical()
                            .show_value(false),
                    )
                    .midi_menu(ui, "sampler.level");
                    ui.small(format!("{:.0}%", bus.gain * 100.0));
                });
                let (meters, _) = ui.allocate_exact_size(Vec2::new(17.0, 132.0), Sense::hover());
                for (channel, &value) in meter.iter().enumerate() {
                    for led in 0..12 {
                        let r = Rect::from_min_size(
                            egui::pos2(
                                meters.left() + channel as f32 * 8.0,
                                meters.bottom() - (led + 1) as f32 * 11.0,
                            ),
                            Vec2::new(5.0, 8.0),
                        );
                        let lit = u16::from(value) * 12 > led as u16 * 127;
                        let color = if !lit {
                            Color32::from_gray(47)
                        } else if led >= 11 {
                            Color32::from_rgb(220, 94, 72)
                        } else if led >= 9 {
                            Color32::from_rgb(222, 196, 90)
                        } else {
                            ACID
                        };
                        ui.painter().rect_filled(r, 0.5, color);
                    }
                }
                ui.vertical(|ui| {
                    ui.set_width(96.0);
                    float_knob(ui, &mut bus.pan, -1.0..=1.0, "PAN", "sampler.pan");
                    float_knob(ui, &mut bus.delay, 0.0..=1.0, "DELAY", "sampler.delay");
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if ui
                        .add_sized([52.0, 22.0], egui::Button::new("DIST").selected(bus.dist))
                        .midi_menu(ui, "sampler.dist")
                        .clicked()
                    {
                        bus.dist = !bus.dist;
                    }
                    let on = bus.reverb_send > 0.0;
                    if ui
                        .add_sized([52.0, 22.0], egui::Button::new("REV").selected(on))
                        .clicked()
                    {
                        bus.reverb_send = if on { 0.0 } else { 0.3 };
                    }
                });
                float_knob(
                    ui,
                    &mut bus.reverb_send,
                    0.0..=1.0,
                    "REV SEND",
                    "sampler.send",
                );
            });
        });
    }

    pub(crate) fn graphite_sampler(&mut self, ui: &mut Ui, rect: Rect) {
        let before = self.studio.settings.clone();
        let mut load = None;
        let mut audition = false;
        let selected = self.studio.selected.min(7);
        let busy = self.studio.worker.is_some() || !self.studio.ready;
        let context = ui.ctx().clone();
        let tick = self.panel.transport.position.max(0) as usize;
        let mixer_width = rect.width() * 0.185;
        let mixer = Rect::from_min_max(
            egui::pos2(rect.right() - mixer_width, rect.top()),
            rect.right_bottom(),
        );
        let rect = Rect::from_min_max(
            rect.left_top(),
            egui::pos2(mixer.left() - 6.0, rect.bottom()),
        );
        surface(ui, rect, "graphite-sampler", |ui| {
            egui::ScrollArea::vertical()
                .id_salt("sampler-rack-scroll")
                .max_height(ui.available_height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(ACID, egui::RichText::new("SAMPLER").strong());
                        ui.small("8 LANES / 64 BARS");
                        if ui.add_enabled(!busy, egui::Button::new("LOAD")).clicked() {
                            load = Some(selected);
                        }
                        if ui
                            .add_enabled(
                                self.studio.waves[selected].is_some(),
                                egui::Button::new("AUDITION"),
                            )
                            .clicked()
                        {
                            audition = true;
                        }
                        if ui.button("EDIT / ALL LANES").clicked() {
                            self.studio.sampler_open = true;
                        }
                        ui.checkbox(&mut self.studio.follow, "Follow");
                        if busy {
                            ui.spinner();
                        }
                    });
                    ui.separator();
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(rect.width() * 0.22);
                            for i in 0..8 {
                                let lane = &mut self.studio.settings.lanes[i];
                                lane_row(
                                    ui,
                                    i,
                                    &self.studio.names[i],
                                    i == selected,
                                    lane,
                                    &mut self.studio.selected,
                                    rect.width() * 0.22,
                                );
                            }
                        });
                        ui.vertical(|ui| {
                            ui.set_width((rect.width() * 0.75 - 16.0).max(100.0));
                            let lane = &mut self.studio.settings.lanes[selected];
                            if let Some(wave) = &self.studio.waves[selected] {
                                let response = waveform(
                                    ui,
                                    &wave.frames,
                                    (rect.height() * 0.25).max(32.0),
                                    lane.start,
                                    lane.end,
                                    self.studio.waveform_epoch,
                                    &mut self.studio.wave_views[selected],
                                );
                                let minimum = 2.0 / wave.frames.len().max(2) as f64;
                                if response.double_clicked() {
                                    lane.start = 0.0;
                                    lane.end = 1.0;
                                } else if let Some(p) = response.interact_pointer_pos() {
                                    let at = f64::from(
                                        (p.x - response.rect.left()) / response.rect.width(),
                                    )
                                    .clamp(0.0, 1.0);
                                    let id = response.id.with("handle");
                                    if response.drag_started() || response.clicked() {
                                        let entry =
                                            (at - lane.start).abs() <= (at - lane.end).abs();
                                        ui.data_mut(|d| d.insert_temp(id, entry));
                                    }
                                    if response.dragged_by(egui::PointerButton::Primary)
                                        || response.clicked()
                                    {
                                        let entry =
                                            ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
                                        trim_at(lane, entry, at, minimum);
                                    }
                                }
                                let duration = wave.frames.len() as f64 / 44100.0;
                                let mut entry = lane.start * duration;
                                let mut exit = lane.end * duration;
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(
                                            crate::midi::drag_value(ui, &mut entry)
                                                .speed(0.001)
                                                .fixed_decimals(3)
                                                .prefix("IN ")
                                                .suffix("s"),
                                        )
                                        .changed()
                                    {
                                        trim_at(lane, true, entry / duration, minimum);
                                    }
                                    if ui
                                        .add(
                                            crate::midi::drag_value(ui, &mut exit)
                                                .speed(0.001)
                                                .fixed_decimals(3)
                                                .prefix("OUT ")
                                                .suffix("s"),
                                        )
                                        .changed()
                                    {
                                        trim_at(lane, false, exit / duration, minimum);
                                    }
                                });
                            } else {
                                ui.allocate_ui(
                                    Vec2::new(ui.available_width(), rect.height() * 0.28),
                                    |ui| {
                                        ui.centered_and_justified(|ui| {
                                            ui.label("LOAD WAV / AIFF / MP3");
                                        });
                                    },
                                );
                            }
                            ui.horizontal_wrapped(|ui| {
                                for (mode, label) in [
                                    (Mode::OneShot, "ONE SHOT"),
                                    (Mode::AutoCut, "AUTO CUT"),
                                    (Mode::Gate, "GATE"),
                                    (Mode::Loop, "LOOP"),
                                ] {
                                    ui.selectable_value(&mut lane.mode, mode, label)
                                        .midi_menu(ui, &format!("sampler{}.mode", selected + 1));
                                }
                                if ui
                                    .selectable_label(lane.reverse, "REVERSE")
                                    .midi_menu(ui, &format!("sampler{}.reverse", selected + 1))
                                    .clicked()
                                {
                                    lane.reverse = !lane.reverse;
                                }
                                ui.add(
                                    crate::midi::drag_value(ui, &mut lane.pitch)
                                        .range(-24..=24)
                                        .prefix("Pitch "),
                                )
                                .midi_menu(ui, &format!("sampler{}.pitch", selected + 1));
                                ui.add(
                                    crate::midi::drag_value(ui, &mut lane.gain)
                                        .range(0.0..=2.0)
                                        .speed(0.005)
                                        .prefix("Level "),
                                )
                                .midi_menu(ui, &format!("sampler{}.level", selected + 1));
                                ui.add(
                                    crate::midi::drag_value(ui, &mut lane.send)
                                        .range(0.0..=1.0)
                                        .speed(0.005)
                                        .prefix("REV "),
                                )
                                .midi_menu(ui, &format!("sampler{}.send", selected + 1));
                            });
                            let frames =
                                self.studio.waves[selected].as_ref().map(|w| w.frames.len());
                            warp_controls(ui, lane, frames, &format!("sampler{}", selected + 1));
                            gater_controls(ui, lane, &format!("sampler{}", selected + 1));
                            let bars = usize::from(lane.bars.clamp(1, 64));
                            if self.studio.follow && self.panel.transport.playing {
                                self.studio.bar = tick / 768 % bars;
                            }
                            self.studio.bar = self.studio.bar.min(bars - 1);
                            ui.horizontal(|ui| {
                                ui.add(
                                    crate::midi::drag_value(ui, &mut lane.bars)
                                        .range(1..=64)
                                        .prefix("Length ")
                                        .suffix(" bars"),
                                );
                                let mut bar = self.studio.bar + 1;
                                if ui
                                    .add(
                                        crate::midi::drag_value(ui, &mut bar)
                                            .range(1..=bars)
                                            .prefix("Bar "),
                                    )
                                    .changed()
                                {
                                    self.studio.bar = bar - 1;
                                    self.studio.follow = false;
                                }
                            });
                            ui.horizontal(|ui| {
                                let width = ((ui.available_width() - 15.0 * 3.0) / 16.0).max(8.0);
                                ui.spacing_mut().item_spacing.x = 3.0;
                                for step in 0..16 {
                                    let index = self.studio.bar * 16 + step;
                                    let on = lane.steps[index] != 0;
                                    let head = self.panel.transport.playing
                                        && tick / 48 % (usize::from(lane.bars) * 16) == index;
                                    let button = egui::Button::new(format!("{}", step + 1))
                                        .fill(if on {
                                            Color32::from_rgb(94, 112, 50)
                                        } else {
                                            FACE
                                        })
                                        .stroke(Stroke::new(
                                            if head { 2.0 } else { 1.0 },
                                            if head { ACID } else { Color32::from_gray(70) },
                                        ));
                                    let response = ui.add_sized([width, 19.0], button);
                                    if response.clicked() {
                                        lane.steps[index] = if on { 0 } else { 100 };
                                    }
                                    response.context_menu(|ui| {
                                        ui.add(
                                            crate::midi::slider(
                                                ui,
                                                &mut lane.steps[index],
                                                0..=127,
                                            )
                                            .text("Velocity"),
                                        );
                                    });
                                }
                            });
                        });
                    });
                });
        });
        self.graphite_sampler_mixer(ui, mixer);
        if self.studio.settings != before {
            self.studio.changed();
            self.doc.modified = true;
        }
        if let Some(lane) = load {
            self.studio_load_wave(&context, lane);
        }
        if audition {
            self.studio_command(Command::StudioAudition(selected));
        }
    }

    pub(crate) fn graphite_fx(&mut self, ui: &mut Ui, rect: Rect) {
        let before = self.studio.settings.clone();
        let width = rect.width();
        let gap = 7.0;
        let heights = [0.24, 0.14, 0.14, 0.14, 0.14, 0.20];
        let mut top = rect.top();
        for (i, fraction) in heights.into_iter().enumerate() {
            let card = Rect::from_min_size(
                egui::pos2(rect.left(), top),
                Vec2::new(width, rect.height() * fraction - gap),
            );
            top = card.bottom() + gap;
            surface(ui, card, &format!("graphite-fx-{i}"), |ui| match i {
                0 => {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.studio.settings.enabled, "SPACE")
                            .midi_menu(ui, "studio.enabled")
                            .clicked()
                        {
                            self.studio.settings.enabled = !self.studio.settings.enabled;
                        }
                        ui.small("CONVOLUTION");
                        if ui.small_button("...").clicked() {
                            self.studio.reverb_open = true;
                        }
                    });
                    ui.label(&self.studio.ir_name);
                    ui.horizontal(|ui| {
                        let s = &mut self.studio.settings;
                        float_knob(ui, &mut s.return_gain, 0.0..=1.0, "RETURN", "studio.return");
                        float_knob(
                            ui,
                            &mut s.predelay_ms,
                            0.0..=200.0,
                            "PREDELAY",
                            "studio.predelay",
                        );
                        float_knob(ui, &mut s.width, 0.0..=2.0, "WIDTH", "studio.width");
                    });
                    ui.horizontal(|ui| {
                        float_knob(
                            ui,
                            &mut self.studio.settings.low_cut,
                            20.0..=2000.0,
                            "LOW CUT",
                            "studio.low_cut",
                        );
                        float_knob(
                            ui,
                            &mut self.studio.settings.high_cut,
                            2000.0..=20000.0,
                            "HIGH CUT",
                            "studio.high_cut",
                        );
                        if ui.small_button("IR / SENDS").clicked() {
                            self.studio.reverb_open = true;
                        }
                    });
                }
                1..=4 => {
                    let (section, title, parameters): (&str, &str, &[(&str, &str)]) = match i {
                        1 => ("delay", "DELAY", &[("pan", "PAN"), ("feedback", "FDBK")]),
                        2 => ("dist", "DIST", &[("amount", "AMOUNT"), ("shape", "SHAPE")]),
                        3 => (
                            "pcf",
                            "PCF",
                            &[("freq", "FREQ"), ("q", "Q"), ("amt", "AMOUNT")],
                        ),
                        _ => (
                            "comp",
                            "COMP",
                            &[("threshold", "THRESH"), ("ratio", "RATIO")],
                        ),
                    };
                    ui.horizontal(|ui| {
                        let id = format!("{section}.on");
                        if ui
                            .selectable_label(self.panel.toggle_on(&id), title)
                            .midi_menu(ui, &id)
                            .clicked()
                        {
                            self.panel.press(&id, 0.0);
                        }
                        if ui.small_button("EDIT").clicked() {
                            self.studio.fx = Some(section.into());
                        }
                    });
                    ui.horizontal(|ui| {
                        for &(name, label) in parameters {
                            let id = format!("{section}.{name}");
                            if self.panel.value(&id).is_none() {
                                continue;
                            }
                            let old = self.panel.knob(&id);
                            let mut value = f64::from(old) / 1000.0;
                            ui.push_id(&id, |ui| {
                                float_knob(ui, &mut value, 0.0..=1.0, label, &id);
                            });
                            let next = (value * 1000.0).round() as i32;
                            if next != old {
                                self.panel.set_knob(&id, next);
                            }
                        }
                    });
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.colored_label(ACID, "MASTER");
                        let on = self.panel.toggle_on("master.comp");
                        if ui
                            .selectable_label(on, "COMP")
                            .midi_menu(ui, "master.comp")
                            .clicked()
                        {
                            self.panel.press("master.comp", 0.0);
                        }
                    });
                    let mut level = self.panel.fader("master.fader");
                    if ui
                        .add(crate::midi::slider(ui, &mut level, 0..=999).text("LEVEL"))
                        .midi_menu(ui, "master.fader")
                        .changed()
                    {
                        self.panel.set_fader("master.fader", level);
                    }
                    for key in ["master.meter_l", "master.meter_r"] {
                        let level = f32::from(*self.panel.meters.get(key).unwrap_or(&0)) / 31.0;
                        ui.add(egui::ProgressBar::new(level.clamp(0.0, 1.0)).desired_height(8.0));
                    }
                    ui.small("Stereo master / compressor before output");
                }
            });
        }
        if self.studio.settings != before {
            self.studio.changed();
            self.doc.modified = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_and_mute_are_independent_clicks_next_to_the_sample_name() {
        let ctx = egui::Context::default();
        let mut lane = Lane::default();
        let mut selected = 0;
        let mut regions = [Rect::NOTHING; 3];
        let mut draw = |events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(300.0, 100.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(240.0);
                    regions = lane_row(
                        ui,
                        4,
                        "A very long sample name to truncate.wav",
                        false,
                        &mut lane,
                        &mut selected,
                        240.0,
                    );
                },
            );
            output.textures_delta.clear();
            regions
        };
        let regions = draw(Vec::new());
        assert!(regions[0].right() < regions[1].left());
        assert!(regions[1].right() < regions[2].left());
        for index in [1, 2] {
            let pos = regions[index].center();
            for pressed in [true, false] {
                draw(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]);
            }
        }
        assert!(lane.solo && lane.muted);
        assert_eq!(
            selected, 0,
            "Quick mix actions must not change the editing lane"
        );
    }

    #[test]
    fn quick_mix_columns_stay_aligned_across_all_eight_sample_names() {
        let ctx = egui::Context::default();
        let mut selected = 0;
        let mut lanes: [Lane; 8] = std::array::from_fn(|_| Lane::default());
        let mut columns = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.0))),
                ..Default::default()
            },
            |ui| {
                ui.set_width(240.0);
                for (index, lane) in lanes.iter_mut().enumerate() {
                    columns.push(lane_row(
                        ui,
                        index,
                        "A long imported sample filename.wav",
                        false,
                        lane,
                        &mut selected,
                        240.0,
                    ));
                }
            },
        );
        output.textures_delta.clear();
        for row in &columns {
            assert_eq!(row[1].left(), columns[0][1].left());
            assert_eq!(row[2].left(), columns[0][2].left());
            assert!(row[2].right() <= row[0].left() + 240.1);
        }
    }

    #[test]
    fn cards_keep_the_scroll_viewport_clip() {
        let ctx = egui::Context::default();
        let viewport = Rect::from_min_size(egui::pos2(20.0, 20.0), Vec2::splat(60.0));
        let outer = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(100.0));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(outer),
                ..Default::default()
            },
            |ui| {
                ui.set_clip_rect(viewport);
                surface(ui, outer, "clipped-card", |ui| {
                    assert_eq!(ui.clip_rect(), viewport);
                });
            },
        );
        output.textures_delta.clear();
    }
}

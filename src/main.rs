mod model;
mod pipewire_conf;
mod system;

use crate::model::{AppConfig, VbanRecv, VbanSend};
use anyhow::Result;
use eframe::egui;
use eframe::egui::{Color32, RichText, Stroke};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Sends,
    Recvs,
}

const APP_ID: &str = "com.rustban.app";
const APP_ICON_BYTES: &[u8] = include_bytes!("../logo.png");

struct App {
    cfg: AppConfig,
    tab: Tab,
    status: String,
    theme_applied: bool,
    microphone_sources: Vec<system::AudioSourceDevice>,
}

impl App {
    fn new() -> Self {
        let (cfg, status) = match system::load_app_config() {
            Ok(cfg) => (cfg, "Ready.".to_string()),
            Err(e) => (AppConfig::default(), format!("Config load error: {e:#}")),
        };

        let mut app = Self {
            cfg,
            tab: Tab::Sends,
            status,
            theme_applied: false,
            microphone_sources: Vec::new(),
        };

        if let Err(e) = app.load_microphone_sources() {
            let scan_error = format!("Microphone scan error: {e:#}");
            app.status = if app.status == "Ready." {
                scan_error
            } else {
                format!("{} | {}", app.status, scan_error)
            };
        }

        app
    }

    fn save(&mut self) {
        self.status = match system::save_app_config(&self.cfg) {
            Ok(()) => "Config saved.".into(),
            Err(e) => format!("Save error: {e:#}"),
        };
    }

    fn apply(&mut self, restart: bool) {
        let result = (|| -> Result<()> {
            self.save();
            system::apply_pipewire_fragments(&self.cfg)?;
            if restart {
                system::restart_pipewire_user_services()?;
            }
            Ok(())
        })();

        self.status = match result {
            Ok(()) if restart => "Fragments applied + pipewire restarted.".into(),
            Ok(()) => "Fragments applied.".into(),
            Err(e) => format!("Apply error: {e:#}"),
        };
    }

    fn load_microphone_sources(&mut self) -> Result<()> {
        self.microphone_sources = system::list_microphone_sources()?;
        Ok(())
    }

    fn refresh_microphone_sources(&mut self) {
        match self.load_microphone_sources() {
            Ok(()) => {
                self.status = format!(
                    "Detected {} microphone source(s).",
                    self.microphone_sources.len()
                )
            }
            Err(e) => {
                self.status = format!("Microphone scan error: {e:#}");
                self.microphone_sources.clear();
            }
        }
    }

    fn ui_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(Color32::from_rgb(22, 29, 45))
            .stroke(Stroke::new(1.0, Color32::from_rgb(67, 91, 146)))
            .rounding(egui::Rounding::same(14.0))
            .inner_margin(egui::Margin::symmetric(14.0, 12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("RustBAN")
                            .size(24.0)
                            .strong()
                            .color(Color32::from_rgb(145, 201, 255)),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("VBAN control panel")
                            .size(14.0)
                            .color(Color32::from_rgb(189, 207, 234)),
                    );
                });
            });
    }

    fn tab_button(ui: &mut egui::Ui, selected: bool, label: &str, color: Color32) -> bool {
        let fill = if selected {
            color
        } else {
            Color32::from_rgb(43, 50, 64)
        };
        ui.add(
            egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
                .fill(fill)
                .stroke(Stroke::new(1.0, color))
                .rounding(egui::Rounding::same(8.0))
                .min_size(egui::vec2(126.0, 34.0)),
        )
        .clicked()
    }

    fn action_button(ui: &mut egui::Ui, label: &str, color: Color32) -> bool {
        ui.add(
            egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
                .fill(color)
                .stroke(Stroke::new(1.0, color.gamma_multiply(1.15)))
                .rounding(egui::Rounding::same(8.0)),
        )
        .clicked()
    }

    fn ui_toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(Color32::from_rgb(26, 33, 45))
            .stroke(Stroke::new(1.0, Color32::from_rgb(56, 70, 92)))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(10.0, 10.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if Self::tab_button(
                        ui,
                        self.tab == Tab::Sends,
                        "VBAN Send",
                        Color32::from_rgb(43, 133, 219),
                    ) {
                        self.tab = Tab::Sends;
                    }
                    if Self::tab_button(
                        ui,
                        self.tab == Tab::Recvs,
                        "VBAN Recv",
                        Color32::from_rgb(23, 176, 127),
                    ) {
                        self.tab = Tab::Recvs;
                    }

                    ui.separator();

                    if Self::action_button(ui, "Save", Color32::from_rgb(68, 150, 110)) {
                        self.save();
                    }
                    if Self::action_button(ui, "Refresh mics", Color32::from_rgb(69, 94, 155)) {
                        self.refresh_microphone_sources();
                    }
                    if Self::action_button(ui, "Apply fragments", Color32::from_rgb(57, 111, 188))
                    {
                        self.apply(false);
                    }
                    if Self::action_button(
                        ui,
                        "Apply + restart",
                        Color32::from_rgb(179, 114, 48),
                    ) {
                        self.apply(true);
                    }
                });
            });
    }

    fn ui_card_frame(fill: Color32, border: Color32) -> egui::Frame {
        egui::Frame::none()
            .fill(fill)
            .stroke(Stroke::new(1.0, border))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
    }

    fn ui_labeled_text(ui: &mut egui::Ui, label: &str, value: &mut String) {
        ui.horizontal(|ui| {
            ui.add_sized(
                egui::vec2(170.0, 22.0),
                egui::Label::new(RichText::new(label).color(Color32::from_rgb(202, 216, 236))),
            );
            ui.add_sized(
                egui::vec2(ui.available_width(), 24.0),
                egui::TextEdit::singleline(value),
            );
        });
    }

    fn microphone_option_label(source: &system::AudioSourceDevice) -> String {
        if source.description == source.node_name {
            source.node_name.clone()
        } else {
            format!("{} ({})", source.description, source.node_name)
        }
    }

    fn selected_microphone_label(
        target_object: &str,
        sources: &[system::AudioSourceDevice],
    ) -> String {
        let target = target_object.trim();
        if target.is_empty() {
            "None (manual patch in qpwgraph)".to_string()
        } else if let Some(source) = sources.iter().find(|source| source.node_name == target) {
            Self::microphone_option_label(source)
        } else {
            format!("Custom: {target}")
        }
    }

    fn ui_sends(&mut self, ui: &mut egui::Ui) {
        if Self::action_button(ui, "+ Add send", Color32::from_rgb(43, 133, 219)) {
            let mut send = VbanSend::default();
            if let Some(source) = self.microphone_sources.first() {
                send.target_object = source.node_name.clone();
            }
            self.cfg.sends.push(send);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} microphone source(s) detected.",
                    self.microphone_sources.len()
                ))
                .color(Color32::from_rgb(175, 186, 204)),
            );
            if self.microphone_sources.is_empty() {
                ui.label(
                    RichText::new("Click `Refresh mics` in toolbar.")
                        .color(Color32::from_rgb(205, 165, 103)),
                );
            }
        });
        ui.add_space(8.0);

        if self.cfg.sends.is_empty() {
            ui.label(RichText::new("No send configured.").color(Color32::from_rgb(175, 186, 204)));
            return;
        }

        let microphone_sources = self.microphone_sources.clone();
        let mut remove_index: Option<usize> = None;
        for (i, send) in self.cfg.sends.iter_mut().enumerate() {
            let accent = if send.enabled {
                Color32::from_rgb(64, 164, 255)
            } else {
                Color32::from_rgb(111, 120, 135)
            };
            let fill = if send.enabled {
                Color32::from_rgb(30, 39, 55)
            } else {
                Color32::from_rgb(34, 39, 48)
            };

            Self::ui_card_frame(fill, accent).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut send.enabled, "");
                    ui.label(
                        RichText::new(format!("Send {}", i + 1))
                            .strong()
                            .size(17.0)
                            .color(accent),
                    );
                    ui.separator();
                    let title = if send.sess_name.trim().is_empty() {
                        "(no stream name)"
                    } else {
                        &send.sess_name
                    };
                    ui.label(RichText::new(title).color(Color32::from_rgb(206, 220, 241)));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Delete").strong().color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(187, 72, 72))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(219, 91, 91)))
                                .rounding(egui::Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            remove_index = Some(i);
                        }
                    });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut send.always_process, "Always process");
                });
                Self::ui_labeled_text(ui, "Stream name", &mut send.sess_name);
                Self::ui_labeled_text(ui, "Sess media", &mut send.sess_media);
                Self::ui_labeled_text(ui, "Destination IP", &mut send.destination_ip);

                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Destination port").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    ui.add(
                        egui::DragValue::new(&mut send.destination_port)
                            .clamp_range(1..=u16::MAX)
                            .speed(1.0),
                    );
                });

                Self::ui_labeled_text(ui, "Audio format", &mut send.audio_format);

                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Audio rate").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    ui.add(
                        egui::DragValue::new(&mut send.audio_rate)
                            .clamp_range(8_000..=384_000)
                            .speed(10.0),
                    );
                });

                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Audio channels").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    ui.add(
                        egui::DragValue::new(&mut send.audio_channels)
                            .clamp_range(1..=32)
                            .speed(1.0),
                    );
                });

                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Microphone source").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    egui::ComboBox::from_id_source(format!("send-source-{}", i))
                        .selected_text(Self::selected_microphone_label(
                            &send.target_object,
                            &microphone_sources,
                        ))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut send.target_object,
                                String::new(),
                                "None (manual patch in qpwgraph)",
                            );
                            for source in &microphone_sources {
                                ui.selectable_value(
                                    &mut send.target_object,
                                    source.node_name.clone(),
                                    Self::microphone_option_label(source),
                                );
                            }
                        });
                });

                Self::ui_labeled_text(ui, "target.object", &mut send.target_object);
                Self::ui_labeled_text(ui, "node.name", &mut send.node_name);
                Self::ui_labeled_text(ui, "node.description", &mut send.node_description);
            });
            ui.add_space(8.0);
        }

        if let Some(i) = remove_index {
            self.cfg.sends.remove(i);
            self.status = "Send removed. Save/apply to update.".into();
        }
    }

    fn ui_recvs(&mut self, ui: &mut egui::Ui) {
        if Self::action_button(ui, "+ Add recv", Color32::from_rgb(23, 176, 127)) {
            self.cfg.recvs.push(VbanRecv::default());
        }
        ui.add_space(8.0);

        if self.cfg.recvs.is_empty() {
            ui.label(RichText::new("No recv configured.").color(Color32::from_rgb(175, 186, 204)));
            return;
        }

        let mut remove_index: Option<usize> = None;
        for (i, recv) in self.cfg.recvs.iter_mut().enumerate() {
            let accent = if recv.enabled {
                Color32::from_rgb(49, 204, 152)
            } else {
                Color32::from_rgb(111, 120, 135)
            };
            let fill = if recv.enabled {
                Color32::from_rgb(27, 44, 48)
            } else {
                Color32::from_rgb(34, 39, 48)
            };

            Self::ui_card_frame(fill, accent).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut recv.enabled, "");
                    ui.label(
                        RichText::new(format!("Recv {}", i + 1))
                            .strong()
                            .size(17.0)
                            .color(accent),
                    );
                    ui.separator();
                    let title = if recv.stream_name.trim().is_empty() {
                        "* (all streams)"
                    } else {
                        &recv.stream_name
                    };
                    ui.label(RichText::new(title).color(Color32::from_rgb(206, 220, 241)));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Delete").strong().color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(187, 72, 72))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(219, 91, 91)))
                                .rounding(egui::Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            remove_index = Some(i);
                        }
                    });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut recv.always_process, "Always process");
                });
                Self::ui_labeled_text(ui, "Stream name (empty = all)", &mut recv.stream_name);
                Self::ui_labeled_text(ui, "Source IP", &mut recv.source_ip);

                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Source port").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    ui.add(
                        egui::DragValue::new(&mut recv.source_port)
                            .clamp_range(1..=u16::MAX)
                            .speed(1.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(170.0, 22.0),
                        egui::Label::new(
                            RichText::new("Latency ms").color(Color32::from_rgb(202, 216, 236)),
                        ),
                    );
                    ui.add(
                        egui::DragValue::new(&mut recv.latency_msec)
                            .clamp_range(0..=5_000)
                            .speed(1.0),
                    );
                });

                Self::ui_labeled_text(ui, "node.name", &mut recv.node_name);
                Self::ui_labeled_text(ui, "node.description", &mut recv.node_description);
            });
            ui.add_space(8.0);
        }

        if let Some(i) = remove_index {
            self.cfg.recvs.remove(i);
            self.status = "Recv removed. Save/apply to update.".into();
        }
    }

    fn status_style(status: &str) -> (Color32, Color32) {
        let low = status.to_ascii_lowercase();
        if low.contains("error") {
            (Color32::from_rgb(70, 30, 32), Color32::from_rgb(211, 84, 84))
        } else if low.contains("applied") || low.contains("saved") || low.contains("ready") {
            (Color32::from_rgb(25, 54, 46), Color32::from_rgb(61, 176, 136))
        } else {
            (Color32::from_rgb(35, 41, 52), Color32::from_rgb(98, 120, 151))
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            apply_visual_theme(ctx);
            self.theme_applied = true;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_header(ui);
            ui.add_space(10.0);
            self.ui_toolbar(ui);
            ui.add_space(10.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.tab {
                    Tab::Sends => self.ui_sends(ui),
                    Tab::Recvs => self.ui_recvs(ui),
                });

            ui.add_space(8.0);
            let (fill, border) = Self::status_style(&self.status);
            egui::Frame::none()
                .fill(fill)
                .stroke(Stroke::new(1.0, border))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&self.status)
                            .color(Color32::from_rgb(226, 235, 249))
                            .strong(),
                    );
                });
        });
    }
}

fn apply_visual_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 7.0);
    style.spacing.text_edit_width = 280.0;
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(Color32::from_rgb(224, 232, 247));
    style.visuals.panel_fill = Color32::from_rgb(17, 21, 30);
    style.visuals.window_fill = Color32::from_rgb(22, 27, 36);
    style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(25, 30, 41);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 47, 60);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(61, 72, 92));
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(62, 107, 181);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(89, 142, 225));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(51, 88, 150);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(92, 136, 215));
    style.visuals.selection.bg_fill = Color32::from_rgb(41, 121, 209);
    style.visuals.faint_bg_color = Color32::from_rgb(28, 34, 46);
    ctx.set_style(style);
}

fn load_app_icon() -> Option<Arc<egui::IconData>> {
    eframe::icon_data::from_png_bytes(APP_ICON_BYTES)
        .ok()
        .map(Arc::new)
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default().with_app_id(APP_ID);
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "RustBAN",
        native_options,
        Box::new(|_cc| Box::new(App::new())),
    )
}

//! Logger panel — read-only structured event log.
//!
//! Reads from the app's shared `ReactiveEventLoggerState` and renders in a
//! saturn-grid-sim style: numbered rows, colored level prefixes, monospace.

use egui::RichText;
use egui_citizen::{Citizen, CitizenId, CitizenState};

use super::citizen_panel;
use crate::event_logger::LogType;
use crate::theme::TokyoNight;

citizen_panel!(LoggerPanel, "logger");

impl LoggerPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let state = app.services.logger_state.get();
        let colors = app.services.log_colors.get();

        let frame = egui::Frame::new()
            .fill(TokyoNight::BG_DARK)
            .inner_margin(8.0);

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("Logger")
                        .color(TokyoNight::BLUE)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("({})", state.logs.len()))
                        .color(TokyoNight::COMMENT)
                        .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear").clicked() {
                        app.services.logger_state.lock().clear_logs();
                    }
                });
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (i, entry) in state.logs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:>4}", i + 1))
                                    .color(TokyoNight::COMMENT)
                                    .monospace(),
                            );

                            if state.show_timestamps {
                                ui.label(
                                    RichText::new(&entry.timestamp)
                                        .color(TokyoNight::COMMENT)
                                        .monospace(),
                                );
                            }

                            let (prefix, prefix_color) = level_display(&entry.log_type, &colors);
                            ui.label(
                                RichText::new(prefix)
                                    .color(prefix_color)
                                    .monospace()
                                    .strong(),
                            );

                            let msg_color = message_color(&entry.log_type, &colors);
                            ui.label(
                                RichText::new(&entry.message)
                                    .color(msg_color)
                                    .monospace(),
                            );
                        });
                    }
                });
        });
    }
}

fn level_display(log_type: &LogType, colors: &crate::event_logger::LogColors) -> (&'static str, egui::Color32) {
    match log_type {
        LogType::Info => ("[INFO] ", colors.info_level),
        LogType::Warning => ("[WARN] ", colors.warning_level),
        LogType::Error => ("[ERR ] ", colors.error_level),
        LogType::Debug => ("[DBG ] ", colors.debug_level),
        LogType::System => ("[SYS ] ", colors.system),
        LogType::Config => ("[CONF] ", colors.config),
        LogType::Status => ("[STAT] ", colors.status),
        LogType::Progress => ("[PROG] ", colors.progress),
        LogType::Success => ("[OK  ] ", colors.success),
        LogType::Timestamp => ("[TIME] ", colors.default),
        LogType::UserAction => ("[USER] ", colors.default),
        LogType::Default => ("[    ] ", colors.default),
        LogType::Custom(_) => ("[CUST] ", colors.default),
    }
}

fn message_color(log_type: &LogType, colors: &crate::event_logger::LogColors) -> egui::Color32 {
    match log_type {
        LogType::Info => colors.info_message,
        LogType::Warning => colors.warning_message,
        LogType::Error => colors.error_message,
        LogType::Debug => colors.debug_message,
        _ => colors.default,
    }
}

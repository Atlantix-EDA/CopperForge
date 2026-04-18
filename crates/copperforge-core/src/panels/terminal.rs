//! Terminal panel — OS shell for running external commands.

use egui::RichText;
use egui_citizen::{Citizen, CitizenId, CitizenState};

use super::citizen_panel;
use crate::theme::TokyoNight;

const PROMPT: &str = "$ ";
const INPUT_ID: &str = "terminal_input";

citizen_panel!(TerminalPanel, "terminal");

impl TerminalPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let frame = egui::Frame::new()
            .fill(TokyoNight::BG_DARK)
            .inner_margin(8.0);

        frame.show(ui, |ui| {
            ui.style_mut().visuals.extreme_bg_color = TokyoNight::BG_DARK;

            let text_id = ui.make_persistent_id(INPUT_ID);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in app.term_output.iter() {
                        render_line(ui, line);
                    }

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            RichText::new(PROMPT)
                                .color(TokyoNight::GREEN)
                                .monospace()
                                .strong(),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut app.term_cmd_buf)
                                .id(text_id)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .frame(false)
                                .text_color(TokyoNight::FG),
                        );

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let input = app.term_cmd_buf.trim().to_string();
                            if !input.is_empty() {
                                if input == "clear" || input == "cls" {
                                    app.term_output.clear();
                                } else {
                                    app.term_output.push(format!("{PROMPT}{input}"));
                                    for line in run_shell_command(&input) {
                                        app.term_output.push(line);
                                    }
                                }
                            }
                            app.term_cmd_buf.clear();
                            ui.memory_mut(|m| m.request_focus(text_id));
                        }

                        if !response.has_focus() && !ui.ctx().wants_keyboard_input() {
                            ui.memory_mut(|m| m.request_focus(text_id));
                        }
                    });
                });
        });
    }
}

fn render_line(ui: &mut egui::Ui, line: &str) {
    let color = if line.starts_with("$ ") {
        TokyoNight::GREEN
    } else if line.starts_with("error:") || line.starts_with("ERROR") {
        TokyoNight::RED
    } else {
        TokyoNight::FG_DIM
    };
    ui.label(
        RichText::new(line)
            .color(color)
            .monospace(),
    );
}

fn run_shell_command(input: &str) -> Vec<String> {
    match std::process::Command::new("bash")
        .arg("-c")
        .arg(input)
        .output()
    {
        Ok(output) => {
            let mut lines = Vec::new();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stdout.lines() {
                lines.push(line.to_string());
            }
            for line in stderr.lines() {
                lines.push(format!("error: {line}"));
            }
            if lines.is_empty() && !output.status.success() {
                lines.push(format!("error: exit code {}", output.status));
            }
            lines
        }
        Err(e) => vec![format!("error: {e}")],
    }
}

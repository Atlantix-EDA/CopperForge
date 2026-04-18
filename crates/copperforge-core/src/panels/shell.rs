//! Shell panel — CopperForge command shell.
//!
//! Built-in commands for inspecting local state, plus `!<cmd>` / `sh <cmd>`
//! passthrough to the OS shell. Future: scripting hooks for gerber analysis.

use egui::RichText;
use egui_citizen::{Citizen, CitizenId, CitizenState};

use super::citizen_panel;
use crate::theme::TokyoNight;

const PROMPT: &str = "forge> ";
const INPUT_ID: &str = "shell_input";

citizen_panel!(ShellPanel, "shell");

impl ShellPanel {
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
                    let lines: Vec<String> = app.shell_log.clone();
                    for line in lines.iter() {
                        render_line(ui, line);
                    }

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            RichText::new(PROMPT)
                                .color(TokyoNight::CYAN)
                                .monospace()
                                .strong(),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut app.shell_cmd_buf)
                                .id(text_id)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .frame(false)
                                .text_color(TokyoNight::FG),
                        );

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let input = app.shell_cmd_buf.trim().to_string();
                            if !input.is_empty() {
                                if input == "clear" || input == "cls" {
                                    app.shell_log.clear();
                                } else {
                                    app.shell_log.push(format!("{PROMPT}{input}"));
                                    let output = execute_command(&input, app);
                                    for line in output {
                                        app.shell_log.push(line);
                                    }
                                }
                            }
                            app.shell_cmd_buf.clear();
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
    let color = line_color(line);
    ui.label(
        RichText::new(line)
            .color(color)
            .monospace(),
    );
}

fn execute_command(input: &str, app: &crate::CopperForgeApp) -> Vec<String> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match cmd.as_str() {
        "help" | "?" => vec![
            "Built-in commands:".into(),
            "  help         Show this help".into(),
            "  ver          Show CopperForge version".into(),
            "  info         Show host OS / CPU / memory / network".into(),
            "  status       Show current project + PCB + layers".into(),
            "  env          Show discovered KiCad + relevant env vars".into(),
            "  clear        Clear the shell".into(),
            "".into(),
            "OS commands:".into(),
            "  !<cmd>       Run an OS shell command (e.g. !uname -a)".into(),
            "  sh <cmd>     Run an OS shell command (e.g. sh ls -la)".into(),
        ],
        "ver" | "version" => version_lines(),
        "info" | "system" | "sysinfo" => info_lines(),
        "status" | "state" => status_lines(app),
        "env" => env_lines(app),
        "sh" => {
            if parts.len() > 1 {
                let shell_cmd = parts[1..].join(" ");
                run_os_command(&shell_cmd)
            } else {
                vec!["Usage: sh <command>".into()]
            }
        }
        "" => vec![],
        other => {
            if let Some(shell_cmd) = other.strip_prefix('!') {
                let full_cmd = if parts.len() > 1 {
                    format!("{} {}", shell_cmd, parts[1..].join(" "))
                } else {
                    shell_cmd.to_string()
                };
                return run_os_command(&full_cmd);
            }

            vec![format!("unknown command: {other}  (try 'help')")]
        }
    }
}

fn version_lines() -> Vec<String> {
    // Same content the Logger shows at startup — welcome banner + dependency versions.
    let mut banner = crate::platform::banner::Banner::new();
    banner.format();
    banner.message
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn info_lines() -> Vec<String> {
    let mut details = crate::platform::details::Details::new();
    details.format_os().lines().map(|s| s.to_string()).collect()
}

fn env_lines(app: &crate::CopperForgeApp) -> Vec<String> {
    let mut lines = vec!["KiCad:".into()];
    lines.push(format!(
        "  version          = {}",
        app.services.kicad_version.as_deref().unwrap_or("(not detected)")
    ));

    // Any KICAD_* env vars that are actually set — skip the hardcoded guesswork.
    let mut kicad_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("KICAD"))
        .collect();
    kicad_vars.sort_by(|a, b| a.0.cmp(&b.0));
    if kicad_vars.is_empty() {
        lines.push("  env vars         = (none set — typical when KiCad runs via Flatpak sandbox)".into());
    } else {
        for (k, v) in kicad_vars {
            lines.push(format!("  {k} = {v}"));
        }
    }

    lines.push(String::new());
    lines.push("Host:".into());
    for var in ["USER", "HOME", "SHELL", "PWD"] {
        let val = std::env::var(var).unwrap_or_else(|_| "(not set)".into());
        lines.push(format!("  {var:<16} = {val}"));
    }
    lines
}

fn status_lines(app: &crate::CopperForgeApp) -> Vec<String> {
    let mut lines = vec!["CopperForge Status:".into()];

    lines.push(format!(
        "  PCB file        = {}",
        app.services.project_state.get().pcb_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into())
    ));
    lines.push(format!(
        "  Gerber layers   = {}",
        app.services.layer_store.layers.len()
    ));
    lines.push(format!(
        "  BOM components  = {}",
        app.bom_state.as_ref().map(|s| s.entries.len()).unwrap_or(0)
    ));
    lines.push(format!(
        "  Units           = {}",
        if app.services.global_units_mils { "mils" } else { "mm" }
    ));
    lines.push(format!(
        "  KiCad           = {}",
        app.services.kicad_version.as_deref().unwrap_or("(not detected)")
    ));
    lines
}

fn run_os_command(cmd: &str) -> Vec<String> {
    match std::process::Command::new("bash")
        .arg("-c")
        .arg(cmd)
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

fn line_color(line: &str) -> egui::Color32 {
    if line.starts_with("error:") || line.starts_with("ERROR") {
        TokyoNight::RED
    } else if line.starts_with("forge>") {
        TokyoNight::CYAN
    } else if line.starts_with("  ") && line.contains('=') {
        TokyoNight::FG_DIM
    } else if line.starts_with("  ") {
        TokyoNight::FG_DIM
    } else if line.ends_with(':') {
        TokyoNight::GREEN
    } else {
        TokyoNight::FG
    }
}

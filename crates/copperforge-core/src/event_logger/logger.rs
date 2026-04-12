//! Reactive Event Logger — local replacement for egui_lens logger.
//!
//! Same public API as egui_lens::ReactiveEventLogger so existing call sites
//! compile unchanged.

use egui;
use egui_mobius_reactive::Dynamic;
use crate::event_logger::logger_colors::LogColors;

/// Log severity / category.
#[derive(Clone, PartialEq)]
pub enum LogType {
    Info,
    Warning,
    Error,
    Debug,
    Timestamp,
    System,
    UserAction,
    Config,
    Status,
    Progress,
    Success,
    Default,
    Custom(String),
}

impl std::fmt::Debug for LogType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogType::Info => write!(f, "INFO"),
            LogType::Warning => write!(f, "WARNING"),
            LogType::Error => write!(f, "ERROR"),
            LogType::Debug => write!(f, "DEBUG"),
            LogType::Timestamp => write!(f, "TIME"),
            LogType::System => write!(f, "SYSTEM"),
            LogType::UserAction => write!(f, "USER"),
            LogType::Config => write!(f, "CONFIG"),
            LogType::Status => write!(f, "STATUS"),
            LogType::Progress => write!(f, "PROGRESS"),
            LogType::Success => write!(f, "SUCCESS"),
            LogType::Default => write!(f, "DEFAULT"),
            LogType::Custom(id) => write!(f, "CUSTOM:{}", id),
        }
    }
}

/// A single log entry.
#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub log_type: LogType,
    pub message: String,
}

/// Shared logger state (stored inside `Dynamic<T>`).
#[derive(Clone)]
pub struct ReactiveEventLoggerState {
    pub show_timestamps: bool,
    pub show_log_level: bool,
    pub show_messages: bool,
    pub logs: Vec<LogEntry>,
    pub max_logs: usize,
}

impl Default for ReactiveEventLoggerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactiveEventLoggerState {
    pub fn new() -> Self {
        Self {
            show_timestamps: true,
            show_log_level: true,
            show_messages: true,
            logs: Vec::with_capacity(1000),
            max_logs: 1000,
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        if self.logs.len() >= self.max_logs {
            self.logs.remove(0);
        }
        self.logs.push(entry);
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn log_count(&self) -> usize {
        self.logs.len()
    }
}

/// The logger handle used by panels to log messages and render the log view.
pub struct ReactiveEventLogger<'a> {
    state: &'a Dynamic<ReactiveEventLoggerState>,
    colors: Option<&'a Dynamic<LogColors>>,
}

impl<'a> ReactiveEventLogger<'a> {
    #[allow(dead_code)]
    pub fn new(state: &'a Dynamic<ReactiveEventLoggerState>) -> Self {
        Self { state, colors: None }
    }

    pub fn with_colors(
        state: &'a Dynamic<ReactiveEventLoggerState>,
        colors: &'a Dynamic<LogColors>,
    ) -> Self {
        Self { state, colors: Some(colors) }
    }

    fn now_str() -> String {
        chrono::Local::now().format("%H:%M:%S%.3f").to_string()
    }

    fn add_log(&self, level: &str, content: &str) {
        let log_type = match level {
            "info" => LogType::Info,
            "warning" => LogType::Warning,
            "error" => LogType::Error,
            "debug" => LogType::Debug,
            s if s.starts_with("custom:") => LogType::Custom(s[7..].to_string()),
            _ => LogType::Default,
        };
        let entry = LogEntry {
            timestamp: Self::now_str(),
            log_type,
            message: content.to_string(),
        };
        self.state.lock().add_log(entry);
    }

    pub fn log_info(&self, content: &str) {
        self.add_log("info", content);
    }

    pub fn log_warning(&self, content: &str) {
        self.add_log("warning", content);
    }

    pub fn log_error(&self, content: &str) {
        self.add_log("error", content);
    }

    pub fn log_debug(&self, content: &str) {
        self.add_log("debug", content);
    }

    pub fn log_custom(&self, custom_type: &str, content: &str) {
        self.add_log(&format!("custom:{}", custom_type), content);
    }

    pub fn log_message(&self, content: &str) {
        self.add_log("info", content);
    }

    /// Render the log panel into the given Ui.
    pub fn show(&self, ui: &mut egui::Ui) {
        let state = self.state.get();
        let colors = self.colors.map(|c| c.get());

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(0x1a, 0x1b, 0x26))
            .inner_margin(4.0)
            .show(ui, |ui| {
                // Toolbar
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Logs: {}", state.logs.len()))
                            .color(egui::Color32::from_rgb(180, 180, 180)),
                    );
                    if ui.small_button("Clear").clicked() {
                        self.state.lock().clear_logs();
                    }
                });
                ui.separator();

                // Log entries
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &state.logs {
                            ui.horizontal(|ui| {
                                if state.show_timestamps {
                                    ui.label(
                                        egui::RichText::new(&entry.timestamp)
                                            .color(egui::Color32::from_rgb(120, 120, 120))
                                            .monospace(),
                                    );
                                }
                                if state.show_log_level {
                                    let (level_text, level_color) = self.level_display(&entry.log_type, colors.as_ref());
                                    ui.label(
                                        egui::RichText::new(level_text)
                                            .color(level_color)
                                            .monospace(),
                                    );
                                }
                                if state.show_messages {
                                    let msg_color = self.message_color(&entry.log_type, colors.as_ref());
                                    ui.label(
                                        egui::RichText::new(&entry.message)
                                            .color(msg_color)
                                            .monospace(),
                                    );
                                }
                            });
                        }
                    });
            });
    }

    fn level_display(&self, log_type: &LogType, colors: Option<&LogColors>) -> (String, egui::Color32) {
        let defaults = LogColors::default();
        let c = colors.unwrap_or(&defaults);
        match log_type {
            LogType::Info => ("[INFO]".into(), c.info_level),
            LogType::Warning => ("[WARN]".into(), c.warning_level),
            LogType::Error => ("[ERR ]".into(), c.error_level),
            LogType::Debug => ("[DBG ]".into(), c.debug_level),
            LogType::System => ("[SYS ]".into(), c.system),
            LogType::Config => ("[CONF]".into(), c.config),
            LogType::Status => ("[STAT]".into(), c.status),
            LogType::Progress => ("[PROG]".into(), c.progress),
            LogType::Success => ("[OK  ]".into(), c.success),
            LogType::Custom(id) => (format!("[{}]", id.to_uppercase()), c.get_custom_color_level(id)),
            _ => ("[    ]".into(), c.default),
        }
    }

    fn message_color(&self, log_type: &LogType, colors: Option<&LogColors>) -> egui::Color32 {
        let defaults = LogColors::default();
        let c = colors.unwrap_or(&defaults);
        match log_type {
            LogType::Info => c.info_message,
            LogType::Warning => c.warning_message,
            LogType::Error => c.error_message,
            LogType::Debug => c.debug_message,
            LogType::Custom(id) => c.get_custom_color_message(id),
            _ => c.default,
        }
    }
}

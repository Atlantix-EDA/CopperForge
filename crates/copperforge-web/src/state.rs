//! Shared application state — currently just an in-house logger
//! buffer. egui_lens would have lived here too (`Dynamic<...>` cells),
//! but until egui_mobius v0.4.0 (egui 0.34) and gerber_viewer 0.5.0
//! (egui 0.33) converge on a common egui version, this crate rolls
//! its own tiny logger instead. The data shape is intentionally close
//! to egui_lens so the swap is mechanical when versions line up.

use std::collections::VecDeque;

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    /// User-defined category (e.g. `upload`, `parse`, `export`,
    /// `origin`) — colored by the matching arm in `level_color`.
    Custom(&'static str),
}

impl LogLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERR ",
            Self::Custom(s) => s,
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            Self::Info => egui::Color32::from_rgb(150, 200, 255),
            Self::Warning => egui::Color32::from_rgb(255, 200, 80),
            Self::Error => egui::Color32::from_rgb(255, 100, 100),
            Self::Custom("upload") => egui::Color32::from_rgb(140, 220, 255),
            Self::Custom("parse") => egui::Color32::from_rgb(140, 220, 140),
            Self::Custom("export") => egui::Color32::from_rgb(255, 200, 100),
            Self::Custom("origin") => egui::Color32::from_rgb(255, 140, 140),
            Self::Custom(_) => egui::Color32::from_rgb(200, 200, 220),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: LogLevel,
    /// Wall-clock "HH:MM:SS.mmm" when the entry was appended. Built
    /// at `push` time so the buffer is render-independent.
    pub when: String,
    pub message: String,
}

/// Bounded log buffer rendered by the Logger dock tab.
pub struct Logger {
    entries: VecDeque<LogEntry>,
    max_entries: usize,
    /// Per-level visibility checkboxes for the panel toolbar. Entries
    /// stay in the buffer; filtering only affects rendering. Same
    /// idiom egui_lens uses.
    pub filter: LogFilter,
}

#[derive(Clone, Debug)]
pub struct LogFilter {
    pub show_info: bool,
    pub show_warning: bool,
    pub show_error: bool,
    pub show_custom: bool,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            show_info: true,
            show_warning: true,
            show_error: true,
            show_custom: true,
        }
    }
}

impl LogFilter {
    pub fn allows(&self, level: LogLevel) -> bool {
        match level {
            LogLevel::Info => self.show_info,
            LogLevel::Warning => self.show_warning,
            LogLevel::Error => self.show_error,
            LogLevel::Custom(_) => self.show_custom,
        }
    }
}

impl Logger {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(256),
            max_entries: 1000,
            filter: LogFilter::default(),
        }
    }

    pub fn entries(&self) -> &VecDeque<LogEntry> {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            level,
            when: now_hms(),
            message: message.into(),
        });
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Info, message);
    }
    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Warning, message);
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(LogLevel::Error, message);
    }
    pub fn custom(&mut self, category: &'static str, message: impl Into<String>) {
        self.push(LogLevel::Custom(category), message);
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

/// Toolbar verbs the panel offers. Returned by `show_log` so the
/// caller (which owns `&mut Logger`) can mutate state without us
/// reaching back through it from inside the immutable render path.
#[derive(Default, Debug, Clone, Copy)]
pub struct LogAction {
    pub clear_requested: bool,
    pub system_info_requested: bool,
}

/// Render the buffer in a scrollable, monospace pane. Toolbar with
/// System / Clear / per-level filter checkboxes mirrors the typical
/// egui_lens header. Sticks to the bottom so new entries surface as
/// they arrive.
pub fn show_log(ui: &mut egui::Ui, logger: &mut Logger) -> LogAction {
    let mut action = LogAction::default();

    // ── Toolbar row ────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Log").strong());
        ui.separator();
        if ui
            .small_button("🛈 System")
            .on_hover_text(
                "Append a one-shot snapshot of app + browser context \
                 (version, target, user agent, timezone) to the log.",
            )
            .clicked()
        {
            action.system_info_requested = true;
        }
        if ui
            .small_button("🗑 Clear")
            .on_hover_text("Empty the log buffer.")
            .clicked()
        {
            action.clear_requested = true;
        }
        ui.separator();
        ui.checkbox(&mut logger.filter.show_info, "info");
        ui.checkbox(&mut logger.filter.show_warning, "warn");
        ui.checkbox(&mut logger.filter.show_error, "err");
        ui.checkbox(&mut logger.filter.show_custom, "custom");
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(format!("{} entries", logger.entries.len()))
                        .small()
                        .color(egui::Color32::from_rgb(140, 150, 170)),
                );
            },
        );
    });
    ui.separator();

    // ── Filtered log content ──────────────────────────────────────
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in &logger.entries {
                if !logger.filter.allows(entry.level) {
                    continue;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(
                        egui::RichText::new(&entry.when)
                            .small()
                            .color(egui::Color32::from_rgb(120, 130, 145)),
                    );
                    ui.monospace(
                        egui::RichText::new(entry.level.label())
                            .small()
                            .color(entry.level.color()),
                    );
                    ui.monospace(&entry.message);
                });
            }
        });

    action
}

/// HH:MM:SS.mmm using browser performance.now() on wasm, system clock
/// elsewhere. Keeps the format stable across targets.
fn now_hms() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        // performance.now() is milliseconds since page load — fine
        // for a session log buffer. Format as a delta clock.
        let ms = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        let total_s = (ms / 1000.0) as u64;
        let h = (total_s / 3600) % 24;
        let m = (total_s / 60) % 60;
        let s = total_s % 60;
        let frac_ms = (ms as u64) % 1000;
        format!("{:02}:{:02}:{:02}.{:03}", h, m, s, frac_ms)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Native build is a stub anyway — keep this side cheap.
        String::from("--:--:--.---")
    }
}

//! Event logger — local replacement for egui_lens.
//!
//! Provides the same public API (`ReactiveEventLogger`, `ReactiveEventLoggerState`,
//! `LogColors`, `LogType`) so existing call sites compile unchanged, but built
//! against the current egui version in the workspace.

mod logger;
mod logger_colors;

pub use logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogType};
pub use logger_colors::{LogColors, Color32Wrapper};

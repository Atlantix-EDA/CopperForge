//! Logging macros for the terminal widget system
//! These macros simplify common logging patterns using the Latch pattern
//!
//! Note: This is a reference implementation that is not actively used in this example.
//! The module is kept for documentation purposes and as a reference for future development.

#![allow(dead_code, unused_macros)]

use std::ops::{Deref, DerefMut};
use std::collections::VecDeque;
use egui_mobius_reactive::Dynamic;

// Define maximum number of logs to keep in the circular buffer
const MAX_LOGS: usize = 1000;

/// A utility struct that provides automatic get/set functionality for Dynamic values
///
pub struct Latch<'a, T: Send + Clone + 'static> {
    dynamic: &'a Dynamic<T>,
    value: T,
}

impl<'a, T: Clone + Send + 'static> Latch<'a, T> {
    /// Create a new Latch for the given Dynamic value
    pub fn new(dynamic: &'a Dynamic<T>) -> Self {
        Self {
            dynamic,
            value: dynamic.get(),
        }
    }

    /// Push to back of VecDeque with circular buffer behavior
    pub fn push_back<U>(&mut self, item: U)
    where
        Self: DerefMut<Target = VecDeque<U>>,
        U: Clone,
    {
        if self.len() >= MAX_LOGS {
            self.pop_front();
        }
        self.deref_mut().push_back(item);
    }
}

impl<T: Clone + Send + 'static> Drop for Latch<'_, T> {
    fn drop(&mut self) {
        self.dynamic.set(self.value.clone());
    }
}

impl<T: Clone + Send> Deref for Latch<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Clone + Send> DerefMut for Latch<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Core macro that handles the common pattern of getting logs and pushing an entry
#[macro_export]
macro_rules! log_to_terminal {
    ($widget:expr, $log_entry:expr) => {
        {
            let mut latch = $crate::logging_macros::Latch::new(&$widget.logs);
            latch.push_back($log_entry);
        }
    };
}

/// Macro for logging timestamped messages
#[macro_export]
macro_rules! set_timestamp_log {
    // Version with default LogType::Default
    ($widget:expr, $message:expr) => {
        set_timestamp_log!($widget, $message, LogType::Default)
    };

    // Version with custom LogType
    ($widget:expr, $message:expr, $log_type:expr) => {
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let log_string = format!("{} : {}", $message, timestamp);

            // Using Latch pattern internally
            let mut latch = Latch::new(&$widget.logs);
            latch.push_back((log_string, $log_type));
        }
    };
}

#[macro_export]
/// Macro for logging slider value changes
///
/// # Arguments
///
/// * `$widget` - The terminal widget to log to
/// * `$value` - The current value of the slider to log
/// * `$log_type` - (Optional) The type of log entry to create. Defaults to `LogType::Slider`
///
/// # Examples
///
/// ```
/// let slider_value : int32 = 32; // would normally get this from the ui code
/// set_slider_log!(terminal_widget, *slider_value);
/// set_slider_log!(terminal_widget, *slider_value, LogType::Custom);
/// ```
macro_rules! set_slider_log {
    ($widget:expr, $log_string_and_value:expr) => {
        set_slider_log!($widget, $value, LogType::Slider)
    };

    ($widget:expr, $log_string_and_value:expr, $log_type:expr) => {
        {
            log_to_terminal!($widget, ($log_string_and_value, $log_type));
        }
    };
}

/// Macro for logging combo/dropdown selections
#[macro_export]
macro_rules! set_combo_log {
    ($widget:expr, $label:expr, $log_type:expr) => {
        {
            let log_string = format!("Selected: {}", $label);
            log_to_terminal!($widget, (log_string, $log_type));
        }
    };
}

/// Generic macro for any kind of log entry
#[macro_export]
macro_rules! terminal_log {
    // Simple message with default log type
    ($widget:expr, $message:expr) => {
        terminal_log!($widget, $message, LogType::Default)
    };

    // Message with custom log type
    ($widget:expr, $message:expr, $log_type:expr) => {
        log_to_terminal!($widget, ($message.to_string(), $log_type))
    };

    // Format string with arguments and default log type
    ($widget:expr, $format:expr, $($arg:tt)*) => {
        terminal_log!($widget, format!($format, $($arg)*), LogType::Default)
    };

    // Format string with arguments and custom log type
    ($widget:expr, $log_type:expr, $format:expr, $($arg:tt)*) => {
        terminal_log!($widget, format!($format, $($arg)*), $log_type)
    };
}

/// With-style macro that provides a block for manipulating logs using a Latch
#[macro_export]
macro_rules! with_terminal_logs {
    ($widget:expr, |$logs:ident| $block:block) => {
        {
            let mut $logs = $crate::Latch::new(&$widget.logs);
            $block
            // No need to set the logs back - Latch handles it automatically
        }
    };
}
//! Messages to logger via a composable payload
//! 
//! This module contains the messages that are sent to the logger.
//! They are used to log events in the application, and each 
//! message is customizable with a timestamp, log level, and message. 
//! 
//! The base type is WithColor, which is used to color code the different
//! constituents of the log messages. The WithColor struct holds a value
//! and a color attribute. The value is a generic type that can be any
//! type that implements the Send, Sync, Clone, and Default traits.
//! The color attribute is a Color32 type that is used to color the
//! value in the UI.
//! 
//! Overall this methodology is based on experience with customizing 
//! loggers in egui, and is used to create a composable payload that can be
//! easily extended or modified in the future.
//!
//! Note: While this file contains a complete implementation of a customizable
//! logging system, it is provided as a reference and is not actively used in
//! this example application. The warning about dead code is suppressed to
//! maintain this file as a complete reference implementation.
//!

#![allow(dead_code)]

pub const SOFT_GREEN : egui::Color32 = egui::Color32::from_rgb(150, 255, 150); 
/// **WithColor**
///
/// The WithColor struct holds a value and a color attribute.
/// The value is a generic type that can be any type that implements
/// the Send, Sync, Clone, and Default traits. The color attribute
/// is a Color32 type that is used to color the value in the UI.
/// The WithColor struct is used to color code the different constituents
/// of the log messages.
#[derive(Clone)]
pub struct WithColor <T : Send + Sync + Clone + Default> {
    pub value : T, 
    pub color : egui::Color32, 
}

impl<T: Send + Sync + Clone + Default> Default for WithColor<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            color: SOFT_GREEN,
        }
    }
}

impl From <WithColor<String>> for String {
    fn from(with_color: WithColor<String>) -> Self {
        with_color.value
    }
}
impl From <WithColor<String>> for egui::Color32 {
    fn from(with_color: WithColor<String>) -> Self {
        with_color.color
    }
}

/// Timestamp struct
/// 
/// The timestamp struct holds a timestamp value and a color
/// attribute. The timestamp value is updated by calling the
/// update function. 
#[derive(Clone)]
pub struct Timestamp {
    pub value : WithColor::<String>, 
}

impl Timestamp {
    pub fn new() -> Self {
        Self {
            value: WithColor::<String>::default(),
        }
    }
    pub fn update (&mut self) {
        let now = chrono::Local::now();
        self.value = WithColor::<String> {
            value: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            color: SOFT_GREEN,
        };
    }
}

/// LogLevel
/// 
/// The LogLevel struct holds the different log levels
/// and their respective colors, and is part of the overall
/// Message sent from Ui Widgets or Ui Runtime to the Logger. 
/// The log levels are Info, Debug, Warning, and Error. 
/// The colors are used to color code the
/// log levels in the UI via the use of the WithColor struct. 
#[derive(Clone)]
pub struct LogLevel {
    pub info    : WithColor::<String>, 
    pub debug   : WithColor::<String>, 
    pub warning : WithColor::<String>, 
    pub error   : WithColor::<String>, 
}

impl LogLevel {
    pub fn new() -> Self {
        Self {
            info    : WithColor::<String>::default(),
            debug   : WithColor::<String>::default(),
            warning : WithColor::<String>::default(),
            error   : WithColor::<String>::default(),
        }
    }
}

/// LogMessage
/// 
/// The LogMessage struct holds the message to be logged
/// and the color attribute. The message is the final constituent
/// of the log message, and is used to display the message
/// in the UI. The color attribute is used to color code
/// the message in the UI via the WithColor struct.
#[derive(Clone)]
pub struct LogMessage {
    pub content : WithColor::<String>,
}

impl LogMessage {
    pub fn new() -> Self {
        Self {
            content: WithColor::<String>::default(),
        }
    }
}


/// LoggerPayload 
/// 
/// The LoggerPayload struct holds the payload that is sent
/// to the logger. The payload is used to log events in the
/// application, and is used to display the log messages
/// in the UI. The payload is a composite of the different
/// constituents of the log message, namely: 
/// - Timestamp
/// - LogLevel
/// - LogMessage
/// 
/// The LoggerPayload comes with a builder pattern that allows for
/// easy customization of the payload. The builder pattern
/// allows for easy addition of new messages in the future.
/// 
#[derive(Clone)]
pub struct LoggerPayload {
    pub timestamp   : Timestamp, 
    pub log_level   : LogLevel, 
    pub log_message : LogMessage, 
}

impl LoggerPayload {
    /// Create a new logger payload with default values
    pub fn new() -> Self {
        Self {
            timestamp: Timestamp::new(),
            log_level: LogLevel::new(),
            log_message: LogMessage::new(),
        }
    }

    /// Update the timestamp to the current time
    pub fn update_timestamp(&mut self) {
        self.timestamp.update();
    }
    
    /// Update the log level
    pub fn with_log_level(&mut self, level: LogLevel) -> &mut Self {
        self.log_level = level;
        self
    }
    
    /// Set the log level to info
    pub fn info(&mut self) -> &mut Self {
        self.log_level.info.value = "INFO".to_string();
        self
    }
    
    /// Set the log level to debug
    pub fn debug(&mut self) -> &mut Self {
        self.log_level.debug.value = "DEBUG".to_string();
        self
    }
    
    /// Set the log level to warning
    pub fn warning(&mut self) -> &mut Self {
        self.log_level.warning.value = "WARNING".to_string();
        self
    }
    
    /// Set the log level to error
    pub fn error(&mut self) -> &mut Self {
        self.log_level.error.value = "ERROR".to_string();
        self
    }
    
    /// Update the log message
    pub fn with_log_message(&mut self, message: LogMessage) -> &mut Self {
        self.log_message = message;
        self
    }
    
    /// Set the message content
    pub fn message(&mut self, content: String) -> &mut Self {
        self.log_message.content.value = content;
        self
    }
    
    /// Create a message-only payload (with auto-generated timestamp and no log level)
    pub fn as_message_only(&mut self) -> &mut Self {
        // Clear log level values
        self.log_level.info.value = "".to_string();
        self.log_level.debug.value = "".to_string();
        self.log_level.warning.value = "".to_string();
        self.log_level.error.value = "".to_string();
        
        self
    }
    
    /// Set the content of a message-only payload
    pub fn with_content(&mut self, content: &str) -> &mut Self {
        self.log_message.content.value = content.to_string();
        self
    }
    
    /// Set custom colors for the payload components
    pub fn with_colors(&mut self, timestamp_color: egui::Color32, level_color: egui::Color32, message_color: egui::Color32) -> &mut Self {
        self.timestamp.value.color = timestamp_color;
        
        // Apply level color to all log level variants
        self.log_level.info.color = level_color;
        self.log_level.debug.color = level_color;
        self.log_level.warning.color = level_color;
        self.log_level.error.color = level_color;
        
        self.log_message.content.color = message_color;
        self
    }

    /// Update timestamp and prepare the payload for sending
    pub fn update(&mut self) -> &mut Self {
        // Update the timestamp
        self.update_timestamp();
        self
    }
    
    /// Compose the payload and prepare it for sending
    pub fn compose(&mut self) {
        // Send the payload to the logger
        // This is where the payload is sent to the logger
        // and displayed in the UI.
        self.update_timestamp();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn test_with_color_default() {
        let with_color = WithColor::<String>::default();
        assert_eq!(with_color.value, String::default());
        assert_eq!(with_color.color, SOFT_GREEN);
    }

    #[test]
    fn test_with_color_from_impls() {
        let test_value = "test message".to_string();
        let test_color = egui::Color32::RED;
        let with_color = WithColor {
            value: test_value.clone(),
            color: test_color,
        };

        let extracted_string: String = with_color.into();
        assert_eq!(extracted_string, test_value);

        let with_color = WithColor {
            value: test_value.clone(),
            color: test_color,
        };
        let extracted_color: egui::Color32 = with_color.into();
        assert_eq!(extracted_color, test_color);
    }

    #[test]
    fn test_timestamp_update() {
        let mut timestamp = Timestamp::new();
        timestamp.update();
        
        // Check that timestamp format matches expected pattern
        let now = Local::now();
        let expected_format = now.format("%Y-%m-%d %H:%M:%S").to_string();
        
        // The timestamp should match the format, though not necessarily the exact value
        // since there might be small time differences
        assert!(!timestamp.value.value.is_empty());
        assert_eq!(timestamp.value.value.len(), expected_format.len());
        
        // Timestamp should have the default color
        assert_eq!(timestamp.value.color, SOFT_GREEN);
    }

    #[test]
    fn test_loglevel_new() {
        let log_level = LogLevel::new();
        assert_eq!(log_level.info.value, String::default());
        assert_eq!(log_level.debug.value, String::default());
        assert_eq!(log_level.warning.value, String::default());
        assert_eq!(log_level.error.value, String::default());
        
        assert_eq!(log_level.info.color, SOFT_GREEN);
        assert_eq!(log_level.debug.color, SOFT_GREEN);
        assert_eq!(log_level.warning.color, SOFT_GREEN);
        assert_eq!(log_level.error.color, SOFT_GREEN);
    }

    #[test]
    fn test_logmessage_new() {
        let log_message = LogMessage::new();
        assert_eq!(log_message.content.value, String::default());
        assert_eq!(log_message.content.color, SOFT_GREEN);
    }

    #[test]
    fn test_loggerpayload_new() {
        let payload = LoggerPayload::new();
        
        // Check timestamp
        assert_eq!(payload.timestamp.value.value, String::default());
        assert_eq!(payload.timestamp.value.color, SOFT_GREEN);
        
        // Check log levels (just checking info as representative)
        assert_eq!(payload.log_level.info.value, String::default());
        assert_eq!(payload.log_level.info.color, SOFT_GREEN);
        
        // Check log message
        assert_eq!(payload.log_message.content.value, String::default());
        assert_eq!(payload.log_message.content.color, SOFT_GREEN);
    }

    #[test]
    fn test_loggerpayload_update_timestamp() {
        let mut payload = LoggerPayload::new();
        payload.update_timestamp();
        
        // The timestamp should not be empty after update
        assert!(!payload.timestamp.value.value.is_empty());
    }

    #[test]
    fn test_loggerpayload_with_log_level() {
        let mut payload = LoggerPayload::new();
        
        // Create a custom log level
        let mut custom_level = LogLevel::new();
        custom_level.info.value = "Custom Info".to_string();
        
        payload.with_log_level(custom_level);
        
        // Check that the log level was updated
        assert_eq!(payload.log_level.info.value, "Custom Info".to_string());
    }

    #[test]
    fn test_loggerpayload_with_log_message() {
        let mut payload = LoggerPayload::new();
        
        // Create a custom log message
        let mut custom_message = LogMessage::new();
        custom_message.content.value = "Test message".to_string();
        
        payload.with_log_message(custom_message);
        
        // Check that the log message was updated
        assert_eq!(payload.log_message.content.value, "Test message".to_string());
    }

    #[test]
    fn test_loggerpayload_compose() {
        let mut payload = LoggerPayload::new();
        
        // Initially timestamp should be empty
        assert_eq!(payload.timestamp.value.value, String::default());
        
        // After compose, timestamp should be updated
        payload.compose();
        assert!(!payload.timestamp.value.value.is_empty());
    }
}
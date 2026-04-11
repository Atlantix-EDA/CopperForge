use egui::Color32;
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

/// LogColors configures the colors for different log types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LogColors {
    #[serde(with = "color32_serde")]
    pub info_level: Color32,
    #[serde(with = "color32_serde")]
    pub warning_level: Color32,
    #[serde(with = "color32_serde")]
    pub error_level: Color32,
    #[serde(with = "color32_serde")]
    pub debug_level: Color32,

    #[serde(with = "color32_serde")]
    pub info_message: Color32,
    #[serde(with = "color32_serde")]
    pub warning_message: Color32,
    #[serde(with = "color32_serde")]
    pub error_message: Color32,
    #[serde(with = "color32_serde")]
    pub debug_message: Color32,

    // Legacy fields
    #[serde(with = "color32_serde")]
    pub info: Color32,
    #[serde(with = "color32_serde")]
    pub warning: Color32,
    #[serde(with = "color32_serde")]
    pub error: Color32,
    #[serde(with = "color32_serde")]
    pub debug: Color32,

    #[serde(with = "color32_serde")]
    pub timestamp: Color32,
    #[serde(with = "color32_serde")]
    pub system: Color32,
    #[serde(with = "color32_serde")]
    pub user_action: Color32,
    #[serde(with = "color32_serde")]
    pub config: Color32,
    #[serde(with = "color32_serde")]
    pub status: Color32,
    #[serde(with = "color32_serde")]
    pub progress: Color32,
    #[serde(with = "color32_serde")]
    pub success: Color32,
    #[serde(with = "color32_serde")]
    pub default: Color32,

    #[serde(default)]
    pub custom_colors: HashMap<String, Color32Wrapper>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Color32Wrapper {
    #[serde(with = "color32_serde")]
    pub level_color: Color32,
    #[serde(with = "color32_serde")]
    pub message_color: Color32,
}

impl Default for Color32Wrapper {
    fn default() -> Self {
        Self {
            level_color: Color32::from_rgb(200, 200, 200),
            message_color: Color32::from_rgb(255, 255, 255),
        }
    }
}

impl Default for LogColors {
    fn default() -> Self {
        let info_level = Color32::from_rgb(150, 255, 150);
        let warning_level = Color32::from_rgb(255, 255, 100);
        let error_level = Color32::from_rgb(255, 100, 100);
        let debug_level = Color32::from_rgb(150, 150, 255);

        let info_message = Color32::from_rgb(180, 255, 180);
        let warning_message = Color32::from_rgb(255, 255, 140);
        let error_message = Color32::from_rgb(255, 140, 140);
        let debug_message = Color32::from_rgb(180, 180, 255);

        Self {
            info_level,
            warning_level,
            error_level,
            debug_level,
            info_message,
            warning_message,
            error_message,
            debug_message,
            info: info_level,
            warning: warning_level,
            error: error_level,
            debug: debug_level,
            timestamp: Color32::from_rgb(180, 180, 180),
            system: Color32::from_rgb(100, 200, 255),
            user_action: Color32::from_rgb(255, 180, 100),
            config: Color32::from_rgb(200, 150, 255),
            status: Color32::from_rgb(200, 200, 200),
            progress: Color32::from_rgb(100, 255, 200),
            success: Color32::from_rgb(100, 255, 100),
            default: Color32::from_rgb(255, 255, 255),
            custom_colors: HashMap::new(),
        }
    }
}

impl LogColors {
    pub fn get_custom_color_level(&self, identifier: &str) -> Color32 {
        self.custom_colors.get(identifier).map_or(self.default, |w| w.level_color)
    }

    pub fn get_custom_color_message(&self, identifier: &str) -> Color32 {
        self.custom_colors.get(identifier).map_or(self.default, |w| w.message_color)
    }

    pub fn get_custom_color(&self, identifier: &str) -> Color32 {
        self.get_custom_color_level(identifier)
    }

    pub fn set_custom_color(&mut self, identifier: &str, color: Color32) {
        self.custom_colors.insert(identifier.to_string(), Color32Wrapper {
            level_color: color,
            message_color: color,
        });
    }

    pub fn set_custom_colors(&mut self, identifier: &str, level_color: Color32, message_color: Color32) {
        self.custom_colors.insert(identifier.to_string(), Color32Wrapper {
            level_color,
            message_color,
        });
    }

    #[allow(dead_code)]
    pub fn load() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("copperforge");
        let config_path = config_dir.join("log_colors.json");

        match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) {
        let colors = self.clone();
        std::thread::spawn(move || {
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("copperforge");
            let _ = fs::create_dir_all(&config_dir);
            let config_path = config_dir.join("log_colors.json");
            if let Ok(json) = serde_json::to_string_pretty(&colors) {
                let _ = fs::write(&config_path, json);
            }
        });
    }
}

pub mod color32_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use egui::Color32;

    pub fn serialize<S>(color: &Color32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let rgba = [color.r(), color.g(), color.b(), color.a()];
        rgba.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Color32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let rgba = <[u8; 4]>::deserialize(deserializer)?;
        Ok(Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]))
    }
}

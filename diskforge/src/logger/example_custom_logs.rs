// Example showing how to use the new custom log types
use reactive_event_logger::{ReactiveEventLogger, LogColors};
use egui_mobius_reactive::Dynamic;

/// Example function showing how to create and use custom log types
pub fn example_custom_logs(logger: &ReactiveEventLogger) {
    // Log with standard types
    logger.log_info("This is a standard info message");
    logger.log_warning("This is a standard warning message");
    logger.log_error("This is a standard error message");
    logger.log_debug("This is a standard debug message");
    
    // Log with custom types
    logger.log_custom("network", "Connected to server on port 8080");
    logger.log_custom("database", "Executed query in 42ms");
    logger.log_custom("security", "User authentication successful");
    logger.log_custom("performance", "Rendering took 16ms");
    logger.log_custom("analytics", "Page view recorded for /dashboard");
}

/// Example function showing how to configure custom log colors
pub fn configure_custom_log_colors(colors: &mut Dynamic<LogColors>) {
    // Get mutable reference to update the colors
    let mut colors_value = colors.get();
    
    // Set colors for various custom log types
    colors_value.set_custom_color("network", egui::Color32::from_rgb(100, 149, 237)); // Cornflower blue
    colors_value.set_custom_color("database", egui::Color32::from_rgb(106, 90, 205)); // Slate blue
    colors_value.set_custom_color("security", egui::Color32::from_rgb(60, 179, 113)); // Medium sea green
    colors_value.set_custom_color("performance", egui::Color32::from_rgb(255, 165, 0)); // Orange
    colors_value.set_custom_color("analytics", egui::Color32::from_rgb(218, 112, 214)); // Orchid
    
    // You can add as many custom types as needed without modifying the enum
    colors_value.set_custom_color("http", egui::Color32::from_rgb(70, 130, 180)); // Steel blue
    colors_value.set_custom_color("websocket", egui::Color32::from_rgb(0, 139, 139)); // Dark cyan
    colors_value.set_custom_color("auth", egui::Color32::from_rgb(85, 107, 47)); // Dark olive green
    
    // Update the dynamic value
    colors.set(colors_value);
}
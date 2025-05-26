pub mod layer_controls;
pub mod orientation_panel;
pub mod drc_panel;
pub mod grid_settings;

use crate::DemoLensApp;
use egui_lens::ReactiveEventLogger;

/// Main function to show all UI panels
pub fn show_properties_panel(ctx: &egui::Context, app: &mut DemoLensApp) {
    egui::SidePanel::left("properties_panel").show(ctx, |ui| {
        // Clone the Dynamic references to avoid borrow issues
        let logger_state = app.logger_state.clone();
        let log_colors = app.log_colors.clone();
        
        layer_controls::show(ui, app, &logger_state, &log_colors);
        ui.separator();
        
        orientation_panel::show(ui, app, &logger_state, &log_colors);
        ui.separator();
        
        drc_panel::show(ui, app, &logger_state, &log_colors);
        ui.separator();
        
        grid_settings::show(ui, app, &logger_state, &log_colors);
        ui.separator();
        
        ui.heading("Event Log");
        let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);
        logger.show(ui);
    });
}
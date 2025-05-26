use crate::{DemoLensApp, constants::LOG_TYPE_GRID, grid::{get_grid_status, GridStatus}};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

pub fn show(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    ui.heading("Grid Settings");
    ui.add_space(4.0);
    if ui.checkbox(&mut app.grid_settings.enabled, "Enable Grid").changed() {
        logger.log_custom(
            LOG_TYPE_GRID,
            &format!("Grid display {}", if app.grid_settings.enabled { "enabled" } else { "disabled" })
        );
    }
    
    ui.horizontal(|ui| {
        ui.label("Grid Spacing (mils):");
        let prev_spacing = app.grid_settings.spacing_mils;
        
        // Add slider
        let slider_response = ui.add(
            egui::Slider::new(&mut app.grid_settings.spacing_mils, 1.0..=1000.0)
                .logarithmic(true)
        );
        
        // Add text input box next to slider
        let text_response = ui.add(
            egui::DragValue::new(&mut app.grid_settings.spacing_mils)
                .speed(1.0)
                .range(1.0..=1000.0)
                .suffix(" mils")
        );
        
        if slider_response.changed() || text_response.changed() {
            logger.log_custom(
                LOG_TYPE_GRID,
                &format!("Grid spacing changed from {:.1} to {:.1} mils", prev_spacing, app.grid_settings.spacing_mils)
            );
        }
    });
    
    ui.horizontal(|ui| {
        ui.label("Grid Dot Size:");
        let prev_dot_size = app.grid_settings.dot_size;
        if ui.add(egui::Slider::new(&mut app.grid_settings.dot_size, 0.5..=5.0)).changed() {
            logger.log_custom(
                LOG_TYPE_GRID,
                &format!("Grid dot size changed from {:.1} to {:.1}", prev_dot_size, app.grid_settings.dot_size)
            );
        }
    });
    
    // Show grid visibility status
    if app.grid_settings.enabled {
        let status = get_grid_status(&app.view_state, app.grid_settings.spacing_mils);
        
        match status {
            GridStatus::TooFine => {
                ui.colored_label(egui::Color32::from_rgb(255, 165, 0), 
                    egui::RichText::new("⚠ Grid too fine to display - zoom in or increase spacing").small());
            }
            GridStatus::TooCoarse => {
                ui.colored_label(egui::Color32::from_rgb(255, 165, 0), 
                    egui::RichText::new("⚠ Grid too coarse - zoom out or decrease spacing").small());
            }
            GridStatus::Visible(spacing_pixels) => {
                ui.colored_label(egui::Color32::from_rgb(0, 255, 0), 
                    egui::RichText::new(format!("✓ Grid visible (~{:.0} pixels)", spacing_pixels)).small());
            }
        }
    }
}
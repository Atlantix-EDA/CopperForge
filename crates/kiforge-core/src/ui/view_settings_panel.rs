use egui;
use egui_mobius_reactive::Dynamic;
use egui_lens::{ReactiveEventLoggerState, LogColors};

use crate::DemoLensApp;
use crate::ui;

/// View Settings Panel following the diskforge pattern with explicit lifetimes
#[allow(dead_code)]
pub struct ViewSettingsPanel<'a> {
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
}

impl<'a> ViewSettingsPanel<'a> {
    /// Create a new ViewSettingsPanel instance
    #[allow(dead_code)]
    pub fn render(
        app: &'a mut DemoLensApp,
        logger_state: &'a Dynamic<ReactiveEventLoggerState>,
        log_colors: &'a Dynamic<LogColors>,
    ) -> Self {
        Self {
            app,
            logger_state,
            log_colors,
        }
    }

    /// Render the UI for the view settings panel
    #[allow(dead_code)]
    pub fn ui(self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Layer Controls Section
            ui.heading("Layer Controls");
            ui.separator();
            ui::show_layers_panel(ui, self.app, self.logger_state, self.log_colors);
            
            ui.add_space(20.0);
            
            // Grid Settings Section
            ui.heading("Grid Settings");
            ui.separator();
            ui::show_grid_panel(ui, self.app, self.logger_state, self.log_colors);
        });
    }
}
use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(ViewSettingsPanel, "view_settings");

impl ViewSettingsPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_layers_panel(ui, app, &app.logger_state.clone(), &app.log_colors.clone());
    }
}

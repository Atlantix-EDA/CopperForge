use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(ProjectPanel, "project");

impl ProjectPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_project_panel(ui, app, &app.services.logger_state.clone(), &app.services.log_colors.clone());
    }
}

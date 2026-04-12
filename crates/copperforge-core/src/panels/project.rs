use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(ProjectPanel, "project");

impl ProjectPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        let _ = (ui, services);
        // TODO: migrate from ui::show_project_panel()
    }
}

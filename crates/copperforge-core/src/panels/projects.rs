use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;
use crate::project_manager::ProjectManagerState;

citizen_panel!(ProjectsPanel, "projects",
    manager_state: Option<ProjectManagerState> = None
);

impl ProjectsPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        let _ = (ui, services);
        // TODO: migrate from ui::show_projects_panel()
    }
}

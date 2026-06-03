use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(ProjectsPanel, "projects");

impl ProjectsPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        // Split the app into disjoint field borrows so show_projects_panel
        // takes only its own state + shared services — no god-object. This
        // is what lets the render move into copperforge-pro.
        let logger_state = app.services.logger_state.clone();
        let log_colors = app.services.log_colors.clone();
        crate::ui::show_projects_panel(
            ui,
            &mut app.projects,
            &mut app.services,
            &logger_state,
            &log_colors,
        );
    }
}

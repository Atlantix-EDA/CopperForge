use super::citizen_panel;

citizen_panel!(ProjectsPanel, "projects",
    panel_state: crate::app::ProjectsPanelState = crate::app::ProjectsPanelState::default()
);

impl ProjectsPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        // Stored citizen: owns its ProjectsPanelState and renders over the
        // shared services only — no CopperForgeApp dependency, so the panel
        // is fully self-contained.
        let logger_state = services.logger_state.clone();
        let log_colors = services.log_colors.clone();
        crate::ui::show_projects_panel(
            ui,
            &mut self.panel_state,
            services,
            &logger_state,
            &log_colors,
        );
    }
}

impl crate::dock_panel::DockPanel for ProjectsPanel {
    fn id(&self) -> &str { "projects" }
    fn title(&self) -> &str { "Projects" }
    fn ui(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        self.show(ui, services);
    }
}

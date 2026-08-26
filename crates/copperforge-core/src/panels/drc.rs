use super::citizen_panel;

citizen_panel!(DrcPanel, "drc");

impl DrcPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_drc_panel(ui, app, &app.services.logger_state.clone(), &app.services.log_colors.clone());
    }
}

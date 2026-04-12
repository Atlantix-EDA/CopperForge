use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(BomPanel, "bom");

impl BomPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_bom_panel(ui, app, &app.logger_state.clone(), &app.log_colors.clone());
    }
}

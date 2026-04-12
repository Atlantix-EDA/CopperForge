use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(PcbFilePanel, "pcb_file");

impl PcbFilePanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_pcb_file_panel(ui, app, &app.logger_state.clone(), &app.log_colors.clone());
    }
}

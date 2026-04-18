use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(BomPanel, "bom",
    state: Option<crate::ui::BomPanelState> = None
);

impl BomPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        crate::ui::show_bom_panel(ui, &mut self.state, services);
    }
}

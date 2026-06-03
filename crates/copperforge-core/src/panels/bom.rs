use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(BomPanel, "bom");

impl BomPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        // BOM state now lives in `services.bom_state` (shared so the
        // Projects panel can read/write it). Clone the cell handle to
        // detach from the `services` borrow, lock it, and hand the inner
        // `&mut Option<BomPanelState>` to the unchanged renderer.
        let cell = services.bom_state.clone();
        let mut guard = cell.lock();
        crate::ui::show_bom_panel(ui, &mut guard, services);
    }
}

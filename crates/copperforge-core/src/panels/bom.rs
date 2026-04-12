use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;
use crate::ui::BomPanelState;
use crate::project_manager::bom::BomComponent;

citizen_panel!(BomPanel, "bom",
    bom_state: Option<BomPanelState> = None,
    pending_bom_components: Option<Vec<BomComponent>> = None,
    cross_probe_slot: Option<egui_mobius::slot::Slot<BomComponent>> = None,
    cross_probe_slot_started: bool = false,
    pending_cross_probe: egui_mobius::types::Value<Option<BomComponent>> = egui_mobius::types::Value::new(None)
);

impl BomPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        crate::ui::show_bom_panel(ui, app, &app.logger_state.clone(), &app.log_colors.clone());
    }
}

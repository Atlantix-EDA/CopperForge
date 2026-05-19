use egui_citizen::{Citizen, CitizenId, CitizenState};

/// The main gerber canvas — largest panel, owns viewport interaction state.
pub struct GerberViewPanel {
    citizen_id: CitizenId,
    citizen_state: CitizenState,
}

impl GerberViewPanel {
    pub fn new(citizen_state: CitizenState) -> Self {
        Self {
            citizen_id: CitizenId::new("gerber_view"),
            citizen_state,
        }
    }

    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let _ = app;
        ui.label("Gerber View renders through legacy Tab path");
    }
}

impl Citizen for GerberViewPanel {
    fn id(&self) -> &CitizenId { &self.citizen_id }
    fn citizen_state(&self) -> &CitizenState { &self.citizen_state }
    fn citizen_state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
}

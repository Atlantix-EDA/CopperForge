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

    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        let _ = (ui, services);
        // TODO: migrate from Tab::render_gerber_view() in ui/tabs.rs
        // This is ~1300 lines including viewport, controls, ruler, zoom window.
        // Viewport interaction state (zoom_window_*, ruler_*, setting_origin_mode)
        // lives on SharedServices for now, moves here when fully wired.
    }
}

impl Citizen for GerberViewPanel {
    fn id(&self) -> &CitizenId { &self.citizen_id }
    fn state(&self) -> &CitizenState { &self.citizen_state }
    fn state_mut(&mut self) -> &mut CitizenState { &mut self.citizen_state }
}

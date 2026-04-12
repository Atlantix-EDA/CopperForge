use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(TerminalPanel, "terminal");

impl TerminalPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let _ = app;
        ui.label("Terminal panel — coming soon");
    }
}

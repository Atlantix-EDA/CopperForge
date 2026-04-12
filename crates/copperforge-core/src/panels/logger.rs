use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(LoggerPanel, "logger");

impl LoggerPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let _ = app;
        ui.label("Logger panel — coming soon");
    }
}

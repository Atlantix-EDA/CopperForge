use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(LoggerPanel, "logger");

impl LoggerPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        let _ = services;
        ui.label("Logger panel — coming soon");
    }
}

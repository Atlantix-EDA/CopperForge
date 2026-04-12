use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(ShellPanel, "shell");

impl ShellPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        let _ = app;
        ui.label("Shell panel — coming soon");
    }
}

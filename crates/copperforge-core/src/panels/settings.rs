use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(SettingsPanel, "settings");

impl SettingsPanel {
    /// Rendering logic migrates from ui/settings_panel.rs in Phase 5.
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        let _ = (ui, services);
        // TODO: migrate from ui::show_settings_panel()
    }
}

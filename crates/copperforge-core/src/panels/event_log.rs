use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(EventLogPanel, "event_log");

impl EventLogPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut crate::services::SharedServices) {
        ui.style_mut().interaction.selectable_labels = true;
        let logger = crate::event_logger::ReactiveEventLogger::with_colors(
            &services.logger_state,
            &services.log_colors,
        );
        logger.show(ui);
    }
}

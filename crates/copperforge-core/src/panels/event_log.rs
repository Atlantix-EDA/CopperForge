use egui_citizen::{Citizen, CitizenId, CitizenState};
use super::citizen_panel;

citizen_panel!(EventLogPanel, "event_log");

impl EventLogPanel {
    pub fn show(&self, ui: &mut egui::Ui, app: &mut crate::CopperForgeApp) {
        ui.style_mut().interaction.selectable_labels = true;
        let logger = crate::event_logger::ReactiveEventLogger::with_colors(
            &app.logger_state,
            &app.log_colors,
        );
        logger.show(ui);
    }
}

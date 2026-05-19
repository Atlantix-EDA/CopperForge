use eframe::egui;
use once_cell::sync::Lazy;
use crate::platform::parameters::gui::VERSION;
use crate::theme::TokyoNight;

static LOGO: Lazy<egui::Image<'static>> = Lazy::new(|| {
    egui::Image::new(egui::include_image!("../../../../assets/media/saturn-logo.png"))
        .fit_to_original_size(0.75)
        .max_size(egui::vec2(320.0, 320.0))
        .clone()
});

pub struct AboutPanel;

impl AboutPanel {
    pub fn render(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);

            ui.add(Lazy::force(&LOGO).clone());

            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("CopperForge")
                    .size(22.0)
                    .color(TokyoNight::BLUE)
                    .strong()
            );

            ui.label(
                egui::RichText::new(format!("v{}", VERSION))
                    .size(14.0)
                    .color(TokyoNight::COMMENT)
            );

            ui.add_space(8.0);

            ui.label(
                egui::RichText::new("KiCad companion for gerber processing and project management")
                    .size(13.0)
                    .color(TokyoNight::FG_DIM)
            );

            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("James Bonanno — Atlantix EDA")
                    .size(14.0)
                    .color(TokyoNight::CYAN)
                    .strong()
            );

            ui.hyperlink_to(
                egui::RichText::new("github.com/saturn77")
                    .size(12.0)
                    .color(TokyoNight::BLUE),
                "https://github.com/saturn77"
            );

            ui.add_space(16.0);

            // Compact dependency credits
            ui.label(
                egui::RichText::new("Built with egui_citizen + Tokyo Night Storm")
                    .size(11.0)
                    .color(TokyoNight::COMMENT)
            );

            ui.horizontal(|ui| {
                let total_width = ui.available_width();
                let approx_content = 250.0; // approximate width of the links
                let pad = ((total_width - approx_content) / 2.0).max(0.0);
                ui.add_space(pad);
                ui.hyperlink_to(
                    egui::RichText::new("egui_citizen").size(11.0).color(TokyoNight::BLUE),
                    "https://github.com/saturn77/egui_mobius"
                );
                ui.label(egui::RichText::new("·").color(TokyoNight::COMMENT));
                ui.hyperlink_to(
                    egui::RichText::new("egui").size(11.0).color(TokyoNight::BLUE),
                    "https://github.com/emilk/egui"
                );
                ui.label(egui::RichText::new("·").color(TokyoNight::COMMENT));
                ui.hyperlink_to(
                    egui::RichText::new("gerber-viewer").size(11.0).color(TokyoNight::BLUE),
                    "https://github.com/MakerPnP/gerber-viewer"
                );
            });

            ui.add_space(8.0);

            // 3D viewer credit — render3d is adapted from Timothy Schmidt's
            // alumina-interface (see render3d/mod.rs for the full note).
            ui.label(
                egui::RichText::new("3D viewer adapted from")
                    .size(11.0)
                    .color(TokyoNight::COMMENT)
            );
            ui.hyperlink_to(
                egui::RichText::new("alumina-interface by Timothy Schmidt (MIT)")
                    .size(11.0)
                    .color(TokyoNight::BLUE),
                "https://github.com/timschmidt/alumina-interface"
            );
        });
    }
}

use eframe::egui;
use once_cell::sync::Lazy;
use crate::platform::parameters::gui::VERSION;

static LOGO: Lazy<egui::Image<'static>> = Lazy::new(|| {
    egui::Image::new(egui::include_image!("../../../../assets/media/ForgeCopper.png"))
        .fit_to_original_size(0.75)
        .max_size(egui::vec2(281.25, 225.0))
        .clone()
});

pub struct AboutPanel;

impl AboutPanel {
    /// Render a dependency link with optional author credit
    fn render_dependency(ui: &mut egui::Ui, name: &str, url: &str, author: Option<&str>) {
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.hyperlink_to(
                    egui::RichText::new(name)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(100, 150, 255)),
                    url
                );
                if let Some(author_name) = author {
                    ui.label(
                        egui::RichText::new(format!(" ({})", author_name))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(150, 150, 150))
                    );
                }
            });
        });
    }

    pub fn render(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            
            // Create a container with fixed width matching the image size
            let image_width = 150.0; 
            egui::Frame::new()
                .show(ui, |ui| {
                    ui.set_width(image_width);
                    ui.vertical_centered(|ui| {
                        // Display CopperForge logo
                        ui.add(Lazy::force(&LOGO).clone());
                        
                        ui.add_space(10.0);
                        
                        ui.label(
                            egui::RichText::new(format!("version {}", VERSION))
                            .color(egui::Color32::from_rgb(150, 150, 150))
                            .size(16.0)
                            .strong()
                        );
                        ui.add_space(10.0);
                        
                        // Description
                        ui.label(
                            egui::RichText::new(
                                "A Modern PCB Design Tool"
                            )
                            .size(16.0)
                            .strong()
                            .italics()
                        );
                        
                        ui.add_space(10.0);
                        
                        // Dependencies section
                        ui.label(
                            egui::RichText::new("Built with:")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(150, 150, 150))
                        );

                        ui.add_space(5.0);

                        // Atlantix-EDA / saturn77
                        ui.label(
                            egui::RichText::new("Atlantix-EDA (@saturn77)")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 200))
                        );
                        Self::render_dependency(ui, "egui_mobius", "https://github.com/saturn77/egui_mobius", None);
                        Self::render_dependency(ui, "egui_lens", "https://github.com/saturn77/egui_lens", None);

                        ui.add_space(5.0);

                        // MakerPnP / hydra
                        ui.label(
                            egui::RichText::new("MakerPnP (@hydra)")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 200))
                        );
                        Self::render_dependency(ui, "gerber-viewer", "https://github.com/MakerPnP/gerber-viewer", None);
                        Self::render_dependency(ui, "gerber_types", "https://github.com/MakerPnP/gerber-types", None);
                        Self::render_dependency(ui, "gerber_parser", "https://github.com/MakerPnP/gerber-parser", None);

                        ui.add_space(5.0);

                        // emilk
                        ui.label(
                            egui::RichText::new("@emilk")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 200))
                        );
                        Self::render_dependency(ui, "egui", "https://github.com/emilk/egui", None);

                        ui.add_space(5.0);

                        // jannistpl
                        ui.label(
                            egui::RichText::new("@jannistpl")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 200))
                        );
                        Self::render_dependency(ui, "egui-file-dialog", "https://github.com/jannistpl/egui-file-dialog", None);

                        ui.add_space(5.0);

                        // Adanos020
                        ui.label(
                            egui::RichText::new("@Adanos020")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 200, 200))
                        );
                        Self::render_dependency(ui, "egui_dock", "https://github.com/Adanos020/egui_dock", None);
                        
                    });
                });
        });
    }
}
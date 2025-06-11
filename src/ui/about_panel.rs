use eframe::egui;
use once_cell::sync::Lazy;
use crate::platform::parameters::gui::VERSION;

static LOGO: Lazy<egui::Image<'static>> = Lazy::new(|| {
    egui::Image::new(egui::include_image!("../../assets/media/KiForgeLogo.png"))
        .fit_to_original_size(0.75)
        .max_size(egui::vec2(281.25, 225.0))
        .clone()
});

pub struct AboutPanel;

impl AboutPanel {
    pub fn render(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            
            // Create a container with fixed width matching the image size
            let image_width = 150.0; 
            egui::Frame::new()
                .show(ui, |ui| {
                    ui.set_width(image_width);
                    ui.vertical_centered(|ui| {
                        // Display KiForge logo
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
                        
                        // Author credits with hyperlinks (vertical layout, centered)
                        ui.label(
                            egui::RichText::new("Built with:")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(150, 150, 150))
                        );
                        
                        ui.add_space(5.0);
                        
                        // egui credit
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(
                                egui::RichText::new("egui")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(100, 150, 255)),
                                "https://github.com/emilk/egui"
                            );
                            });
                            
                        });
                        
                        // egui_mobius credit  
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(
                                    egui::RichText::new("egui_mobius")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(100, 150, 255)),
                                    "https://github.com/saturn77/egui_mobius"
                                );
                                ui.label(
                                    egui::RichText::new(" (@saturn77)")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(150, 150, 150))
                                );
                            });
                        });
                        
                        // gerber-viewer credit
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                ui.hyperlink_to(
                                    egui::RichText::new("gerber-viewer")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(100, 150, 255)),
                                    "https://github.com/MakerPnP/gerber-viewer"
                                );
                                ui.label(
                                    egui::RichText::new(" (@MakerPnP)")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(150, 150, 150))
                                );
                            });
                        });
                        
                    });
                });
        });
    }
}
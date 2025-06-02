use eframe::egui;
use once_cell::sync::Lazy;

static LOGO: Lazy<egui::Image<'static>> = Lazy::new(|| {
    egui::Image::new(egui::include_image!("../../src/assets/saturn_rocket_company.png"))
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
                        // Display Saturn Rocket Company logo
                        ui.add(Lazy::force(&LOGO).clone());
                        
                        ui.add_space(10.0);
                        ui.heading("Alpha SD Formatter");
                        ui.add_space(10.0);
                        
                        ui.label("Version: 0.1.0");
                        ui.add_space(10.0);
                        
                        // Add Saturn Rocket Company credit
                        ui.label(
                            egui::RichText::new(
                                "An Enhanced SD Card Formatter by Saturn Rocket Company"
                            )
                            .size(12.0)
                            .strong()
                            .color(egui::Color32::from_rgb(0, 200, 255))
                        );
                        
                    });
                });
        });
    }
}
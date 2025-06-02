use eframe::egui;
use egui::Vec2;

pub struct SDPanel {
    selected_option  : usize,
    volume_label     : String,
    is_formatted     : bool,
}

impl SDPanel {
    pub fn new(selected_option: usize, volume_label: &str, is_formatted: bool) -> Self {
        Self {
            selected_option,
            volume_label: volume_label.to_string(),
            is_formatted,
        }
    }

    pub fn render(ui: &mut egui::Ui, selected_option: &usize, volume_label: &str, is_formatted: &bool) {
        let panel = Self::new(*selected_option, volume_label, *is_formatted);
        panel.ui(ui);
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("SD Card Visualization");
            ui.add_space(20.0);
            
            // SD Card visualization
            let (rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 200.0), egui::Sense::hover());
            
            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                
                // Draw SD card outline
                let card_color = egui::Color32::from_rgb(100, 100, 100);
                let border_color = egui::Color32::from_rgb(80, 80, 80);
                
                // Draw border as filled rectangle behind main card
                let border_rect = rect.expand(2.0);
                painter.rect_filled(
                    border_rect,
                    egui::CornerRadius::same(12),
                    border_color,
                );
                
                // Main card body
                painter.rect_filled(
                    rect,
                    egui::CornerRadius::same(10),
                    card_color,
                );
                
                // Draw notch in top right
                let notch_width = 40.0;
                let notch_height = 15.0;
                let notch_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - notch_width, rect.top()),
                    egui::vec2(notch_width, notch_height),
                );
                painter.rect_filled(
                    notch_rect,
                    egui::CornerRadius::ZERO,
                    ui.visuals().window_fill,
                );
                
                // Draw metal contacts
                let contact_color = egui::Color32::from_rgb(180, 180, 180);
                let contact_height = 8.0;
                let contact_gap = 4.0;
                let contact_width = 25.0;
                let contacts_start_x = rect.left() + 30.0;
                
                for i in 0..8 {
                    let y_pos = rect.top() + 40.0 + (i as f32) * (contact_height + contact_gap);
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(contacts_start_x, y_pos),
                            egui::vec2(contact_width, contact_height),
                        ),
                        egui::CornerRadius::same(1),
                        contact_color,
                    );
                }
                
                // Draw SD logo
                let sd_logo_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 70.0, rect.bottom() - 50.0),
                    egui::vec2(40.0, 30.0),
                );
                painter.rect_filled(
                    sd_logo_rect,
                    egui::CornerRadius::same(4),
                    egui::Color32::from_rgb(200, 200, 200),
                );
                
                // Add text "SD" inside logo
                painter.text(
                    sd_logo_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "SD",
                    egui::FontId::proportional(14.0),
                    egui::Color32::BLACK,
                );
                
                // Draw filesystem type based on selection
                let fs_text = match self.selected_option {
                    0 => "FAT32",
                    1 => "exFAT",
                    _ => "",
                };
                
                // Choose text color based on filesystem type
                let fs_color = match self.selected_option {
                    0 => egui::Color32::from_rgb(100, 200, 255), // FAT32 color
                    1 => egui::Color32::from_rgb(150, 150, 255), // exFAT color
                    _ => egui::Color32::WHITE,
                };
                
                // Draw filesystem text in center of card
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y - 20.0),
                    egui::Align2::CENTER_CENTER,
                    fs_text,
                    egui::FontId::proportional(24.0),
                    fs_color,
                );
                
                // Add capacity text
                let capacity_text = match self.selected_option {
                    0 => "Max file: 4GB",
                    1 => "Max file: 16EB",
                    _ => "",
                };
                
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 10.0),
                    egui::Align2::CENTER_CENTER,
                    capacity_text,
                    egui::FontId::proportional(14.0),
                    egui::Color32::LIGHT_GRAY,
                );
                
                // Add volume label
                let volume_label_text = format!("Label: {}", self.volume_label);
                
                painter.text(
                    egui::pos2(rect.center().x, rect.center().y + 40.0),
                    egui::Align2::CENTER_CENTER,
                    volume_label_text,
                    egui::FontId::proportional(16.0),
                    egui::Color32::from_rgb(220, 220, 120),
                );
                
                // Add formatted indicator if SD card is formatted
                if self.is_formatted {
                    // Draw a checkmark badge or "FORMATTED" ribbon
                    let formatted_badge_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 120.0, rect.top() + 15.0),
                        egui::vec2(100.0, 30.0),
                    );
                    
                    // Draw badge background
                    painter.rect_filled(
                        formatted_badge_rect,
                        egui::CornerRadius::same(15),
                        egui::Color32::from_rgb(40, 180, 80), // Green background
                    );
                    
                    // Draw checkmark and text
                    painter.text(
                        formatted_badge_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "✓ FORMATTED",
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }
            }
            
            ui.add_space(20.0);
            ui.label("The visualization updates based on settings in the Settings panel.");
        });
    }
}
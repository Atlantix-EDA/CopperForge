use crate::DemoLensApp;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

pub fn show_pcb_layer_panel(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    ui.heading("PCB Layers");
    
    if let Some(pcb) = &app.pcb_data {
        let mut sorted_layers: Vec<_> = pcb.layers.values().collect();
        sorted_layers.sort_by_key(|layer| layer.id);
        
        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                for layer in sorted_layers {
                    let mut visible = app.active_pcb_layers.contains(&layer.name);
                    let prev_visible = visible;
                    
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut visible, &layer.name).changed() {
                            if visible && !prev_visible {
                                if !app.active_pcb_layers.contains(&layer.name) {
                                    app.active_pcb_layers.push(layer.name.clone());
                                    logger.log_info(&format!("Enabled layer: {}", layer.name));
                                }
                            } else if !visible && prev_visible {
                                app.active_pcb_layers.retain(|name| name != &layer.name);
                                logger.log_info(&format!("Disabled layer: {}", layer.name));
                            }
                        }
                        
                        // Show layer type in smaller text
                        ui.small(&format!("({})", layer.layer_type));
                        
                        // Show layer color indicator
                        let color = get_layer_color(&layer.name);
                        ui.add_space(4.0);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                    });
                }
            });
        
        ui.separator();
        
        // Quick action buttons
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                app.active_pcb_layers = pcb.layers.values().map(|l| l.name.clone()).collect();
                logger.log_info("Enabled all PCB layers");
            }
            
            if ui.button("None").clicked() {
                app.active_pcb_layers.clear();
                logger.log_info("Disabled all PCB layers");
            }
            
            if ui.button("Copper").clicked() {
                app.active_pcb_layers.clear();
                for (_, layer) in &pcb.layers {
                    if layer.name.contains(".Cu") {
                        app.active_pcb_layers.push(layer.name.clone());
                    }
                }
                logger.log_info("Showing only copper layers");
            }
        });
    } else {
        ui.label("No PCB loaded");
    }
}

fn get_layer_color(layer_name: &str) -> egui::Color32 {
    match layer_name {
        "F.Cu" => egui::Color32::from_rgb(200, 50, 50),      // Front copper - red
        "B.Cu" => egui::Color32::from_rgb(50, 50, 200),      // Back copper - blue
        "F.SilkS" => egui::Color32::from_rgb(200, 200, 200), // Front silk - light gray
        "B.SilkS" => egui::Color32::from_rgb(150, 150, 150), // Back silk - gray
        "F.Mask" => egui::Color32::from_rgba_premultiplied(50, 200, 50, 100), // Front mask - green
        "B.Mask" => egui::Color32::from_rgba_premultiplied(50, 150, 50, 100), // Back mask - dark green
        "Edge.Cuts" => egui::Color32::from_rgb(255, 255, 0), // Edge cuts - yellow
        "F.Paste" => egui::Color32::from_rgba_premultiplied(128, 128, 128, 100), // Front paste
        "B.Paste" => egui::Color32::from_rgba_premultiplied(100, 100, 100, 100), // Back paste
        _ if layer_name.contains("Cu") => egui::Color32::from_rgb(150, 100, 50), // Other copper
        _ => egui::Color32::from_rgb(100, 100, 100), // Default gray
    }
}
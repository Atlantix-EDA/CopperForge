use crate::{DemoLensApp, layers::LayerType};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use eframe::emath::Vec2;

pub fn show(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    ui.heading("Layer Controls");
    ui.separator();
    
    // Layer visibility controls
    ui.label(&format!("Visible Layers (Showing {} side):", if app.showing_top { "TOP" } else { "BOTTOM" }));
    ui.add_space(4.0);
    
    // Quick controls
    ui.horizontal(|ui| {
        if ui.button("Show All").clicked() {
            for layer_info in app.layers.values_mut() {
                layer_info.visible = true;
            }
            logger.log_info("All layers shown");
        }
        if ui.button("Hide All").clicked() {
            for layer_info in app.layers.values_mut() {
                layer_info.visible = false;
            }
            logger.log_info("All layers hidden");
        }
    });
    ui.add_space(4.0);
    
    for layer_type in LayerType::all() {
        if let Some(layer_info) = app.layers.get_mut(&layer_type) {
            // Only show relevant layers based on showing_top
            let show_control = layer_type.should_render(app.showing_top) || 
                              layer_type == LayerType::MechanicalOutline;
            
            if show_control {
                ui.horizontal(|ui| {
                    let was_visible = layer_info.visible;
                    ui.checkbox(&mut layer_info.visible, "");
                    
                    // Color indicator box
                    let (_, rect) = ui.allocate_space(Vec2::new(20.0, 16.0));
                    ui.painter().rect_filled(rect, 2.0, layer_type.color());
                    
                    ui.label(layer_type.display_name());
                    
                    if was_visible != layer_info.visible {
                        logger.log_info(&format!("{} layer {}", 
                            layer_type.display_name(),
                            if layer_info.visible { "shown" } else { "hidden" }
                        ));
                    }
                });
            }
        }
    }
    
    ui.add_space(8.0);
    ui.separator();
    ui.label("Board: CMOD S7 (PCBWAY)");
    ui.label("Each layer loaded from separate gerber file.");
    ui.label("Different colors help distinguish layers.");
}
use crate::{DemoLensApp, project::constants::{LOG_TYPE_ROTATION, LOG_TYPE_MIRROR, LOG_TYPE_CENTER_OFFSET, LOG_TYPE_DESIGN_OFFSET}};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use crate::display::VectorOffset;

pub fn show_orientation_panel<'a>(    
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Orientation controls
    ui.horizontal(|ui| {
        if ui.button("📍 Center").clicked() {
            app.display_manager.center_offset = VectorOffset { x: 0.0, y: 0.0 };
            app.display_manager.design_offset = VectorOffset { x: 0.0, y: 0.0 };
            app.needs_initial_view = true;
            logger.log_info("Centered gerber at (0,0)");
        }
        
        if ui.button("🔄 Flip Top/Bottom").clicked() {
            app.display_manager.showing_top = !app.display_manager.showing_top;
            logger.log_info(&format!("Showing {} layers", if app.display_manager.showing_top { "top" } else { "bottom" }));
        }
    });
    
    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.display_manager.mirroring.x, "X Mirror").clicked() {
            logger.log_custom(
                LOG_TYPE_MIRROR,
                &format!("X mirroring {}", if app.display_manager.mirroring.x { "enabled" } else { "disabled" })
            );
        }
        
        if ui.checkbox(&mut app.display_manager.mirroring.y, "Y Mirror").clicked() {
            logger.log_custom(
                LOG_TYPE_MIRROR,
                &format!("Y mirroring {}", if app.display_manager.mirroring.y { "enabled" } else { "disabled" })
            );
        }
    });
    
    ui.horizontal(|ui| {
        ui.label("Rotate by");
        let prev_rotation = app.rotation_degrees;
        let mut new_rotation = app.rotation_degrees;
        if ui.add(egui::DragValue::new(&mut new_rotation).suffix("°").speed(1.0)).changed() {
            app.rotation_degrees = new_rotation;
            
            // When rotation changes, we need to trigger a view update
            // The actual rotation handling will happen in the render method
            app.needs_initial_view = true;
            
            logger.log_custom(
                LOG_TYPE_ROTATION, 
                &format!("Rotation changed from {:.1}° to {:.1}°", prev_rotation, app.rotation_degrees)
            );
        }
        ui.label("degrees");
        
        // Add reset button
        if ui.button("Reset").clicked() {
            if app.rotation_degrees != 0.0 {
                app.rotation_degrees = 0.0;
                logger.log_custom(LOG_TYPE_ROTATION, "Reset rotation to 0°");
            }
        }
    });
    
    // Quadrant View controls
    ui.separator();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.display_manager.quadrant_view_enabled, "Quadrant View").clicked() {
            logger.log_info(&format!("Quadrant view {}", 
                if app.display_manager.quadrant_view_enabled { "enabled" } else { "disabled" }));
            app.needs_initial_view = true;
        }
        
        if app.display_manager.quadrant_view_enabled {
            ui.separator();
            ui.label("Offset:");
            
            // Always store in mm internally, but display in user's preferred units
            let (mut offset_value, units_suffix, conversion_factor) = if app.global_units_mils {
                (app.display_manager.quadrant_offset_magnitude / 0.0254, "mils", 0.0254)
            } else {
                (app.display_manager.quadrant_offset_magnitude, "mm", 1.0)
            };
            
            let speed = if app.global_units_mils { 10.0 } else { 1.0 };
            let max_range = if app.global_units_mils { 20000.0 } else { 500.0 }; // Larger range for mils
            
            if ui.add(egui::DragValue::new(&mut offset_value)
                .suffix(units_suffix)
                .speed(speed)
                .range(0.0..=max_range))
                .changed() 
            {
                // Always convert to mm for internal storage
                let offset_mm = offset_value * conversion_factor;
                app.display_manager.set_quadrant_offset_magnitude(offset_mm);
                logger.log_info(&format!("Quadrant offset: {:.1} {} ({:.2} mm)", offset_value, units_suffix, offset_mm));
            }
        }
    });
    
    // Advanced offset controls (initially hidden)
    egui::CollapsingHeader::new("Advanced Offsets")
        .default_open(false)
        .show(ui, |ui| {
            ui.columns(2, |columns| {
                // Column 1: Center Offset
                columns[0].group(|ui| {
                    ui.heading("Center Offset");
                    ui.add_space(4.0);
                    
                    let mut center_changed = false;
                    let old_center_x = app.display_manager.center_offset.x;
                    let old_center_y = app.display_manager.center_offset.y;
                    
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        if ui.add(egui::DragValue::new(&mut app.display_manager.center_offset.x).speed(0.1)).changed() {
                            center_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y:");
                        if ui.add(egui::DragValue::new(&mut app.display_manager.center_offset.y).speed(0.1)).changed() {
                            center_changed = true;
                        }
                    });
                    
                    if center_changed {
                        logger.log_custom(
                            LOG_TYPE_CENTER_OFFSET,
                            &format!("Center offset changed from ({:.1}, {:.1}) to ({:.1}, {:.1})", 
                                    old_center_x, old_center_y, app.display_manager.center_offset.x, app.display_manager.center_offset.y)
                        );
                    }
                });
                
                // Column 2: Design Offset
                columns[1].group(|ui| {
                    ui.heading("Design Offset");
                    ui.add_space(4.0);
                    
                    let mut design_changed = false;
                    let old_design_x = app.display_manager.design_offset.x;
                    let old_design_y = app.display_manager.design_offset.y;
                    
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        if ui.add(egui::DragValue::new(&mut app.display_manager.design_offset.x).speed(0.1)).changed() {
                            design_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y:");
                        if ui.add(egui::DragValue::new(&mut app.display_manager.design_offset.y).speed(0.1)).changed() {
                            design_changed = true;
                        }
                    });
                    
                    if design_changed {
                        logger.log_custom(
                            LOG_TYPE_DESIGN_OFFSET,
                            &format!("Design offset changed from ({:.1}, {:.1}) to ({:.1}, {:.1})", 
                                    old_design_x, old_design_y, app.display_manager.design_offset.x, app.display_manager.design_offset.y)
                        );
                    }
                });
            });
        });
}
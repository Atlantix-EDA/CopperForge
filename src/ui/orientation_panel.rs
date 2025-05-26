use crate::{DemoLensApp, constants::{LOG_TYPE_ROTATION, LOG_TYPE_MIRROR, LOG_TYPE_CENTER_OFFSET, LOG_TYPE_DESIGN_OFFSET}};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use gerber_viewer::position::Vector;

pub fn show(ui: &mut egui::Ui, app: &mut DemoLensApp, logger_state: &Dynamic<ReactiveEventLoggerState>, log_colors: &Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    ui.heading("Orientation");
    
    // Orientation controls
    ui.horizontal(|ui| {
        if ui.button("📍 Center").clicked() {
            app.center_offset = Vector::new(0.0, 0.0);
            app.design_offset = Vector::new(0.0, 0.0);
            app.needs_initial_view = true;
            logger.log_info("Centered gerber at (0,0)");
        }
        
        if ui.button("🔄 Flip Top/Bottom").clicked() {
            app.showing_top = !app.showing_top;
            logger.log_info(&format!("Showing {} layers", if app.showing_top { "top" } else { "bottom" }));
        }
    });
    
    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.mirroring.x, "X Mirror").clicked() {
            logger.log_custom(
                LOG_TYPE_MIRROR,
                &format!("X mirroring {}", if app.mirroring.x { "enabled" } else { "disabled" })
            );
        }
        
        if ui.checkbox(&mut app.mirroring.y, "Y Mirror").clicked() {
            logger.log_custom(
                LOG_TYPE_MIRROR,
                &format!("Y mirroring {}", if app.mirroring.y { "enabled" } else { "disabled" })
            );
        }
    });
    
    ui.horizontal(|ui| {
        ui.label("Rotate by");
        let prev_rotation = app.rotation_degrees;
        if ui.add(egui::DragValue::new(&mut app.rotation_degrees).suffix("°").speed(1.0)).changed() {
            logger.log_custom(
                LOG_TYPE_ROTATION, 
                &format!("Rotation changed from {:.1}° to {:.1}°", prev_rotation, app.rotation_degrees)
            );
        }
        ui.label("degrees");
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
                    let old_center_x = app.center_offset.x;
                    let old_center_y = app.center_offset.y;
                    
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        if ui.add(egui::DragValue::new(&mut app.center_offset.x).speed(0.1)).changed() {
                            center_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y:");
                        if ui.add(egui::DragValue::new(&mut app.center_offset.y).speed(0.1)).changed() {
                            center_changed = true;
                        }
                    });
                    
                    if center_changed {
                        logger.log_custom(
                            LOG_TYPE_CENTER_OFFSET,
                            &format!("Center offset changed from ({:.1}, {:.1}) to ({:.1}, {:.1})", 
                                    old_center_x, old_center_y, app.center_offset.x, app.center_offset.y)
                        );
                    }
                });
                
                // Column 2: Design Offset
                columns[1].group(|ui| {
                    ui.heading("Design Offset");
                    ui.add_space(4.0);
                    
                    let mut design_changed = false;
                    let old_design_x = app.design_offset.x;
                    let old_design_y = app.design_offset.y;
                    
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        if ui.add(egui::DragValue::new(&mut app.design_offset.x).speed(0.1)).changed() {
                            design_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y:");
                        if ui.add(egui::DragValue::new(&mut app.design_offset.y).speed(0.1)).changed() {
                            design_changed = true;
                        }
                    });
                    
                    if design_changed {
                        logger.log_custom(
                            LOG_TYPE_DESIGN_OFFSET,
                            &format!("Design offset changed from ({:.1}, {:.1}) to ({:.1}, {:.1})", 
                                    old_design_x, old_design_y, app.design_offset.x, app.design_offset.y)
                        );
                    }
                });
            });
        });
}
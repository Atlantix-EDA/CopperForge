use crate::{DemoLensApp, layers::LayerType};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use eframe::emath::Vec2;
use egui_mobius_reactive::*; 

pub fn show_layers_panel<'a>(    ui: &mut egui::Ui, 
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Layer visibility controls
    ui.label(&format!("Visible Layers (Showing {} side):", if app.display_manager.showing_top { "TOP" } else { "BOTTOM" }));
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
            let show_control = layer_type.should_render(app.display_manager.showing_top) || 
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
    
    // Show unassigned gerbers section if any exist
    if !app.unassigned_gerbers.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.heading("Unassigned Gerber Files");
        ui.label("Please assign these files to their correct layer types:");
        ui.add_space(4.0);
        
        let mut assignments_to_make = Vec::new();
        
        for unassigned in &app.unassigned_gerbers {
            ui.horizontal(|ui| {
                ui.label(&unassigned.filename);
                ui.add_space(10.0);
                
                // Create dropdown for layer type selection
                let current_selection = app.layer_assignments.get(&unassigned.filename)
                    .copied()
                    .unwrap_or(LayerType::TopCopper); // Default selection
                
                egui::ComboBox::from_id_salt(&unassigned.filename)
                    .selected_text(current_selection.display_name())
                    .show_ui(ui, |ui| {
                        for layer_type in LayerType::all() {
                            // Check if this layer type is already assigned to another file
                            let already_assigned = app.layers.contains_key(&layer_type);
                            
                            if already_assigned {
                                ui.add_enabled(false, egui::SelectableLabel::new(
                                    false,
                                    format!("✓ {} (assigned)", layer_type.display_name())
                                ));
                            } else if ui.selectable_value(&mut assignments_to_make, vec![(unassigned.filename.clone(), layer_type)], layer_type.display_name()).clicked() {
                                assignments_to_make.push((unassigned.filename.clone(), layer_type));
                            }
                        }
                    });
            });
        }
        
        // Apply assignments
        for (filename, layer_type) in assignments_to_make {
            if let Some(unassigned_idx) = app.unassigned_gerbers.iter().position(|u| u.filename == filename) {
                let unassigned = app.unassigned_gerbers.remove(unassigned_idx);
                
                // Create layer info from unassigned gerber
                let layer_info = crate::layers::LayerInfo::new(
                    layer_type,
                    Some(unassigned.parsed_layer),
                    Some(unassigned.content),
                    true,
                );
                
                app.layers.insert(layer_type, layer_info);
                app.layer_assignments.insert(filename.clone(), layer_type);
                logger.log_info(&format!("Assigned {} to {:?}", filename, layer_type));
                app.needs_initial_view = true;
            }
        }
        
        if !app.unassigned_gerbers.is_empty() {
            ui.add_space(8.0);
            if ui.button("Auto-detect All").clicked() {
                let mut newly_assigned = Vec::new();
                
                for unassigned in &app.unassigned_gerbers {
                    if let Some(detected_type) = app.layer_detector.detect_layer_type(&unassigned.filename) {
                        if !app.layers.contains_key(&detected_type) {
                            newly_assigned.push((unassigned.filename.clone(), detected_type));
                        }
                    }
                }
                
                for (filename, layer_type) in &newly_assigned {
                    if let Some(unassigned_idx) = app.unassigned_gerbers.iter().position(|u| &u.filename == filename) {
                        let unassigned = app.unassigned_gerbers.remove(unassigned_idx);
                        
                        let layer_info = crate::layers::LayerInfo::new(
                            *layer_type,
                            Some(unassigned.parsed_layer),
                            Some(unassigned.content),
                            true,
                        );
                        
                        app.layers.insert(*layer_type, layer_info);
                        logger.log_info(&format!("Auto-detected {} as {:?}", filename, layer_type));
                        app.layer_assignments.insert(filename.clone(), *layer_type);
                    }
                }
                
                if newly_assigned.is_empty() {
                    logger.log_warning("Could not auto-detect any remaining files");
                } else {
                    app.needs_initial_view = true;
                }
            }
        }
    }
    
    ui.add_space(8.0);
    ui.separator();
    ui.label("Board: CMOD S7 (PCBWAY)");
    ui.label("Each layer loaded from separate gerber file.");
    ui.label("Different colors help distinguish layers.");
}
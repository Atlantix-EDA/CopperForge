use crate::{DemoLensApp, ecs::{LayerType, Side}};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use eframe::emath::Vec2;
use egui_mobius_reactive::*; 

pub fn show_layers_panel<'a>(    ui: &mut egui::Ui, 
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    // Layer visibility controls
    ui.label("All Gerber Layers:");
    ui.add_space(4.0);
    
    // Quick controls
    ui.horizontal(|ui| {
        // All On/Off toggle (using ECS)
        let visible_layers = crate::ecs::get_visible_layer_entities(&mut app.ecs_world);
        let total_layers = crate::ecs::get_layer_count(&mut app.ecs_world);
        let all_visible = visible_layers.len() == total_layers && total_layers > 0;
        let mut all_on = all_visible;
        if ui.checkbox(&mut all_on, "All").clicked() {
            for layer_type in LayerType::all() {
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, all_on);
            }
            logger.log_info(if all_on { "All layers shown" } else { "All layers hidden" });
            ui.ctx().request_repaint();
        }
        
        ui.separator();
        
        if ui.button("Show All").clicked() {
            for layer_type in LayerType::all() {
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, true);
            }
            logger.log_info("All layers shown");
        }
        if ui.button("Hide All").clicked() {
            for layer_type in LayerType::all() {
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, false);
            }
            logger.log_info("All layers hidden");
        }
        if ui.button("TOP").clicked() {
            for layer_type in LayerType::all() {
                let visible = match layer_type {
                    LayerType::Copper(1) | LayerType::Silkscreen(Side::Top) | LayerType::Soldermask(Side::Top) | LayerType::Paste(Side::Top) => true,
                    LayerType::Copper(_) => false,  // All other copper layers (inner/bottom)
                    LayerType::Silkscreen(Side::Bottom) | LayerType::Soldermask(Side::Bottom) | LayerType::Paste(Side::Bottom) => false,
                    LayerType::MechanicalOutline => true, // Keep outline visible
                };
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, visible);
            }
            logger.log_info("Top layers shown");
            ui.ctx().request_repaint();
        }
        if ui.button("BOTTOM").clicked() {
            for layer_type in LayerType::all() {
                let visible = match layer_type {
                    LayerType::Copper(1) | LayerType::Silkscreen(Side::Top) | LayerType::Soldermask(Side::Top) | LayerType::Paste(Side::Top) => false,
                    LayerType::Copper(_) => true,  // All other copper layers (inner/bottom)
                    LayerType::Silkscreen(Side::Bottom) | LayerType::Soldermask(Side::Bottom) | LayerType::Paste(Side::Bottom) => true,
                    LayerType::MechanicalOutline => true, // Keep outline visible
                };
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, visible);
            }
            logger.log_info("Bottom layers shown");
            ui.ctx().request_repaint();
        }
        if ui.button("ASSEMBLY").clicked() {
            for layer_type in LayerType::all() {
                let visible = match layer_type {
                    LayerType::Silkscreen(_) | LayerType::MechanicalOutline => true,
                    _ => false, // Hide copper, soldermask, and paste layers
                };
                crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, visible);
            }
            logger.log_info("Assembly layers shown (silkscreen + outline)");
            ui.ctx().request_repaint();
        }
    });
    ui.add_space(4.0);
    
    // Track actions to perform after the UI loop
    let mut show_only_layer: Option<LayerType> = None;
    let mut toggle_color_picker: Option<LayerType> = None;
    
    // Track visibility changes to apply after reading
    let mut visibility_changes = Vec::new();
    let mut color_changes = Vec::new();
    
    for layer_type in LayerType::all() {
        // Get layer data from ECS
        if let Some((_entity, _layer_info, _gerber_data, visibility)) = crate::ecs::get_layer_data(&mut app.ecs_world, layer_type) {
            let was_visible = visibility.visible;
            let current_color = crate::ecs::get_layer_render_properties(&mut app.ecs_world, layer_type)
                .map(|props| props.color)
                .unwrap_or(layer_type.color());
            
            // Show ALL layers regardless of top/bottom view
            ui.horizontal(|ui| {
                let mut current_visible = was_visible;
                ui.checkbox(&mut current_visible, "");
                
                // Track visibility changes
                if current_visible != was_visible {
                    visibility_changes.push((layer_type, current_visible));
                }
                
                // Color picker - clickable color indicator box
                let response = ui.allocate_response(Vec2::new(20.0, 16.0), egui::Sense::click());
                ui.painter().rect_filled(response.rect, 2.0, current_color);
                
                // Handle single and double clicks on color box
                if response.double_clicked() {
                    // Double-click: Show only this layer
                    show_only_layer = Some(layer_type);
                } else if response.clicked() {
                    // Single click: Show color picker popup
                    toggle_color_picker = Some(layer_type);
                }
                
                // Show color picker popup if active
                let show_picker = ui.ctx().memory(|mem| {
                    mem.data.get_temp::<bool>(egui::Id::new(format!("color_picker_{:?}", layer_type))).unwrap_or(false)
                });
                
                if show_picker {
                    egui::Window::new(format!("Color for {}", layer_type.display_name()))
                        .id(egui::Id::new(format!("color_window_{:?}", layer_type)))
                        .collapsible(false)
                        .resizable(false)
                        .show(ui.ctx(), |ui| {
                            let mut color_array = [
                                current_color.r() as f32 / 255.0,
                                current_color.g() as f32 / 255.0,
                                current_color.b() as f32 / 255.0,
                            ];
                            
                            if ui.color_edit_button_rgb(&mut color_array).changed() {
                                let new_color = egui::Color32::from_rgb(
                                    (color_array[0] * 255.0) as u8,
                                    (color_array[1] * 255.0) as u8,
                                    (color_array[2] * 255.0) as u8,
                                );
                                color_changes.push((layer_type, new_color));
                            }
                            
                            ui.horizontal(|ui| {
                                if ui.button("Reset to Default").clicked() {
                                    color_changes.push((layer_type, layer_type.color()));
                                }
                                if ui.button("Close").clicked() {
                                    ui.ctx().memory_mut(|mem| {
                                        mem.data.remove::<bool>(egui::Id::new(format!("color_picker_{:?}", layer_type)));
                                    });
                                }
                            });
                        });
                }
                
                ui.label(layer_type.display_name());
                
                if current_visible != was_visible {
                    logger.log_info(&format!("{} layer {}", 
                        layer_type.display_name(),
                        if current_visible { "shown" } else { "hidden" }
                    ));
                }
            });
        }
    }
    
    // Apply visibility changes
    for (layer_type, visible) in visibility_changes {
        crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type, visible);
    }
    
    // Apply color changes
    for (layer_type, color) in color_changes {
        crate::ecs::update_layer_render_properties(&mut app.ecs_world, layer_type, color);
    }
    
    // Handle deferred actions after the UI loop
    if let Some(target_layer) = show_only_layer {
        for layer_type_iter in LayerType::all() {
            let visible = layer_type_iter == target_layer;
            crate::ecs::set_layer_visibility(&mut app.ecs_world, layer_type_iter, visible);
        }
        logger.log_info(&format!("Showing only {} layer", target_layer.display_name()));
    }
    
    if let Some(target_layer) = toggle_color_picker {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(
                egui::Id::new(format!("color_picker_{:?}", target_layer)),
                !mem.data.get_temp::<bool>(egui::Id::new(format!("color_picker_{:?}", target_layer))).unwrap_or(false)
            );
        });
    }
    
    // Show unassigned gerbers section if any exist
    if crate::ecs::has_unassigned_gerbers(&app.ecs_world) {
        ui.add_space(8.0);
        ui.separator();
        ui.heading("Unassigned Gerber Files");
        ui.label("Please assign these files to their correct layer types:");
        ui.add_space(4.0);
        
        let mut assignments_to_make = Vec::new();
        
        for unassigned in crate::ecs::get_unassigned_gerbers(&app.ecs_world) {
            ui.horizontal(|ui| {
                ui.label(&unassigned.filename);
                ui.add_space(10.0);
                
                // Create dropdown for layer type selection
                let layer_assignments = crate::ecs::get_layer_assignments(&app.ecs_world);
                let current_selection = layer_assignments.get(&unassigned.filename)
                    .copied()
                    .unwrap_or(LayerType::Copper(1)); // Default selection
                
                egui::ComboBox::from_id_salt(&unassigned.filename)
                    .selected_text(current_selection.display_name())
                    .show_ui(ui, |ui| {
                        for layer_type in LayerType::all() {
                            // Check if this layer type is already assigned to another file
                            let already_assigned = crate::ecs::get_layer_by_type_readonly(&mut app.ecs_world, layer_type).is_some();
                            
                            if already_assigned {
                                ui.add_enabled(false, egui::Button::new(format!("✓ {} (assigned)", layer_type.display_name())));
                            } else if ui.selectable_value(&mut assignments_to_make, vec![(unassigned.filename.clone(), layer_type)], layer_type.display_name()).clicked() {
                                assignments_to_make.push((unassigned.filename.clone(), layer_type));
                            }
                        }
                    });
            });
        }
        
        // Apply assignments using ECS system
        for (filename, layer_type) in assignments_to_make {
            match crate::ecs::assign_gerber_to_layer_system(&mut app.ecs_world, filename.clone(), layer_type) {
                Ok(_entity) => {
                    logger.log_info(&format!("Assigned {} to {:?}", filename, layer_type));
                    app.needs_initial_view = true;
                }
                Err(e) => {
                    logger.log_error(&format!("Failed to assign {}: {}", filename, e));
                }
            }
        }
        
        if crate::ecs::has_unassigned_gerbers(&app.ecs_world) {
            ui.add_space(8.0);
            if ui.button("Auto-detect All").clicked() {
                // Use ECS system for auto-detection and assignment
                let newly_assigned = crate::ecs::auto_assign_gerbers_system(&mut app.ecs_world);
                
                if newly_assigned.is_empty() {
                    logger.log_warning("Could not auto-detect any remaining files");
                } else {
                    for (filename, layer_type) in newly_assigned {
                        logger.log_info(&format!("Auto-detected {} as {:?}", filename, layer_type));
                    }
                    app.needs_initial_view = true;
                }
            }
        }
    }
    
}
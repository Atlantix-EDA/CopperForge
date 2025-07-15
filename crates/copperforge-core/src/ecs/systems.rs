use bevy_ecs::prelude::*;
use crate::ecs::components::*;
use crate::ecs::resources::*;
use gerber_viewer::{GerberRenderer, RenderConfiguration, GerberTransform, ViewState};
use egui::Painter;
use crate::display::DisplayManager;
use super::{LayerType, UnassignedGerber};

/// ECS-based rendering system for gerber layers
/// This system queries all layer entities and renders them using gerber-viewer
pub fn render_layers_system(
    world: &mut World,
    painter: &Painter,
    view_state: ViewState,
    display_manager: &DisplayManager,
) {
    let config = RenderConfiguration::default();
    let renderer = GerberRenderer::default();
    
    // Query all layer entities including ImageTransform
    let mut layer_query = world.query::<(&GerberData, &Transform, &ImageTransform, &Visibility, &RenderProperties, &LayerInfo)>();
    let mut layers: Vec<_> = layer_query.iter(world).collect();
    
    // Sort layers by z-order for proper rendering depth
    layers.sort_by_key(|(_, _, _, _, props, _)| props.z_order);
    
    // Render each visible layer
    for (gerber_data, transform, image_transform, visibility, render_props, _layer_info) in layers {
        if !visibility.visible {
            continue;
        }
        
        // Note: We rely solely on visibility.visible to determine if a layer should be shown
        // This allows manual layer control overrides regardless of top/bottom view
        
        // Create GerberTransform from ECS Transform and ImageTransform
        let gerber_transform = create_gerber_transform_composed(transform, image_transform, display_manager);
        
        // Render the layer
        renderer.paint_layer(
            painter,
            view_state,
            &gerber_data.0,
            render_props.color,
            &config,
            &gerber_transform,
        );
    }
}

/// Enhanced ECS-based rendering system with quadrant support
/// This system supports quadrant view mode and proper layer positioning
pub fn render_layers_system_enhanced(
    world: &mut World,
    painter: &Painter,
    view_state: ViewState,
    display_manager: &DisplayManager,
) {
    let config = RenderConfiguration::default();
    let renderer = GerberRenderer::default();
    
    // Get mechanical outline for quadrant view (do this first to avoid borrow issues)
    let mechanical_outline = if display_manager.quadrant_view_enabled {
        get_mechanical_outline_layer(world)
    } else {
        None
    };
    
    // Query all layer entities including ImageTransform
    let mut layer_query = world.query::<(&GerberData, &Transform, &ImageTransform, &Visibility, &RenderProperties, &LayerInfo)>();
    let mut layers: Vec<_> = layer_query.iter(world).collect();
    
    // Sort layers by z-order for proper rendering depth
    layers.sort_by_key(|(_, _, _, _, props, _)| props.z_order);
    
    // Render each visible layer
    for (gerber_data, transform, image_transform, visibility, render_props, layer_info) in layers {
        if !visibility.visible {
            continue;
        }
        
        // Note: We rely solely on visibility.visible to determine if a layer should be shown
        // This allows manual layer control overrides regardless of top/bottom view
        
        // Skip mechanical outline in quadrant view (it will be rendered with each layer)
        if display_manager.quadrant_view_enabled && layer_info.layer_type == LayerType::MechanicalOutline {
            continue;
        }
        
        // Skip paste layers in quadrant view (user doesn't want to see them)
        if display_manager.quadrant_view_enabled && matches!(layer_info.layer_type, LayerType::Paste(_)) {
            continue;
        }
        
        // Calculate quadrant offset if needed
        let quadrant_offset = if display_manager.quadrant_view_enabled {
            display_manager.get_quadrant_offset(&layer_info.layer_type)
        } else {
            crate::display::VectorOffset { x: 0.0, y: 0.0 }
        };
        
        // Create GerberTransform with quadrant offset and image transform
        let gerber_transform = create_gerber_transform_with_offset_composed(transform, image_transform, display_manager, quadrant_offset.clone());
        
        // Render main layer
        renderer.paint_layer(
            painter,
            view_state,
            &gerber_data.0,
            render_props.color,
            &config,
            &gerber_transform,
        );
        
        // Render mechanical outline in quadrant view
        if display_manager.quadrant_view_enabled {
            if let Some((mechanical_gerber, mechanical_color)) = &mechanical_outline {
                // Use the same transform as the layer for proper alignment
                let mechanical_transform = create_gerber_transform_with_offset_composed(
                    transform,
                    image_transform,
                    display_manager,
                    quadrant_offset,
                );
                
                renderer.paint_layer(
                    painter,
                    view_state,
                    mechanical_gerber,
                    *mechanical_color,
                    &config,
                    &mechanical_transform,
                );
            }
        }
    }
}

/// Helper function to create GerberTransform from ECS Transform
fn create_gerber_transform(transform: &Transform, _display_manager: &DisplayManager) -> GerberTransform {
    GerberTransform {
        rotation: transform.rotation,
        mirroring: transform.mirroring.clone().into(),
        origin: transform.origin.clone().into(),
        offset: transform.position.clone().into(),
        scale: transform.scale,
    }
}

/// Helper function to create GerberTransform with quadrant offset
fn create_gerber_transform_with_offset(
    transform: &Transform,
    _display_manager: &DisplayManager,
    quadrant_offset: crate::display::VectorOffset,
) -> GerberTransform {
    // Combine transform position with quadrant offset
    let combined_offset = crate::display::VectorOffset {
        x: transform.position.x + quadrant_offset.x,
        y: transform.position.y + quadrant_offset.y,
    };
    
    GerberTransform {
        rotation: transform.rotation,
        mirroring: transform.mirroring.clone().into(),
        origin: transform.origin.clone().into(),
        offset: combined_offset.into(),
        scale: transform.scale,
    }
}

/// Helper function to create composed GerberTransform from ECS Transform and ImageTransform
fn create_gerber_transform_composed(
    transform: &Transform, 
    image_transform: &ImageTransform, 
    _display_manager: &DisplayManager
) -> GerberTransform {
    // According to gerber-viewer 0.2.0, we need to compose:
    // matrix = image_transform_matrix * render_transform_matrix
    
    // First create the render transform matrix
    let render_transform = GerberTransform {
        rotation: transform.rotation,
        mirroring: transform.mirroring.clone().into(),
        origin: transform.origin.clone().into(),
        offset: transform.position.clone().into(),
        scale: transform.scale,
    };
    
    // Get the matrices
    let image_matrix = image_transform.transform.to_matrix();
    let render_matrix = render_transform.to_matrix();
    
    // Compose: final_matrix = image_matrix * render_matrix
    let composed_matrix = image_matrix * render_matrix;
    
    // Convert back to GerberTransform
    GerberTransform::from_matrix(&composed_matrix)
}

/// Helper function to create composed GerberTransform with quadrant offset
fn create_gerber_transform_with_offset_composed(
    transform: &Transform,
    image_transform: &ImageTransform,
    _display_manager: &DisplayManager,
    quadrant_offset: crate::display::VectorOffset,
) -> GerberTransform {
    // Combine transform position with quadrant offset
    let combined_offset = crate::display::VectorOffset {
        x: transform.position.x + quadrant_offset.x,
        y: transform.position.y + quadrant_offset.y,
    };
    
    // Create the render transform with combined offset
    let render_transform = GerberTransform {
        rotation: transform.rotation,
        mirroring: transform.mirroring.clone().into(),
        origin: transform.origin.clone().into(),
        offset: combined_offset.into(),
        scale: transform.scale,
    };
    
    // Get the matrices
    let image_matrix = image_transform.transform.to_matrix();
    let render_matrix = render_transform.to_matrix();
    
    // Compose: final_matrix = image_matrix * render_matrix
    let composed_matrix = image_matrix * render_matrix;
    
    // Convert back to GerberTransform
    GerberTransform::from_matrix(&composed_matrix)
}

/// Helper function to get mechanical outline layer for quadrant rendering
fn get_mechanical_outline_layer(world: &mut World) -> Option<(gerber_viewer::GerberLayer, egui::Color32)> {
    let mut query = world.query::<(&GerberData, &RenderProperties, &LayerInfo)>();
    
    for (gerber_data, render_props, layer_info) in query.iter(world) {
        if layer_info.layer_type == LayerType::MechanicalOutline {
            return Some((gerber_data.0.clone(), render_props.color));
        }
    }
    
    None
}

/// System to render layers with proper ECS system approach
/// This is the main entry point for ECS-based rendering
pub fn execute_render_system(
    world: &mut World,
    painter: &Painter,
    view_state: ViewState,
    display_manager: &DisplayManager,
    use_enhanced_rendering: bool,
) {
    if use_enhanced_rendering {
        render_layers_system_enhanced(world, painter, view_state, display_manager);
    } else {
        render_layers_system(world, painter, view_state, display_manager);
    }
}

/// System to update bounding boxes when transforms change
/// This system recalculates bounding boxes for entities with modified transforms
pub fn update_bounds_system(
    mut query: Query<(&GerberData, &Transform, &mut BoundingBoxCache), Changed<Transform>>,
) {
    for (gerber_data, _transform, mut bounds_cache) in &mut query {
        // Calculate transformed bounding box
        let original_bounds = gerber_data.0.bounding_box();
        
        // Apply transform to the original bounding box
        // For now, we'll use the original bounds as a simple implementation
        // TODO: Apply actual transform to calculate proper transformed bounds
        bounds_cache.bounds = original_bounds.clone();
        
        // Log the update for debugging
        println!("Updated bounds for layer: {:?}", bounds_cache.bounds);
    }
}

/// System to handle layer visibility updates
/// This system can be used to synchronize visibility between ECS and legacy systems
pub fn visibility_system(
    mut query: Query<(&mut Visibility, &LayerInfo), Changed<Visibility>>,
) {
    for (visibility, layer_info) in &mut query {
        // Log visibility changes for debugging
        println!("Layer {} visibility changed to: {}", 
                 layer_info.layer_type.display_name(), 
                 visibility.visible);
    }
}

/// System to handle layer transforms when display settings change
/// This system updates layer transforms based on display manager settings
pub fn transform_system(
    mut query: Query<(&mut Transform, &LayerInfo)>,
    display_manager: &DisplayManager,
) {
    for (mut transform, layer_info) in &mut query {
        // Update transform based on display manager settings
        
        // Apply quadrant offset if enabled
        if display_manager.quadrant_view_enabled {
            let quadrant_offset = display_manager.get_quadrant_offset(&layer_info.layer_type);
            transform.position = crate::display::VectorOffset {
                x: quadrant_offset.x,
                y: quadrant_offset.y,
            };
        } else {
            // Reset position for normal view
            transform.position = crate::display::VectorOffset { x: 0.0, y: 0.0 };
        }
        
        // Apply mirroring
        transform.mirroring = display_manager.mirroring.clone();
        
        // Apply rotation if needed
        // transform.rotation = display_manager.rotation; // if rotation is managed by display manager
    }
}

/// System to handle layer color updates
/// This system can be used to update layer colors dynamically
pub fn color_system(
    mut query: Query<(&mut RenderProperties, &LayerInfo), Changed<RenderProperties>>,
) {
    for (render_props, layer_info) in &mut query {
        // Log color changes for debugging
        println!("Layer {} color changed to: {:?}", 
                 layer_info.layer_type.display_name(), 
                 render_props.color);
    }
}

/// System to handle z-order updates for proper layer rendering
/// This system ensures layers are rendered in the correct order
pub fn z_order_system(
    mut query: Query<(&mut RenderProperties, &LayerInfo)>,
) {
    for (mut render_props, layer_info) in &mut query {
        // Update z-order based on layer type
        render_props.z_order = match layer_info.layer_type {
            LayerType::Paste(crate::ecs::Side::Top) => 90,
            LayerType::Silkscreen(crate::ecs::Side::Top) => 80,
            LayerType::Soldermask(crate::ecs::Side::Top) => 70,
            LayerType::Copper(1) => 60,  // Top copper
            LayerType::Copper(n) => 50 - (n as i32),  // All other copper layers (inner/bottom)
            LayerType::Soldermask(crate::ecs::Side::Bottom) => 40,
            LayerType::Silkscreen(crate::ecs::Side::Bottom) => 30,
            LayerType::Paste(crate::ecs::Side::Bottom) => 20,
            LayerType::MechanicalOutline => 10,
        };
    }
}

// Note: view_mode_system has been removed to allow manual layer control
// Visibility is now controlled entirely through the layer controls UI

/// Master system runner that executes all ECS systems in the correct order
/// This function provides a single entry point for running all ECS systems
pub fn run_ecs_systems(
    world: &mut World,
    display_manager: &DisplayManager,
    rotation_degrees: f32,
) {
    // First, get the combined bounding box to determine the PCB center for mirroring
    let pcb_center = {
        let mut bbox_query = world.query::<&GerberData>();
        let mut combined_bbox: Option<gerber_viewer::BoundingBox> = None;
        
        for gerber_data in bbox_query.iter(world) {
            let layer_bbox = gerber_data.0.bounding_box();
            combined_bbox = match combined_bbox {
                None => Some(layer_bbox.clone()),
                Some(mut existing) => {
                    existing.expand(layer_bbox);
                    Some(existing)
                }
            };
        }
        
        combined_bbox.map(|bbox| bbox.center()).unwrap_or_else(|| nalgebra::Point2::new(0.0, 0.0))
    };
    // Update transforms based on display settings
    let mut transform_query = world.query::<(&mut Transform, &LayerInfo)>();
    for (mut transform, layer_info) in transform_query.iter_mut(world) {
        // Apply quadrant offset if enabled
        if display_manager.quadrant_view_enabled {
            let quadrant_offset = display_manager.get_quadrant_offset(&layer_info.layer_type);
            transform.position = crate::display::VectorOffset {
                x: quadrant_offset.x,
                y: quadrant_offset.y,
            };
        } else {
            // Reset position for normal view
            transform.position = crate::display::VectorOffset { x: 0.0, y: 0.0 };
        }
        
        // Apply mirroring
        transform.mirroring = display_manager.mirroring.clone();
        
        // Apply rotation
        transform.rotation = rotation_degrees.to_radians();
        
        // Set origin to PCB center for proper in-place mirroring and rotation
        transform.origin = crate::display::VectorOffset {
            x: pcb_center.x,
            y: pcb_center.y,
        };
    }
    
    // Note: Visibility is now controlled manually through layer controls
    // We no longer automatically update visibility based on view mode
    // This allows users to show any combination of layers they want
    
    // Update z-order for proper rendering
    let mut z_order_query = world.query::<(&mut RenderProperties, &LayerInfo)>();
    for (mut render_props, layer_info) in z_order_query.iter_mut(world) {
        render_props.z_order = match layer_info.layer_type {
            LayerType::Paste(crate::ecs::Side::Top) => 90,
            LayerType::Silkscreen(crate::ecs::Side::Top) => 80,
            LayerType::Soldermask(crate::ecs::Side::Top) => 70,
            LayerType::Copper(1) => 60,  // Top copper
            LayerType::Copper(n) => 50 - (n as i32),  // All other copper layers (inner/bottom)
            LayerType::Soldermask(crate::ecs::Side::Bottom) => 40,
            LayerType::Silkscreen(crate::ecs::Side::Bottom) => 30,
            LayerType::Paste(crate::ecs::Side::Bottom) => 20,
            LayerType::MechanicalOutline => 10,
        };
    }
    
    // Update bounding boxes when transforms change (for quadrant view)
    let mut bounds_query = world.query::<(&GerberData, &Transform, &mut BoundingBoxCache)>();
    for (gerber_data, transform, mut bounds_cache) in bounds_query.iter_mut(world) {
        // Calculate transformed bounding box based on quadrant position
        let original_bounds = gerber_data.0.bounding_box();
        
        // Apply transform to the bounding box
        let transformed_bounds = if transform.position.x != 0.0 || transform.position.y != 0.0 {
            // Layer is positioned in a quadrant - offset the bounding box
            let mut new_bounds = original_bounds.clone();
            new_bounds.min.x += transform.position.x;
            new_bounds.min.y += transform.position.y;
            new_bounds.max.x += transform.position.x;
            new_bounds.max.y += transform.position.y;
            new_bounds
        } else {
            // Layer is at origin - use original bounds
            original_bounds.clone()
        };
        
        bounds_cache.bounds = transformed_bounds;
    }
}

// ============================================================================
// GERBER ASSIGNMENT SYSTEMS
// ============================================================================

/// System to assign an unassigned gerber to a layer type
/// This creates a new layer entity and removes the gerber from unassigned list
pub fn assign_gerber_to_layer_system(
    world: &mut World,
    filename: String,
    layer_type: LayerType,
) -> Result<Entity, String> {
    // Find and remove the unassigned gerber
    let unassigned_gerber = {
        let mut unassigned_res = world.get_resource_mut::<UnassignedGerbers>()
            .ok_or("UnassignedGerbers resource not found")?;
        
        let unassigned_idx = unassigned_res.0.iter().position(|u| u.filename == filename)
            .ok_or("Unassigned gerber not found")?;
        
        unassigned_res.0.remove(unassigned_idx)
    };
    
    // Check if layer type is already assigned
    if crate::ecs::get_layer_by_type(world, layer_type).is_some() {
        // Put the gerber back in unassigned list
        if let Some(mut unassigned_res) = world.get_resource_mut::<UnassignedGerbers>() {
            unassigned_res.0.push(unassigned_gerber);
        }
        return Err(format!("Layer type {:?} is already assigned", layer_type));
    }
    
    // Create new layer entity using ECS factory
    let entity = crate::ecs::create_gerber_layer_entity(
        world,
        layer_type,
        unassigned_gerber.parsed_layer.clone(),
        Some(unassigned_gerber.content.clone()),
        Some(filename.clone().into()),
        true, // visible by default
    );
    
    // Update layer assignments
    crate::ecs::add_layer_assignment(world, filename, layer_type);
    
    Ok(entity)
}

/// System to auto-detect and assign multiple unassigned gerbers
/// Returns a list of successfully assigned (filename, layer_type) pairs
pub fn auto_assign_gerbers_system(world: &mut World) -> Vec<(String, LayerType)> {
    let mut newly_assigned = Vec::new();
    
    // Get list of unassigned gerbers to process (clone to avoid borrow conflicts)
    let unassigned_list = {
        let unassigned_res = world.get_resource::<UnassignedGerbers>();
        unassigned_res.map(|res| res.0.clone()).unwrap_or_default()
    };
    
    // Try to detect and assign each unassigned gerber
    for unassigned in unassigned_list {
        if let Some(detected_type) = crate::ecs::detect_layer_type(world, &unassigned.filename) {
            // Check if this layer type is already assigned
            if crate::ecs::get_layer_by_type(world, detected_type).is_none() {
                // Try to assign it
                if assign_gerber_to_layer_system(world, unassigned.filename.clone(), detected_type).is_ok() {
                    newly_assigned.push((unassigned.filename, detected_type));
                }
            }
        }
    }
    
    newly_assigned
}

/// System to clear all layers and unassigned gerbers
/// This is used when loading a new project
pub fn clear_all_layers_system(world: &mut World) {
    // Remove all layer entities
    let entities_to_remove: Vec<Entity> = {
        let mut query = world.query::<(Entity, &LayerInfo)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };
    
    for entity in entities_to_remove {
        world.despawn(entity);
    }
    
    // Clear unassigned gerbers
    if let Some(mut unassigned_res) = world.get_resource_mut::<UnassignedGerbers>() {
        unassigned_res.0.clear();
    }
    
    // Clear layer assignments
    if let Some(mut assignments_res) = world.get_resource_mut::<LayerAssignments>() {
        assignments_res.0.clear();
    }
}

/// System to add multiple unassigned gerbers
/// This is used when loading gerber files from a directory
pub fn add_unassigned_gerbers_system(world: &mut World, gerbers: Vec<UnassignedGerber>) {
    if let Some(mut unassigned_res) = world.get_resource_mut::<UnassignedGerbers>() {
        unassigned_res.0.extend(gerbers);
    }
}

/// System to load gerbers from a directory and assign them
/// Returns (loaded_count, unassigned_count)
pub fn load_gerbers_from_directory_system(
    world: &mut World,
    gerber_dir: &std::path::Path,
) -> Result<(usize, usize), String> {
    use std::io::BufReader;
    use gerber_viewer::gerber_parser::parse;
    use gerber_viewer::GerberLayer;
    
    let mut loaded_count = 0;
    let mut unassigned_count = 0;
    let mut gerbers_to_add = Vec::new();
    
    // Read directory and collect all gerber files
    let entries = std::fs::read_dir(gerber_dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("gbr") {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            
            // Try to load and parse the gerber file
            match std::fs::read_to_string(&path) {
                Ok(gerber_content) => {
                    let reader = BufReader::new(gerber_content.as_bytes());
                    match parse(reader) {
                        Ok(doc) => {
                            let commands = doc.into_commands();
                            let gerber_layer = GerberLayer::new(commands);
                            
                            // Try to detect layer type
                            if let Some(detected_type) = crate::ecs::detect_layer_type(world, &filename) {
                                // Check if this layer type is already assigned
                                let layer_assignments = crate::ecs::get_layer_assignments(world);
                                if let Some(existing_assignment) = layer_assignments.iter()
                                    .find(|(_, layer_type)| **layer_type == detected_type)
                                    .map(|(fname, _)| fname.clone()) {
                                    // Layer type already assigned - add to unassigned
                                    gerbers_to_add.push((filename, gerber_content, gerber_layer, None, existing_assignment));
                                    unassigned_count += 1;
                                } else {
                                    // Try to assign directly
                                    gerbers_to_add.push((filename, gerber_content, gerber_layer, Some(detected_type), String::new()));
                                    loaded_count += 1;
                                }
                            } else {
                                // Could not detect - add to unassigned
                                gerbers_to_add.push((filename, gerber_content, gerber_layer, None, String::new()));
                                unassigned_count += 1;
                            }
                        }
                        Err(_e) => {
                            // Parse failed - skip this file
                            continue;
                        }
                    }
                }
                Err(_e) => {
                    // Read failed - skip this file
                    continue;
                }
            }
        }
    }
    
    // Now process all the collected gerbers
    for (filename, gerber_content, gerber_layer, detected_type_opt, _existing_assignment) in gerbers_to_add {
        if let Some(detected_type) = detected_type_opt {
            // Create layer entity directly
            let _entity = crate::ecs::create_gerber_layer_entity(
                world,
                detected_type,
                gerber_layer,
                Some(gerber_content),
                Some(filename.clone().into()),
                true, // visible by default
            );
            
            // Update layer assignments
            crate::ecs::add_layer_assignment(world, filename, detected_type);
        } else {
            // Add to unassigned
            let unassigned = UnassignedGerber {
                filename,
                content: gerber_content,
                parsed_layer: gerber_layer,
            };
            if let Some(mut unassigned_res) = world.get_resource_mut::<UnassignedGerbers>() {
                unassigned_res.0.push(unassigned);
            }
        }
    }
    
    Ok((loaded_count, unassigned_count))
}
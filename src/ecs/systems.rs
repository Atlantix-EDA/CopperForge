use bevy_ecs::prelude::*;
use crate::ecs::components::*;
use gerber_viewer::{GerberRenderer, RenderConfiguration, GerberTransform, ViewState};
use egui::Painter;
use crate::display::DisplayManager;

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
    
    // Query all layer entities
    let mut layer_query = world.query::<(&GerberData, &Transform, &Visibility, &RenderProperties, &LayerInfo)>();
    let mut layers: Vec<_> = layer_query.iter(world).collect();
    
    // Sort layers by z-order for proper rendering depth
    layers.sort_by_key(|(_, _, _, props, _)| props.z_order);
    
    // Render each visible layer
    for (gerber_data, transform, visibility, render_props, layer_info) in layers {
        if !visibility.visible {
            continue;
        }
        
        // Skip layers that shouldn't be rendered in current view
        if !layer_info.layer_type.should_render(display_manager.showing_top) {
            continue;
        }
        
        // Create GerberTransform from ECS Transform
        let gerber_transform = create_gerber_transform(transform, display_manager);
        
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
    
    // Query all layer entities
    let mut layer_query = world.query::<(&GerberData, &Transform, &Visibility, &RenderProperties, &LayerInfo)>();
    let mut layers: Vec<_> = layer_query.iter(world).collect();
    
    // Sort layers by z-order for proper rendering depth
    layers.sort_by_key(|(_, _, _, props, _)| props.z_order);
    
    // Render each visible layer
    for (gerber_data, transform, visibility, render_props, layer_info) in layers {
        if !visibility.visible {
            continue;
        }
        
        // Skip layers that shouldn't be rendered in current view
        if !layer_info.layer_type.should_render(display_manager.showing_top) {
            continue;
        }
        
        // Skip mechanical outline in quadrant view (it will be rendered with each layer)
        if display_manager.quadrant_view_enabled && layer_info.layer_type == crate::layer_operations::LayerType::MechanicalOutline {
            continue;
        }
        
        // Calculate quadrant offset if needed
        let quadrant_offset = if display_manager.quadrant_view_enabled {
            display_manager.get_quadrant_offset(&layer_info.layer_type)
        } else {
            crate::display::VectorOffset { x: 0.0, y: 0.0 }
        };
        
        // Create GerberTransform with quadrant offset
        let gerber_transform = create_gerber_transform_with_offset(transform, display_manager, quadrant_offset.clone());
        
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
                let mechanical_transform = create_gerber_transform_with_offset(
                    &Transform::default(),
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

/// Helper function to get mechanical outline layer for quadrant rendering
fn get_mechanical_outline_layer(world: &mut World) -> Option<(gerber_viewer::GerberLayer, egui::Color32)> {
    let mut query = world.query::<(&GerberData, &RenderProperties, &LayerInfo)>();
    
    for (gerber_data, render_props, layer_info) in query.iter(world) {
        if layer_info.layer_type == crate::layer_operations::LayerType::MechanicalOutline {
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
            crate::layer_operations::LayerType::TopPaste => 90,
            crate::layer_operations::LayerType::TopSilk => 80,
            crate::layer_operations::LayerType::TopSoldermask => 70,
            crate::layer_operations::LayerType::TopCopper => 60,
            crate::layer_operations::LayerType::BottomCopper => 50,
            crate::layer_operations::LayerType::BottomSoldermask => 40,
            crate::layer_operations::LayerType::BottomSilk => 30,
            crate::layer_operations::LayerType::BottomPaste => 20,
            crate::layer_operations::LayerType::MechanicalOutline => 10,
        };
    }
}

/// System to update layer display based on view mode (top/bottom)
/// This system handles layer visibility based on the current view mode
pub fn view_mode_system(
    mut query: Query<(&mut Visibility, &LayerInfo)>,
    display_manager: &DisplayManager,
) {
    for (mut visibility, layer_info) in &mut query {
        // Update visibility based on view mode
        let should_render = layer_info.layer_type.should_render(display_manager.showing_top);
        
        // Only update if visibility would change to avoid triggering Change detection unnecessarily
        if visibility.visible != should_render {
            visibility.visible = should_render;
        }
    }
}

/// Master system runner that executes all ECS systems in the correct order
/// This function provides a single entry point for running all ECS systems
pub fn run_ecs_systems(
    world: &mut World,
    display_manager: &DisplayManager,
) {
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
    }
    
    // Update visibility based on view mode
    let mut visibility_query = world.query::<(&mut Visibility, &LayerInfo)>();
    for (mut visibility, layer_info) in visibility_query.iter_mut(world) {
        let should_render = layer_info.layer_type.should_render(display_manager.showing_top);
        if visibility.visible != should_render {
            visibility.visible = should_render;
        }
    }
    
    // Update z-order for proper rendering
    let mut z_order_query = world.query::<(&mut RenderProperties, &LayerInfo)>();
    for (mut render_props, layer_info) in z_order_query.iter_mut(world) {
        render_props.z_order = match layer_info.layer_type {
            crate::layer_operations::LayerType::TopPaste => 90,
            crate::layer_operations::LayerType::TopSilk => 80,
            crate::layer_operations::LayerType::TopSoldermask => 70,
            crate::layer_operations::LayerType::TopCopper => 60,
            crate::layer_operations::LayerType::BottomCopper => 50,
            crate::layer_operations::LayerType::BottomSoldermask => 40,
            crate::layer_operations::LayerType::BottomSilk => 30,
            crate::layer_operations::LayerType::BottomPaste => 20,
            crate::layer_operations::LayerType::MechanicalOutline => 10,
        };
    }
}
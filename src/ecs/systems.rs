use bevy_ecs::prelude::*;
use crate::ecs::components::*;
use crate::ecs::resources::*;
use gerber_viewer::{GerberRenderer, RenderConfiguration, GerberTransform};
use egui::Painter;

// ECS-based rendering system
pub fn render_layers_system(
    painter: &Painter,
    layer_query: Query<(&GerberData, &Transform, &Visibility, &RenderProperties)>,
    view_state: Res<ViewStateResource>,
    _render_config: Res<RenderConfig>,
) {
    let config = RenderConfiguration::default();
    let renderer = GerberRenderer::default();
    
    // Collect and sort layers by z-order
    let mut layers: Vec<_> = layer_query.iter().collect();
    layers.sort_by_key(|(_, _, _, props)| props.z_order);
    
    // Render each visible layer
    for (gerber_data, transform, visibility, render_props) in layers {
        if !visibility.visible {
            continue;
        }
        
        // Create GerberTransform from ECS Transform
        let gerber_transform = GerberTransform {
            rotation: transform.rotation,
            mirroring: transform.mirroring.clone().into(),
            origin: transform.origin.clone().into(),
            offset: transform.position.clone().into(),
            scale: transform.scale,
        };
        
        // Render the layer
        renderer.paint_layer(
            painter,
            view_state.view_state,
            &gerber_data.0,
            render_props.color,
            &config,
            &gerber_transform,
        );
    }
}

// System to update bounding boxes when transforms change
pub fn update_bounds_system(
    mut query: Query<(&GerberData, &Transform, &mut BoundingBoxCache), Changed<Transform>>,
) {
    for (_gerber, _transform, _bounds) in &mut query {
        // Update bounding box based on transform
        // This will cache transformed bounds for efficient culling
    }
}

// System to handle visibility toggling
pub fn visibility_system(
    _query: Query<&mut Visibility>,
    // Will add input handling later
) {
    // Placeholder for visibility logic
}
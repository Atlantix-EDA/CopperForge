use bevy_ecs::prelude::*;
use super::components::*;

/// System to update layer visibility and properties
pub fn layer_update_system(
    mut query: Query<(&mut EcsLayerInfo, &Position)>
) {
    for (mut layer_info, _position) in query.iter_mut() {
        // Example: Update layer properties based on some conditions
        // This could be driven by UI state or other game logic
        if layer_info.layer_type == "copper" && !layer_info.visible {
            // Auto-enable copper layers for better visibility
            layer_info.visible = true;
        }
    }
}

/// System to update PCB element states
pub fn pcb_element_system(
    query: Query<(Entity, &PcbElement, &Position, Option<&Selected>)>
) {
    for (entity, pcb_element, position, selected) in query.iter() {
        // Example: Log selected PCB elements
        if let Some(_selected) = selected {
            match pcb_element {
                PcbElement::Component { name, .. } => {
                    log::debug!("Selected component '{}' at ({}, {})", name, position.x, position.y);
                }
                PcbElement::Via { radius, .. } => {
                    log::debug!("Selected via (radius {}) at ({}, {})", radius, position.x, position.y);
                }
                PcbElement::Trace { width, .. } => {
                    log::debug!("Selected trace (width {}) at ({}, {})", width, position.x, position.y);
                }
                PcbElement::Pad { width, height, .. } => {
                    log::debug!("Selected pad ({}x{}) at ({}, {})", width, height, position.x, position.y);
                }
            }
        }
    }
}

/// System for spatial queries and collision detection
pub fn spatial_query_system(
    query: Query<(Entity, &Position, &BoundingBox)>
) {
    // Example: Find entities within certain bounds
    let search_bounds = BoundingBox::new(
        nalgebra::Point2::new(-10.0, -10.0),
        nalgebra::Point2::new(10.0, 10.0)
    );
    
    for (entity, position, bbox) in query.iter() {
        if bbox.intersects(&search_bounds) {
            log::trace!("Entity {:?} intersects search bounds at ({}, {})", entity, position.x, position.y);
        }
    }
}

/// System to update bounding boxes based on transforms
pub fn bounding_box_update_system(
    mut query: Query<(&Position, &Transform, &mut BoundingBox, &PcbElement)>
) {
    for (position, transform, mut bbox, pcb_element) in query.iter_mut() {
        // Update bounding box based on position, transform, and element type
        let base_size = match pcb_element {
            PcbElement::Via { radius, .. } => *radius * 2.0,
            PcbElement::Pad { width, height, .. } => width.max(*height),
            PcbElement::Component { .. } => 5.0, // Default component size
            PcbElement::Trace { width, start, end } => {
                let length = (end - start).norm();
                length.max(*width)
            }
        };
        
        let half_size = base_size * transform.scale.x as f64 * 0.5;
        bbox.min = nalgebra::Point2::new(position.x - half_size, position.y - half_size);
        bbox.max = nalgebra::Point2::new(position.x + half_size, position.y + half_size);
    }
}
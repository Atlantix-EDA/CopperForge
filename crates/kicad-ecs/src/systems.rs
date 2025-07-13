//! ECS Systems for analyzing PCB data

use bevy_ecs::prelude::*;
use crate::components::*;

/// System to analyze component distribution by layer
pub fn layer_analysis_system(
    query: Query<(&ComponentInfo, &Layer)>
) {
    let mut layer_counts = std::collections::HashMap::new();
    
    for (_info, layer) in query.iter() {
        *layer_counts.entry(layer.layer_name.clone()).or_insert(0) += 1;
    }
    
    println!("Component distribution by layer:");
    for (layer, count) in layer_counts {
        println!("  {}: {} components", layer, count);
    }
}

/// System to find mounting holes and analyze them
pub fn mounting_hole_analysis_system(
    query: Query<(&ComponentInfo, &Position, &MountingHole)>
) {
    println!("Mounting hole analysis:");
    
    for (info, pos, hole) in query.iter() {
        println!("  {} at ({:.1}, {:.1}): {} hole, {} screw", 
                 info.reference, pos.x, pos.y, 
                 hole.diameter_mm, hole.screw_size);
    }
}
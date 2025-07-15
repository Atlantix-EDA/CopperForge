use bevy_ecs::prelude::*;
use gerber_viewer::GerberLayer;
use super::{LayerType, Side};
use crate::ecs::components::*;
use std::path::PathBuf;

/// Entity Factory Pattern for creating layer entities
/// These functions encapsulate the "recipe" for creating different types of layer entities
/// and ensure they have all the necessary components

/// Factory for creating a gerber layer entity
pub fn create_gerber_layer_entity(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    _raw_gerber_data: Option<String>,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    let bounds = gerber_layer.bounding_box().clone();
    
    world.spawn((
        GerberData(gerber_layer),
        LayerInfo {
            layer_type,
            name: layer_type.display_name().to_string(),
            file_path,
        },
        Transform::default(),
        ImageTransform::default(),
        Visibility {
            visible,
            opacity: 1.0,
        },
        RenderProperties {
            color: layer_type.color(),
            highlight_color: None,
            z_order: layer_type_to_z_order(&layer_type),
        },
        BoundingBoxCache { bounds },
    )).id()
}

/* DEPRECATED: LayerManager migration function (no longer needed)
/// Factory for creating a layer entity from existing LayerInfo
pub fn create_layer_from_info(
    world: &mut World,
    layer_info: &LayerInfoOrig,
    gerber_layer: GerberLayer,
) -> Entity {
    let bounds = gerber_layer.bounding_box().clone();
    
    world.spawn((
        GerberData(gerber_layer),
        LayerInfo {
            layer_type: layer_info.layer_type,
            name: layer_info.layer_type.display_name().to_string(),
            file_path: None, // TODO: Extract from layer_info if needed
        },
        Transform::default(),
        ImageTransform::default(),
        Visibility {
            visible: layer_info.visible,
            opacity: 1.0,
        },
        RenderProperties {
            color: layer_info.color,
            highlight_color: None,
            z_order: layer_type_to_z_order(&layer_info.layer_type),
        },
        BoundingBoxCache { bounds },
    )).id()
}
*/

/// Factory for creating a mechanical outline layer entity
pub fn create_mechanical_outline_entity(
    world: &mut World,
    gerber_layer: GerberLayer,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    create_gerber_layer_entity(
        world,
        LayerType::MechanicalOutline,
        gerber_layer,
        None,
        file_path,
        visible,
    )
}

/// Factory for creating a copper layer entity
pub fn create_copper_layer_entity(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    _raw_gerber_data: Option<String>,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    // Ensure we're creating a copper layer
    match layer_type {
        LayerType::Copper(_) => {
            let entity_id = create_gerber_layer_entity(
                world,
                layer_type,
                gerber_layer,
                _raw_gerber_data,
                file_path,
                visible,
            );
            
            // Add DRC requirement for copper layers
            world.entity_mut(entity_id).insert(RequiresDrc);
            
            entity_id
        }
        _ => panic!("create_copper_layer_entity called with non-copper layer type: {:?}", layer_type),
    }
}

/// Factory for creating a silk layer entity
pub fn create_silk_layer_entity(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    // Ensure we're creating a silk layer
    match layer_type {
        LayerType::Silkscreen(_) => {
            create_gerber_layer_entity(
                world,
                layer_type,
                gerber_layer,
                None,
                file_path,
                visible,
            )
        }
        _ => panic!("create_silk_layer_entity called with non-silk layer type: {:?}", layer_type),
    }
}

/// Factory for creating a soldermask layer entity
pub fn create_soldermask_layer_entity(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    // Ensure we're creating a soldermask layer
    match layer_type {
        LayerType::Soldermask(_) => {
            create_gerber_layer_entity(
                world,
                layer_type,
                gerber_layer,
                None,
                file_path,
                visible,
            )
        }
        _ => panic!("create_soldermask_layer_entity called with non-soldermask layer type: {:?}", layer_type),
    }
}

/// Factory for creating a paste layer entity
pub fn create_paste_layer_entity(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    file_path: Option<PathBuf>,
    visible: bool,
) -> Entity {
    // Ensure we're creating a paste layer
    match layer_type {
        LayerType::Paste(_) => {
            create_gerber_layer_entity(
                world,
                layer_type,
                gerber_layer,
                None,
                file_path,
                visible,
            )
        }
        _ => panic!("create_paste_layer_entity called with non-paste layer type: {:?}", layer_type),
    }
}

/// Utility function to determine z-order for layer rendering
fn layer_type_to_z_order(layer_type: &LayerType) -> i32 {
    match layer_type {
        LayerType::Paste(Side::Top) => 90,
        LayerType::Silkscreen(Side::Top) => 80,
        LayerType::Soldermask(Side::Top) => 70,
        LayerType::Copper(1) => 60,  // Top copper
        LayerType::Copper(n) => 50 - (*n as i32),  // All other copper layers (inner/bottom)
        LayerType::Soldermask(Side::Bottom) => 40,
        LayerType::Silkscreen(Side::Bottom) => 30,
        LayerType::Paste(Side::Bottom) => 20,
        LayerType::MechanicalOutline => 10,
    }
}

/* DEPRECATED: LayerManager factory (no longer needed)
/// Bulk factory for creating multiple layer entities from a LayerManager (deprecated)
pub fn create_layers_from_manager(
    world: &mut World,
    layer_manager: &crate::layer_operations::LayerManager,
) -> Vec<Entity> {
    let mut entities = Vec::new();
    
    for (_layer_type, layer_info) in &layer_manager.layers {
        if let Some(ref gerber_layer) = layer_info.gerber_layer {
            let entity = create_layer_from_info(world, layer_info, gerber_layer.clone());
            entities.push(entity);
        }
    }
    
    entities
}
*/

/// Factory for creating a layer entity with custom transform
pub fn create_layer_with_transform(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    transform: Transform,
    visible: bool,
) -> Entity {
    let bounds = gerber_layer.bounding_box().clone();
    
    world.spawn((
        GerberData(gerber_layer),
        LayerInfo {
            layer_type,
            name: layer_type.display_name().to_string(),
            file_path: None,
        },
        transform,
        Visibility {
            visible,
            opacity: 1.0,
        },
        RenderProperties {
            color: layer_type.color(),
            highlight_color: None,
            z_order: layer_type_to_z_order(&layer_type),
        },
        BoundingBoxCache { bounds },
    )).id()
}

/// Factory for creating a layer entity with custom color
pub fn create_layer_with_color(
    world: &mut World,
    layer_type: LayerType,
    gerber_layer: GerberLayer,
    color: egui::Color32,
    visible: bool,
) -> Entity {
    let bounds = gerber_layer.bounding_box().clone();
    
    world.spawn((
        GerberData(gerber_layer),
        LayerInfo {
            layer_type,
            name: layer_type.display_name().to_string(),
            file_path: None,
        },
        Transform::default(),
        Visibility {
            visible,
            opacity: 1.0,
        },
        RenderProperties {
            color,
            highlight_color: None,
            z_order: layer_type_to_z_order(&layer_type),
        },
        BoundingBoxCache { bounds },
    )).id()
}
pub mod components;
pub mod resources;
pub mod systems;

pub use components::*;
pub use resources::*;
pub use systems::*;

use bevy_ecs::prelude::*;
use crate::layer_operations::{LayerType, LayerInfo as LayerInfoOrig, LayerManager};
use gerber_viewer::GerberLayer;

pub fn setup_ecs_world() -> World {
    let mut world = World::new();
    
    // Initialize resources
    world.insert_resource(ViewStateResource::default());
    world.insert_resource(RenderConfig::default());
    world.insert_resource(ActiveLayer(LayerType::TopCopper));
    
    world
}

// Helper function to spawn a layer entity
pub fn spawn_layer(
    world: &mut World,
    layer_info: &LayerInfoOrig,
    gerber_layer: GerberLayer,
) -> Entity {
    let bounds = gerber_layer.bounding_box().clone();
    
    world.spawn((
        GerberData(gerber_layer),
        components::LayerInfo {
            layer_type: layer_info.layer_type.clone(),
            name: layer_info.layer_type.display_name().to_string(),
            file_path: None, // Will be set later
        },
        Transform::default(),
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

fn layer_type_to_z_order(layer_type: &LayerType) -> i32 {
    match layer_type {
        LayerType::TopPaste => 90,
        LayerType::TopSilk => 80,
        LayerType::TopSoldermask => 70,
        LayerType::TopCopper => 60,
        LayerType::BottomCopper => 50,
        LayerType::BottomSoldermask => 40,
        LayerType::BottomSilk => 30,
        LayerType::BottomPaste => 20,
        LayerType::MechanicalOutline => 10,
    }
}

// Migration function to populate ECS world from LayerManager data
pub fn migrate_layers_to_ecs(world: &mut World, layer_manager: &LayerManager) {
    // Spawn layers from LayerManager
    for (_layer_type, layer_info) in &layer_manager.layers {
        if let Some(ref gerber_layer) = layer_info.gerber_layer {
            spawn_layer(world, layer_info, gerber_layer.clone());
        }
    }
    
    // Set active layer
    world.insert_resource(ActiveLayer(layer_manager.active_layer));
}

// Query functions for LayerManager facade
pub fn get_layer_entities(world: &mut World) -> Vec<Entity> {
    let mut query = world.query::<Entity>();
    query.iter(world).collect()
}

pub fn get_visible_layer_entities(world: &mut World) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &Visibility)>();
    query.iter(world)
        .filter(|(_, visibility)| visibility.visible)
        .map(|(entity, _)| entity)
        .collect()
}

pub fn get_layer_by_type(world: &mut World, layer_type: LayerType) -> Option<Entity> {
    let mut query = world.query::<(Entity, &components::LayerInfo)>();
    query.iter(world)
        .find(|(_, layer_info)| layer_info.layer_type == layer_type)
        .map(|(entity, _)| entity)
}

pub fn set_layer_visibility(world: &mut World, layer_type: LayerType, visible: bool) {
    if let Some(entity) = get_layer_by_type(world, layer_type) {
        if let Some(mut visibility) = world.get_mut::<Visibility>(entity) {
            visibility.visible = visible;
        }
    }
}

pub fn get_layer_count(world: &mut World) -> usize {
    let mut query = world.query::<Entity>();
    query.iter(world).count()
}
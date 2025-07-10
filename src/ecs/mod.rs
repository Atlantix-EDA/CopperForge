#![allow(dead_code)]
pub mod components;
pub mod resources;
pub mod systems;
pub mod factories;

pub use components::*;
pub use resources::*;
pub use systems::*;
pub use factories::*;

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

// Deprecated: Use factories::create_layer_from_info instead
pub fn spawn_layer(
    world: &mut World,
    layer_info: &LayerInfoOrig,
    gerber_layer: GerberLayer,
) -> Entity {
    create_layer_from_info(world, layer_info, gerber_layer)
}

// Migration function to populate ECS world from LayerManager data
pub fn migrate_layers_to_ecs(world: &mut World, layer_manager: &LayerManager) {
    // Use the bulk factory to create all layer entities
    create_layers_from_manager(world, layer_manager);
    
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
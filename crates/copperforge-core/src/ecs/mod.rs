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
use crate::layer_operations::{LayerType, LayerInfo as LayerInfoOrig, LayerManager, detection::UnassignedGerber};
use gerber_viewer::GerberLayer;

pub fn setup_ecs_world() -> World {
    let mut world = World::new();
    
    // Initialize resources
    world.insert_resource(ViewStateResource::default());
    world.insert_resource(RenderConfig::default());
    world.insert_resource(ActiveLayer(LayerType::TopCopper));
    world.insert_resource(LayerAssignments::default());
    world.insert_resource(UnassignedGerbers::default());
    world.insert_resource(LayerDetectorResource::default());
    world.insert_resource(CoordinateUpdateTracker::default());
    
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

// Read-only version of get_layer_by_type  
pub fn get_layer_by_type_readonly(world: &mut World, layer_type: LayerType) -> Option<Entity> {
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

// Helper functions for new ECS resources

pub fn add_layer_assignment(world: &mut World, filename: String, layer_type: LayerType) {
    if let Some(mut assignments) = world.get_resource_mut::<LayerAssignments>() {
        assignments.0.insert(filename, layer_type);
    }
}

pub fn remove_layer_assignment(world: &mut World, filename: &str) -> Option<LayerType> {
    if let Some(mut assignments) = world.get_resource_mut::<LayerAssignments>() {
        assignments.0.remove(filename)
    } else {
        None
    }
}

pub fn get_layer_assignment(world: &World, filename: &str) -> Option<LayerType> {
    world.get_resource::<LayerAssignments>()
        .and_then(|assignments| assignments.0.get(filename).copied())
}

pub fn add_unassigned_gerber(world: &mut World, gerber: UnassignedGerber) {
    if let Some(mut unassigned) = world.get_resource_mut::<UnassignedGerbers>() {
        unassigned.0.push(gerber);
    }
}

pub fn remove_unassigned_gerber(world: &mut World, index: usize) -> Option<UnassignedGerber> {
    if let Some(mut unassigned) = world.get_resource_mut::<UnassignedGerbers>() {
        if index < unassigned.0.len() {
            Some(unassigned.0.remove(index))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn clear_unassigned_gerbers(world: &mut World) {
    if let Some(mut unassigned) = world.get_resource_mut::<UnassignedGerbers>() {
        unassigned.0.clear();
    }
}

pub fn clear_layer_assignments(world: &mut World) {
    if let Some(mut assignments) = world.get_resource_mut::<LayerAssignments>() {
        assignments.0.clear();
    }
}

pub fn detect_layer_type(world: &World, filename: &str) -> Option<LayerType> {
    world.get_resource::<LayerDetectorResource>()
        .and_then(|detector| detector.0.detect_layer_type(filename))
}

pub fn mark_coordinates_dirty(world: &mut World) {
    if let Some(mut tracker) = world.get_resource_mut::<CoordinateUpdateTracker>() {
        tracker.dirty = true;
    }
}

pub fn mark_coordinates_updated(world: &mut World) {
    if let Some(mut tracker) = world.get_resource_mut::<CoordinateUpdateTracker>() {
        tracker.dirty = false;
        tracker.last_updated = std::time::Instant::now();
    }
}

pub fn coordinates_need_update(world: &World) -> bool {
    world.get_resource::<CoordinateUpdateTracker>()
        .map(|tracker| tracker.dirty)
        .unwrap_or(false)
}

// Get layer data by type (replaces LayerManager::get_layer_ecs)
pub fn get_layer_data(world: &mut World, layer_type: LayerType) -> Option<(Entity, &components::LayerInfo, &components::GerberData, &components::Visibility)> {
    let mut query = world.query::<(Entity, &components::LayerInfo, &components::GerberData, &components::Visibility)>();
    query.iter(world)
        .find(|(_, layer_info, _, _)| layer_info.layer_type == layer_type)
}

// Get layer render properties (replaces LayerManager::get_layer_render_properties_ecs)
pub fn get_layer_render_properties(world: &mut World, layer_type: LayerType) -> Option<&components::RenderProperties> {
    if let Some(entity) = get_layer_by_type_readonly(world, layer_type) {
        world.get::<components::RenderProperties>(entity)
    } else {
        None
    }
}

// Update layer render properties (replaces LayerManager::update_layer_render_properties_ecs)
pub fn update_layer_render_properties(world: &mut World, layer_type: LayerType, color: egui::Color32) -> bool {
    if let Some(entity) = get_layer_by_type(world, layer_type) {
        if let Some(mut render_props) = world.get_mut::<components::RenderProperties>(entity) {
            render_props.color = color;
            return true;
        }
    }
    false
}

// Get unassigned gerbers (replaces LayerManager::unassigned_gerbers access)
pub fn get_unassigned_gerbers(world: &World) -> Vec<crate::layer_operations::detection::UnassignedGerber> {
    world.get_resource::<UnassignedGerbers>()
        .map(|unassigned| unassigned.0.clone())
        .unwrap_or_default()
}

// Check if unassigned gerbers exist
pub fn has_unassigned_gerbers(world: &World) -> bool {
    world.get_resource::<UnassignedGerbers>()
        .map(|unassigned| !unassigned.0.is_empty())
        .unwrap_or(false)
}

// Get layer assignments (replaces LayerManager::layer_assignments access)
pub fn get_layer_assignments(world: &World) -> std::collections::HashMap<String, LayerType> {
    world.get_resource::<LayerAssignments>()
        .map(|assignments| assignments.0.clone())
        .unwrap_or_default()
}
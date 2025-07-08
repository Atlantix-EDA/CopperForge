pub mod components;
pub mod resources;
pub mod systems;

pub use components::*;
pub use resources::*;
pub use systems::*;

use bevy_ecs::prelude::*;
use crate::layer_operations::{LayerType, LayerInfo as LayerInfoOrig};
use gerber_viewer::GerberLayer;
use egui::Color32;

pub fn setup_ecs_world() -> World {
    let mut world = World::new();
    
    // Initialize resources
    world.insert_resource(ViewStateResource::default());
    world.insert_resource(RenderConfig::default());
    
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
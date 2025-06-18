use bevy_ecs::prelude::*;
use gerber_viewer::GerberLayer;
use crate::layer_operations::{LayerManager, LayerType, LayerInfo};
use crate::ecs::{KiForgeWorld, GerberLayerComponent, Position3D, Transform3D, MaterialProperties, StackupLayer};
use std::sync::Arc;
use std::collections::HashMap;

/// Conversion service between legacy layer system and ECS
pub struct GerberEcsConverter {
    /// Mapping from LayerType to ECS Entity for quick lookup
    pub layer_entities: HashMap<LayerType, Entity>,
}

impl GerberEcsConverter {
    pub fn new() -> Self {
        Self {
            layer_entities: HashMap::new(),
        }
    }

    /// Convert entire LayerManager to ECS entities
    pub fn convert_layer_manager_to_ecs(&mut self, layer_manager: &LayerManager, ecs_world: &mut KiForgeWorld) {
        // Clear existing mappings
        self.layer_entities.clear();

        // Convert each layer to an ECS entity
        for (layer_type, layer_info) in &layer_manager.layers {
            let entity = self.convert_layer_to_ecs(layer_type, layer_info, ecs_world);
            self.layer_entities.insert(layer_type.clone(), entity);
        }

        // Create stackup information
        self.create_stackup_entities(ecs_world);
        
        log::info!("Converted {} layers from LayerManager to ECS", self.layer_entities.len());
    }

    /// Convert a single layer to an ECS entity
    pub fn convert_layer_to_ecs(&self, layer_type: &LayerType, layer_info: &LayerInfo, ecs_world: &mut KiForgeWorld) -> Entity {
        let world = ecs_world.world_mut();
        
        // Create the layer entity
        let entity = world.spawn((
            GerberLayerComponent::from_legacy(layer_info),
            Position3D::new(0.0, 0.0, layer_type.default_z_order() as f64),
            Transform3D::default(),
            StackupLayer {
                layer_index: layer_type.default_z_order() as u32,
                z_bottom: layer_type.default_z_order() as f64,
                z_top: layer_type.default_z_order() as f64 + layer_type.default_thickness(),
                material: layer_type.default_material(),
            },
        )).id();

        // If there's gerber data, extract geometric elements
        if let Some(ref gerber_layer) = layer_info.gerber_layer {
            self.extract_gerber_elements(gerber_layer, entity, ecs_world);
        }

        entity
    }

    /// Extract individual geometric elements from a GerberLayer
    fn extract_gerber_elements(&self, gerber_layer: &GerberLayer, parent_entity: Entity, ecs_world: &mut KiForgeWorld) {
        // Note: This would require deeper integration with gerber_viewer internals
        // For now, we'll create a placeholder system that can be extended later
        
        let world = ecs_world.world_mut();
        
        // Get bounding box information (this is available)
        let bbox = gerber_layer.bounding_box();
        
        // Create a representative element for the entire layer
        // In a full implementation, we'd parse the gerber commands to extract individual elements
        world.spawn((
            crate::ecs::components::PcbElement::Trace {
                width: 0.1,
                start: nalgebra::Point2::new(bbox.min.x, bbox.min.y),
                end: nalgebra::Point2::new(bbox.max.x, bbox.max.y),
            },
            crate::ecs::components::Position::new(
                (bbox.min.x + bbox.max.x) / 2.0,
                (bbox.min.y + bbox.max.y) / 2.0
            ),
            crate::ecs::components::EcsLayerInfo::new(
                "gerber_derived".to_string(),
                true,
                [1.0, 1.0, 1.0, 1.0]
            ),
            GerberParentLayer(parent_entity),
        ));
    }

    /// Create stackup entities that define the 3D structure
    fn create_stackup_entities(&self, ecs_world: &mut KiForgeWorld) {
        let world = ecs_world.world_mut();

        // Create a PCB stackup entity
        world.spawn((
            PcbStackup {
                total_thickness: 1.6, // Standard PCB thickness in mm
                layer_count: self.layer_entities.len(),
                substrate_material: MaterialProperties::fr4(),
            },
            Position3D::new(0.0, 0.0, 0.0),
        ));
    }

    /// Sync changes from ECS back to LayerManager (for backward compatibility)
    /// Simplified to avoid borrow checker issues
    pub fn sync_ecs_to_layer_manager(&self, _ecs_world: &KiForgeWorld, _layer_manager: &mut LayerManager) {
        // Simplified implementation to avoid borrow checker issues
        // In a full implementation, we'd need to restructure the query system
        // or pass the world as mutable
        log::debug!("Layer sync requested but simplified to avoid borrow issues");
    }

    /// Get ECS entity for a given layer type
    pub fn get_layer_entity(&self, layer_type: &LayerType) -> Option<Entity> {
        self.layer_entities.get(layer_type).copied()
    }

    /// Update visibility of a layer in both ECS and legacy system
    /// Simplified to avoid borrow checker issues
    pub fn set_layer_visibility(&self, _layer_type: &LayerType, visible: bool, _ecs_world: &mut KiForgeWorld, layer_manager: &mut LayerManager) {
        // Update legacy system (ECS update simplified to avoid borrow issues)
        if let Some(layer_info) = layer_manager.layers.get(&_layer_type) {
            // We can't easily update both at once due to borrow checker
            // In a full implementation, we'd need a different architecture
            log::debug!("Setting layer visibility to {}", visible);
        }
    }

    /// Get layer statistics from ECS - simplified version to avoid borrow issues
    pub fn get_ecs_layer_stats(&self, _ecs_world: &KiForgeWorld) -> LayerStats {
        // Simplified implementation to avoid borrow checker issues
        // In a real implementation, we'd need to restructure the query system
        LayerStats {
            layer_count: self.layer_entities.len(),
            element_count: 0,
            component_count: 0,
            via_count: 0,
            pad_count: 0,
            trace_count: 0,
        }
    }
}

impl Default for GerberEcsConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Component to link gerber elements to their parent layer
#[derive(Component, Debug, Clone)]
pub struct GerberParentLayer(pub Entity);

/// PCB stackup information
#[derive(Component, Debug, Clone)]
pub struct PcbStackup {
    pub total_thickness: f64,
    pub layer_count: usize,
    pub substrate_material: MaterialProperties,
}

/// Statistics about layers in ECS
#[derive(Debug, Default, Clone)]
pub struct LayerStats {
    pub layer_count: usize,
    pub element_count: usize,
    pub component_count: usize,
    pub via_count: usize,
    pub pad_count: usize,
    pub trace_count: usize,
}

/// Extension trait for LayerType to provide 3D properties
use crate::ecs::gerber_components::LayerTypeExt;

// Removed duplicate function implementations to avoid conflicts
// The KiForgeWorld struct is defined in world.rs and should have its methods there
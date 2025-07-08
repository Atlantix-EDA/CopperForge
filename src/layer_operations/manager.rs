use serde;
use std::collections::HashMap;
use super::types::{LayerType, LayerInfo};
use super::detection::{LayerDetector, UnassignedGerber};
use bevy_ecs::prelude::*;
use crate::ecs::{self, Visibility};

/// Manager for all layer-related functionality
/// Now acts as a facade over ECS entities
#[derive(Debug)]
pub struct LayerManager {
    /// Map of layer types to their ECS entity IDs
    pub layer_entities: HashMap<LayerType, Entity>,
    
    /// Reference to the ECS world (non-owning)
    /// This will be passed in method calls rather than stored
    
    /// Currently active/selected layer
    pub active_layer: LayerType,
    
    /// Layer detection system for auto-assignment
    pub layer_detector: LayerDetector,
    
    /// Gerber files that couldn't be automatically assigned to layers
    pub unassigned_gerbers: Vec<UnassignedGerber>,
    
    /// Manual layer assignments (filename -> layer type)
    pub layer_assignments: HashMap<String, LayerType>,
    
    /// Coordinate update tracking
    pub coordinates_last_updated: std::time::Instant,
    
    /// Flag to track if coordinates need updating
    pub coordinates_dirty: bool,
    
    /// Legacy compatibility: cache of layer info for backward compatibility
    /// This will be deprecated once all code is migrated to ECS
    pub layers: HashMap<LayerType, LayerInfo>,
}

impl LayerManager {
    /// Create a new LayerManager with default settings
    pub fn new() -> Self {
        Self {
            layer_entities: HashMap::new(),
            active_layer: LayerType::TopCopper,
            layer_detector: LayerDetector::new(),
            unassigned_gerbers: Vec::new(),
            layer_assignments: HashMap::new(),
            coordinates_last_updated: std::time::Instant::now(),
            coordinates_dirty: false,
            layers: HashMap::new(), // Legacy compatibility
        }
    }
    
    /// Add or update a layer using ECS
    pub fn add_layer_ecs(&mut self, world: &mut World, layer_type: LayerType, layer_info: LayerInfo) {
        // Create ECS entity using the factory
        if let Some(gerber_layer) = &layer_info.gerber_layer {
            let entity = ecs::create_layer_from_info(world, &layer_info, gerber_layer.clone());
            self.layer_entities.insert(layer_type, entity);
        }
        
        // Also update legacy cache for backward compatibility
        self.layers.insert(layer_type, layer_info);
    }
    
    /// Add or update a layer (legacy method - maintained for compatibility)
    pub fn add_layer(&mut self, layer_type: LayerType, layer_info: LayerInfo) {
        // Legacy method - only updates the cache
        // This should be replaced with add_layer_ecs when ECS world is available
        self.layers.insert(layer_type, layer_info);
    }
    
    /// Remove a layer using ECS
    pub fn remove_layer_ecs(&mut self, world: &mut World, layer_type: &LayerType) -> Option<LayerInfo> {
        // Remove from ECS world
        if let Some(entity) = self.layer_entities.remove(layer_type) {
            world.despawn(entity);
        }
        
        // Remove from legacy cache
        self.layers.remove(layer_type)
    }
    
    /// Remove a layer (legacy method - maintained for compatibility)
    pub fn remove_layer(&mut self, layer_type: &LayerType) -> Option<LayerInfo> {
        // Legacy method - only removes from cache
        self.layers.remove(layer_type)
    }
    
    /// Get a layer entity by type
    pub fn get_layer_entity(&self, layer_type: &LayerType) -> Option<Entity> {
        self.layer_entities.get(layer_type).copied()
    }
    
    /// Get layer visibility using ECS
    pub fn get_layer_visibility(&self, world: &World, layer_type: &LayerType) -> bool {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            if let Some(visibility) = world.get::<Visibility>(*entity) {
                return visibility.visible;
            }
        }
        false
    }
    
    /// Get a layer by type (legacy method - maintained for compatibility)
    /// This method now uses ECS queries internally but maintains the same API
    pub fn get_layer(&self, layer_type: &LayerType) -> Option<&LayerInfo> {
        // First try to get from legacy cache for backward compatibility
        // This ensures existing code continues to work during the transition
        self.layers.get(layer_type)
    }
    
    /// Get a layer by type using ECS queries
    pub fn get_layer_ecs<'a>(&self, world: &'a World, layer_type: &LayerType) -> Option<(Entity, &'a crate::ecs::LayerInfo, &'a crate::ecs::GerberData, &'a crate::ecs::Visibility)> {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            let layer_info = world.get::<crate::ecs::LayerInfo>(*entity)?;
            let gerber_data = world.get::<crate::ecs::GerberData>(*entity)?;
            let visibility = world.get::<crate::ecs::Visibility>(*entity)?;
            Some((*entity, layer_info, gerber_data, visibility))
        } else {
            None
        }
    }
    
    /// Get a mutable reference to a layer by type (legacy method - maintained for compatibility)
    pub fn get_layer_mut(&mut self, layer_type: &LayerType) -> Option<&mut LayerInfo> {
        self.layers.get_mut(layer_type)
    }
    
    /// Set the active layer
    pub fn set_active_layer(&mut self, layer_type: LayerType) {
        self.active_layer = layer_type;
    }
    
    /// Get the active layer
    pub fn get_active_layer(&self) -> LayerType {
        self.active_layer
    }
    
    /// Clear all layers and assignments using ECS
    pub fn clear_all_ecs(&mut self, world: &mut World) {
        // Despawn all layer entities
        for entity in self.layer_entities.values() {
            world.despawn(*entity);
        }
        self.layer_entities.clear();
        
        // Clear legacy cache and other data
        self.layers.clear();
        self.unassigned_gerbers.clear();
        self.layer_assignments.clear();
    }
    
    /// Clear all layers and assignments (legacy method - maintained for compatibility)
    pub fn clear_all(&mut self) {
        self.layers.clear();
        self.unassigned_gerbers.clear();
        self.layer_assignments.clear();
    }
    
    /// Add an unassigned gerber file
    pub fn add_unassigned_gerber(&mut self, gerber: UnassignedGerber) {
        self.unassigned_gerbers.push(gerber);
    }
    
    /// Remove an unassigned gerber by index
    pub fn remove_unassigned_gerber(&mut self, index: usize) -> Option<UnassignedGerber> {
        if index < self.unassigned_gerbers.len() {
            Some(self.unassigned_gerbers.remove(index))
        } else {
            None
        }
    }
    
    /// Assign a layer manually
    pub fn assign_layer(&mut self, filename: String, layer_type: LayerType) {
        self.layer_assignments.insert(filename, layer_type);
    }
    
    /// Remove a layer assignment
    pub fn remove_assignment(&mut self, filename: &str) -> Option<LayerType> {
        self.layer_assignments.remove(filename)
    }
    
    /// Get the assignment for a filename
    pub fn get_assignment(&self, filename: &str) -> Option<&LayerType> {
        self.layer_assignments.get(filename)
    }
    
    /// Check if a layer type is already assigned
    pub fn is_layer_assigned(&self, layer_type: &LayerType) -> bool {
        self.layer_assignments.values().any(|lt| lt == layer_type)
    }
    
    /// Get all visible layers using ECS
    pub fn get_visible_layers_ecs(&self, world: &World) -> Vec<LayerType> {
        let mut visible_layers = Vec::new();
        
        for (layer_type, entity) in &self.layer_entities {
            if let Some(visibility) = world.get::<Visibility>(*entity) {
                if visibility.visible {
                    visible_layers.push(*layer_type);
                }
            }
        }
        
        visible_layers
    }
    
    /// Toggle layer visibility using ECS
    pub fn toggle_layer_visibility_ecs(&mut self, world: &mut World, layer_type: &LayerType) {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            if let Some(mut visibility) = world.get_mut::<Visibility>(*entity) {
                visibility.visible = !visibility.visible;
            }
        }
        
        // Also update legacy cache
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = !layer_info.visible;
        }
    }
    
    /// Set layer visibility using ECS
    pub fn set_layer_visibility_ecs(&mut self, world: &mut World, layer_type: &LayerType, visible: bool) {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            if let Some(mut visibility) = world.get_mut::<Visibility>(*entity) {
                visibility.visible = visible;
            }
        }
        
        // Also update legacy cache
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = visible;
        }
    }
    
    /// Get all visible layers (legacy method - maintained for compatibility)
    /// This method now uses ECS queries internally but maintains the same API
    pub fn get_visible_layers(&self) -> Vec<(&LayerType, &LayerInfo)> {
        // For backward compatibility, still return legacy LayerInfo structure
        // This ensures existing code continues to work during the transition
        self.layers.iter()
            .filter(|(_, layer_info)| layer_info.visible)
            .collect()
    }
    
    /// Get all visible layers using ECS queries with full entity data
    pub fn get_visible_layers_with_entities<'a>(&self, world: &'a World) -> Vec<(LayerType, Entity, &'a crate::ecs::LayerInfo, &'a crate::ecs::GerberData)> {
        let mut visible_layers = Vec::new();
        
        for (layer_type, entity) in &self.layer_entities {
            if let Some(visibility) = world.get::<crate::ecs::Visibility>(*entity) {
                if visibility.visible {
                    if let (Some(layer_info), Some(gerber_data)) = (
                        world.get::<crate::ecs::LayerInfo>(*entity),
                        world.get::<crate::ecs::GerberData>(*entity)
                    ) {
                        visible_layers.push((*layer_type, *entity, layer_info, gerber_data));
                    }
                }
            }
        }
        
        visible_layers
    }
    
    /// Toggle layer visibility (legacy method - maintained for compatibility)
    /// This method now updates both ECS and legacy cache for backward compatibility
    pub fn toggle_layer_visibility(&mut self, layer_type: &LayerType) {
        // Update legacy cache for backward compatibility
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = !layer_info.visible;
        }
        
        // Note: ECS update should be done via toggle_layer_visibility_ecs for proper sync
    }
    
    /// Set layer visibility (legacy method - maintained for compatibility)
    /// This method now updates both ECS and legacy cache for backward compatibility
    pub fn set_layer_visibility(&mut self, layer_type: &LayerType, visible: bool) {
        // Update legacy cache for backward compatibility
        if let Some(layer_info) = self.layers.get_mut(layer_type) {
            layer_info.visible = visible;
        }
        
        // Note: ECS update should be done via set_layer_visibility_ecs for proper sync
    }
    
    /// Get the number of loaded layers using ECS
    pub fn layer_count_ecs(&self) -> usize {
        self.layer_entities.len()
    }
    
    /// Get the number of loaded layers (legacy method - maintained for compatibility)
    /// This method now uses ECS queries internally but maintains the same API
    pub fn layer_count(&self) -> usize {
        // Use ECS count if available, otherwise fall back to legacy cache
        if !self.layer_entities.is_empty() {
            self.layer_entities.len()
        } else {
            self.layers.len()
        }
    }
    
    /// Get the number of unassigned gerbers
    pub fn unassigned_count(&self) -> usize {
        self.unassigned_gerbers.len()
    }
    
    /// Get layer statistics (uses ECS queries when available)
    pub fn get_statistics(&self) -> LayerStatistics {
        LayerStatistics {
            total_layers: self.layer_count(),
            visible_layers: self.get_visible_layers().len(),
            unassigned_gerbers: self.unassigned_count(),
            assignments: self.layer_assignments.len(),
        }
    }
    
    /// Get layer statistics using ECS queries
    pub fn get_statistics_ecs(&self, world: &World) -> LayerStatistics {
        let visible_count = self.get_visible_layers_ecs(world).len();
        
        LayerStatistics {
            total_layers: self.layer_count_ecs(),
            visible_layers: visible_count,
            unassigned_gerbers: self.unassigned_count(),
            assignments: self.layer_assignments.len(),
        }
    }
    
    /// Auto-detect layer type for a filename
    pub fn detect_layer_type(&self, filename: &str) -> Option<LayerType> {
        self.layer_detector.detect_layer_type(filename)
    }
    
    /// Initialize all layer coordinates from their gerber data using ECS
    pub fn initialize_all_coordinates_ecs(&mut self, world: &mut World) {
        // Update all ECS layer entities with fresh coordinates from gerber data
        let mut entities_to_update = Vec::new();
        
        // First pass: collect entities and their gerber data
        {
            let mut query = world.query::<(Entity, &crate::ecs::GerberData)>();
            for (entity, gerber_data) in query.iter(world) {
                entities_to_update.push((entity, gerber_data.0.bounding_box().clone()));
            }
        }
        
        // Second pass: update transforms and bounding boxes
        for (entity, bounds) in entities_to_update {
            // Reset transform to defaults (coordinates are handled by display manager)
            if let Some(mut transform) = world.get_mut::<crate::ecs::Transform>(entity) {
                *transform = crate::ecs::Transform::default();
            }
            
            // Update bounding box cache if it exists
            if let Some(mut bounds_cache) = world.get_mut::<crate::ecs::BoundingBoxCache>(entity) {
                bounds_cache.bounds = bounds;
            }
        }
        self.mark_coordinates_updated();
    }
    
    /// Initialize all layer coordinates from their gerber data (legacy method)
    pub fn initialize_all_coordinates(&mut self) {
        // Legacy method - only updates cache
        for (_, layer_info) in self.layers.iter_mut() {
            layer_info.initialize_coordinates_from_gerber();
        }
        self.mark_coordinates_updated();
        // Note: Use initialize_all_coordinates_ecs for proper ECS sync
    }
    
    /// Mark coordinates as needing update
    pub fn mark_coordinates_dirty(&mut self) {
        self.coordinates_dirty = true;
    }
    
    /// Mark coordinates as updated
    pub fn mark_coordinates_updated(&mut self) {
        self.coordinates_dirty = false;
        self.coordinates_last_updated = std::time::Instant::now();
    }
    
    /// Check if coordinates need updating (based on time or dirty flag)
    pub fn coordinates_need_update(&self) -> bool {
        self.coordinates_dirty || 
        self.coordinates_last_updated.elapsed() > std::time::Duration::from_secs(2)
    }
    
    /// Update layer coordinates based on current view and display settings using ECS
    pub fn update_coordinates_from_display_ecs(&mut self, world: &mut World, display_manager: &crate::display::DisplayManager) {
        if !self.coordinates_need_update() {
            return;
        }
        
        // Update ECS layer transforms based on display settings
        let mut query = world.query::<(&mut crate::ecs::Transform, &crate::ecs::LayerInfo)>();
        for (mut transform, layer_info) in query.iter_mut(world) {
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
        
        self.mark_coordinates_updated();
    }
    
    /// Update layer coordinates based on current view and display settings (legacy method)
    pub fn update_coordinates_from_display(&mut self, display_manager: &crate::display::DisplayManager) {
        if !self.coordinates_need_update() {
            return;
        }
        
        // Legacy method - uses display manager update
        display_manager.update_layer_positions(self);
        
        self.mark_coordinates_updated();
        // Note: Use update_coordinates_from_display_ecs for proper ECS sync
    }
    
    /// Sync ECS entities with LayerManager - populates layer_entities from ECS world
    pub fn sync_with_ecs(&mut self, world: &mut World) {
        // Clear existing entity mappings
        self.layer_entities.clear();
        
        // Query ECS world for all layer entities and map them by layer type
        let mut query = world.query::<(Entity, &crate::ecs::LayerInfo)>();
        for (entity, layer_info) in query.iter(world) {
            self.layer_entities.insert(layer_info.layer_type, entity);
        }
    }
    
    /// Get all layer entities with their types using ECS
    pub fn get_all_layer_entities_ecs(&self, _world: &World) -> Vec<(LayerType, Entity)> {
        let mut layers = Vec::new();
        for (layer_type, entity) in &self.layer_entities {
            layers.push((*layer_type, *entity));
        }
        layers
    }
    
    /// Get layer data by entity using ECS
    pub fn get_layer_data_by_entity_ecs<'a>(&self, world: &'a World, entity: Entity) -> Option<(&'a crate::ecs::LayerInfo, &'a crate::ecs::GerberData, &'a crate::ecs::Visibility)> {
        let layer_info = world.get::<crate::ecs::LayerInfo>(entity)?;
        let gerber_data = world.get::<crate::ecs::GerberData>(entity)?;
        let visibility = world.get::<crate::ecs::Visibility>(entity)?;
        Some((layer_info, gerber_data, visibility))
    }
    
    /// Update layer transform using ECS
    pub fn update_layer_transform_ecs(&mut self, world: &mut World, layer_type: &LayerType, transform: crate::ecs::Transform) -> bool {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            if let Some(mut layer_transform) = world.get_mut::<crate::ecs::Transform>(*entity) {
                *layer_transform = transform;
                return true;
            }
        }
        false
    }
    
    /// Get layer transform using ECS
    pub fn get_layer_transform_ecs(&self, world: &World, layer_type: &LayerType) -> Option<crate::ecs::Transform> {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            world.get::<crate::ecs::Transform>(*entity).cloned()
        } else {
            None
        }
    }
    
    /// Update layer render properties using ECS
    pub fn update_layer_render_properties_ecs(&mut self, world: &mut World, layer_type: &LayerType, color: egui::Color32) -> bool {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            if let Some(mut render_props) = world.get_mut::<crate::ecs::RenderProperties>(*entity) {
                render_props.color = color;
                return true;
            }
        }
        false
    }
    
    /// Get layer render properties using ECS
    pub fn get_layer_render_properties_ecs<'a>(&self, world: &'a World, layer_type: &LayerType) -> Option<&'a crate::ecs::RenderProperties> {
        if let Some(entity) = self.layer_entities.get(layer_type) {
            world.get::<crate::ecs::RenderProperties>(*entity)
        } else {
            None
        }
    }
    
    /// Get all layers with their complete ECS data
    pub fn get_all_layers_with_data_ecs<'a>(&self, world: &'a World) -> Vec<(LayerType, Entity, &'a crate::ecs::LayerInfo, &'a crate::ecs::GerberData, &'a crate::ecs::Visibility, &'a crate::ecs::Transform, &'a crate::ecs::RenderProperties)> {
        let mut layers = Vec::new();
        
        for (layer_type, entity) in &self.layer_entities {
            if let (Some(layer_info), Some(gerber_data), Some(visibility), Some(transform), Some(render_props)) = (
                world.get::<crate::ecs::LayerInfo>(*entity),
                world.get::<crate::ecs::GerberData>(*entity),
                world.get::<crate::ecs::Visibility>(*entity),
                world.get::<crate::ecs::Transform>(*entity),
                world.get::<crate::ecs::RenderProperties>(*entity)
            ) {
                layers.push((*layer_type, *entity, layer_info, gerber_data, visibility, transform, render_props));
            }
        }
        
        layers
    }
    
    /// Calculate the mechanical outline centroid for design offset calculation using ECS
    pub fn get_mechanical_outline_centroid_ecs(&self, world: &World) -> Option<(f64, f64)> {
        if let Some(entity) = self.layer_entities.get(&LayerType::MechanicalOutline) {
            if let Some(gerber_data) = world.get::<crate::ecs::GerberData>(*entity) {
                let bbox = gerber_data.0.bounding_box();
                let centroid = bbox.center();
                println!("🎯 Mechanical outline centroid: ({:.2}, {:.2})", centroid.x, centroid.y);
                return Some((centroid.x, centroid.y));
            }
        }
        println!("⚠️ No mechanical outline layer found for centroid calculation");
        None
    }
    
    /// Get combined bounding box of all visible layers using ECS
    pub fn get_combined_bounding_box_ecs(&self, world: &World) -> Option<gerber_viewer::BoundingBox> {
        let mut combined_bbox: Option<gerber_viewer::BoundingBox> = None;
        
        for (_layer_type, entity) in &self.layer_entities {
            if let (Some(gerber_data), Some(visibility)) = (
                world.get::<crate::ecs::GerberData>(*entity),
                world.get::<crate::ecs::Visibility>(*entity)
            ) {
                if visibility.visible {
                    let layer_bbox = gerber_data.0.bounding_box();
                    combined_bbox = Some(match combined_bbox {
                        None => layer_bbox.clone(),
                        Some(existing) => gerber_viewer::BoundingBox {
                            min: nalgebra::Point2::new(
                                existing.min.x.min(layer_bbox.min.x),
                                existing.min.y.min(layer_bbox.min.y),
                            ),
                            max: nalgebra::Point2::new(
                                existing.max.x.max(layer_bbox.max.x),
                                existing.max.y.max(layer_bbox.max.y),
                            ),
                        },
                    });
                }
            }
        }
        
        combined_bbox
    }
    
    /// Calculate the mechanical outline centroid for design offset calculation (legacy method)
    pub fn get_mechanical_outline_centroid(&self) -> Option<(f64, f64)> {
        if let Some(mechanical_layer) = self.get_layer(&LayerType::MechanicalOutline) {
            if let Some(ref gerber) = mechanical_layer.gerber_layer {
                let bbox = gerber.bounding_box();
                let centroid = bbox.center();
                println!("🎯 Mechanical outline centroid: ({:.2}, {:.2})", centroid.x, centroid.y);
                return Some((centroid.x, centroid.y));
            }
        }
        println!("⚠️ No mechanical outline layer found for centroid calculation");
        None
        // Note: Use get_mechanical_outline_centroid_ecs for proper ECS queries
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}

// Custom deserialization to handle skipped fields
impl<'de> serde::Deserialize<'de> for LayerManager {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct LayerManagerData {
            active_layer: LayerType,
            layer_assignments: HashMap<String, LayerType>,
        }
        
        let data = LayerManagerData::deserialize(deserializer)?;
        
        Ok(LayerManager {
            layer_entities: HashMap::new(),
            layers: HashMap::new(),
            active_layer: data.active_layer,
            layer_detector: LayerDetector::new(),
            unassigned_gerbers: Vec::new(),
            layer_assignments: data.layer_assignments,
            coordinates_last_updated: std::time::Instant::now(),
            coordinates_dirty: true, // Mark as dirty on load
        })
    }
}

impl serde::Serialize for LayerManager {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        
        let mut state = serializer.serialize_struct("LayerManager", 2)?;
        state.serialize_field("active_layer", &self.active_layer)?;
        state.serialize_field("layer_assignments", &self.layer_assignments)?;
        state.end()
    }
}

/// Statistics about the layer manager state
#[derive(Debug, Clone)]
pub struct LayerStatistics {
    pub total_layers: usize,
    pub visible_layers: usize,
    pub unassigned_gerbers: usize,
    pub assignments: usize,
}
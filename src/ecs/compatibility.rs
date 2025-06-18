use crate::layer_operations::{LayerManager, LayerType, LayerInfo};
use crate::ecs::{KiForgeWorld, GerberEcsConverter, GerberLayerComponent, LayerStats};
use std::sync::{Arc, Mutex};

/// Wrapper that provides seamless integration between legacy LayerManager and ECS
pub struct HybridLayerManager {
    /// Original layer manager for backward compatibility
    pub legacy_manager: LayerManager,
    /// ECS converter for bidirectional sync
    converter: GerberEcsConverter,
    /// Whether ECS sync is enabled
    ecs_sync_enabled: bool,
}

impl HybridLayerManager {
    pub fn new(legacy_manager: LayerManager) -> Self {
        Self {
            legacy_manager,
            converter: GerberEcsConverter::new(),
            ecs_sync_enabled: true,
        }
    }

    /// Initialize ECS representation from current legacy state
    pub fn sync_to_ecs(&mut self, ecs_world: &mut KiForgeWorld) {
        if self.ecs_sync_enabled {
            self.converter.convert_layer_manager_to_ecs(&self.legacy_manager, ecs_world);
        }
    }

    /// Sync changes from ECS back to legacy system
    pub fn sync_from_ecs(&mut self, ecs_world: &KiForgeWorld) {
        if self.ecs_sync_enabled {
            self.converter.sync_ecs_to_layer_manager(ecs_world, &mut self.legacy_manager);
        }
    }

    /// Set layer visibility with automatic sync
    pub fn set_layer_visibility(&mut self, layer_type: &LayerType, visible: bool, ecs_world: &mut KiForgeWorld) {
        if self.ecs_sync_enabled {
            self.converter.set_layer_visibility(layer_type, visible, ecs_world, &mut self.legacy_manager);
        } else {
            // Fallback to legacy-only
            if let Some(layer_info) = self.legacy_manager.layers.get_mut(layer_type) {
                layer_info.visible = visible;
            }
        }
    }

    /// Add new layer with automatic ECS sync
    pub fn add_layer(&mut self, layer_type: LayerType, layer_info: LayerInfo, ecs_world: &mut KiForgeWorld) {
        // Add to ECS if sync is enabled
        if self.ecs_sync_enabled {
            let entity = self.converter.convert_layer_to_ecs(&layer_type, &layer_info, ecs_world);
            log::info!("Added layer {:?} to both legacy and ECS systems (entity: {:?})", layer_type, entity);
        }
        
        // Add to legacy system (move ownership)
        self.legacy_manager.layers.insert(layer_type, layer_info);
    }

    /// Remove layer from both systems
    pub fn remove_layer(&mut self, layer_type: &LayerType, ecs_world: &mut KiForgeWorld) {
        // Remove from legacy system
        self.legacy_manager.layers.remove(layer_type);

        // Remove from ECS if sync is enabled
        if self.ecs_sync_enabled {
            if let Some(entity) = self.converter.get_layer_entity(layer_type) {
                ecs_world.world_mut().despawn(entity);
                log::info!("Removed layer {:?} from both legacy and ECS systems", layer_type);
            }
        }
    }

    /// Get comprehensive statistics from both systems
    pub fn get_combined_stats(&self, ecs_world: &KiForgeWorld) -> CombinedLayerStats {
        let legacy_stats = LegacyLayerStats {
            layer_count: self.legacy_manager.layers.len(),
            visible_layers: self.legacy_manager.layers.values().filter(|l| l.visible).count(),
            layers_with_data: self.legacy_manager.layers.values().filter(|l| l.gerber_layer.is_some()).count(),
            unassigned_gerbers: self.legacy_manager.unassigned_gerbers.len(),
        };

        let ecs_stats = if self.ecs_sync_enabled {
            Some(self.converter.get_ecs_layer_stats(ecs_world))
        } else {
            None
        };

        CombinedLayerStats {
            legacy: legacy_stats,
            ecs: ecs_stats,
            sync_enabled: self.ecs_sync_enabled,
        }
    }

    /// Enable or disable ECS synchronization
    pub fn set_ecs_sync(&mut self, enabled: bool, ecs_world: &mut KiForgeWorld) {
        if enabled && !self.ecs_sync_enabled {
            // Enabling sync - convert current state to ECS
            self.ecs_sync_enabled = true;
            self.sync_to_ecs(ecs_world);
            log::info!("Enabled ECS synchronization and synced current state");
        } else if !enabled && self.ecs_sync_enabled {
            // Disabling sync - optionally clear ECS data
            self.ecs_sync_enabled = false;
            log::info!("Disabled ECS synchronization");
        }
    }

    /// Get ECS entity for a layer type (if sync is enabled)
    pub fn get_ecs_entity(&self, layer_type: &LayerType) -> Option<bevy_ecs::entity::Entity> {
        if self.ecs_sync_enabled {
            self.converter.get_layer_entity(layer_type)
        } else {
            None
        }
    }

    /// Check if systems are in sync
    pub fn verify_sync(&self, _ecs_world: &KiForgeWorld) -> SyncStatus {
        if !self.ecs_sync_enabled {
            return SyncStatus::SyncDisabled;
        }

        let legacy_count = self.legacy_manager.layers.len();
        // Simplified for now - in real implementation we'd need proper query access
        let ecs_count = self.converter.layer_entities.len();

        if legacy_count == ecs_count {
            SyncStatus::InSync { layer_count: legacy_count }
        } else {
            SyncStatus::OutOfSync { 
                legacy_count, 
                ecs_count,
                difference: (legacy_count as i32 - ecs_count as i32).abs() as usize
            }
        }
    }

    /// Force a full resync from legacy to ECS
    pub fn force_resync(&mut self, ecs_world: &mut KiForgeWorld) {
        if self.ecs_sync_enabled {
            // Clear existing mappings
            self.converter.layer_entities.clear();

            // Rebuild from legacy state
            self.sync_to_ecs(ecs_world);
            log::info!("Forced complete resync from legacy to ECS");
        }
    }

    /// Get layer info (legacy accessor for compatibility)
    pub fn get_layer(&self, layer_type: &LayerType) -> Option<&LayerInfo> {
        self.legacy_manager.layers.get(layer_type)
    }

    /// Get mutable layer info (legacy accessor for compatibility)  
    pub fn get_layer_mut(&mut self, layer_type: &LayerType) -> Option<&mut LayerInfo> {
        self.legacy_manager.layers.get_mut(layer_type)
    }

    /// Delegate all other LayerManager methods for transparency
    pub fn get_visible_layers(&self) -> Vec<&LayerType> {
        self.legacy_manager.get_visible_layers().into_iter().map(|(layer_type, _)| layer_type).collect()
    }

    pub fn toggle_layer_visibility(&mut self, layer_type: &LayerType, ecs_world: &mut KiForgeWorld) {
        if let Some(layer) = self.legacy_manager.layers.get(layer_type) {
            let new_visibility = !layer.visible;
            self.set_layer_visibility(layer_type, new_visibility, ecs_world);
        }
    }
}

/// Statistics from legacy LayerManager
#[derive(Debug, Clone)]
pub struct LegacyLayerStats {
    pub layer_count: usize,
    pub visible_layers: usize,
    pub layers_with_data: usize,
    pub unassigned_gerbers: usize,
}

/// Combined statistics from both systems
#[derive(Debug, Clone)]
pub struct CombinedLayerStats {
    pub legacy: LegacyLayerStats,
    pub ecs: Option<LayerStats>,
    pub sync_enabled: bool,
}

/// Synchronization status between systems
#[derive(Debug, Clone)]
pub enum SyncStatus {
    InSync { layer_count: usize },
    OutOfSync { legacy_count: usize, ecs_count: usize, difference: usize },
    SyncDisabled,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStatus::InSync { layer_count } => write!(f, "In sync ({} layers)", layer_count),
            SyncStatus::OutOfSync { legacy_count, ecs_count, difference } => {
                write!(f, "Out of sync: Legacy={}, ECS={}, Diff={}", legacy_count, ecs_count, difference)
            }
            SyncStatus::SyncDisabled => write!(f, "Sync disabled"),
        }
    }
}

// Removed duplicate DemoLensApp impl block - these functions should be defined in main.rs
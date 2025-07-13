//! PCB World - the main ECS container

use bevy_ecs::prelude::*;
use eyre::Result;
use tracing::{debug, instrument};
use crate::components::*;

/// Main ECS world for PCB data
pub struct PcbWorld {
    pub world: World,
    component_count: usize,
}

impl PcbWorld {
    /// Create a new empty PCB world
    pub fn new() -> Self {
        Self {
            world: World::new(),
            component_count: 0,
        }
    }
    
    /// Load board data from KiCad (placeholder)
    pub fn from_board(_board_data: ()) -> Result<Self> {
        // TODO: Implement board loading
        Ok(Self::new())
    }
    
    /// Add a component to the world
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn add_component(&mut self, 
                        id: String,
                        reference: String, 
                        value: String,
                        footprint_name: String,
                        position: (f64, f64, f64), // x, y, rotation
                        layer: String,
                        description: String,
                        flags: (bool, bool, bool)) -> Entity { // exclude_bom, dnp, locked
        
        debug!("Adding component {} to ECS world", reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: id },
            ComponentInfo { reference, value, footprint_name },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            ComponentDescription { description },
            ComponentFlags {
                exclude_from_bom: flags.0,
                do_not_populate: flags.1,
                locked: flags.2,
            },
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Add a mounting hole to the world
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn add_mounting_hole(&mut self,
                            id: String,
                            reference: String,
                            position: (f64, f64, f64), // x, y, rotation
                            layer: String,
                            diameter: f64,
                            screw_size: String) -> Entity {
        
        debug!("Adding mounting hole {} to ECS world", reference);
        
        self.world.spawn((
            ComponentId { uuid: id },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            MountingHole { diameter_mm: diameter, screw_size },
        )).id()
    }
    
    // ===== Factory-style spawn methods =====
    
    /// Spawn a generic PCB footprint/component entity
    #[instrument(skip(self), fields(reference = %footprint_data.reference))]
    pub fn spawn_footprint(&mut self, footprint_data: crate::client::FootprintData) -> Entity {
        debug!("Spawning footprint {} to ECS world", footprint_data.reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: footprint_data.id },
            ComponentInfo { 
                reference: footprint_data.reference,
                value: footprint_data.value,
                footprint_name: footprint_data.footprint_name,
            },
            Position { 
                x: footprint_data.position.0, 
                y: footprint_data.position.1, 
                rotation: footprint_data.rotation,
            },
            Layer { layer_name: footprint_data.layer },
            ComponentDescription { 
                description: "Component".to_string(),
            },
            ComponentFlags {
                exclude_from_bom: footprint_data.exclude_from_bom,
                do_not_populate: footprint_data.do_not_populate,
                locked: footprint_data.locked,
            },
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Spawn a resistor entity
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn spawn_resistor(&mut self, 
                         id: String,
                         reference: String,
                         value: String, 
                         footprint: String,
                         position: (f64, f64, f64),
                         layer: String) -> Entity {
        debug!("Spawning resistor {} to ECS world", reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: id },
            ComponentInfo { reference, value, footprint_name: footprint },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            ComponentDescription { description: "Resistor".to_string() },
            Resistor, // Marker component for resistors
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Spawn a capacitor entity
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn spawn_capacitor(&mut self,
                          id: String, 
                          reference: String,
                          value: String,
                          footprint: String,
                          position: (f64, f64, f64),
                          layer: String) -> Entity {
        debug!("Spawning capacitor {} to ECS world", reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: id },
            ComponentInfo { reference, value, footprint_name: footprint },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            ComponentDescription { description: "Capacitor".to_string() },
            Capacitor, // Marker component for capacitors
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Spawn an IC entity
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn spawn_ic(&mut self,
                   id: String,
                   reference: String, 
                   value: String,
                   footprint: String,
                   position: (f64, f64, f64),
                   layer: String) -> Entity {
        debug!("Spawning IC {} to ECS world", reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: id },
            ComponentInfo { reference, value, footprint_name: footprint },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            ComponentDescription { description: "Integrated Circuit".to_string() },
            IntegratedCircuit, // Marker component for ICs
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Spawn a connector entity
    #[instrument(skip(self), fields(reference = %reference))]
    pub fn spawn_connector(&mut self,
                          id: String,
                          reference: String,
                          value: String, 
                          footprint: String,
                          position: (f64, f64, f64),
                          layer: String) -> Entity {
        debug!("Spawning connector {} to ECS world", reference);
        
        let entity = self.world.spawn((
            ComponentId { uuid: id },
            ComponentInfo { reference, value, footprint_name: footprint },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            ComponentDescription { description: "Connector".to_string() },
            Connector, // Marker component for connectors
        ));
        
        self.component_count += 1;
        entity.id()
    }
    
    /// Spawn a mounting hole entity
    #[instrument(skip(self), fields(reference = %reference))]  
    pub fn spawn_mounting_hole(&mut self,
                              id: String,
                              reference: String,
                              position: (f64, f64, f64),
                              layer: String,
                              diameter: f64,
                              screw_size: String) -> Entity {
        debug!("Spawning mounting hole {} to ECS world", reference);
        
        self.world.spawn((
            ComponentId { uuid: id },
            Position { x: position.0, y: position.1, rotation: position.2 },
            Layer { layer_name: layer },
            MountingHole { diameter_mm: diameter, screw_size },
        )).id()
    }
    
    /// Get the number of components
    pub fn component_count(&self) -> usize {
        self.component_count
    }
    
    /// Get statistics about the PCB
    #[instrument(skip(self))]
    pub fn get_statistics(&mut self) -> PcbStatistics {
        let mut stats = PcbStatistics::default();
        
        // Count components by layer
        let mut component_query = self.world.query::<(&ComponentInfo, &Layer)>();
        for (_info, layer) in component_query.iter(&self.world) {
            match layer.layer_name.as_str() {
                "F.Cu" => stats.front_components += 1,
                "B.Cu" => stats.back_components += 1,
                _ => stats.other_layer_components += 1,
            }
        }
        
        // Count mounting holes
        let mut mounting_hole_query = self.world.query::<&MountingHole>();
        stats.mounting_holes = mounting_hole_query.iter(&self.world).count();
        
        // Count special flags
        let mut flag_query = self.world.query::<&ComponentFlags>();
        for flags in flag_query.iter(&self.world) {
            if flags.do_not_populate { stats.dnp_components += 1; }
            if flags.exclude_from_bom { stats.exclude_bom_components += 1; }
            if flags.locked { stats.locked_components += 1; }
        }
        
        stats.total_components = self.component_count;
        
        debug!("Generated PCB statistics: {:?}", stats);
        stats
    }
}

impl Default for PcbWorld {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the PCB
#[derive(Debug, Default, Clone)]
pub struct PcbStatistics {
    pub total_components: usize,
    pub front_components: usize,
    pub back_components: usize,
    pub other_layer_components: usize,
    pub mounting_holes: usize,
    pub dnp_components: usize,
    pub exclude_bom_components: usize,
    pub locked_components: usize,
}
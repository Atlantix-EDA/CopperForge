//! The ECS World for KiForge
//! This maintains all PCB entities, components, and systems alongside the egui app
//! 
//! This module defines the `KiForgeWorld` struct which encapsulates the ECS world and its systems.
//! It provides methods for spawning entities, running systems, and tracking entity counts.
//!
//! What are the main components of the ECS world?
//! - **World**: The main ECS world that holds all entities and components.
//! - **Schedule**: The system schedule that defines the order of system execution.
//! 
//! Typical components include:
//! - **Electrical Components**: PCB elements like traces, pads, vias, 2 terminal passive device, IC, connector, etc.
//! - **Mechanical components** like 3d step models, or other physical representations.
use bevy_ecs::prelude::*;

#[derive(Default)]
pub struct KiForgeWorld {
    pub world: World,
    pub schedule: Schedule,
    // Simple counters for entity types
    pub component_count: usize,
    pub via_count: usize,
    pub pad_count: usize,
    pub trace_count: usize,
}

impl KiForgeWorld {
    pub fn new() -> Self {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        
        // Register systems
        schedule.add_systems((
            super::systems::layer_update_system,
            super::systems::pcb_element_system,
        ));
        
        Self {
            world,
            schedule,
            component_count: 0,
            via_count: 0,
            pad_count: 0,
            trace_count: 0,
        }
    }
    
    /// Run all ECS systems once
    pub fn update(&mut self) {
        self.schedule.run(&mut self.world);
    }
    
    /// Get a reference to the world for querying
    pub fn world(&self) -> &World {
        &self.world
    }
    
    /// Get a mutable reference to the world for spawning entities
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
    
    /// Get count of entities by type
    pub fn get_entity_counts(&self) -> (usize, usize, usize, usize) {
        (self.component_count, self.via_count, self.pad_count, self.trace_count)
    }
    
    /// Add entity with type tracking
    pub fn spawn_component(&mut self, bundle: impl Bundle) {
        self.world.spawn(bundle);
        self.component_count += 1;
    }
    
    pub fn spawn_via(&mut self, bundle: impl Bundle) {
        self.world.spawn(bundle);
        self.via_count += 1;
    }
    
    pub fn spawn_pad(&mut self, bundle: impl Bundle) {
        self.world.spawn(bundle);
        self.pad_count += 1;
    }
    
    pub fn spawn_trace(&mut self, bundle: impl Bundle) {
        self.world.spawn(bundle);
        self.trace_count += 1;
    }
}
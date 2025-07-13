//! ECS Components for KiCad PCB data

use bevy_ecs::prelude::*;

/// Position and orientation of a PCB component
#[derive(Component, Debug, Clone)]
pub struct Position {
    pub x: f64,          // millimeters
    pub y: f64,          // millimeters  
    pub rotation: f64,   // degrees
}

/// Basic component information
#[derive(Component, Debug, Clone)]
pub struct ComponentInfo {
    pub reference: String,        // R1, C2, U3, etc.
    pub value: String,           // 10k, 100nF, etc.
    pub footprint_name: String,  // Library:Footprint
}

/// Component description/comment
#[derive(Component, Debug, Clone)]
pub struct ComponentDescription {
    pub description: String,
}

/// Layer information
#[derive(Component, Debug, Clone)]
pub struct Layer {
    pub layer_name: String,  // F.Cu, B.Cu, etc.
}

/// Component flags and attributes
#[derive(Component, Debug, Clone)]
pub struct ComponentFlags {
    pub exclude_from_bom: bool,
    pub do_not_populate: bool,
    pub locked: bool,
}

/// Unique identifier
#[derive(Component, Debug, Clone)]
pub struct ComponentId {
    pub uuid: String,
}

/// Mounting hole specific data
#[derive(Component, Debug, Clone)]
pub struct MountingHole {
    pub diameter_mm: f64,
    pub screw_size: String,  // M2, M3, M4, etc.
}

// ===== Component type markers =====

/// Marker component for resistors
#[derive(Component, Debug, Clone)]
pub struct Resistor;

/// Marker component for capacitors
#[derive(Component, Debug, Clone)]
pub struct Capacitor;

/// Marker component for integrated circuits
#[derive(Component, Debug, Clone)]
pub struct IntegratedCircuit;

/// Marker component for connectors
#[derive(Component, Debug, Clone)]
pub struct Connector;
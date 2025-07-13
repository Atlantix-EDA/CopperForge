//! LibrePCB ECS Components
//! 
//! Placeholder components for LibrePCB data mapping to ECS.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

/// Unique identifier for LibrePCB components
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LibrePcbComponentId(pub String);

/// LibrePCB component information
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct LibrePcbComponentInfo {
    pub name: String,
    pub value: String,
    pub device_name: String,
    pub library: String,
}

/// Component position in LibrePCB coordinates
#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LibrePcbPosition {
    pub x: f64,
    pub y: f64,
    pub rotation: f64, // in degrees
}

/// LibrePCB layer information
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct LibrePcbLayer {
    pub name: String,
    pub layer_type: LibrePcbLayerType,
    pub visible: bool,
}

/// LibrePCB layer types (placeholder)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibrePcbLayerType {
    TopCopper,
    BottomCopper,
    TopSilkscreen,
    BottomSilkscreen,
    TopSoldermask,
    BottomSoldermask,
    TopPaste,
    BottomPaste,
    Outline,
    Drill,
    Mechanical(u8),
}

/// Component flags (DNP, etc.)
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibrePcbComponentFlags {
    pub do_not_populate: bool,
    pub exclude_from_bom: bool,
    pub locked: bool,
}

/// Marker components for different component types
#[derive(Component, Debug, Clone, Copy)]
pub struct LibrePcbResistor;

#[derive(Component, Debug, Clone, Copy)]
pub struct LibrePcbCapacitor;

#[derive(Component, Debug, Clone, Copy)]
pub struct LibrePcbIntegratedCircuit;

#[derive(Component, Debug, Clone, Copy)]
pub struct LibrePcbConnector;
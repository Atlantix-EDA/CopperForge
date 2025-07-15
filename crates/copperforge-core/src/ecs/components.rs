use bevy_ecs::prelude::*;
use gerber_viewer::{GerberLayer, BoundingBox, GerberImageTransform};
use crate::display::manager::MirroringSettings;
use crate::display::VectorOffset;
use egui::Color32;
use std::path::PathBuf;
use super::LayerType; // Import LayerType from types module

// Note: kicad-ecs components are for individual PCB components (R1, C2, etc.)
// while KiForge works with entire layers. We might use kicad-ecs later for
// component-level analysis, but for now we need layer-level components.

// Core gerber data wrapper
#[derive(Component)]
pub struct GerberData(pub GerberLayer);

// Layer identification
#[derive(Component, Clone, Debug)]
pub struct LayerInfo {
    pub layer_type: LayerType,
    pub name: String,
    pub file_path: Option<PathBuf>,
}

// Transform components
#[derive(Component, Clone, Debug)]
pub struct Transform {
    pub position: VectorOffset,
    pub rotation: f32,
    pub scale: f64,
    pub mirroring: MirroringSettings,
    pub origin: VectorOffset,
}

// Gerber image transform component for legacy transformations
#[derive(Component, Clone, Debug)]
pub struct ImageTransform {
    pub transform: GerberImageTransform,
}

impl Default for ImageTransform {
    fn default() -> Self {
        Self {
            transform: GerberImageTransform::default(),
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: VectorOffset { x: 0.0, y: 0.0 },
            rotation: 0.0,
            scale: 1.0,
            mirroring: MirroringSettings { x: false, y: false },
            origin: VectorOffset { x: 0.0, y: 0.0 },
        }
    }
}

// Visibility control
#[derive(Component, Clone, Debug)]
pub struct Visibility {
    pub visible: bool,
    pub opacity: f32,
}

impl Default for Visibility {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 1.0,
        }
    }
}

// Rendering properties
#[derive(Component, Clone, Debug)]
pub struct RenderProperties {
    pub color: Color32,
    pub highlight_color: Option<Color32>,
    pub z_order: i32,
}

// Bounding box cache
#[derive(Component, Clone, Debug)]
pub struct BoundingBoxCache {
    pub bounds: BoundingBox,
}

// Marker component for layers that need DRC
#[derive(Component)]
pub struct RequiresDrc;

// Marker for selected layers
#[derive(Component)]
pub struct Selected;
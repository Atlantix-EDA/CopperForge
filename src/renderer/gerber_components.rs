//! Gerber layer components and 3D extensions for the renderer
//!
//! This module provides components and utilities for handling Gerber layers
//! in a 3D rendering context, including layer stacking, material properties,
//! and integration with the existing layer management system.

use gerber_viewer::GerberLayer;
use crate::layer_operations::{LayerType, LayerInfo as LegacyLayerInfo};
use nalgebra::{Point2, Vector2};
use std::sync::Arc;

/// Enhanced layer information component compatible with existing layer system
#[derive(Debug, Clone)]
pub struct GerberLayerComponent {
    pub layer_type: LayerType,
    pub visible: bool,
    pub color: egui::Color32,
    /// Reference to the original gerber layer for backward compatibility
    pub gerber_layer: Option<Arc<GerberLayer>>,
    /// Raw gerber data for DRC operations
    pub raw_gerber_data: Option<String>,
    /// Z-order for 3D stacking (lower values = closer to bottom)
    pub z_order: f32,
    /// Layer thickness for 3D rendering
    pub thickness: f64,
}

impl GerberLayerComponent {
    pub fn new(layer_type: LayerType) -> Self {
        Self {
            color: layer_type.color(),
            visible: true,
            gerber_layer: None,
            raw_gerber_data: None,
            z_order: layer_type.default_z_order(),
            thickness: layer_type.default_thickness(),
            layer_type,
        }
    }

    pub fn from_legacy(layer_info: &LegacyLayerInfo) -> Self {
        Self {
            layer_type: layer_info.layer_type.clone(),
            visible: layer_info.visible,
            color: layer_info.layer_type.color(),
            gerber_layer: layer_info.gerber_layer.as_ref().map(|g| Arc::new(g.clone())),
            raw_gerber_data: layer_info.raw_gerber_data.clone(),
            z_order: layer_info.layer_type.default_z_order(),
            thickness: layer_info.layer_type.default_thickness(),
        }
    }

    pub fn to_legacy(&self) -> LegacyLayerInfo {
        LegacyLayerInfo {
            layer_type: self.layer_type.clone(),
            visible: self.visible,
            gerber_layer: self.gerber_layer.as_ref().map(|arc| (**arc).clone()),
            raw_gerber_data: self.raw_gerber_data.clone(),
        }
    }
}

/// Gerber geometric element extracted from layer data
#[derive(Debug, Clone)]
pub enum GerberElement {
    /// Linear trace with width
    Trace {
        start: Point2<f64>,
        end: Point2<f64>,
        width: f64,
        aperture_id: Option<u32>,
    },
    /// Arc trace segment
    Arc {
        start: Point2<f64>,
        end: Point2<f64>,
        center: Point2<f64>,
        width: f64,
        clockwise: bool,
        aperture_id: Option<u32>,
    },
    /// Flash (aperture instance at specific location)
    Flash {
        position: Point2<f64>,
        aperture_id: u32,
    },
    /// Filled polygon region
    Polygon {
        vertices: Vec<Point2<f64>>,
        holes: Vec<Vec<Point2<f64>>>,
    },
    /// Text element (from silkscreen layers)
    Text {
        content: String,
        position: Point2<f64>,
        size: f64,
        rotation: f32,
    },
    /// Drill hole
    Drill {
        position: Point2<f64>,
        diameter: f64,
        plated: bool,
    },
}

/// Aperture definition from Gerber file
#[derive(Debug, Clone)]
pub struct ApertureDefinition {
    pub id: u32,
    pub shape: ApertureShape,
}

#[derive(Debug, Clone)]
pub enum ApertureShape {
    Circle { diameter: f64 },
    Rectangle { width: f64, height: f64 },
    Oval { width: f64, height: f64 },
    Polygon { vertices: Vec<Point2<f64>> },
    Macro { name: String, parameters: Vec<f64> },
}

/// 3D position component extending the basic Position
#[derive(Debug, Clone)]
pub struct Position3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Position3D {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn from_2d(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn to_2d(&self) -> (f64, f64) {
        (self.x, self.y)
    }
}

/// 3D transform component for future 3D rendering
#[derive(Debug, Clone)]
pub struct Transform3D {
    pub rotation: Vector2<f32>, // X and Y rotation in addition to Z
    pub scale: Vector2<f32>,
    pub z_rotation: f32, // Original 2D rotation
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            rotation: Vector2::new(0.0, 0.0),
            scale: Vector2::new(1.0, 1.0),
            z_rotation: 0.0,
        }
    }
}

/// Material properties for physical simulation
#[derive(Debug, Clone)]
pub struct MaterialProperties {
    /// Dielectric constant for insulators
    pub dielectric_constant: f64,
    /// Conductivity for metals (S/m)
    pub conductivity: f64,
    /// Loss tangent for frequency-dependent losses
    pub loss_tangent: f64,
    /// Material density (kg/m³)
    pub density: f64,
    /// Visual material properties
    pub material_type: MaterialType,
}

#[derive(Debug, Clone)]
pub enum MaterialType {
    Copper,
    FR4,
    Soldermask,
    Silkscreen,
    SolderPaste,
}

impl MaterialProperties {
    /// Copper properties
    pub fn copper() -> Self {
        Self {
            dielectric_constant: 1.0,
            conductivity: 5.96e7, // S/m
            loss_tangent: 0.0,
            density: 8960.0, // kg/m³
            material_type: MaterialType::Copper,
        }
    }

    /// FR4 properties
    pub fn fr4() -> Self {
        Self {
            dielectric_constant: 4.5,
            conductivity: 0.0,
            loss_tangent: 0.02,
            density: 1850.0, // kg/m³
            material_type: MaterialType::FR4,
        }
    }

    /// Soldermask properties
    pub fn soldermask() -> Self {
        Self {
            dielectric_constant: 3.3,
            conductivity: 0.0,
            loss_tangent: 0.025,
            density: 1200.0, // kg/m³
            material_type: MaterialType::Soldermask,
        }
    }

    /// Silkscreen properties  
    pub fn silkscreen() -> Self {
        Self {
            dielectric_constant: 3.0,
            conductivity: 0.0,
            loss_tangent: 0.02,
            density: 1100.0, // kg/m³
            material_type: MaterialType::Silkscreen,
        }
    }

    /// Solder paste properties
    pub fn solder_paste() -> Self {
        Self {
            dielectric_constant: 2.0,
            conductivity: 1.0e6, // Lower than solid copper
            loss_tangent: 0.01,
            density: 7500.0, // kg/m³
            material_type: MaterialType::SolderPaste,
        }
    }
}

/// Stackup information for 3D PCB representation
#[derive(Debug, Clone)]
pub struct StackupLayer {
    /// Layer index from bottom (0 = bottom-most)
    pub layer_index: u32,
    /// Z position of layer bottom
    pub z_bottom: f64,
    /// Z position of layer top  
    pub z_top: f64,
    /// Material properties
    pub material: MaterialProperties,
}

impl StackupLayer {
    pub fn thickness(&self) -> f64 {
        self.z_top - self.z_bottom
    }

    /// Create a new stackup layer
    pub fn new(layer_index: u32, z_bottom: f64, thickness: f64, material: MaterialProperties) -> Self {
        Self {
            layer_index,
            z_bottom,
            z_top: z_bottom + thickness,
            material,
        }
    }
}

/// Extensions to LayerType for 3D properties
pub trait LayerTypeExt {
    fn default_z_order(&self) -> f32;
    fn default_thickness(&self) -> f64;
    fn default_material(&self) -> MaterialProperties;
    fn material_id(&self) -> u32;
}

impl LayerTypeExt for LayerType {
    fn default_z_order(&self) -> f32 {
        match self {
            LayerType::BottomCopper => 0.0,
            LayerType::BottomSoldermask => 1.0,
            LayerType::BottomSilk => 2.0,
            LayerType::BottomPaste => 3.0,
            LayerType::TopPaste => 4.0,
            LayerType::TopSilk => 5.0,
            LayerType::TopSoldermask => 6.0,
            LayerType::TopCopper => 7.0,
            LayerType::MechanicalOutline => 8.0,
        }
    }

    fn default_thickness(&self) -> f64 {
        match self {
            LayerType::TopCopper | LayerType::BottomCopper => 0.035, // 35μm copper
            LayerType::TopSoldermask | LayerType::BottomSoldermask => 0.025, // 25μm soldermask
            LayerType::TopSilk | LayerType::BottomSilk => 0.012, // 12μm silkscreen
            LayerType::TopPaste | LayerType::BottomPaste => 0.15, // 150μm paste
            LayerType::MechanicalOutline => 1.6, // Standard PCB thickness
        }
    }

    fn default_material(&self) -> MaterialProperties {
        match self {
            LayerType::TopCopper | LayerType::BottomCopper => MaterialProperties::copper(),
            LayerType::TopSoldermask | LayerType::BottomSoldermask => MaterialProperties::soldermask(),
            LayerType::TopSilk | LayerType::BottomSilk => MaterialProperties::silkscreen(),
            LayerType::TopPaste | LayerType::BottomPaste => MaterialProperties::solder_paste(),
            LayerType::MechanicalOutline => MaterialProperties::fr4(),
        }
    }

    fn material_id(&self) -> u32 {
        match self {
            LayerType::TopCopper | LayerType::BottomCopper => 1, // Copper
            LayerType::TopSoldermask | LayerType::BottomSoldermask => 2, // Soldermask
            LayerType::TopSilk | LayerType::BottomSilk => 3, // Silkscreen
            LayerType::TopPaste | LayerType::BottomPaste => 4, // Solder paste
            LayerType::MechanicalOutline => 5, // FR4 substrate
        }
    }
}

/// Rendering component marker for 3D objects
#[derive(Debug, Clone)]
pub struct Renderable3D {
    pub visible: bool,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
}

impl Default for Renderable3D {
    fn default() -> Self {
        Self {
            visible: true,
            cast_shadows: true,
            receive_shadows: true,
        }
    }
}

/// PCB layer mesh component for efficient rendering
#[derive(Debug, Clone)]
pub struct LayerMesh {
    pub layer_type: LayerType,
    pub mesh_id: u32,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub bounds: Option<(Point2<f64>, Point2<f64>)>, // 2D bounding box
}

impl LayerMesh {
    pub fn new(layer_type: LayerType, mesh_id: u32, vertex_count: usize, triangle_count: usize) -> Self {
        Self {
            layer_type,
            mesh_id,
            vertex_count,
            triangle_count,
            bounds: None,
        }
    }

    pub fn with_bounds(mut self, min: Point2<f64>, max: Point2<f64>) -> Self {
        self.bounds = Some((min, max));
        self
    }
}
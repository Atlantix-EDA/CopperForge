use bevy_ecs::prelude::*;
use nalgebra::{Point2, Vector2};

/// Position component for PCB elements
#[derive(Component, Debug, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    
    pub fn from_point(point: Point2<f64>) -> Self {
        Self { x: point.x, y: point.y }
    }
    
    pub fn to_point(&self) -> Point2<f64> {
        Point2::new(self.x, self.y)
    }
}

/// Transform component for rotation, scaling, etc.
#[derive(Component, Debug, Clone)]
pub struct Transform {
    pub rotation: f32,
    pub scale: Vector2<f32>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            scale: Vector2::new(1.0, 1.0),
        }
    }
}

/// ECS Layer information component (renamed to avoid conflict)
#[derive(Component, Debug, Clone)]
pub struct EcsLayerInfo {
    pub layer_type: String,
    pub visible: bool,
    pub color: [f32; 4], // RGBA
}

impl EcsLayerInfo {
    pub fn new(layer_type: String, visible: bool, color: [f32; 4]) -> Self {
        Self {
            layer_type,
            visible,
            color,
        }
    }
}

/// PCB element types
#[derive(Component, Debug, Clone)]
pub enum PcbElement {
    Trace {
        width: f64,
        start: Point2<f64>,
        end: Point2<f64>,
    },
    Via {
        radius: f64,
        drill_radius: f64,
    },
    Pad {
        width: f64,
        height: f64,
        shape: PadShape,
    },
    Component {
        name: String,
        footprint: String,
    },
}

#[derive(Debug, Clone)]
pub enum PadShape {
    Rectangle,
    Circle,
    Oval,
}

/// Bounding box component for efficient spatial queries
#[derive(Component, Debug, Clone)]
pub struct BoundingBox {
    pub min: Point2<f64>,
    pub max: Point2<f64>,
}

impl BoundingBox {
    pub fn new(min: Point2<f64>, max: Point2<f64>) -> Self {
        Self { min, max }
    }
    
    pub fn contains_point(&self, point: Point2<f64>) -> bool {
        point.x >= self.min.x && point.x <= self.max.x &&
        point.y >= self.min.y && point.y <= self.max.y
    }
    
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x &&
        self.min.y <= other.max.y && self.max.y >= other.min.y
    }
}

/// Selection component for UI interaction
#[derive(Component, Debug, Clone)]
pub struct Selected {
    pub selected_at: std::time::Instant,
}

impl Selected {
    pub fn new() -> Self {
        Self {
            selected_at: std::time::Instant::now(),
        }
    }
}
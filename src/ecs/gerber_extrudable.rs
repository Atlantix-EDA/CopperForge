//! Extrudable trait for Gerber primitives
//! 
//! This module provides implementations of the `Extrudable` trait for various Gerber primitives,
//! allowing them to be extruded into 3D meshes. The nalgebra library is used for 2D point representation,
//! and the `ExtrusionEngine` is used to handle the extrusion process.
//! 
//! In each of the extrusion implementations, it's basically a calculus problem where a 2D shape is extended vertically
//! to create a 3D mesh. The number of segments for circles is set to 32, which is a common choice for smoothness.
//! The choice of 32 segments is a balance between performance and visual quality, but it can be adjusted based on specific needs.
//! // The `ExtrusionError` enum is used to handle various errors that may occur during the extrusion process,
//! such as invalid geometry or triangulation failures.


use crate::ecs::{Mesh3D, Polygon2D, ExtrusionEngine, ExtrusionError};
use gerber_viewer::{GerberLayer, GerberPrimitive, CircleGerberPrimitive, RectangleGerberPrimitive, LineGerberPrimitive, ArcGerberPrimitive, PolygonGerberPrimitive};
use nalgebra::Point2;

/// Trait for extruding 2D gerber primitives into 3D meshes
pub trait Extrudable {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError>;
}

/// Implement Extrudable for CircleGerberPrimitive
/// 
/// This will create a cylinder mesh based on the circle's center and diameter.
/// The circle is extruded vertically to the specified height, and nalgebra's Point2 is used for 2D coordinates.
///
impl Extrudable for CircleGerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        let radius = self.diameter / 2.0;
        let center = Point2::new(self.center.x, self.center.y);
        
        // Generate circle vertices
        let segments = 32;
        let mut vertices = Vec::new();
        
        for i in 0..segments {
            let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            vertices.push(Point2::new(
                center.x as f64 + radius as f64 * angle.cos(),
                center.y as f64 + radius as f64 * angle.sin(),
            ));
        }
        
        let polygon2d = Polygon2D::new(vertices);
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("circle")))?;
        mesh.material_id = Some(material_id);
        Ok(mesh)
    }
}

impl Extrudable for RectangleGerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        let vertices = vec![
            Point2::new(self.origin.x as f64, self.origin.y as f64),
            Point2::new((self.origin.x + self.width) as f64, self.origin.y as f64),
            Point2::new((self.origin.x + self.width) as f64, (self.origin.y + self.height) as f64),
            Point2::new(self.origin.x as f64, (self.origin.y + self.height) as f64),
        ];
        
        let polygon2d = Polygon2D::new(vertices);
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("rectangle")))?;
        mesh.material_id = Some(material_id);
        Ok(mesh)
    }
}

impl Extrudable for LineGerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        let width = self.width as f64;
        let half_width = width / 2.0;
        
        // Calculate line direction and perpendicular
        let dx = (self.end.x - self.start.x) as f64;
        let dy = (self.end.y - self.start.y) as f64;
        let length = (dx * dx + dy * dy).sqrt();
        
        if length == 0.0 {
            return Err(ExtrusionError::InvalidGeometry("Zero-length line".to_string()));
        }
        
        // Normalize direction
        let ux = dx / length;
        let uy = dy / length;
        
        // Perpendicular direction
        let perp_x = -uy;
        let perp_y = ux;
        
        // Generate rectangle vertices for the line
        let vertices = vec![
            Point2::new(
                self.start.x as f64 - perp_x * half_width,
                self.start.y as f64 - perp_y * half_width,
            ),
            Point2::new(
                self.start.x as f64 + perp_x * half_width,
                self.start.y as f64 + perp_y * half_width,
            ),
            Point2::new(
                self.end.x as f64 + perp_x * half_width,
                self.end.y as f64 + perp_y * half_width,
            ),
            Point2::new(
                self.end.x as f64 - perp_x * half_width,
                self.end.y as f64 - perp_y * half_width,
            ),
        ];
        
        let polygon2d = Polygon2D::new(vertices);
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("line")))?;
        mesh.material_id = Some(material_id);
        Ok(mesh)
    }
}

impl Extrudable for ArcGerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        let width = self.width;
        let inner_radius = self.radius - width / 2.0;
        let outer_radius = self.radius + width / 2.0;
        let center = Point2::new(self.center.x, self.center.y);
        
        // Generate arc vertices
        let segments = 32;
        let start_angle = self.start_angle;
        let sweep_angle = self.sweep_angle;
        
        let mut vertices = Vec::new();
        
        // Outer arc vertices
        for i in 0..=segments {
            let t = i as f64 / segments as f64;
            let angle = start_angle + sweep_angle * t;
            vertices.push(Point2::new(
                center.x + outer_radius * angle.cos(),
                center.y + outer_radius * angle.sin(),
            ));
        }
        
        // Inner arc vertices (reverse order)
        for i in (0..=segments).rev() {
            let t = i as f64 / segments as f64;
            let angle = start_angle + sweep_angle * t;
            vertices.push(Point2::new(
                center.x + inner_radius * angle.cos(),
                center.y + inner_radius * angle.sin(),
            ));
        }
        
        let polygon2d = Polygon2D::new(vertices);
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("arc")))?;
        mesh.material_id = Some(material_id);
        Ok(mesh)
    }
}

impl Extrudable for PolygonGerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        // Convert relative vertices to absolute positions
        let mut vertices = Vec::new();
        for vertex in &self.geometry.relative_vertices {
            vertices.push(Point2::new(
                (self.center.x + vertex.x) as f64,
                (self.center.y + vertex.y) as f64,
            ));
        }
        
        let polygon2d = Polygon2D::new(vertices);
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("polygon")))?;
        mesh.material_id = Some(material_id);
        Ok(mesh)
    }
}

impl Extrudable for GerberPrimitive {
    fn extrude(&self, height: f32, material_id: u32, engine: &mut ExtrusionEngine) -> Result<Mesh3D, ExtrusionError> {
        match self {
            GerberPrimitive::Circle(primitive) => primitive.extrude(height, material_id, engine),
            GerberPrimitive::Rectangle(primitive) => primitive.extrude(height, material_id, engine),
            GerberPrimitive::Line(primitive) => primitive.extrude(height, material_id, engine),
            GerberPrimitive::Arc(primitive) => primitive.extrude(height, material_id, engine),
            GerberPrimitive::Polygon(primitive) => primitive.extrude(height, material_id, engine),
        }
    }
}

/// Convert an entire GerberLayer to 3D meshes
pub fn layer_to_3d_meshes(
    layer: &GerberLayer,
    layer_height: f32,
    material_id: u32,
    engine: &mut ExtrusionEngine,
) -> Vec<Mesh3D> {
    let mut meshes = Vec::new();
    
    // Process each primitive in the layer
    for primitive in layer.primitives() {
        match primitive.extrude(layer_height, material_id, engine) {
            Ok(mesh) => meshes.push(mesh),
            Err(e) => {
                log::warn!("Failed to extrude primitive: {}", e);
            }
        }
    }
    
    meshes
}


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

use crate::renderer::mesh3d::{Mesh3D, Polygon2D, ExtrusionEngine, ExtrusionError};
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
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("circle_{:.2}_{:.2}_{:.2}", center.x, center.y, radius)))?;
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
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("rect_{:.2}_{:.2}_{:.2}_{:.2}", self.origin.x, self.origin.y, self.width, self.height)))?;
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
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("line_{:.2}_{:.2}_{:.2}_{:.2}_{:.2}", self.start.x, self.start.y, self.end.x, self.end.y, width)))?;
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
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("arc_{:.2}_{:.2}_{:.2}_{:.2}_{:.2}", center.x, center.y, self.radius, start_angle, sweep_angle)))?;
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
        let mut mesh = engine.extrude_polygon(&polygon2d, 0.0, height, Some(format!("polygon_{:.2}_{:.2}_{}", self.center.x, self.center.y, self.geometry.relative_vertices.len())))?;
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
/// This is the core function that implements the algorithm you described:
/// 1) Ingest a gerber layer object
/// 2) Iterate on all the gerber geometry primitives  
/// 3) Convert each to a mesh
/// 4) Create a Vec<Mesh3D>
pub fn layer_to_3d_meshes(
    layer: &GerberLayer,
    layer_height: f32,
    material_id: u32,
    engine: &mut ExtrusionEngine,
) -> Vec<Mesh3D> {
    let mut meshes = Vec::new();
    
    log::info!("Converting Gerber layer to 3D meshes: {} primitives, height: {}", layer.primitives().len(), layer_height);
    
    // Process each primitive in the layer
    for (index, primitive) in layer.primitives().iter().enumerate() {
        match primitive.extrude(layer_height, material_id, engine) {
            Ok(mesh) => {
                let stats = mesh.stats();
                log::debug!("Primitive {}: {} vertices, {} triangles", index, stats.vertex_count, stats.triangle_count);
                meshes.push(mesh);
            },
            Err(e) => {
                log::warn!("Failed to extrude primitive {}: {}", index, e);
            }
        }
    }
    
    log::info!("Generated {} meshes from layer", meshes.len());
    meshes
}

/// Combine multiple meshes into a single mesh for efficient rendering
pub fn combine_meshes(meshes: Vec<Mesh3D>) -> Mesh3D {
    if meshes.is_empty() {
        return Mesh3D::new();
    }
    
    if meshes.len() == 1 {
        return meshes.into_iter().next().unwrap();
    }
    
    let mut combined = Mesh3D::new();
    
    for mesh in &meshes {
        combined.merge(mesh);
    }
    
    log::info!("Combined {} meshes into single mesh with {} vertices and {} triangles", 
               meshes.len(), combined.vertices.len(), combined.indices.len() / 3);
    
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::mesh3d::ExtrusionEngine;

    #[test]
    fn test_circle_extrusion() {
        let mut engine = ExtrusionEngine::new();
        let circle = CircleGerberPrimitive {
            center: nalgebra::Point2::new(0.0, 0.0),
            diameter: 2.0,
        };
        
        let result = circle.extrude(1.0, 1, &mut engine);
        assert!(result.is_ok());
        
        let mesh = result.unwrap();
        assert_eq!(mesh.vertices.len(), 64); // 32 segments * 2 (top/bottom)
        assert_eq!(mesh.material_id, Some(1));
    }

    #[test]
    fn test_rectangle_extrusion() {
        let mut engine = ExtrusionEngine::new();
        let rect = RectangleGerberPrimitive {
            origin: nalgebra::Point2::new(0.0, 0.0),
            width: 2.0,
            height: 1.0,
        };
        
        let result = rect.extrude(0.5, 2, &mut engine);
        assert!(result.is_ok());
        
        let mesh = result.unwrap();
        assert_eq!(mesh.vertices.len(), 8); // 4 corners * 2 (top/bottom)
        assert_eq!(mesh.material_id, Some(2));
    }
}
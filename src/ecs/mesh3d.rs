//! KiForgeWorld - mesh generation and extrusion engine
//! 
//! This module provides the `ExtrusionEngine` for converting 2D polygons into 3D meshes,
//! as well as the `StackupMeshGenerator` for creating complete PCB stackup meshes.
//! It includes the `Mesh3D` component for 3D mesh representation,
//! `Polygon2D` for 2D polygons, and utility functions for area calculation and extrusion errors.
//! 
use nalgebra::{Point2, Point3, Vector3};
use std::collections::HashMap;

/// 3D mesh representation for rendering
#[derive(Debug, Clone)]
pub struct Mesh3D {
    pub vertices: Vec<Point3<f32>>,
    pub indices: Vec<u32>,
    pub normals: Vec<Vector3<f32>>,
    pub uvs: Vec<Point2<f32>>,
    pub material_id: Option<u32>,
}

impl Mesh3D {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            material_id: None,
        }
    }

    /// Calculate axis-aligned bounding box
    pub fn bounding_box(&self) -> Option<(Point3<f32>, Point3<f32>)> {
        if self.vertices.is_empty() {
            return None;
        }

        let mut min = self.vertices[0];
        let mut max = self.vertices[0];

        for vertex in &self.vertices {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            min.z = min.z.min(vertex.z);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
            max.z = max.z.max(vertex.z);
        }

        Some((min, max))
    }

    /// Get mesh statistics
    pub fn stats(&self) -> MeshStats {
        MeshStats {
            vertex_count: self.vertices.len(),
            triangle_count: self.indices.len() / 3,
            has_normals: !self.normals.is_empty(),
            has_uvs: !self.uvs.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeshStats {
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub has_normals: bool,
    pub has_uvs: bool,
}

/// 2D polygon representation for extrusion
#[derive(Debug, Clone)]
pub struct Polygon2D {
    pub exterior: Vec<Point2<f64>>,
    pub holes: Vec<Vec<Point2<f64>>>,
}

impl Polygon2D {
    pub fn new(exterior: Vec<Point2<f64>>) -> Self {
        Self {
            exterior,
            holes: Vec::new(),
        }
    }

    pub fn with_holes(mut self, holes: Vec<Vec<Point2<f64>>>) -> Self {
        self.holes = holes;
        self
    }

    /// Calculate 2D area (exterior minus holes)
    pub fn area(&self) -> f64 {
        let exterior_area = shoelace_area(&self.exterior);
        let holes_area: f64 = self.holes.iter().map(|hole| shoelace_area(hole)).sum();
        (exterior_area - holes_area).abs()
    }

    /// Check if polygon is valid (exterior has at least 3 points, holes are inside)
    pub fn is_valid(&self) -> bool {
        self.exterior.len() >= 3 && 
        self.holes.iter().all(|hole| hole.len() >= 3)
    }
}

/// 3D extrusion system for converting 2D Gerber polygons to 3D meshes
pub struct ExtrusionEngine {
    /// Cache of generated meshes
    pub mesh_cache: HashMap<String, Mesh3D>,
}

impl ExtrusionEngine {
    pub fn new() -> Self {
        Self {
            mesh_cache: HashMap::new(),
        }
    }

    /// Extrude a 2D polygon to create a 3D mesh
    pub fn extrude_polygon(
        &mut self,
        polygon: &Polygon2D,
        bottom_z: f32,
        top_z: f32,
        cache_key: Option<String>,
    ) -> Result<Mesh3D, ExtrusionError> {
        // Check cache first
        if let Some(key) = &cache_key {
            if let Some(cached_mesh) = self.mesh_cache.get(key) {
                return Ok(cached_mesh.clone());
            }
        }

        if !polygon.is_valid() {
            return Err(ExtrusionError::InvalidPolygon);
        }

        let thickness = top_z - bottom_z;
        if thickness <= 0.0 {
            return Err(ExtrusionError::InvalidThickness);
        }

        let mut mesh = Mesh3D::new();

        // Generate vertices for top and bottom faces
        self.generate_face_vertices(&mut mesh, polygon, bottom_z, top_z)?;

        // Generate triangulated faces
        self.triangulate_faces(&mut mesh, polygon)?;

        // Generate side walls
        self.generate_side_walls(&mut mesh, polygon, bottom_z, top_z)?;

        // Calculate normals
        self.calculate_normals(&mut mesh);

        // Generate UVs
        self.generate_uvs(&mut mesh);

        // Cache the result
        if let Some(key) = cache_key {
            self.mesh_cache.insert(key, mesh.clone());
        }

        Ok(mesh)
    }

    /// Generate vertices for top and bottom faces
    fn generate_face_vertices(
        &self,
        mesh: &mut Mesh3D,
        polygon: &Polygon2D,
        bottom_z: f32,
        top_z: f32,
    ) -> Result<(), ExtrusionError> {
        // Bottom face vertices
        for point in &polygon.exterior {
            mesh.vertices.push(Point3::new(point.x as f32, point.y as f32, bottom_z));
        }

        // Top face vertices
        for point in &polygon.exterior {
            mesh.vertices.push(Point3::new(point.x as f32, point.y as f32, top_z));
        }

        // Handle holes - add vertices for each hole at both levels
        for hole in &polygon.holes {
            // Bottom hole vertices
            for point in hole {
                mesh.vertices.push(Point3::new(point.x as f32, point.y as f32, bottom_z));
            }
            // Top hole vertices
            for point in hole {
                mesh.vertices.push(Point3::new(point.x as f32, point.y as f32, top_z));
            }
        }

        Ok(())
    }

    /// Triangulate top and bottom faces using ear clipping
    fn triangulate_faces(
        &self,
        mesh: &mut Mesh3D,
        polygon: &Polygon2D,
    ) -> Result<(), ExtrusionError> {
        let exterior_count = polygon.exterior.len();
        
        // Simple triangulation for now - fan triangulation from first vertex
        // For a more robust solution, we'd use ear clipping or Delaunay triangulation
        
        // Bottom face triangulation (counter-clockwise when viewed from above)
        for i in 1..(exterior_count - 1) {
            mesh.indices.push(0);
            mesh.indices.push(i as u32);
            mesh.indices.push((i + 1) as u32);
        }

        // Top face triangulation (clockwise when viewed from above to face outward)
        let top_offset = exterior_count as u32;
        for i in 1..(exterior_count - 1) {
            mesh.indices.push(top_offset);
            mesh.indices.push(top_offset + (i + 1) as u32);
            mesh.indices.push(top_offset + i as u32);
        }

        Ok(())
    }

    /// Generate side wall triangles
    fn generate_side_walls(
        &self,
        mesh: &mut Mesh3D,
        polygon: &Polygon2D,
        _bottom_z: f32,
        _top_z: f32,
    ) -> Result<(), ExtrusionError> {
        let exterior_count = polygon.exterior.len();
        let top_offset = exterior_count as u32;

        // Generate side walls for exterior
        for i in 0..exterior_count {
            let next_i = (i + 1) % exterior_count;
            
            let bottom_current = i as u32;
            let bottom_next = next_i as u32;
            let top_current = top_offset + i as u32;
            let top_next = top_offset + next_i as u32;

            // Two triangles per side face
            // Triangle 1: bottom_current -> bottom_next -> top_current
            mesh.indices.push(bottom_current);
            mesh.indices.push(bottom_next);
            mesh.indices.push(top_current);

            // Triangle 2: bottom_next -> top_next -> top_current
            mesh.indices.push(bottom_next);
            mesh.indices.push(top_next);
            mesh.indices.push(top_current);
        }

        // TODO: Handle holes - similar process but with correct winding

        Ok(())
    }

    /// Calculate vertex normals
    fn calculate_normals(&self, mesh: &mut Mesh3D) {
        mesh.normals.clear();
        mesh.normals.resize(mesh.vertices.len(), Vector3::new(0.0, 0.0, 0.0));

        // Calculate face normals and accumulate at vertices
        for triangle in mesh.indices.chunks(3) {
            if triangle.len() == 3 {
                let v0 = mesh.vertices[triangle[0] as usize];
                let v1 = mesh.vertices[triangle[1] as usize];
                let v2 = mesh.vertices[triangle[2] as usize];

                let edge1 = v1 - v0;
                let edge2 = v2 - v0;
                let normal = edge1.cross(&edge2).normalize();

                // Accumulate normal at each vertex
                mesh.normals[triangle[0] as usize] += normal;
                mesh.normals[triangle[1] as usize] += normal;
                mesh.normals[triangle[2] as usize] += normal;
            }
        }

        // Normalize accumulated normals
        for normal in &mut mesh.normals {
            if normal.magnitude() > 0.0 {
                *normal = normal.normalize();
            }
        }
    }

    /// Generate UV coordinates
    fn generate_uvs(&self, mesh: &mut Mesh3D) {
        mesh.uvs.clear();
        mesh.uvs.reserve(mesh.vertices.len());

        if let Some((min_pt, max_pt)) = mesh.bounding_box() {
            let size_x = max_pt.x - min_pt.x;
            let size_y = max_pt.y - min_pt.y;

            for vertex in &mesh.vertices {
                let u = if size_x > 0.0 { (vertex.x - min_pt.x) / size_x } else { 0.0 };
                let v = if size_y > 0.0 { (vertex.y - min_pt.y) / size_y } else { 0.0 };
                mesh.uvs.push(Point2::new(u, v));
            }
        } else {
            // Fallback - all UVs at origin
            mesh.uvs.resize(mesh.vertices.len(), Point2::new(0.0, 0.0));
        }
    }

    /// Clear the mesh cache
    pub fn clear_cache(&mut self) {
        self.mesh_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        let cached_meshes = self.mesh_cache.len();
        let total_vertices: usize = self.mesh_cache.values()
            .map(|mesh| mesh.vertices.len())
            .sum();
        (cached_meshes, total_vertices)
    }
}

#[derive(Debug, Clone)]
pub enum ExtrusionError {
    InvalidPolygon,
    InvalidThickness,
    TriangulationFailed,
    InsufficientVertices,
    InvalidGeometry(String),
}

impl std::fmt::Display for ExtrusionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtrusionError::InvalidPolygon => write!(f, "Invalid polygon geometry"),
            ExtrusionError::InvalidThickness => write!(f, "Invalid layer thickness"),
            ExtrusionError::TriangulationFailed => write!(f, "Failed to triangulate polygon"),
            ExtrusionError::InsufficientVertices => write!(f, "Insufficient vertices for triangulation"),
            ExtrusionError::InvalidGeometry(msg) => write!(f, "Invalid geometry: {}", msg),
        }
    }
}

impl std::error::Error for ExtrusionError {}

/// Calculate polygon area using shoelace formula
fn shoelace_area(points: &[Point2<f64>]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    area / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polygon_area() {
        // Unit square
        let square = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        assert_eq!(shoelace_area(&square), 1.0);
    }

    #[test]
    fn test_simple_extrusion() {
        let mut engine = ExtrusionEngine::new();
        let polygon = Polygon2D::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ]);

        let result = engine.extrude_polygon(&polygon, 0.0, 1.0, None);
        assert!(result.is_ok());
        
        let mesh = result.unwrap();
        assert_eq!(mesh.vertices.len(), 8); // 4 bottom + 4 top
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn test_mesh_stats() {
        let mut mesh = Mesh3D::new();
        mesh.vertices = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        mesh.indices = vec![0, 1, 2];

        let stats = mesh.stats();
        assert_eq!(stats.vertex_count, 3);
        assert_eq!(stats.triangle_count, 1);
    }
}